//! Kind-to-sentence rendering decisions.
//!
//! Only a decision table lives here: each fact kind maps to at most one
//! sentence shape. String surgery belongs to the text module, worthiness
//! checks belong to the guard module. Adding a sentence shape means adding
//! one match arm here plus its vocabulary entry.

use cce_types::{BehaviorFactKind, ControlFlowFactKind};

use super::code_text::{
    collapse_whitespace, discriminant_before_brace, first_meaningful_line,
    leading_keyword_stripped, looks_like_call, split_assignment, starts_with_control_keyword,
    strip_binding_prefix, truncate_object,
};
use super::guards::{
    is_bare_error_return, is_boilerplate_guard, is_vacuous_condition, is_vacuous_rhs,
    mentions_error,
};

/// Render one control-flow fact as a proposition, if it carries intent.
pub(super) fn render_control_flow(kind: ControlFlowFactKind, text: &str) -> Option<String> {
    match kind {
        ControlFlowFactKind::If => {
            let cond = discriminant_before_brace(text)?;
            if is_boilerplate_guard(&cond) {
                return None;
            }
            if is_vacuous_condition(&cond) {
                return None;
            }
            Some(format!("checks {}.", cond))
        }
        ControlFlowFactKind::Match => {
            let disc = discriminant_before_brace(text)?;
            Some(format!("dispatches on {}.", disc))
        }
        ControlFlowFactKind::Loop => {
            let header = discriminant_before_brace(text)?;
            Some(format!("iterates over {}.", header))
        }
        ControlFlowFactKind::Return => {
            let expr = leading_keyword_stripped(text, &["return"])?;
            let expr = expr.trim().trim_end_matches(';').trim();
            if expr.is_empty() {
                return None;
            }
            if is_bare_error_return(expr) {
                return None;
            }
            Some(format!("returns {}.", truncate_object(expr)))
        }
        ControlFlowFactKind::Break
        | ControlFlowFactKind::Continue
        | ControlFlowFactKind::Yield
        | ControlFlowFactKind::Try => None,
    }
}

/// Render one behavior fact as a proposition, if it carries intent.
///
/// References and shift operators stay silent: references usually repeat an
/// enclosing bind that already speaks, and shifts carry no intent of their
/// own. Dropping them is the allowed downweighting.
pub(super) fn render_behavior(kind: BehaviorFactKind, text: &str) -> Option<String> {
    match kind {
        BehaviorFactKind::DataBind => render_bind(text),
        BehaviorFactKind::DataStatement => {
            if starts_with_control_keyword(text) {
                return None;
            }
            let line = first_meaningful_line(text)?;
            // Assignments belong to the bind renderer: emitting both would
            // restate the same identifiers in two sentences of one entity.
            if split_assignment(&collapse_whitespace(&line)).is_some() {
                return None;
            }
            if !looks_like_call(&line) {
                return None;
            }
            Some(format!("invokes {}.", truncate_object(&line)))
        }
        BehaviorFactKind::DataQuery => {
            let line = first_meaningful_line(text)?;
            Some(format!("queries {}.", truncate_object(&line)))
        }
        BehaviorFactKind::EffectError => {
            let line = first_meaningful_line(text)?;
            if !mentions_error(&line) {
                return None;
            }
            Some(format!("reports error via {}.", truncate_object(&line)))
        }
        BehaviorFactKind::DataObject => {
            let line = first_meaningful_line(text)?;
            Some(format!("builds object {}.", truncate_object(&line)))
        }
        BehaviorFactKind::DataArray => {
            let line = first_meaningful_line(text)?;
            Some(format!("builds array {}.", truncate_object(&line)))
        }
        BehaviorFactKind::MacroBody => {
            let line = first_meaningful_line(text)?;
            Some(format!("defines macro {}.", truncate_object(&line)))
        }
        _ => None,
    }
}

/// Render an assignment as a derivation proposition.
///
/// A call on the right-hand side names the producer; a plain right-hand
/// side names the source identifier or literal. Fragments that open with a
/// control keyword are not assignments at all: the extractor expands a
/// match to its enclosing statement, so the kind label lies and the
/// control-flow side already speaks for the condition.
pub(super) fn render_bind(text: &str) -> Option<String> {
    if starts_with_control_keyword(text) {
        return None;
    }
    let flat = collapse_whitespace(text);
    let (lhs, rhs) = split_assignment(&flat)?;
    let lhs = strip_binding_prefix(lhs.trim());
    let rhs = rhs.trim().trim_end_matches(';').trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    if looks_like_call(rhs) {
        return Some(format!(
            "{} derived from call {}.",
            truncate_object(lhs),
            truncate_object(rhs)
        ));
    }
    if is_vacuous_rhs(rhs) {
        return None;
    }
    Some(format!(
        "{} derived from {}.",
        truncate_object(lhs),
        truncate_object(rhs)
    ))
}

#[cfg(test)]
mod tests {
    use super::super::assembly::render_propositions;
    use crate::grouper::ProcessingResult;
    use cce_types::EntityId;
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
    ) -> super::super::assembly::PropositionSet {
        render_propositions(EntityId(1), &stores(cf, bf), "")
    }

    #[test]
    fn test_bind_call_rhs_renders_derivation() {
        let set = render(
            vec![],
            vec![(
                BehaviorFactKind::DataBind,
                "body, err = io.ReadAll(c.Request.Body)",
            )],
        );
        assert_eq!(set.head.len(), 1);
        assert!(
            set.head[0].contains("derived from call"),
            "got: {}",
            set.head[0]
        );
        assert!(set.head[0].contains("io.ReadAll(c.Request.Body)"));
        assert!(!set.overflow);
    }

    #[test]
    fn test_bind_plain_rhs_renders_source() {
        let set = render(vec![], vec![(BehaviorFactKind::DataBind, "body = cbb")]);
        assert_eq!(set.head.len(), 1);
        assert!(set.head[0].contains("derived from cbb"));
        assert!(!set.head[0].contains("call"));
    }

    #[test]
    fn test_vacuous_rhs_stays_silent_but_values_speak() {
        let silent = render(
            vec![],
            vec![
                (BehaviorFactKind::DataBind, "n = 0"),
                (BehaviorFactKind::DataBind, "mpd = NULL"),
            ],
        );
        assert!(silent.head.is_empty(), "got: {:?}", silent.head);
        let speaking = render(
            vec![],
            vec![(BehaviorFactKind::DataBind, "session->timeout = 60000")],
        );
        assert_eq!(speaking.head.len(), 1, "got: {:?}", speaking.head);
        assert!(speaking.head[0].contains("derived from 60000"));
    }

    #[test]
    fn test_if_renders_condition_and_skips_err_guard() {
        let set = render(
            vec![
                (ControlFlowFactKind::If, "if (hdrlen < 10) {\nreturn -1;\n}"),
                (ControlFlowFactKind::If, "if err != nil {\nreturn err\n}"),
            ],
            vec![],
        );
        assert_eq!(set.head.len(), 1, "got: {:?}", set.head);
        assert!(set.head[0].contains("checks hdrlen < 10"));
    }

    #[test]
    fn test_vacuous_conditions_stay_silent_but_comparisons_speak() {
        let silent = render(
            vec![
                (ControlFlowFactKind::If, "if (p) {\nreturn -1;\n}"),
                (ControlFlowFactKind::If, "if (!ptr1) {\nreturn -1;\n}"),
            ],
            vec![],
        );
        assert!(silent.head.is_empty(), "got: {:?}", silent.head);
        let speaking = render(
            vec![(ControlFlowFactKind::If, "if (hdrlen < 10) {\nreturn -1;\n}")],
            vec![],
        );
        assert_eq!(speaking.head.len(), 1, "got: {:?}", speaking.head);
    }

    #[test]
    fn test_bare_error_returns_stay_silent_but_values_speak() {
        let silent = render(
            vec![
                (ControlFlowFactKind::Return, "return -ENOMEM;"),
                (ControlFlowFactKind::Return, "return NULL;"),
            ],
            vec![],
        );
        assert!(silent.head.is_empty(), "got: {:?}", silent.head);
        let speaking = render(
            vec![(
                ControlFlowFactKind::Return,
                "return hdrlen + pkt->payloadlen;",
            )],
            vec![],
        );
        assert_eq!(speaking.head.len(), 1, "got: {:?}", speaking.head);
    }

    #[test]
    fn test_braceless_guard_keeps_condition() {
        let set = render(
            vec![(ControlFlowFactKind::If, "if (bytes < 1)\nreturn 0;")],
            vec![],
        );
        assert_eq!(set.head.len(), 1, "got: {:?}", set.head);
        assert!(set.head[0].contains("checks bytes < 1"));
    }

    #[test]
    fn test_braceless_call_body_leaves_no_malformed_sentence() {
        let set = render(
            vec![(
                ControlFlowFactKind::If,
                "if (period->event_streams) free(period->event_streams);",
            )],
            vec![],
        );
        assert!(
            set.head.iter().all(|s| !s.contains("free(period")),
            "got: {:?}",
            set.head
        );
    }

    #[test]
    fn test_match_renders_discriminant() {
        let set = render(
            vec![(
                ControlFlowFactKind::Match,
                "switch (type)\n{\ncase A:\nreturn f();\n}",
            )],
            vec![],
        );
        assert_eq!(set.head.len(), 1, "got: {:?}", set.head);
        assert!(set.head[0].contains("dispatches on type"));
    }

    #[test]
    fn test_loop_and_return_render() {
        let set = render(
            vec![
                (
                    ControlFlowFactKind::Loop,
                    "for (i = 0; i < n; i++) {\nemit(i);\n}",
                ),
                (
                    ControlFlowFactKind::Return,
                    "return hdrlen + pkt->payloadlen;",
                ),
                (ControlFlowFactKind::Return, "return;"),
            ],
            vec![],
        );
        assert_eq!(set.head.len(), 2, "got: {:?}", set.head);
        assert!(set.head.iter().any(|s| s.contains("iterates over")));
        assert!(
            set.head
                .iter()
                .any(|s| s.contains("returns hdrlen + pkt->payloadlen"))
        );
    }

    #[test]
    fn test_statement_call_renders_invocation() {
        let set = render(
            vec![],
            vec![(
                BehaviorFactKind::DataStatement,
                "memcpy(session->session, field, n);",
            )],
        );
        assert_eq!(set.head.len(), 1, "got: {:?}", set.head);
        assert!(set.head[0].starts_with("invokes "));
    }

    #[test]
    fn test_error_effect_requires_error_shape() {
        let reporting = render(
            vec![],
            vec![(
                BehaviorFactKind::EffectError,
                "if(0 == strncmp(\"timeout=\", p, 8))\nsession->timeout = 1;",
            )],
        );
        assert!(
            reporting.head.is_empty(),
            "no error word means no error sentence"
        );

        let throwing = render(
            vec![],
            vec![(BehaviorFactKind::EffectError, "throw ConfigError;")],
        );
        assert_eq!(throwing.head.len(), 1);
        assert!(throwing.head[0].contains("reports error via"));
    }

    #[test]
    fn test_with_blocks_stay_silent() {
        let set = render(
            vec![],
            vec![(
                BehaviorFactKind::EffectError,
                "with open(path) as f:\n data = f.read()",
            )],
        );
        assert!(set.head.is_empty(), "got: {:?}", set.head);
    }

    #[test]
    fn test_noise_kinds_stay_silent() {
        let set = render(
            vec![
                (ControlFlowFactKind::Break, "break;"),
                (ControlFlowFactKind::Continue, "continue;"),
            ],
            vec![
                (BehaviorFactKind::OpShiftLeft, "value <<= 1;"),
                (BehaviorFactKind::DataReference, "session->session"),
            ],
        );
        assert!(set.head.is_empty());
        assert!(!set.overflow);
    }

    #[test]
    fn test_expanded_control_bind_stays_silent() {
        let set = render(
            vec![],
            vec![(
                BehaviorFactKind::DataBind,
                "if cb, ok := c.Get(k); ok { body = cbb }",
            )],
        );
        assert!(
            set.head.is_empty(),
            "control-expanded binds belong to the condition renderer"
        );
    }

    #[test]
    fn test_statement_assignment_left_to_bind() {
        let set = render(
            vec![],
            vec![(
                BehaviorFactKind::DataStatement,
                "hdrlen = rtp_packet_serialize_header(pkt, data, bytes);",
            )],
        );
        assert!(
            set.head.is_empty(),
            "assignments must not double-speak as invocations"
        );
    }

    #[test]
    fn test_binding_prefix_stripped() {
        let set = render(
            vec![],
            vec![(
                BehaviorFactKind::DataBind,
                "let mut decode_buffer = self.decode_buffer.borrow_mut()",
            )],
        );
        assert_eq!(set.head.len(), 1, "got: {:?}", set.head);
        assert!(
            set.head[0].starts_with("decode_buffer derived from"),
            "got: {}",
            set.head[0]
        );
    }
}
