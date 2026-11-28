//! Post-processing: Rust attribute extraction from source text
//!
//! Scans source text preceding an entity for Rust attributes like #[inline],
//! #[must_use], #[cfg(...)], #[derive(...)], etc.
//!
//! Simple attributes (no parameters) are added to entity.modifiers for uniform handling.
//! Parameterized attributes are stored in metadata["annotations"].
//!
//! # Scope Boundaries
//!
//! To prevent annotations from parent scopes (e.g. a struct's #[derive] appearing on
//! its fields), the extractor checks for structural block boundaries (`{`) between
//! a matched annotation and the entity. If a `{` exists in between, the annotation
//! belongs to a parent scope and is skipped.

use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::Entity;
use cce_types::entity::meta_keys;

use super::super::capture;

/// Extract Rust-specific attributes from source context
///
/// Handles:
/// - Simple attributes (→ modifiers): #[inline], #[cold], #[must_use], etc.
/// - Parameterized attributes (→ annotations): #[cfg(...)], #[derive(...)], #[allow(...)], etc.
pub fn extract_rust_attributes(mat: &QueryMatch, source: &str, entity: &mut Entity) {
    let main_capture = match capture::parser::find_main_capture(mat) {
        Some(c) => c,
        None => return,
    };

    let lookback =
        cce_utils::floor_char_boundary(source, main_capture.start_byte.saturating_sub(300));
    let preceding_text = &source[lookback..main_capture.start_byte];

    let simple_attrs: &[&str] = &[
        "#[inline]",
        "#[cold]",
        "#[must_use]",
        "#[no_mangle]",
        "#[non_exhaustive]",
        "#[deprecated]",
        "#[doc(hidden)]",
        "#[automatically_derived]",
    ];

    for attr in simple_attrs {
        if let Some(rel_pos) = preceding_text.rfind(attr) {
            // Check that no structural scope boundary exists between the
            // attribute and the entity — annotation belongs to parent scope.
            let after_attr = rel_pos + attr.len();
            if after_attr < preceding_text.len() && preceding_text[after_attr..].contains('{') {
                continue;
            }
            let attr_name = attr.trim_start_matches("#[").trim_end_matches(']');
            entity.modifiers.push(attr_name.to_string());
            entity
                .attributes
                .insert(attr_name.to_string(), String::new());
        }
    }

    let param_attr_starts: &[&str] = &[
        "#[cfg(",
        "#[cfg_attr(",
        "#[derive(",
        "#[allow(",
        "#[deny(",
        "#[warn(",
        "#[forbid(",
        "#[repr(",
        "#[doc(cfg(",
    ];

    let mut cfg_predicates: Vec<String> = Vec::new();

    for attr_start in param_attr_starts {
        let is_cfg = *attr_start == "#[cfg(" || *attr_start == "#[cfg_attr(";
        let mut search_pos = 0;
        while let Some(start) = preceding_text[search_pos..].find(attr_start) {
            let abs_start = search_pos + start;
            let inner_start = abs_start + attr_start.len();
            let mut depth = 1u32;
            let mut bracket_end: Option<usize> = None;

            for (i, c) in preceding_text[inner_start..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            let after_paren = inner_start + i + 1;
                            if let Some(close_bracket) = preceding_text[after_paren..].find(']') {
                                bracket_end = Some(after_paren + close_bracket);
                            }
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if let Some(end_pos) = bracket_end {
                // Check whether a structural scope boundary (`{`) exists between
                // this annotation's closing `]` and the entity. If so, the annotation
                // belongs to a parent scope (e.g. a struct's #[derive], not the field's).
                let after_bracket = end_pos + 1;
                if after_bracket < preceding_text.len()
                    && preceding_text[after_bracket..].contains('{')
                {
                    search_pos = abs_start + 1;
                    continue;
                }

                let full_attr = &preceding_text[abs_start..=end_pos];
                let attr_content = full_attr.trim_start_matches("#[").trim_end_matches(']');
                if is_cfg {
                    cfg_predicates.push(attr_content.to_string());
                }
                if let Some(existing) = entity.metadata.get(meta_keys::ANNOTATIONS) {
                    let combined = format!("{}, {}", existing, attr_content);
                    entity.set_metadata(meta_keys::ANNOTATIONS.to_string(), combined);
                } else {
                    entity
                        .set_metadata(meta_keys::ANNOTATIONS.to_string(), attr_content.to_string());
                }
            }

            search_pos = abs_start + 1;
        }
    }

    if !cfg_predicates.is_empty() {
        // Sort so the predicate string is independent of source attribute order
        // and capture iteration order, keeping the derived symbol identity stable.
        cfg_predicates.sort();
        cfg_predicates.dedup();
        entity.set_metadata(
            meta_keys::CFG_PREDICATE.to_string(),
            cfg_predicates.join(" ; "),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_sitter_query::executor::{Capture, QueryMatch};
    use cce_types::entity::{EntityId, EntityKind};
    use std::collections::HashMap;

    fn empty_entity() -> Entity {
        Entity {
            id: EntityId(1),
            kind: EntityKind::Function,
            name: "target".to_string(),
            signature: "fn target()".to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: Default::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        }
    }

    fn function_match(start_byte: usize) -> QueryMatch {
        QueryMatch {
            index: 0,
            pattern_index: 0,
            captures: vec![Capture {
                name: "entity.function".to_string(),
                text: "fn target()".to_string(),
                start_byte,
                end_byte: start_byte + "fn target()".len(),
                start_point: (0, 0),
                end_point: (0, 0),
            }],
        }
    }

    /// The lookback window is a raw byte offset, so a multibyte character at
    /// its edge used to panic while slicing. The filler length is swept so the
    /// window start lands inside the three bytes of the multi-byte character
    /// as well as on either side of it.
    #[test]
    fn lookback_window_never_splits_a_multibyte_character() {
        for filler in 284..=291 {
            let mut source = "x".repeat(10);
            source.push('—');
            source.push_str(&"y".repeat(filler));
            // No body brace: a `{` between the attribute and the entity means
            // the attribute belongs to a parent scope.
            source.push_str("\n#[inline]\nfn target");
            let start = source.find("fn target").expect("anchor must exist");
            let mat = function_match(start);
            let mut entity = empty_entity();

            extract_rust_attributes(&mat, &source, &mut entity);

            assert!(
                entity.modifiers.iter().any(|m| m == "inline"),
                "attribute must survive a multibyte window edge (filler={filler})"
            );
        }
    }
}
