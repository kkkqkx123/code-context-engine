//! Index-only enrichment for AST to natural language conversion.
//!
//! This module augments entity conversion results with sidecar data during
//! indexing. It keeps export-oriented presentation conversion unchanged.
//!
//! The orchestrator ([`IndexTextEnricher`]) runs four focused stages; each
//! stage lives in its own submodule so language tables, text surgery and
//! source plumbing evolve independently:
//!
//! - [`calls_prune`] trims the template's `calls:` line,
//! - [`topic_anchor`] lifts branch discriminants to the text head,
//! - [`syntax_strip`] removes language-specific syntax noise,
//! - [`source_extract`] re-extracts fact ranges from source.

mod calls_prune;
mod source_extract;
mod syntax_strip;
mod topic_anchor;

use crate::ast_to_nl::noise::NoiseProfile;
use crate::grouper::ProcessingResult;
use calls_prune::prune_redundant_calls_lines;
use cce_plugin::vocabulary::FileVocabulary;
use cce_text::Bm25TextCleaner;
use cce_types::ast_to_nl::ConversionResult;
use cce_types::language::Language;
use cce_types::{BehaviorFactKind, EntityId};
use source_extract::{
    extract_body_without_signature, extract_clean_source, find_entity_doc_comment,
    find_entity_signature, find_entity_span, is_docstring_statement, is_meaningful_code,
    strip_trailing_closers,
};
use syntax_strip::strip_syntax_noise;
use topic_anchor::{insert_after_first_line, topic_anchor};

/// Enriches entity conversion text with index-only sidecar data.
///
/// The cleaners are borrowed from the converter, so the enricher stays
/// cheap to build for every entity.
#[derive(Clone, Copy)]
pub(super) struct IndexTextEnricher<'a> {
    bm25_cleaner: &'a Bm25TextCleaner,
}

impl<'a> IndexTextEnricher<'a> {
    pub fn new(bm25_cleaner: &'a Bm25TextCleaner) -> Self {
        Self { bm25_cleaner }
    }

    pub fn enrich_conversion(
        &self,
        conversion: &mut ConversionResult,
        processing_result: &ProcessingResult,
        source: &str,
        language: Language,
        vocabulary: Option<&FileVocabulary>,
        plugin_bare: Option<&[String]>,
    ) {
        // Topic anchor: what the entity branches on. A function that
        // dispatches on protocol constants states its whole subject in
        // those constants, but they sit deep inside a syntax-heavy body.
        // The anchor lifts them to the head of the text, where a retriever
        // reads them before the body drowns them.
        if let Some(anchor) = topic_anchor(conversion.entity_id, processing_result) {
            if let Some(text) = conversion.embedding_text.as_mut() {
                *text = insert_after_first_line(text, &anchor);
            }
            if let Some(text) = conversion.bm25_text.as_mut() {
                *text = insert_after_first_line(text, &anchor);
            }
        }

        let extra_text = self
            .build_extra_text(conversion.entity_id, processing_result, source)
            .unwrap_or_default();

        if !extra_text.trim().is_empty() {
            let bm25_cleaned = self.bm25_cleaner.clean(&extra_text);

            // Embedding path: strip language-specific syntax noise (`unsafe {}`,
            // `&*`, `&mut *` for Rust) while preserving implementation logic, then
            // remove safety boilerplate comments. BM25 keeps the raw code so the
            // tokenizer can produce dual-form tokens (whole identifier + subword
            // splits), maximizing recall for both exact-spelling and fuzzy queries.
            let profile = NoiseProfile::for_language(language);
            let stripped = strip_syntax_noise(&extra_text, profile);
            let emb_cleaned = crate::ast_to_nl::common::filter_nl_noise(&stripped, profile);

            conversion.append_index_context_raw_separate(&bm25_cleaned, &emb_cleaned);
        }

        // The template emits a `calls:` line naming the entity's call
        // targets, but the behavior sidecar above appends the raw body that
        // already states most of those calls verbatim. Restating them in a
        // dedicated line only duplicates the identifiers and adds a shared
        // common component across every callable chunk, pulling unrelated
        // vectors toward each other. Drop the callees the surrounding text
        // already names; drop the line when nothing distinctive remains.
        // BM25 text is untouched: the keyword index relies on the raw
        // identifier occurrences.
        prune_redundant_calls_lines(conversion, language, plugin_bare);

        if let Some(vocabulary) = vocabulary {
            self.compute_glossary(conversion, processing_result, &extra_text, vocabulary);
        }
    }

    /// Collect the domain terminology of the entity.
    ///
    /// The scan covers the entity name, its signature, its documentation
    /// comment and the function body text produced by the index enrichment
    /// stage. The terminology is carried on the conversion result instead of
    /// being written into the texts: the chunker places the expansions into
    /// the indexed text, while the export path renders them into the
    /// entity description it writes for a reader.
    fn compute_glossary(
        &self,
        conversion: &mut ConversionResult,
        processing_result: &ProcessingResult,
        body_text: &str,
        vocabulary: &FileVocabulary,
    ) {
        let signature = find_entity_signature(processing_result, conversion.entity_id)
            .map(str::to_string)
            .unwrap_or_default();
        let doc_comment = find_entity_doc_comment(processing_result, conversion.entity_id)
            .map(str::to_string)
            .unwrap_or_default();
        let matched = vocabulary.matched_entries(&[
            conversion.name.as_str(),
            signature.as_str(),
            doc_comment.as_str(),
            body_text,
        ]);
        conversion.set_glossary(matched);
    }

    /// Build merged code text from control flow and behavior facts, sorted by
    /// byte offset. Overlapping ranges are deduplicated. Uncovered body gaps
    /// inside the entity span are filled from de-commented source so the full
    /// function body stays searchable. No label headers.
    fn build_extra_text(
        &self,
        entity_id: EntityId,
        processing_result: &ProcessingResult,
        source: &str,
    ) -> Option<String> {
        // FileDocumentation groups are pure prose; their span covers the doc
        // comment itself. Extracting a "body" from it would re-append the
        // full document the template already compressed to ~300 tokens,
        // undoing the cap and splitting one summary into many near-identical
        // chunks. Docs carry no callable behavior, so there is no sidecar.
        if processing_result.groups.iter().any(|group| {
            group.group_type == cce_types::GroupType::FileDocumentation
                && (group.header.as_ref().is_some_and(|h| h.id == entity_id)
                    || group.members.iter().any(|m| m.id == entity_id))
        }) {
            return None;
        }
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
                // Docstring and comment facts duplicate the template's cleaned
                // docstring output, so they are skipped here to avoid emitting
                // raw RST (`"""`, `:attr:`, `.. versionchanged::`) twice.
                // Template owns documentation; the sidecar owns code behavior.
                if matches!(fact.kind, BehaviorFactKind::Comment) {
                    continue;
                }
                // A docstring is captured as a plain expression-statement
                // fact; its raw literal would reintroduce the RST markup the
                // template already converted to natural language.
                if is_docstring_statement(&fact.text) {
                    continue;
                }
                let is_inside_cf = cf_ranges
                    .iter()
                    .any(|(s, e)| fact.start_byte >= *s && fact.end_byte <= *e);
                if is_inside_cf {
                    continue;
                }
                // Macro-body facts carry pre-cleaned text; other facts are
                // extracted from source with comments stripped. Macro bodies
                // keep their stored text because they are flat token sequences
                // where `#` markers (e.g. `#[derive(...)]`) are significant
                // and would be mangled by source re-extraction.
                let text = if matches!(fact.kind, BehaviorFactKind::MacroBody) {
                    fact.text.clone()
                } else {
                    extract_clean_source(source, fact.start_byte, fact.end_byte)
                };
                if !text.trim().is_empty() {
                    all_fragments.push((fact.start_byte, fact.end_byte, text));
                }
            }
        }

        let entity_span = find_entity_span(processing_result, entity_id);

        if all_fragments.is_empty() {
            if let Some((span_start, span_end)) = entity_span {
                let body = extract_body_without_signature(source, span_start, span_end);
                if !body.trim().is_empty() {
                    return Some(body);
                }
            }
            return None;
        }

        all_fragments.sort_by_key(|(s, _, _)| *s);

        let mut merged: Vec<(usize, usize, String)> = Vec::new();
        let mut last_end: Option<usize> = None;
        for (start, end, text) in &all_fragments {
            if let Some(prev_end) = last_end {
                if *start <= prev_end {
                    continue;
                }
            }
            merged.push((*start, *end, text.clone()));
            last_end = Some(*end);
        }

        if let Some((span_start, span_end)) = entity_span {
            let mut gap_texts: Vec<(usize, String)> = Vec::new();
            for window in merged.windows(2) {
                let gap_start = window[0].1;
                let gap_end = window[1].0;
                if gap_end > gap_start {
                    let gap = extract_clean_source(source, gap_start, gap_end);
                    if is_meaningful_code(&gap) {
                        gap_texts.push((gap_start, gap));
                    }
                }
            }
            if let Some((_, last_end_byte, _)) = merged.last() {
                if span_end > *last_end_byte {
                    let tail = extract_clean_source(source, *last_end_byte, span_end);
                    let tail = strip_trailing_closers(&tail);
                    if is_meaningful_code(&tail) {
                        gap_texts.push((*last_end_byte, tail));
                    }
                }
            }
            let _ = span_start;
            for (gap_start, gap) in gap_texts {
                let pos = merged
                    .iter()
                    .position(|(s, _, _)| *s > gap_start)
                    .unwrap_or(merged.len());
                merged.insert(pos, (gap_start, gap_start, gap));
            }
        }

        let texts: Vec<String> = merged.into_iter().map(|(_, _, t)| t).collect();
        Some(texts.join("\n"))
    }
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

            behavior: bf_store,
            control_flow: cf_store,
            stats: Default::default(),
        };

        let bm25_cleaner = Bm25TextCleaner::new();
        let enricher = IndexTextEnricher::new(&bm25_cleaner);
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

            behavior: bf_store,
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let bm25_cleaner = Bm25TextCleaner::new();
        let enricher = IndexTextEnricher::new(&bm25_cleaner);
        let result = enricher
            .build_extra_text(EntityId(1), &processing_result, source)
            .expect("should produce text");
        assert!(result.contains("let x = 1;"));
    }

    #[test]
    fn test_build_extra_text_skips_docstring_statement() {
        let source =
            "def f():\n    \"\"\"Raw docstring with .. versionadded:: 0.9\"\"\"\n    return 1\n";
        let mut bf_store = BehaviorStore::default();
        let doc_start = source.find("\"\"\"").unwrap();
        let doc_end = source.rfind("\"\"\"").unwrap() + 3;
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataStatement,
            &source[doc_start..doc_end],
            doc_start,
            doc_end,
        ));
        let ret_start = source.find("return 1").unwrap();
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataStatement,
            "return 1",
            ret_start,
            ret_start + "return 1".len(),
        ));

        let processing_result = ProcessingResult {
            groups: vec![],

            behavior: bf_store,
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let bm25_cleaner = Bm25TextCleaner::new();
        let enricher = IndexTextEnricher::new(&bm25_cleaner);
        let result = enricher
            .build_extra_text(EntityId(1), &processing_result, source)
            .expect("code facts should still produce text");
        assert!(!result.contains("\"\"\""), "raw docstring must be skipped");
        assert!(
            !result.contains("versionadded"),
            "raw RST directives must be skipped"
        );
        assert!(result.contains("return 1"));
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

            behavior: BehaviorStore::default(),
            control_flow: cf_store,
            stats: Default::default(),
        };

        let bm25_cleaner = Bm25TextCleaner::new();
        let enricher = IndexTextEnricher::new(&bm25_cleaner);
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

            behavior: bf_store,
            control_flow: cf_store,
            stats: Default::default(),
        };

        let bm25_cleaner = Bm25TextCleaner::new();
        let enricher = IndexTextEnricher::new(&bm25_cleaner);
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

            behavior: BehaviorStore::default(),
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let bm25_cleaner = Bm25TextCleaner::new();
        let enricher = IndexTextEnricher::new(&bm25_cleaner);
        let result = enricher.build_extra_text(EntityId(1), &processing_result, "");
        assert!(result.is_none());
    }

    #[test]
    fn test_build_extra_text_skips_file_documentation() {
        use crate::grouper::types::{EntityGroup, GroupType};
        use cce_types::{EntityKind, GroupedEntity};

        let mut doc_group = EntityGroup::new("file_doc_test", GroupType::FileDocumentation);
        doc_group.header = Some(GroupedEntity {
            id: EntityId(1),
            name: "lib.rs".to_string(),
            kind: EntityKind::Module,
            doc_comment: Some("The crate docs.".to_string()),
            ..Default::default()
        });
        let processing_result = ProcessingResult {
            groups: vec![doc_group],

            behavior: BehaviorStore::default(),
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let bm25_cleaner = Bm25TextCleaner::new();
        let enricher = IndexTextEnricher::new(&bm25_cleaner);
        let result = enricher.build_extra_text(
            EntityId(1),
            &processing_result,
            "//! The crate docs.\n//! More docs.\n",
        );
        assert!(
            result.is_none(),
            "file docs carry no code sidecar, got: {result:?}"
        );
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

            behavior: BehaviorStore::default(),
            control_flow: cf_store,
            stats: Default::default(),
        };

        let bm25_cleaner = Bm25TextCleaner::new();
        let enricher = IndexTextEnricher::new(&bm25_cleaner);
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

            behavior: bf_store,
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let bm25_cleaner = Bm25TextCleaner::new();
        let enricher = IndexTextEnricher::new(&bm25_cleaner);
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
}
