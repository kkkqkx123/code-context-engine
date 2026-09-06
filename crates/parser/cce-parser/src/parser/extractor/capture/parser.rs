//! Capture parser: extract raw entity data from tree-sitter query matches
//!
//! Provides pure functions that extract typed data from tree-sitter captures
//! without modifying any entity state. All functions are deterministic and
//! depend only on the capture/match data.

use std::collections::{BTreeMap, HashMap};

use crate::parser::extractor::utils;
use crate::tree_sitter_query::capture;
use crate::tree_sitter_query::executor::{Capture, QueryMatch};

/// Find the main entity capture (e.g., @entity.type.class, @entity.function.definition)
///
/// When multiple candidates exist (e.g., both @entity.function and @entity.function.generator),
/// selects the one with the largest valid span (widest range).
///
/// Phantom nodes from tree-sitter error recovery are filtered out:
/// - end_byte < start_byte (negative byte range → usize underflow)
/// - end_point.row < start_point.row (reversed line positions)
pub fn find_main_capture(mat: &QueryMatch) -> Option<&Capture> {
    let candidates: Vec<&Capture> = mat
        .captures
        .iter()
        .filter(|c| {
            capture::is_main_entity_capture(&c.name)
                && c.start_byte <= c.end_byte
                && c.start_point.0 <= c.end_point.0
        })
        .collect();

    match candidates.len() {
        0 => None,
        1 => Some(candidates[0]),
        _ => candidates
            .into_iter()
            .max_by_key(|c| c.end_byte - c.start_byte),
    }
}

/// Find the name capture (e.g., @entity.type.class.name)
pub fn find_name_capture(mat: &QueryMatch) -> Option<&Capture> {
    utils::find_capture_by_name(&mat.captures, capture::is_name_capture)
}

/// Extract subtype from capture name (e.g., "generator" from "entity.function.generator")
pub fn extract_subtype_from_capture(capture_name: &str) -> Option<String> {
    let parts: Vec<&str> = capture_name.split('.').collect();
    if parts.len() >= 3 {
        Some(parts[2].to_string())
    } else {
        None
    }
}

/// Reconstruct signature text from sub-captures of a signature match.
///
/// For a signature match like:
/// ```text
/// (struct_item
///   name: (type_identifier) @entity.struct.signature.name
///   type_parameters: (_)? @entity.struct.signature.type_params
/// ) @entity.struct.signature
/// ```
///
/// This extracts and concatenates the sub-capture texts (name, type_params, etc.)
/// in source order, producing a clean signature without body/fields/comments.
pub fn reconstruct_signature_from_subcaptures(mat: &QueryMatch, source: &str) -> String {
    // Collect sub-captures (those with ".signature." in their name, excluding the main
    // signature capture which has ".signature" at the end without trailing dot).
    let mut sub_captures: Vec<&Capture> = mat
        .captures
        .iter()
        .filter(|c| c.name.contains(".signature."))
        .collect();

    if sub_captures.is_empty() {
        return String::new();
    }

    // Sort by start_byte to preserve source order
    sub_captures.sort_by_key(|c| c.start_byte);

    // Extract text from each sub-capture and join with space
    sub_captures
        .iter()
        .map(|c| utils::extract_text_from_source(source, c.start_byte, c.end_byte))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract full entity signature text from source
///
/// Priority:
/// 1. Reconstruct from signature sub-captures (e.g., @entity.struct.signature.type_params)
/// 2. Fall back to main capture and extract signature part
pub fn extract_signature(mat: &QueryMatch, source: &str) -> String {
    // Priority 1: Reconstruct from signature sub-captures if available
    let sig = reconstruct_signature_from_subcaptures(mat, source);
    if !sig.is_empty() {
        return sig;
    }

    // Priority 2: Fall back to main capture and extract signature part
    if let Some(main) = find_main_capture(mat) {
        let full_text = utils::extract_text_from_source(source, main.start_byte, main.end_byte);
        return extract_signature_from_text(&full_text);
    }

    String::new()
}

/// Extract signature part from full text (e.g., remove body, fields, comments)
fn extract_signature_from_text(text: &str) -> String {
    // Find the first '{' position
    if let Some(brace_pos) = text.find('{') {
        let signature = text[..brace_pos].trim();
        // Remove comments and whitespace
        signature
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        text.to_string()
    }
}

/// Extract parameters from match, returning (name, optional_type) pairs
pub fn extract_parameters(
    mat: &QueryMatch,
    language: &cce_types::language::Language,
) -> Vec<(String, Option<String>)> {
    let mut param_captures: BTreeMap<usize, (Option<String>, Option<String>)> = BTreeMap::new();

    for capture in mat.captures.iter() {
        if !utils::capture_name_contains(&capture.name, capture::SUBSTRING_PARAMETER)
            && !utils::capture_name_contains(&capture.name, capture::SUBSTRING_PARAM)
        {
            continue;
        }

        if utils::capture_name_contains(&capture.name, capture::SUBSTRING_SELF_PARAM) {
            continue;
        }

        if let Some(suffix) = capture.name.split('.').next_back() {
            match suffix {
                "params" => {
                    for (idx, (name, typ)) in parse_parameters_text(&capture.text, language)
                        .into_iter()
                        .enumerate()
                    {
                        param_captures
                            .entry(idx)
                            .or_insert_with(|| (Some(name), typ));
                    }
                }
                "name" => {
                    param_captures
                        .entry(capture.start_byte)
                        .or_insert((None, None))
                        .0 = Some(capture.text.clone());
                }
                "type" => {
                    param_captures
                        .entry(capture.start_byte)
                        .or_insert((None, None))
                        .1 = Some(capture.text.clone());
                }
                _ => {}
            }
        }
    }

    param_captures
        .into_values()
        .filter_map(|(name, type_)| name.map(|n| (n, type_)))
        .collect()
}

/// Parse parameter text (e.g., "(self, x: int, y: str = 'foo')") into individual (name, type) pairs
fn strip_inline_comments(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut depth: i32 = 0;
    let mut skip_line = false;
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                if !skip_line {
                    result.push(ch);
                }
            }
            ')' | ']' | '}' => {
                // Clamp at zero: an unmatched closer (e.g. inside a
                // default expression) must not suppress `#` handling
                // for the rest of the text. (`saturating_sub` alone is
                // not enough: it clamps at `i32::MIN`, not at zero.)
                depth = (depth - 1).max(0);
                if !skip_line {
                    result.push(ch);
                }
            }
            '#' if depth == 0 => {
                skip_line = true;
            }
            '\n' => {
                skip_line = false;
                result.push(ch);
            }
            _ => {
                if !skip_line {
                    result.push(ch);
                }
            }
        }
    }
    result
}

fn parse_parameters_text(
    text: &str,
    language: &cce_types::language::Language,
) -> Vec<(String, Option<String>)> {
    let text = text.trim();
    // Rust closure parameters are pipe-delimited (`|x: i32|`): unwrap them so
    // the `|` characters are not misread as part of a parameter name.
    let text = if let Some(stripped) = text.strip_prefix('|') {
        match stripped.find('|') {
            Some(rel) => &stripped[..rel],
            None => text,
        }
    } else {
        text
    };
    let text = text.trim();
    let inner = if text.starts_with('(') && text.ends_with(')') {
        &text[1..text.len() - 1]
    } else {
        text
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return Vec::new();
    }
    let inner = strip_inline_comments(inner);
    let mut params = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            // Clamp at zero: an unpaired `>` (Rust `->`, `=>`, `>=`,
            // shifts in default expressions) must not drive the depth
            // negative and swallow the following top-level commas.
            // (`saturating_sub` clamps at `i32::MIN`, not at zero, so an
            // explicit `max(0)` is required.)
            ')' | ']' | '}' | '>' => depth = (depth - 1).max(0),
            ',' if depth == 0 => {
                let p = inner[start..i].trim();
                if !p.is_empty() && p != "*" && p != "**" {
                    params.push(parse_single_param(p, language));
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let remaining = inner[start..].trim();
    if !remaining.is_empty() && remaining != "*" && remaining != "**" {
        params.push(parse_single_param(remaining, language));
    }
    params
}

/// Parse a Rust method receiver into (`self`, type).
///
/// Accepts `self`, `mut self`, `&self`, `&mut self` (with optional
/// lifetime `&'a mut self`) and explicitly typed `self: Type` /
/// `mut self: Type`. Returns `None` for ordinary parameters so the
/// generic splitter handles them.
fn parse_rust_receiver(text: &str) -> Option<(String, Option<String>)> {
    // An explicit type (`self: Box<Self>`) wins; a colon here always
    // separates name from type (receivers contain no `::` paths).
    let (head, explicit) = match text.split_once(':') {
        Some((head, ty)) => (head.trim(), Some(ty.trim())),
        None => (text.trim(), None),
    };
    let mut rest = head;
    let mut is_ref = false;
    if let Some(stripped) = rest.strip_prefix('&') {
        is_ref = true;
        rest = stripped.trim_start();
        // Skip an optional lifetime (`&'a self`, `&'a mut self`).
        if let Some(lifetime) = rest.strip_prefix('\'') {
            let end = lifetime
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(lifetime.len());
            rest = lifetime[end..].trim_start();
        }
    }
    let mut is_mut = false;
    if let Some(after) = rest.strip_prefix("mut")
        && (after.is_empty() || after.starts_with(char::is_whitespace))
    {
        is_mut = true;
        rest = after.trim_start();
    }
    if rest != "self" {
        return None;
    }
    let ty = match (explicit, is_ref, is_mut) {
        (Some(ty), _, _) if !ty.is_empty() => ty.to_string(),
        (Some(_), _, _) => return None,
        (None, false, _) => "Self".to_string(),
        (None, true, false) => "&Self".to_string(),
        (None, true, true) => "&mut Self".to_string(),
    };
    Some(("self".to_string(), Some(ty)))
}

/// Parse a single parameter string like "x: int = 5" or "self" or "*args"
fn parse_single_param(
    text: &str,
    language: &cce_types::language::Language,
) -> (String, Option<String>) {
    let text = text.trim();
    if text.is_empty() {
        return (String::new(), None);
    }
    // Rust method receivers (`&mut self`, `&'a self`, `mut self: Type`)
    // carry the reference on the name side; without special handling the
    // generic splitter reports (`self`, `&mut`), which the inferer then
    // wraps in another reference (`&mut &mut`).
    if *language == cce_types::language::Language::Rust
        && let Some(receiver) = parse_rust_receiver(text)
    {
        return receiver;
    }
    let bytes = text.as_bytes();
    let mut depth: i32 = 0;
    let mut colon_pos = None;
    let mut eq_pos = None;
    for (i, ch) in text.char_indices() {
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            // Clamp at zero for the same reason as the parameter
            // splitter: `->` and friends carry an unpaired `>`.
            // (`saturating_sub` clamps at `i32::MIN`, not at zero.)
            ')' | ']' | '}' | '>' => depth = (depth - 1).max(0),
            ':' if depth == 0 && colon_pos.is_none() => {
                // A `::` path separator (C++ `std::vector`) is not a
                // name/type separator. Skip either colon of a `::` pair.
                let prev_is_colon = i > 0 && bytes[i - 1] == b':';
                let next_is_colon = bytes.get(i + 1).is_some_and(|b| *b == b':');
                if !prev_is_colon && !next_is_colon {
                    colon_pos = Some(i);
                }
            }
            '=' if depth == 0 && eq_pos.is_none() => {
                // Skip `==`, `=>`, `>=`, `<=`, `!=` in default expressions.
                let next = bytes.get(i + 1).copied().unwrap_or(0);
                let prev = if i > 0 { bytes[i - 1] } else { 0 };
                if next != b'='
                    && next != b'>'
                    && prev != b'='
                    && prev != b'!'
                    && prev != b'<'
                    && prev != b'>'
                {
                    eq_pos = Some(i);
                }
            }
            _ => {}
        }
    }
    match colon_pos {
        Some(cpos) => {
            let name = text[..cpos].trim().to_string();
            let type_end = eq_pos.unwrap_or(text.len());
            let typ = text[cpos + 1..type_end].trim().to_string();
            (name, Some(typ))
        }
        None => {
            let before_eq = match eq_pos {
                Some(epos) => text[..epos].trim(),
                None => text,
            };
            // Go declares `name type` (`name string`, `age int`), the reverse
            // of C-style `Type name`. The grammar guarantees name-first, so
            // the first token is the name and the remainder is the type.
            if *language == cce_types::language::Language::Go {
                let mut parts = before_eq.split_whitespace();
                if let Some(first) = parts.next() {
                    let rest: Vec<&str> = parts.collect();
                    if rest.is_empty() {
                        return (first.to_string(), None);
                    }
                    let name = first
                        .trim_start_matches("this.")
                        .trim_start_matches("super.")
                        .trim_start_matches("...")
                        .to_string();
                    let typ = rest.join(" ").trim().to_string();
                    if name.is_empty() {
                        return (before_eq.to_string(), None);
                    }
                    if typ.is_empty() {
                        return (name, None);
                    }
                    return (name, Some(typ));
                }
                return (before_eq.to_string(), None);
            }
            let mut parts: Vec<&str> = before_eq.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Some(last) = parts.pop() {
                    let name = last
                        .trim_start_matches("this.")
                        .trim_start_matches("super.")
                        .trim_end_matches(['?', '*'])
                        .to_string();
                    let typ = parts
                        .join(" ")
                        .replace("required ", "")
                        .replace("covariant ", "")
                        .replace("final ", "")
                        .replace("var ", "")
                        .trim()
                        .to_string();
                    if name.is_empty() {
                        return (before_eq.to_string(), None);
                    }
                    if typ.is_empty() {
                        return (name, None);
                    }
                    return (name, Some(typ));
                }
            }
            let name = before_eq
                .trim_start_matches("this.")
                .trim_start_matches("super.")
                .to_string();
            (name, None)
        }
    }
}

/// Extract return type from match
pub fn extract_return_type(mat: &QueryMatch) -> Option<String> {
    utils::find_capture_by_name(&mat.captures, |name| {
        utils::capture_name_contains(name, capture::SUBSTRING_RETURN)
            || utils::capture_name_contains(name, capture::SUBSTRING_RESULT)
    })
    .map(|c| c.text.clone())
}

/// Extract doc comment from match
pub fn extract_doc_comment(mat: &QueryMatch) -> Option<String> {
    utils::find_capture_by_name(&mat.captures, |name| {
        utils::capture_name_contains(name, capture::SUBSTRING_DOC)
            || utils::capture_name_contains(name, capture::SUBSTRING_COMMENT)
    })
    .map(|c| c.text.clone())
}

/// Extract element attributes (class, id, etc.) from HTML/Vue/JSX elements
pub fn extract_attributes(mat: &QueryMatch) -> HashMap<String, String> {
    let mut attributes = HashMap::new();
    let mut attr_names: HashMap<String, (usize, String)> = HashMap::new();

    for capture in &mat.captures {
        let name_lower = capture.name.to_lowercase();
        if name_lower.contains(capture::CATEGORY_ATTRIBUTE)
            && (name_lower.ends_with(".name")
                || name_lower.ends_with(".attr_name")
                || name_lower.ends_with(".attr"))
        {
            let attr_name = capture.text.trim().to_string();
            attr_names.insert(attr_name.clone(), (capture.start_byte, attr_name));
        }
    }

    for capture in &mat.captures {
        let name_lower = capture.name.to_lowercase();
        if name_lower.contains(capture::CATEGORY_ATTRIBUTE)
            && (name_lower.ends_with(".value")
                || name_lower.ends_with(".attr_value")
                || name_lower.ends_with(".quoted_value")
                || name_lower.ends_with(".expr_value"))
        {
            let mut closest_attr: Option<&String> = None;
            let mut min_distance: usize = usize::MAX;

            for (attr_name, (name_pos, _)) in &attr_names {
                if capture.start_byte > *name_pos {
                    let distance = capture.start_byte - name_pos;
                    if distance < min_distance {
                        min_distance = distance;
                        closest_attr = Some(attr_name);
                    }
                }
            }

            if let Some(attr_name) = closest_attr {
                let value = capture
                    .text
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                if !value.is_empty() {
                    attributes.insert(attr_name.clone(), value);
                }
            }
        }
    }

    attributes
}

/// Extract CSS property value from match
pub fn extract_css_property_value(mat: &QueryMatch) -> Option<String> {
    mat.captures
        .iter()
        .find(|c| c.name.contains("style_property") && c.name.ends_with(".value"))
        .map(|c| c.text.trim().to_string())
}

/// Extract Python method type from captures
pub fn extract_python_method_type(mat: &QueryMatch) -> Option<String> {
    for capture in &mat.captures {
        let name_lower = capture.name.to_lowercase();
        if name_lower.contains("method") {
            if name_lower.contains(".class.") {
                return Some("class_method".to_string());
            } else if name_lower.contains(".instance.") {
                return Some("instance_method".to_string());
            } else if name_lower.contains(".static.") {
                return Some("static_method".to_string());
            } else if name_lower.contains(".getter.") {
                return Some("getter".to_string());
            }
        }
    }
    None
}

/// Extract base class names from `@entity.class.base` captures
pub fn extract_base_classes(mat: &QueryMatch) -> Vec<String> {
    mat.captures
        .iter()
        .filter(|c| {
            let name_lower = c.name.to_lowercase();
            name_lower.ends_with(".base")
        })
        .map(|c| c.text.clone())
        .collect()
}

/// Extract enum variant type from captures
pub fn extract_enum_variant_type(mat: &QueryMatch) -> Option<String> {
    for capture in &mat.captures {
        let name_lower = capture.name.to_lowercase();
        if name_lower.contains("enum") {
            if name_lower.contains("variant") {
                return Some("variant".to_string());
            } else if name_lower.contains("constant") {
                return Some("constant".to_string());
            } else if name_lower.contains("member") {
                return Some("member".to_string());
            } else if name_lower.contains("value") {
                return Some("value".to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_sitter_query::executor::Capture;

    fn make_capture(name: &str, text: &str, start: usize, end: usize) -> Capture {
        Capture {
            name: name.to_string(),
            text: text.to_string(),
            start_byte: start,
            end_byte: end,
            start_point: (0, 0),
            end_point: (0, 0),
        }
    }

    fn make_capture_with_pos(
        name: &str,
        text: &str,
        start: usize,
        end: usize,
        start_row: usize,
        end_row: usize,
    ) -> Capture {
        Capture {
            name: name.to_string(),
            text: text.to_string(),
            start_byte: start,
            end_byte: end,
            start_point: (start_row, 0),
            end_point: (end_row, 0),
        }
    }

    fn make_match(captures: Vec<Capture>) -> QueryMatch {
        QueryMatch {
            captures,
            pattern_index: 0,
            index: 0,
        }
    }

    #[test]
    fn test_find_main_capture_none() {
        let mat = make_match(vec![make_capture("some.other", "test", 0, 4)]);
        assert!(find_main_capture(&mat).is_none());
    }

    #[test]
    fn test_find_main_capture_single() {
        let mat = make_match(vec![make_capture(
            "entity.function.definition",
            "fn foo() {}",
            0,
            13,
        )]);
        assert_eq!(find_main_capture(&mat).unwrap().text, "fn foo() {}");
    }

    #[test]
    fn test_find_main_capture_picks_largest_span() {
        let mat = make_match(vec![
            make_capture("entity.function", "fn foo<T>() {}", 0, 16),
            make_capture("entity.function.generator", "fn foo<T>() {}", 0, 16),
        ]);
        assert!(find_main_capture(&mat).is_some());
    }

    #[test]
    fn test_find_main_capture_filters_phantom_nodes() {
        // Phantom nodes (end_byte < start_byte) from tree-sitter error
        // recovery must be filtered out to prevent usize underflow in
        // max_by_key.
        let mat = make_match(vec![make_capture(
            "entity.enum.definition",
            "Void",
            124,
            123,
        )]);
        assert!(
            find_main_capture(&mat).is_none(),
            "phantom node with end_byte < start_byte must be filtered"
        );
    }

    #[test]
    fn test_find_main_capture_phantom_not_selected_over_valid() {
        // When a valid capture and a phantom capture coexist, only the
        // valid one should be returned.
        let mat = make_match(vec![
            make_capture("entity.enum.definition", "Void", 124, 123),
            make_capture("entity.struct.definition", "Foo", 10, 50),
        ]);
        let result = find_main_capture(&mat);
        assert!(result.is_some());
        assert_eq!(result.unwrap().text, "Foo");
    }

    #[test]
    fn test_find_main_capture_filters_reversed_rows() {
        // Phantom nodes can have valid byte ranges (start < end) but
        // reversed row positions (end_row < start_row). Both must be
        // checked independently.
        let mat = make_match(vec![make_capture_with_pos(
            "entity.function.definition",
            "_dummy",
            100,
            115,
            497,
            496,
        )]);
        assert!(
            find_main_capture(&mat).is_none(),
            "capture with end_row < start_row must be filtered even with valid bytes"
        );
    }

    #[test]
    fn test_find_main_capture_valid_position_not_filtered() {
        // A valid capture with consistent positions must not be filtered.
        let mat = make_match(vec![make_capture_with_pos(
            "entity.function.definition",
            "real_func",
            100,
            150,
            5,
            6,
        )]);
        let result = find_main_capture(&mat);
        assert!(result.is_some());
        assert_eq!(result.unwrap().text, "real_func");
    }

    #[test]
    fn test_extract_subtype_from_capture() {
        assert_eq!(
            extract_subtype_from_capture("entity.function.generator"),
            Some("generator".to_string())
        );
        assert_eq!(extract_subtype_from_capture("entity.function"), None);
    }

    #[test]
    fn test_extract_doc_comment_some() {
        let mat = make_match(vec![
            make_capture("entity.function", "fn foo() {}", 0, 11),
            make_capture("entity.function.doc", "foo docs", 0, 8),
        ]);
        assert_eq!(extract_doc_comment(&mat), Some("foo docs".to_string()));
    }

    #[test]
    fn test_extract_doc_comment_none() {
        let mat = make_match(vec![make_capture("entity.function", "fn foo() {}", 0, 11)]);
        assert!(extract_doc_comment(&mat).is_none());
    }

    #[test]
    fn test_strip_inline_comments_no_comment() {
        assert_eq!(strip_inline_comments("x, y, z"), "x, y, z");
    }

    #[test]
    fn test_strip_inline_comments_with_inline() {
        assert_eq!(
            strip_inline_comments("x: int = 5,  # type: ignore"),
            "x: int = 5,  "
        );
    }

    #[test]
    fn test_strip_inline_comments_in_brackets() {
        assert_eq!(
            strip_inline_comments("x: dict[str, int] = {}  # type: ignore"),
            "x: dict[str, int] = {}  "
        );
    }

    #[test]
    fn test_strip_inline_comments_preserves_following_line() {
        assert_eq!(
            strip_inline_comments("cli_group: str = _sentinel,  # type: ignore[assignment]\nself"),
            "cli_group: str = _sentinel,  \nself"
        );
    }

    #[test]
    fn test_parse_parameters_text_strips_inline_comments() {
        let params = parse_parameters_text(
            "(x: int,  # type: ignore\ny: str)",
            &cce_types::language::Language::Python,
        );
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "x");
        assert_eq!(params[1].0, "y");
    }

    #[test]
    fn test_parse_parameters_text_type_ignore_filtered() {
        let params = parse_parameters_text(
            "(cli_group: str | None = _sentinel,  # type: ignore[assignment]\nself)",
            &cce_types::language::Language::Python,
        );
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "cli_group");
        assert_eq!(params[1].0, "self");
    }

    #[test]
    fn test_parse_parameters_go_name_first() {
        let params =
            parse_parameters_text("(name string, age int)", &cce_types::language::Language::Go);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], ("name".to_string(), Some("string".to_string())));
        assert_eq!(params[1], ("age".to_string(), Some("int".to_string())));
    }

    #[test]
    fn test_parse_single_param_skips_double_colon() {
        let (name, typ) = parse_single_param(
            "const std::vector<int>& items",
            &cce_types::language::Language::Cpp,
        );
        assert_eq!(name, "items");
        assert_eq!(typ, Some("const std::vector<int>&".to_string()));
    }

    #[test]
    fn test_parse_single_param_rust_receiver() {
        use cce_types::language::Language::Rust;
        let cases = [
            ("self", "Self"),
            ("mut self", "Self"),
            ("&self", "&Self"),
            ("&mut self", "&mut Self"),
            ("&'a self", "&Self"),
            ("&'a mut self", "&mut Self"),
            ("self: Box<Self>", "Box<Self>"),
        ];
        for (text, ty) in cases {
            let (name, parsed) = parse_single_param(text, &Rust);
            assert_eq!(name, "self", "receiver name for {text:?}");
            assert_eq!(parsed.as_deref(), Some(ty), "receiver type for {text:?}");
        }
        // Ordinary parameters are untouched.
        let (name, typ) = parse_single_param("count: i32", &Rust);
        assert_eq!(name, "count");
        assert_eq!(typ.as_deref(), Some("i32"));
    }

    #[test]
    fn test_parse_parameters_text_rust_fn_trait_return_arrow() {
        use cce_types::language::Language::Rust;
        // The `>` in `->` is unpaired; it must not swallow the comma that
        // separates the two parameters.
        let params = parse_parameters_text("(f: impl Fn(i32) -> i32, value: i32)", &Rust);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "f");
        assert_eq!(params[0].1.as_deref(), Some("impl Fn(i32) -> i32"));
        assert_eq!(params[1].0, "value");
        assert_eq!(params[1].1.as_deref(), Some("i32"));
    }

    #[test]
    fn test_parse_parameters_text_rust_nested_generics_and_shift() {
        use cce_types::language::Language::Rust;
        // Paired `>>` keeps working, and a `>` inside a default value
        // expression does not break splitting either.
        let params = parse_parameters_text("(a: HashMap<String, Vec<i32>>, b: i32)", &Rust);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "a");
        assert_eq!(params[1].0, "b");

        let params = parse_parameters_text("(x: i32 = y >> 1, y: i32)", &Rust);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "x");
        assert_eq!(params[1].0, "y");
    }
}
