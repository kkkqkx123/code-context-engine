//! Topic anchor for index-only enrichment.
//!
//! A function that dispatches on protocol constants states its whole
//! subject in those constants, but the body buries them under a
//! `switch`/`case` skeleton. The anchor lifts the discriminants to the head
//! of the text so a retriever meets them before the syntax does.
//!
//! The anchor is purely lexical: it restates identifiers that the entity's
//! own source already contains, so it cannot invent a subject the code does
//! not have. An entity without branches gets no anchor.

use crate::grouper::ProcessingResult;
use cce_types::{ControlFlowFactKind, EntityId};

/// Maximum number of branch discriminants listed in a topic anchor.
const MAX_ANCHOR_DISCRIMINANTS: usize = 8;

/// Topic anchor of an entity: the values its branches dispatch on.
pub(super) fn topic_anchor(
    entity_id: EntityId,
    processing_result: &ProcessingResult,
) -> Option<String> {
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
pub(super) fn insert_after_first_line(text: &str, line: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_discriminants_handles_multibyte() {
        let text = "switch (x) { case 世界: break; case default: break; }";
        let labels = branch_discriminants(text);
        assert!(labels.contains(&"世界".to_string()) || labels.is_empty());
        // Should not panic on multibyte input.
        let _ = branch_discriminants("switch (x) { case ¡café: break; }");
    }
}
