//! Index-only enrichment for AST to natural language conversion
//!
//! This module augments entity conversion results with sidecar data during
//! indexing. It keeps export-oriented presentation conversion unchanged.

use crate::ast_to_nl::noise::NoiseProfile;
use crate::grouper::ProcessingResult;
use cce_plugin::vocabulary::FileVocabulary;
use cce_text::Bm25TextCleaner;
use cce_types::ast_to_nl::ConversionResult;
use cce_types::language::Language;
use cce_types::{BehaviorFactKind, ControlFlowFactKind, EntityId};

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
            let emb_cleaned =
                crate::ast_to_nl::embedding::filter_embedding_noise(&stripped, profile);

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
        prune_redundant_calls_lines(conversion);

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

/// Maximum number of branch discriminants listed in a topic anchor.
const MAX_ANCHOR_DISCRIMINANTS: usize = 8;

/// Bare call-target names that carry no subject of their own.
///
/// Error constructors, assertions, formatting and collection plumbing occur
/// in nearly every callable body. Naming them in a dedicated line states
/// nothing the body does not already imply, so they never survive the
/// redundancy prune on their own.
const BOILERPLATE_CALLEES: &[&str] = &[
    "Ok",
    "Err",
    "Some",
    "None",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "panic",
    "todo",
    "unimplemented",
    "unreachable",
    "vec",
    "format",
    "write",
    "writeln",
    "print",
    "println",
    "eprint",
    "eprintln",
    "clone",
    "cloned",
    "to_string",
    "to_owned",
    "as_ref",
    "as_mut",
    "len",
    "is_empty",
    "is_some",
    "is_none",
    "is_ok",
    "is_err",
    "unwrap",
    "expect",
    "push",
    "pop",
    "iter",
    "iter_mut",
    "into",
    "from",
    "map",
    "and_then",
];

/// Whether a call target restates what the surrounding text already names.
///
/// A qualified target is redundant when it occurs verbatim, or when both
/// its head qualifier and its final segment occur as identifier tokens
/// (the receiver is spelled differently but both parts are stated). A bare
/// target is redundant when it occurs as an identifier token or belongs to
/// the language boilerplate set.
fn callee_is_redundant(
    callee: &str,
    full_text: &str,
    tokens: &std::collections::HashSet<&str>,
) -> bool {
    let callee = callee.trim().trim_end_matches('.').trim();
    if callee.is_empty() {
        return true;
    }
    if full_text.contains(callee) {
        return true;
    }
    let qualified = callee.contains("::") || callee.contains('.');
    if qualified {
        let head = callee.split([':', '.']).next().unwrap_or("").trim();
        let tail = callee.rsplit([':', '.']).next().unwrap_or("").trim();
        if !head.is_empty() && head != tail && tokens.contains(head) && tokens.contains(tail) {
            return true;
        }
        return false;
    }
    tokens.contains(callee) || BOILERPLATE_CALLEES.contains(&callee)
}

/// Drop `calls:` callees the embedding text already states.
///
/// Operates on the embedding text only, after the behavior sidecar is
/// appended, so redundancy is judged against the complete text the
/// retriever will read (identity, documentation and raw body).
fn prune_redundant_calls_lines(conversion: &mut ConversionResult) {
    let Some(text) = conversion.embedding_text.as_mut() else {
        return;
    };
    if !text.contains("calls:") {
        return;
    }
    let mut body_lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        if !line.trim_start().starts_with("calls:") {
            body_lines.push(line);
        }
    }
    let full_text = body_lines.join("\n");
    let tokens: std::collections::HashSet<&str> = full_text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty())
        .collect();
    let mut kept_lines: Vec<String> = Vec::new();
    let mut changed = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("calls:") else {
            kept_lines.push(line.to_string());
            continue;
        };
        let indent_len = line.len() - trimmed.len();
        let indent: String = line.chars().take(indent_len).collect();
        let original: Vec<&str> = rest
            .trim_end_matches('.')
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        let kept: Vec<&str> = original
            .iter()
            .copied()
            .filter(|c| !callee_is_redundant(c, &full_text, &tokens))
            .collect();
        if kept.len() != original.len() {
            changed = true;
        }
        if kept.is_empty() {
            continue;
        }
        kept_lines.push(format!("{}calls: {}.", indent, kept.join(", ")));
    }
    if changed {
        *text = kept_lines.join("\n");
    }
}

/// Topic anchor of an entity: the values its branches dispatch on.
///
/// A function that dispatches on protocol constants states its whole
/// subject in those constants, but the body buries them under a
/// `switch`/`case` skeleton. The anchor lifts the discriminants to the head
/// of the text so a retriever meets them before the syntax does.
///
/// The anchor is purely lexical: it restates identifiers that the entity's
/// own source already contains, so it cannot invent a subject the code does
/// not have. An entity without branches gets no anchor.
fn topic_anchor(entity_id: EntityId, processing_result: &ProcessingResult) -> Option<String> {
    let facts = processing_result.control_flow.get(entity_id)?;

    let mut discriminants: Vec<String> = Vec::new();

    for fact in &facts.facts {
        if matches!(
            fact.kind,
            ControlFlowFactKind::If | ControlFlowFactKind::Match
        ) {
            for label in branch_discriminants(&fact.text) {
                if discriminants.len() >= MAX_ANCHOR_DISCRIMINANTS {
                    break;
                }
                if !discriminants.iter().any(|d| d == &label) {
                    discriminants.push(label);
                }
            }
        }
    }

    if !discriminants.is_empty() {
        return Some(format!("branches on: {}.", discriminants.join(", ")));
    }
    // No discriminants: no anchor. A generic shape note ("N branches, M
    // loops") states no subject the entity does not already name; it only
    // adds a shared common component across every branching function and
    // dilutes the member's own signal. Branch-heavy functions already
    // carry their bodies in the sidecar.
    None
}

/// Discriminant identifiers of a branch statement.
///
/// The labels of a `case` arm are the values the branch dispatches on. They
/// are read from the branch's own source text, so they are exactly the
/// identifiers the code mentions.
fn branch_discriminants(text: &str) -> Vec<String> {
    const CASE: &str = "case";
    let bytes = text.as_bytes();
    let mut labels = Vec::new();
    let mut index = 0usize;

    while index < text.len() {
        // Only ever advance on UTF-8 char boundaries, so a multibyte
        // character in a comment or string literal can never be split.
        if !text[index..].starts_with(CASE) {
            index = cce_utils::text::next_char_boundary(text, index + 1);
            continue;
        }
        // The keyword must stand on its own: `case` inside a longer
        // identifier is not a branch label.
        let before_ok = index == 0 || !is_identifier_byte(bytes[index.saturating_sub(1)]);
        let after = index + CASE.len();
        let after_ok = after >= bytes.len() || !is_identifier_byte(bytes[after]);
        if !before_ok || !after_ok {
            index = cce_utils::text::next_char_boundary(text, after);
            continue;
        }
        let label_start = after
            + bytes[after..]
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .unwrap_or(0);
        let label_end = label_start
            + bytes[label_start..]
                .iter()
                .take_while(|b| is_identifier_byte(**b))
                .count();
        if label_end > label_start {
            let label = cce_utils::text::slice_exact(text, label_start, label_end).unwrap_or("");
            if !label.is_empty() && label != "default" {
                labels.push(label.to_string());
            }
        }
        index = cce_utils::text::next_char_boundary(text, label_end.max(after));
    }

    labels
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Insert a line right behind the first line of `text`.
///
/// The first line of a converted entity is its identity line, so the anchor
/// lands directly behind the identity and ahead of the body.
fn insert_after_first_line(text: &str, line: &str) -> String {
    match text.find('\n') {
        Some(end) => {
            let mut result = String::with_capacity(text.len() + line.len() + 1);
            result.push_str(&text[..=end]);
            result.push_str(line);
            result.push('\n');
            result.push_str(&text[end + 1..]);
            result
        }
        None => format!("{}\n{}", text, line),
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
        if !text.is_char_boundary(i) {
            i = cce_utils::text::ceil_char_boundary(text, i);
            continue;
        }
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

/// Whether a behavior fact is a docstring statement.
///
/// Tree-sitter represents a docstring as a plain expression statement, so
/// the behavior query captures its raw literal. The template already emits
/// the cleaned documentation; keeping the raw literal would duplicate the
/// docstring and reintroduce RST markup (`"""`, `:attr:`,
/// `.. versionchanged::`) into the sidecar text.
fn is_docstring_statement(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''")
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
            // Protocol comments (`// TagType`, `// DataSize`, trailing
            // `// AVCPacketType`) and preprocessor directives (`#define`,
            // `#include`) carry field semantics for C-like languages and
            // must survive into the embedding sidecar. Comment facts and
            // docstring statements are already skipped by the caller, so
            // keeping comment text here does not duplicate template output.
            let trimmed = line.trim_end();
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

fn find_entity_span(
    processing_result: &ProcessingResult,
    entity_id: EntityId,
) -> Option<(usize, usize)> {
    for group in &processing_result.groups {
        if let Some(span) = group.entity_spans.get(&entity_id) {
            if span.end_byte > span.start_byte {
                return Some((span.start_byte, span.end_byte));
            }
        }
    }
    None
}

/// The signature of an entity, taken from the group it belongs to.
fn find_entity_signature(
    processing_result: &ProcessingResult,
    entity_id: EntityId,
) -> Option<&str> {
    find_entity(processing_result, entity_id).map(|entity| entity.signature.as_str())
}

/// The documentation comment of an entity, taken from the group it belongs to.
fn find_entity_doc_comment(
    processing_result: &ProcessingResult,
    entity_id: EntityId,
) -> Option<&str> {
    find_entity(processing_result, entity_id).and_then(|entity| entity.doc_comment.as_deref())
}

/// The entity with the given id, taken from the group tree it belongs to.
fn find_entity(
    processing_result: &ProcessingResult,
    entity_id: EntityId,
) -> Option<&cce_types::entity::GroupedEntity> {
    for group in &processing_result.groups {
        if let Some(header) = &group.header
            && header.id == entity_id
        {
            return Some(header);
        }
        if let Some(member) = group.members.iter().find(|member| member.id == entity_id) {
            return Some(member);
        }
        if let Some(entity) = find_nested_entity(&group.nested_groups, entity_id) {
            return Some(entity);
        }
    }
    None
}

/// The entity with the given id inside a nested group tree.
fn find_nested_entity(
    groups: &[cce_types::grouper::EntityGroup],
    entity_id: EntityId,
) -> Option<&cce_types::entity::GroupedEntity> {
    for group in groups {
        if let Some(header) = &group.header
            && header.id == entity_id
        {
            return Some(header);
        }
        if let Some(member) = group.members.iter().find(|member| member.id == entity_id) {
            return Some(member);
        }
        if let Some(entity) = find_nested_entity(&group.nested_groups, entity_id) {
            return Some(entity);
        }
    }
    None
}

fn extract_body_without_signature(source: &str, start_byte: usize, end_byte: usize) -> String {
    let full = extract_clean_source(source, start_byte, end_byte);
    if full.trim().is_empty() {
        return String::new();
    }
    let mut lines = full.lines();
    let first = lines.next().unwrap_or("");
    if is_docstring_statement(first.trim_start()) {
        let rest: Vec<&str> = lines.collect();
        let joined = rest.join("\n");
        if is_meaningful_code(&joined) {
            return joined.trim().to_string();
        }
        return String::new();
    }
    let rest: Vec<&str> = lines.collect();
    if rest.is_empty() {
        return String::new();
    }
    let joined = rest.join("\n");
    let stripped = strip_trailing_closers(&joined);
    if is_meaningful_code(&stripped) {
        stripped.trim().to_string()
    } else {
        String::new()
    }
}

fn is_meaningful_code(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed
        .chars()
        .all(|c| matches!(c, '{' | '}' | '(' | ')' | ';' | ',' | ':'))
    {
        return false;
    }
    trimmed.chars().any(|c| c.is_alphanumeric())
}

fn strip_trailing_closers(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    while lines.last().is_some_and(|l| {
        let t = l.trim();
        t.is_empty() || t == "}" || t == "};" || t == "});" || t == "})"
    }) {
        lines.pop();
    }
    lines.join("\n")
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
            entity_meta: Default::default(),
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
            entity_meta: Default::default(),
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
            entity_meta: Default::default(),
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
            entity_meta: Default::default(),
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
            entity_meta: Default::default(),
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
            entity_meta: Default::default(),
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
            entity_meta: Default::default(),
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
        // Protocol comments and trailing field notes are retrieval signals
        // and must survive into the embedding sidecar.
        let source = "let x = 5; // initialize\nlet y = 10;";
        let result = extract_clean_source(source, 0, source.len());
        assert!(result.contains("// initialize"), "should keep comments");
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

    #[test]
    fn branch_discriminants_handles_multibyte() {
        let text = "switch (x) { case 世界: break; case default: break; }";
        let labels = branch_discriminants(text);
        assert!(labels.contains(&"世界".to_string()) || labels.is_empty());
        // Should not panic on multibyte input.
        let _ = branch_discriminants("switch (x) { case ¡café: break; }");
    }

    fn prune_case(text: &str) -> String {
        let mut conversion = ConversionResult::embedding_only(
            EntityId(1),
            cce_types::EntityKind::Function,
            "demo".to_string(),
            "demo.rs".to_string(),
            text.to_string(),
        );
        prune_redundant_calls_lines(&mut conversion);
        conversion
            .embedding_text
            .expect("embedding text survives pruning")
    }

    #[test]
    fn prune_drops_callees_stated_by_body() {
        let out = prune_case(
            "function demo.\n\
             calls: Candidate::new, as_ref, is_match_candidate.\n\
             Returns true.\n\
             self.is_match_candidate(&Candidate::new(path.as_ref()))",
        );
        assert!(!out.contains("calls:"), "all callees restated: {out:?}");
        assert!(out.contains("self.is_match_candidate"));
    }

    #[test]
    fn prune_drops_boilerplate_only_line() {
        let out = prune_case("function demo.\ncalls: Ok.\nReturns a match.\nOk(self.find())");
        assert!(!out.contains("calls:"), "boilerplate restated: {out:?}");
    }

    #[test]
    fn prune_drops_boilerplate_absent_from_body() {
        let out = prune_case("function demo.\ncalls: vec, push.\nDoes things.\nlet x = 1;");
        assert!(!out.contains("calls:"), "bare boilerplate dropped: {out:?}");
    }

    #[test]
    fn prune_keeps_distinctive_unstated_callee() {
        let out =
            prune_case("function demo.\ncalls: build_from_hir, Ok.\nBuilds a regex.\nOk(output)");
        assert!(
            out.contains("calls: build_from_hir."),
            "keeps novel callee: {out:?}"
        );
        assert!(!out.contains("Ok,"));
    }
}
