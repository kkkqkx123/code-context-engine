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

    let lookback = main_capture.start_byte.saturating_sub(300);
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

    for attr_start in param_attr_starts {
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
}
