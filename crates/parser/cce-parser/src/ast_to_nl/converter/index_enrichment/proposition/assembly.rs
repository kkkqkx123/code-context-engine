//! Budget assembly for propositions.
//!
//! Orders sentences by source offset, deduplicates whole sentences, caps
//! the head, and keeps the raw fragments of mapped facts the head could
//! not cover as the overflow tail. An empty head means the entity keeps its
//! previous sidecar: no sentence earned its place.

use std::collections::HashSet;

use cce_types::{BehaviorFactKind, EntityId};

use super::render::{render_behavior, render_control_flow};
use super::vocab::MAX_PROPOSITION_SENTENCES;
use crate::grouper::ProcessingResult;

use super::super::source_extract::{code_body_start, find_entity_span, is_docstring_statement};

/// Rendered propositions of one entity.
///
/// `head` holds at most [`MAX_PROPOSITION_SENTENCES`] sentences for the
/// head of the embedding text. `tail` holds the raw fragments of mapped
/// facts the head could not cover, for the embedding tail. `overflow`
/// tells whether such a tail exists.
pub(crate) struct PropositionSet {
    pub head: Vec<String>,
    pub tail: Vec<(usize, usize, String)>,
    pub overflow: bool,
}

/// Render the embedding-path propositions of one entity.
///
/// Control-flow and behavior facts are rendered independently: the keyword
/// path skips behavior facts inside control-flow ranges because the outer
/// raw text already contains them, but here the outer text is reduced to a
/// condition sentence, so inner facts must each speak for themselves.
/// Output is ordered by source offset, deduplicated by whole sentence, and
/// the head is capped. Facts that only restate already-rendered sentences
/// need no tail of their own. An empty head means the entity keeps its
/// previous sidecar.
pub(crate) fn render_propositions(
    entity_id: EntityId,
    processing_result: &ProcessingResult,
    source: &str,
) -> PropositionSet {
    let entity_span = find_entity_span(processing_result, entity_id);
    let body_start =
        entity_span.map(|(span_start, span_end)| code_body_start(source, span_start, span_end));

    // (offset, start, end, sentence) in source order.
    let mut ordered: Vec<(usize, usize, usize, String)> = Vec::new();
    // Mapped fragments that may need a raw tail, keyed by range.
    let mut mapped: Vec<(usize, usize, String)> = Vec::new();

    if let Some(store) = processing_result.control_flow.get(entity_id) {
        for fact in &store.facts {
            if fact.content_line_count == 0 {
                continue;
            }
            if let Some(body) = body_start {
                if fact.end_byte <= body {
                    continue;
                }
            }
            if let Some(sentence) = render_control_flow(fact.kind, &fact.text) {
                mapped.push((fact.start_byte, fact.end_byte, fact.text.clone()));
                ordered.push((fact.start_byte, fact.start_byte, fact.end_byte, sentence));
            }
        }
    }

    if let Some(store) = processing_result.behavior.get(entity_id) {
        for fact in &store.facts {
            if fact.content_line_count == 0 {
                continue;
            }
            if matches!(fact.kind, BehaviorFactKind::Comment) {
                continue;
            }
            if is_docstring_statement(&fact.text) {
                continue;
            }
            if let Some(body) = body_start {
                if fact.end_byte <= body {
                    continue;
                }
            }
            if let Some(sentence) = render_behavior(fact.kind, &fact.text) {
                mapped.push((fact.start_byte, fact.end_byte, fact.text.clone()));
                ordered.push((fact.start_byte, fact.start_byte, fact.end_byte, sentence));
            }
        }
    }

    ordered.sort_by_key(|(offset, _, _, _)| *offset);

    let mut seen = HashSet::new();
    let mut head = Vec::new();
    for (_, _, _, sentence) in &ordered {
        if seen.insert(sentence.clone()) {
            if head.len() >= MAX_PROPOSITION_SENTENCES {
                continue;
            }
            head.push(sentence.clone());
        }
    }

    // A fact is covered only when its sentence survived in the head, so a
    // duplicate of a dropped sentence still earns its raw tail.
    let head_set: HashSet<&String> = head.iter().collect();
    let mut tail: Vec<(usize, usize, String)> = Vec::new();
    for (start, end, text) in mapped {
        let sentence = ordered
            .iter()
            .find(|(_, s, e, _)| *s == start && *e == end)
            .map(|(_, _, _, sentence)| sentence);
        match sentence {
            Some(sentence) if head_set.contains(sentence) => {}
            _ => tail.push((start, end, text)),
        }
    }
    tail.sort_by_key(|(start, _, _)| *start);
    // Same range merge as the raw assembly: an outer fragment already
    // contains the inner ones, so inner ranges add no new raw text.
    let mut merged: Vec<(usize, usize, String)> = Vec::new();
    let mut last_end: Option<usize> = None;
    for (start, end, text) in tail {
        if let Some(prev_end) = last_end {
            if start <= prev_end {
                continue;
            }
        }
        last_end = Some(end);
        merged.push((start, end, text));
    }

    let overflow = !merged.is_empty();
    PropositionSet {
        head,
        tail: merged,
        overflow,
    }
}

/// Coverage of data-reference facts that no proposition sentence speaks for.
///
/// References never render a sentence: they usually repeat an enclosing
/// bind that already speaks. A reference whose byte range sits inside
/// another fact's range is covered by that facts sentence or raw tail;
/// one standing alone carries signal the current mapping silently drops.
/// The ratio guides whether a reference sentence shape is warranted.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct DataReferenceStats {
    /// Data-reference facts with content, across all entities.
    pub total: usize,
    /// References not contained in any other fact range of the same entity.
    pub independent: usize,
}

impl DataReferenceStats {
    /// Share of references standing alone, `0.0` when there are none.
    pub(crate) fn proportion(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.independent as f64 / self.total as f64
        }
    }
}

/// Count independent data-reference facts in one file result.
///
/// Only the entity's own facts are consulted, never cross-entity ranges.
/// Empty facts carry no signal and are excluded from both counters.
pub(crate) fn data_reference_stats(processing_result: &ProcessingResult) -> DataReferenceStats {
    let mut total = 0usize;
    let mut independent = 0usize;
    // Collect every entity id from the groups plus the behavior sidecar
    // so reference-only entities are still visited. Control-flow ids are
    // a subset of the group entities.
    let mut entity_ids: std::collections::HashSet<EntityId> = std::collections::HashSet::new();
    for group in &processing_result.groups {
        for id in group.all_entity_ids() {
            entity_ids.insert(id);
        }
    }
    for (id, _) in processing_result.behavior.iter() {
        entity_ids.insert(id);
    }
    for entity_id in entity_ids {
        let mut reference_ranges: Vec<(usize, usize)> = Vec::new();
        let mut cover_ranges: Vec<(usize, usize)> = Vec::new();
        if let Some(store) = processing_result.behavior.get(entity_id) {
            for fact in &store.facts {
                if fact.content_line_count == 0 {
                    continue;
                }
                if matches!(fact.kind, BehaviorFactKind::Comment) {
                    continue;
                }
                let range = (fact.start_byte, fact.end_byte);
                if matches!(fact.kind, BehaviorFactKind::DataReference) {
                    reference_ranges.push(range);
                } else {
                    cover_ranges.push(range);
                }
            }
        }
        if let Some(store) = processing_result.control_flow.get(entity_id) {
            for fact in &store.facts {
                if fact.content_line_count == 0 {
                    continue;
                }
                cover_ranges.push((fact.start_byte, fact.end_byte));
            }
        }
        for (start, end) in reference_ranges {
            total += 1;
            let covered = cover_ranges
                .iter()
                .any(|(s, e)| *s <= start && end <= *e && (*s, *e) != (start, end));
            if !covered {
                independent += 1;
            }
        }
    }
    DataReferenceStats { total, independent }
}

#[cfg(test)]
mod tests {
    use super::super::vocab::TEMPLATE_WORDS;
    use super::*;
    use cce_types::{
        BehaviorFact, BehaviorFactKind, BehaviorStore, ControlFlowFact, ControlFlowFactKind,
        ControlFlowStore,
    };

    fn stores(
        cf: Vec<(ControlFlowFactKind, &str)>,
        bf: Vec<(BehaviorFactKind, &str)>,
    ) -> ProcessingResult {
        let mut cf_store = ControlFlowStore::default();
        for (kind, text) in cf {
            let fact = ControlFlowFact::new(kind, text, 0, text.len());
            cf_store.entry_mut(EntityId(1)).push_fact(fact);
        }
        let mut bf_store = BehaviorStore::default();
        for (kind, text) in bf {
            let fact = BehaviorFact::new(kind, text, 0, text.len());
            bf_store.entry_mut(EntityId(1)).push_fact(fact);
        }
        ProcessingResult {
            groups: vec![],
            behavior: bf_store,
            control_flow: cf_store,
            stats: Default::default(),
        }
    }

    fn render(
        cf: Vec<(ControlFlowFactKind, &str)>,
        bf: Vec<(BehaviorFactKind, &str)>,
    ) -> PropositionSet {
        render_propositions(EntityId(1), &stores(cf, bf), "")
    }

    #[test]
    fn test_cap_and_dedup() {
        let mut bf = Vec::new();
        for i in 0..10 {
            bf.push((
                BehaviorFactKind::DataStatement,
                match i % 2 {
                    0 => "memcpy(dst, src, n);",
                    _ => "nbo_w32(ptr, word);",
                },
            ));
        }
        let set = render(vec![], bf);
        assert_eq!(set.head.len(), 2, "got: {:?}", set.head);
        assert!(set.head.len() <= MAX_PROPOSITION_SENTENCES);
        assert!(!set.overflow, "duplicates count as covered");
    }

    #[test]
    fn test_overflow_keeps_uncovered_tail() {
        let mut bf_store = BehaviorStore::default();
        let mut offset = 0usize;
        for i in 0..12 {
            let text = format!("emit_{i}(channel_{i}, payload_{i});");
            let fact = BehaviorFact::new(
                BehaviorFactKind::DataStatement,
                text.clone(),
                offset,
                offset + text.len(),
            );
            offset += text.len() + 1;
            bf_store.entry_mut(EntityId(1)).push_fact(fact);
        }
        let processing_result = ProcessingResult {
            groups: vec![],
            behavior: bf_store,
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };
        let set = render_propositions(EntityId(1), &processing_result, "");
        assert_eq!(set.head.len(), MAX_PROPOSITION_SENTENCES);
        assert!(set.overflow);
        assert_eq!(set.tail.len(), 12 - MAX_PROPOSITION_SENTENCES);
    }

    #[test]
    fn test_pre_body_facts_stay_silent() {
        use crate::grouper::types::{EntityGroup, GroupType};
        use cce_types::Span;

        let source = "func demo() { body = compute() }";
        let call_text = "body = compute()";
        let call_start = source.find(call_text).expect("call");
        let mut group = EntityGroup::new("g", GroupType::Standalone);
        group
            .entity_spans
            .insert(EntityId(1), Span::new(0, source.len(), 0, 0, 0, 0));
        let mut bf_store = BehaviorStore::default();
        // A fact ending inside the signature region must not speak.
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataBind,
            "func demo()",
            0,
            "func demo()".len(),
        ));
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataBind,
            call_text,
            call_start,
            call_start + call_text.len(),
        ));
        let processing_result = ProcessingResult {
            groups: vec![group],
            behavior: bf_store,
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };
        let set = render_propositions(EntityId(1), &processing_result, source);
        assert_eq!(set.head.len(), 1, "got: {:?}", set.head);
        assert!(set.head[0].contains("body derived from call compute()"));
    }

    /// Every non-template token in a proposition must occur verbatim in the
    /// entity's own fact texts. The truncated tail token may be a prefix of
    /// an input token, since truncation only ever cuts the sentence end.
    #[test]
    fn test_propositions_introduce_no_new_words() {
        let cf = vec![
            (
                ControlFlowFactKind::If,
                "if (hdrlen < RTP_FIXED_HEADER || hdrlen + pkt->payloadlen > bytes)\nreturn -1;",
            ),
            (
                ControlFlowFactKind::Match,
                "switch (type)\n{\ncase FLV_TYPE_AUDIO:\nreturn flv_demuxer_audio(flv, data, bytes, timestamp);\n}",
            ),
            (ControlFlowFactKind::Loop, "for (++p; ' ' == *p; p++);"),
            (
                ControlFlowFactKind::Return,
                "return bb.BindBody(body, obj);",
            ),
        ];
        let bf = vec![
            (
                BehaviorFactKind::DataBind,
                "body, err = io.ReadAll(c.Request.Body)",
            ),
            (
                BehaviorFactKind::DataStatement,
                "memcpy(session->session, field, n);",
            ),
            (
                BehaviorFactKind::DataQuery,
                "from item in playlist select item.Duration",
            ),
            (BehaviorFactKind::EffectError, "throw ConfigError;"),
            (BehaviorFactKind::DataObject, "config = { retries: 3 };"),
            (BehaviorFactKind::DataArray, "names = [a, b];"),
            (BehaviorFactKind::MacroBody, "MAX = 1024"),
        ];
        let result = stores(cf.clone(), bf.clone());
        let set = render_propositions(EntityId(1), &result, "");
        assert!(!set.head.is_empty());

        let mut inputs = String::new();
        for text in cf.iter().map(|(_, t)| *t).chain(bf.iter().map(|(_, t)| *t)) {
            inputs.push_str(text);
            inputs.push('\n');
        }
        let input_tokens: Vec<&str> = identifier_tokens(&inputs).collect();

        for sentence in &set.head {
            let tokens: Vec<&str> = identifier_tokens(sentence).collect();
            for (i, token) in tokens.iter().enumerate() {
                if TEMPLATE_WORDS.contains(&token.to_ascii_lowercase().as_str()) {
                    continue;
                }
                let last = i + 1 == tokens.len();
                let verbatim = inputs.contains(*token);
                let truncated_tail = last && input_tokens.iter().any(|t| t.starts_with(*token));
                assert!(
                    verbatim || truncated_tail,
                    "new word {token:?} in {sentence:?}"
                );
            }
        }
    }

    /// Identifier-ish tokens of a text, used by the no-new-words audit.
    fn identifier_tokens(text: &str) -> impl Iterator<Item = &str> {
        text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|t| !t.is_empty())
    }

    #[test]
    fn test_reference_stats_counts_independent() {
        let mut bf_store = BehaviorStore::default();
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataBind,
            "body = compute(x)",
            0,
            17,
        ));
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataReference,
            "x",
            15,
            16,
        ));
        bf_store.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataReference,
            "lonely",
            100,
            106,
        ));
        let processing_result = ProcessingResult {
            groups: vec![],
            behavior: bf_store,
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };
        let stats = data_reference_stats(&processing_result);
        assert_eq!(stats.total, 2);
        assert_eq!(stats.independent, 1);
        assert!((stats.proportion() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_reference_stats_empty_is_zero() {
        let processing_result = ProcessingResult {
            groups: vec![],
            behavior: BehaviorStore::default(),
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };
        let stats = data_reference_stats(&processing_result);
        assert_eq!(stats.total, 0);
        assert_eq!(stats.proportion(), 0.0);
    }
}
