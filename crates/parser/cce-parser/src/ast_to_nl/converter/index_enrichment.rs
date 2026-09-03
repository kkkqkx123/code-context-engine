//! Index-only enrichment for AST to natural language conversion
//!
//! This module augments entity conversion results with sidecar data during
//! indexing. It keeps export-oriented presentation conversion unchanged.

use crate::ast_to_nl::noise::NoiseProfile;
use crate::grouper::ProcessingResult;
use cce_text::Bm25TextCleaner;
use cce_types::ast_to_nl::ConversionResult;
use cce_types::language::Language;
use cce_types::{BehaviorFactKind, EntityId};

/// Enriches entity conversion text with index-only sidecar data.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct IndexTextEnricher;

impl IndexTextEnricher {
    pub fn new() -> Self {
        Self
    }

    pub fn enrich_conversion(
        &self,
        conversion: &mut ConversionResult,
        processing_result: &ProcessingResult,
        source: &str,
        language: Language,
        bm25_cleaner: &Bm25TextCleaner,
    ) {
        let Some(extra_text) =
            self.build_extra_text(conversion.entity_id, processing_result, source)
        else {
            return;
        };

        let bm25_cleaned = bm25_cleaner.clean(&extra_text);

        // Embedding path: strip language-specific syntax noise (`unsafe {}`,
        // `&*`, `&mut *` for Rust) while preserving implementation logic, then
        // remove safety boilerplate comments. BM25 keeps the raw code so the
        // tokenizer can produce dual-form tokens (whole identifier + subword
        // splits), maximizing recall for both exact-spelling and fuzzy queries.
        let profile = NoiseProfile::for_language(language);
        let stripped = strip_syntax_noise(&extra_text, profile);
        let emb_cleaned = crate::ast_to_nl::embedding::filter_embedding_noise(&stripped, profile);

        conversion.append_index_context_raw_separate(&bm25_cleaned, &emb_cleaned);
    }

    /// Build merged code text from control flow and behavior facts, sorted by
    /// byte offset. Overlapping ranges are deduplicated. No label headers.
    fn build_extra_text(
        &self,
        entity_id: EntityId,
        processing_result: &ProcessingResult,
        source: &str,
    ) -> Option<String> {
        let cf_facts = processing_result.control_flow.get(entity_id);
        let bf_facts = processing_result.behavior.get(entity_id);

        let mut all_fragments: Vec<(usize, usize, String)> = Vec::new();

        if let Some(cf) = cf_facts {
            for fact in &cf.facts {
                if fact.content_line_count == 0 {
                    continue;
                }
                let text = extract_clean_source(source, fact.start_byte, fact.end_byte);
                if !text.trim().is_empty() {
                    all_fragments.push((fact.start_byte, fact.end_byte, text));
                }
            }
        }

        if let Some(bf) = bf_facts {
            let cf_ranges: Vec<(usize, usize)> =
                all_fragments.iter().map(|(s, e, _)| (*s, *e)).collect();
            for fact in &bf.facts {
                if fact.content_line_count == 0 {
                    continue;
                }
                let is_inside_cf = cf_ranges
                    .iter()
                    .any(|(s, e)| fact.start_byte >= *s && fact.end_byte <= *e);
                if is_inside_cf {
                    continue;
                }
                // Comment and macro-body facts carry pre-cleaned text; other
                // facts are extracted from source with comments stripped.
                // Macro bodies keep their stored text because they are flat
                // token sequences where `#` markers (e.g. `#[derive(...)]`)
                // are significant and would be mangled by source re-extraction.
                let text = if matches!(
                    fact.kind,
                    BehaviorFactKind::Comment | BehaviorFactKind::MacroBody
                ) {
                    fact.text.clone()
                } else {
                    extract_clean_source(source, fact.start_byte, fact.end_byte)
                };
                if !text.trim().is_empty() {
                    all_fragments.push((fact.start_byte, fact.end_byte, text));
                }
            }
        }

        if all_fragments.is_empty() {
            return None;
        }

        all_fragments.sort_by_key(|(s, _, _)| *s);

        let mut merged: Vec<String> = Vec::new();
        let mut last_end: Option<usize> = None;
        for (start, end, text) in &all_fragments {
            if let Some(prev_end) = last_end {
                if *start <= prev_end {
                    continue;
                }
            }
            merged.push(text.clone());
            last_end = Some(*end);
        }

        Some(merged.join("\n"))
    }
}

/// Strip language-specific syntax noise from code fragments while preserving
/// implementation logic.
///
/// For profiles where they are enabled (Rust), removes `unsafe { ... }`
/// wrappers (keeping inner content) and the `&*` / `&mut *` dereference
/// markers. These tokens carry no semantic signal for embedding and dilute
/// the vector representation. For other languages the text is passed through
/// unchanged, since constructs like `&*` are valid and meaningful source.
fn strip_syntax_noise(text: &str, profile: NoiseProfile) -> String {
    let text = if profile.unwrap_unsafe_blocks {
        strip_unsafe_blocks(text)
    } else {
        text.to_string()
    };
    let text = if profile.strip_deref_markers {
        text.replace("&mut *", "").replace("&*", "")
    } else {
        text
    };
    let text = if profile.strip_macro_repetition {
        strip_macro_repetitions(&text)
    } else {
        text
    };
    let text = text.replace(" .", ".");
    let text = text.replace(" ;", ";");
    // Collapse horizontal whitespace artifacts left by block removal while
    // preserving newline structure of the code fragment.
    cce_utils::normalize_whitespace_preserving_newlines(&text)
        .trim()
        .to_string()
}

/// Remove `unsafe { ... }` wrappers, keeping the inner content.
/// Handles nested brace blocks.
fn strip_unsafe_blocks(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let is_keyword_start =
            i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if is_keyword_start && bytes[i..].starts_with(b"unsafe") {
            let mut j = i + "unsafe".len();
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'{' {
                let mut depth = 1usize;
                let mut k = j + 1;
                while k < bytes.len() && depth > 0 {
                    match bytes[k] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    k += 1;
                }
                if depth == 0 {
                    result.push_str(&text[j + 1..k - 1]);
                    i = k;
                    continue;
                }
            }
        }
        let ch = text[i..].chars().next().expect("valid utf8 boundary");
        result.push(ch);
        i += ch.len_utf8();
    }
    result
}

/// Remove macro repetition wrappers (`$( ... )*`), keeping the inner pattern.
///
/// Macro-body fragments use `$(...)` repetition syntax whose wrapper tokens
/// carry no lexical meaning for embedding; the inner pattern (e.g. `tt`) and
/// `$metavar` identifiers are preserved.
fn strip_macro_repetitions(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'(') {
            i += 2;
            continue;
        }
        if matches!(bytes[i], b')' | b'}')
            && matches!(bytes.get(i + 1), Some(b'*') | Some(b'+') | Some(b'?'))
        {
            i += 2;
            continue;
        }
        let ch = text[i..].chars().next().expect("valid utf8 boundary");
        result.push(ch);
        i += ch.len_utf8();
    }
    result
}

fn deindent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| {
            if l.len() > min_indent {
                &l[min_indent..]
            } else {
                l.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_clean_source(source: &str, start_byte: usize, end_byte: usize) -> String {
    let end = end_byte.min(source.len());
    if start_byte >= end {
        return String::new();
    }

    let keyword_indent = source[..start_byte]
        .rfind('\n')
        .map(|pos| start_byte - pos - 1)
        .unwrap_or(0);

    let cleaned: String = source[start_byte..end]
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let stripped = match line.find("//") {
                Some(pos) => &line[..pos],
                None => line,
            };
            let stripped = match stripped.find('#') {
                Some(pos) => &stripped[..pos],
                None => stripped,
            };
            let trimmed = stripped.trim_end();
            if i == 0 {
                format!("{}{}", " ".repeat(keyword_indent), trimmed)
            } else {
                trimmed.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    deindent(&cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{
        BehaviorFact, BehaviorFactKind, BehaviorStore, ControlFlowFact, ControlFlowFactKind,
        ControlFlowStore,
    };

    #[test]
    fn test_build_extra_text_merges_cf_and_behavior() {
        let source = "fn demo() { let x = 1; if x > 0 { return; } }";
        let mut cf_store = ControlFlowStore::default();
        let if_start = source.find("if x > 0").unwrap();
        cf_store
            .entry_mut(EntityId(1))
            .push_fact(ControlFlowFact::new(
                ControlFlowFactKind::If,
                &source[if_start..source.len() - 1],
                if_start,
                source.len() - 1,
            ));

        let mut bf_store = BehaviorStore::default();
        let bind_start = source.find("let x = 1;").unwrap();
        let bind_end = bind_start + "let x = 1;".len();
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataBind,
            "let x = 1;",
            bind_start,
            bind_end,
        ));

        let processing_result = ProcessingResult {
            groups: vec![],
            entity_meta: Default::default(),
            behavior: bf_store,
            control_flow: cf_store,
            stats: Default::default(),
        };

        let enricher = IndexTextEnricher::new();
        let result = enricher
            .build_extra_text(EntityId(1), &processing_result, source)
            .expect("should produce text");
        assert!(result.contains("let x = 1;"));
        assert!(result.contains("if x > 0"));
        assert!(result.contains("return;"));
    }

    #[test]
    fn test_build_extra_text_behavior_only() {
        let source = "fn demo() { let x = 1; }";
        let mut bf_store = BehaviorStore::default();
        let bind_start = source.find("let x = 1;").unwrap();
        let bind_end = bind_start + "let x = 1;".len();
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataBind,
            "let x = 1;",
            bind_start,
            bind_end,
        ));

        let processing_result = ProcessingResult {
            groups: vec![],
            entity_meta: Default::default(),
            behavior: bf_store,
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let enricher = IndexTextEnricher::new();
        let result = enricher
            .build_extra_text(EntityId(1), &processing_result, source)
            .expect("should produce text");
        assert!(result.contains("let x = 1;"));
    }

    #[test]
    fn test_build_extra_text_control_flow_only() {
        let source = "fn demo() { for i in 0..10 { process(i); } }";
        let mut cf_store = ControlFlowStore::default();
        let loop_start = source.find("for i in").unwrap();
        let loop_end = source.rfind('}').unwrap() + 1;
        cf_store
            .entry_mut(EntityId(1))
            .push_fact(ControlFlowFact::new(
                ControlFlowFactKind::Loop,
                &source[loop_start..loop_end],
                loop_start,
                loop_end,
            ));

        let processing_result = ProcessingResult {
            groups: vec![],
            entity_meta: Default::default(),
            behavior: BehaviorStore::default(),
            control_flow: cf_store,
            stats: Default::default(),
        };

        let enricher = IndexTextEnricher::new();
        let result = enricher
            .build_extra_text(EntityId(1), &processing_result, source)
            .expect("should produce text");
        assert!(result.contains("for i in"));
        assert!(result.contains("process(i)"));
    }

    #[test]
    fn test_build_extra_text_data_statement_dedupes_with_cf_and_behavior() {
        let source = "fn demo() { if ready { return; } log(config.verbosity); value = compute(); }";
        let mut cf_store = ControlFlowStore::default();
        let if_start = source.find("if ready").unwrap();
        let if_end = source.find("} log(").unwrap() + 1;
        cf_store
            .entry_mut(EntityId(1))
            .push_fact(ControlFlowFact::new(
                ControlFlowFactKind::If,
                &source[if_start..if_end],
                if_start,
                if_end,
            ));

        let mut bf_store = BehaviorStore::default();
        let log_start = source.find("log(config.verbosity)").unwrap();
        let log_end = log_start + "log(config.verbosity);".len();
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataStatement,
            "log(config.verbosity);",
            log_start,
            log_end,
        ));
        let assign_start = source.find("value = compute();").unwrap();
        let assign_end = assign_start + "value = compute();".len();
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataStatement,
            "value = compute();",
            assign_start,
            assign_end,
        ));

        let processing_result = ProcessingResult {
            groups: vec![],
            entity_meta: Default::default(),
            behavior: bf_store,
            control_flow: cf_store,
            stats: Default::default(),
        };

        let enricher = IndexTextEnricher::new();
        let result = enricher
            .build_extra_text(EntityId(1), &processing_result, source)
            .expect("should produce text");
        assert!(result.contains("log(config.verbosity);"));
        assert!(result.contains("value = compute();"));
        assert!(result.contains("if ready"));
        assert!(result.contains("return;"));
    }

    #[test]
    fn test_build_extra_text_empty_returns_none() {
        let processing_result = ProcessingResult {
            groups: vec![],
            entity_meta: Default::default(),
            behavior: BehaviorStore::default(),
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let enricher = IndexTextEnricher::new();
        let result = enricher.build_extra_text(EntityId(1), &processing_result, "");
        assert!(result.is_none());
    }

    #[test]
    fn test_strip_syntax_noise_rust_profile() {
        let source = "unsafe { &mut *self.inner.get() }.as_mut()";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Rust));
        assert!(!cleaned.contains("unsafe"), "got: {cleaned}");
        assert!(!cleaned.contains("&mut *"), "got: {cleaned}");
        assert!(
            cleaned.contains("self.inner.get().as_mut()"),
            "got: {cleaned}"
        );
    }

    #[test]
    fn test_strip_syntax_noise_non_rust_profile() {
        let source = "auto x = &*ptr;";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Cpp));
        assert!(
            cleaned.contains("&*ptr"),
            "should preserve C++ deref, got: {cleaned}"
        );
    }

    #[test]
    fn test_strip_syntax_noise_keeps_unsafe_for_non_rust() {
        let source = "unsafe { &mut *self.inner.get() }";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Python));
        assert!(cleaned.contains("unsafe"), "got: {cleaned}");
        assert!(cleaned.contains("&mut *"), "got: {cleaned}");
    }

    #[test]
    fn test_build_extra_text_deduplicates_overlapping_ranges() {
        let source = "fn demo() { loop { for item in items { if *item < 0 { continue; } buffer.push(*item); } } }";
        let mut cf_store = ControlFlowStore::default();

        // Outer loop
        cf_store
            .entry_mut(EntityId(1))
            .push_fact(ControlFlowFact::new(
                ControlFlowFactKind::Loop,
                "loop { for item in items { if *item < 0 { continue; } buffer.push(*item); } }",
                11,
                source.len() - 2,
            ));
        // Inner for loop (contained in outer)
        let for_start = source.find("for item").unwrap();
        let for_end = source.rfind("buffer.push").unwrap() + "buffer.push(*item)".len();
        cf_store
            .entry_mut(EntityId(1))
            .push_fact(ControlFlowFact::new(
                ControlFlowFactKind::Loop,
                "for item in items { if *item < 0 { continue; } buffer.push(*item); }",
                for_start,
                for_end,
            ));

        let processing_result = ProcessingResult {
            groups: vec![],
            entity_meta: Default::default(),
            behavior: BehaviorStore::default(),
            control_flow: cf_store,
            stats: Default::default(),
        };

        let enricher = IndexTextEnricher::new();
        let result = enricher
            .build_extra_text(EntityId(1), &processing_result, source)
            .expect("should produce text");
        // Outer loop covers inner — only one text block
        assert!(result.contains("loop"));
        assert!(result.contains("for item in items"));
        assert!(result.contains("if *item < 0"));
        assert!(result.contains("continue"));
    }

    #[test]
    fn test_build_extra_text_macro_body_uses_stored_text() {
        let source = "macro_rules! demo { () => {{ if ready { proceed!(); } }} }";
        let mut bf_store = BehaviorStore::default();
        let body_start = source.find("{{").unwrap() + 1;
        let body_end = source.rfind("}}").unwrap() + 1;
        // Stored pre-cleaned text keeps `#`-style tokens intact and contains
        // the code that tree-sitter would never parse structurally.
        let stored = "if ready { proceed!(); }";
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::MacroBody,
            stored,
            body_start,
            body_end,
        ));

        let processing_result = ProcessingResult {
            groups: vec![],
            entity_meta: Default::default(),
            behavior: bf_store,
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let enricher = IndexTextEnricher::new();
        let result = enricher
            .build_extra_text(EntityId(1), &processing_result, source)
            .expect("should produce text");
        assert!(
            result.contains("if ready"),
            "macro body should keep if statement, got: {result}"
        );
        assert!(
            result.contains("proceed!()"),
            "macro body should keep invocations, got: {result}"
        );
    }

    #[test]
    fn test_strip_macro_repetitions_rust_profile() {
        let source = "write!($($arg)*) { $($tt)* }";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Rust));
        assert!(
            cleaned.contains("write!($arg)"),
            "should strip $()* wrappers while keeping $metavar, got: {cleaned}"
        );
        assert!(
            cleaned.contains("$tt"),
            "should keep metavariables, got: {cleaned}"
        );
        assert!(
            !cleaned.contains("$(") && !cleaned.contains(")*"),
            "should remove repetition wrappers, got: {cleaned}"
        );
    }

    #[test]
    fn test_strip_macro_repetitions_non_rust_profile() {
        let source = "$(cmd)";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Bash));
        assert!(
            cleaned.contains("$(cmd)"),
            "Bash command substitution must be preserved, got: {cleaned}"
        );
    }

    #[test]
    fn test_extract_clean_source_strips_comments() {
        let source = "let x = 5; // initialize\nlet y = 10;";
        let result = extract_clean_source(source, 0, source.len());
        assert!(!result.contains("//"), "should strip comments");
        assert!(result.contains("let x = 5;"), "should keep code");
        assert!(result.contains("let y = 10;"), "should keep code");
    }

    #[test]
    fn test_extract_clean_source_handles_out_of_bounds() {
        let source = "short";
        let result = extract_clean_source(source, 0, 100);
        assert_eq!(result, "short");

        let result = extract_clean_source(source, 10, 20);
        assert_eq!(result, "");
    }

    #[test]
    fn test_extract_clean_source_deindents() {
        let source = "fn demo() {\n    if true {\n        return;\n    }\n}";
        let result = extract_clean_source(source, 12, source.len() - 1);
        assert!(
            !result.lines().next().unwrap().starts_with(' '),
            "first line should be de-indented: {:?}",
            result
        );
    }

    #[test]
    fn test_deindent_removes_common_leading_whitespace() {
        let input = "    if true {\n        return;\n    }";
        let result = deindent(input);
        assert_eq!(result, "if true {\n    return;\n}");
    }

    #[test]
    fn test_extract_clean_source_preserves_indentation_when_keyword_is_indented() {
        let source = "fn demo() {\n    if let Some(items) = values {\n        let mut buffer = Vec::new();\n    } else {\n        return;\n    }\n}";
        let if_start = source.find("if let").unwrap();
        let if_end = source.rfind('}').unwrap();
        let result = extract_clean_source(source, if_start, if_end);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines[0], "if let Some(items) = values {");
        assert_eq!(lines[1], "    let mut buffer = Vec::new();");
        assert_eq!(lines[2], "} else {");
        assert_eq!(lines[3], "    return;");
    }
}
