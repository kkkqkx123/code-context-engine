//! Closed vocabulary of the proposition renderer.
//!
//! Every substantive word in a proposition is copied verbatim from the
//! entity's own fact text. Only the words listed here may be introduced by
//! the sentence templates, so a reviewer can trace each word back to either
//! this table or the source. The audit test next to the assembly asserts
//! the closed-world property over all sentence shapes.

/// Maximum proposition sentences kept per entity.
///
/// Matches the call-target cap and the branch-anchor budget: enough to hold
/// the distinctive facts, few enough that boilerplate cannot dominate.
pub(in super::super) const MAX_PROPOSITION_SENTENCES: usize = 8;

/// Maximum characters kept for the code-derived object of one proposition.
///
/// A fact can span a whole branch body; the proposition keeps the head so
/// the sentence stays a statement of intent instead of a code dump.
pub(super) const MAX_OBJECT_CHARS: usize = 160;

/// Words the sentence templates may introduce.
///
/// The no-new-words audit requires every other token in a proposition to
/// occur verbatim in the entity's own fact texts. Adding a template shape
/// with a new word requires adding the word here first.
#[cfg(test)]
pub(super) const TEMPLATE_WORDS: &[&str] = &[
    "checks",
    "dispatches",
    "on",
    "derived",
    "from",
    "call",
    "returns",
    "iterates",
    "over",
    "queries",
    "invokes",
    "builds",
    "object",
    "array",
    "defines",
    "macro",
    "reports",
    "error",
    "via",
];

/// Words marking an effect fact as an error-handling shape.
///
/// Blocks that carry none of these words (such as an unsafe wrapper, which
/// the extractor files under effects for lack of a better home) must not
/// be labeled as error reports. Resource-management lines are deliberately
/// absent: a `with` block names no error, so it stays silent and lets its
/// inner bindings speak.
pub(super) const ERROR_MARKERS: &[&str] = &[
    "throw", "try", "catch", "except", "raise", "error", "Error", "assert", "panic",
];

/// Null words across languages, matched as whole expressions only.
///
/// A small closed set: every language spells null with one of these.
/// Kept as data so extending coverage means adding a table row.
pub(super) const NULL_WORDS: &[&str] = &["NULL", "nullptr", "nil", "Nil", "None", "null"];

/// Template guard conditions that repeat on nearly every function.
///
/// Rendering them would add a shared sentence to every entity instead of
/// signal. Each entry names a whole condition, never a substring.
pub(super) const BOILERPLATE_GUARDS: &[&str] = &["err != nil", "err == nil"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_words_cover_every_verb_shape() {
        for verb in [
            "checks",
            "dispatches",
            "derived",
            "returns",
            "iterates",
            "queries",
            "invokes",
            "builds",
            "defines",
            "reports",
        ] {
            assert!(
                TEMPLATE_WORDS.contains(&verb),
                "verb shape {verb:?} has no vocabulary entry"
            );
        }
    }
}
