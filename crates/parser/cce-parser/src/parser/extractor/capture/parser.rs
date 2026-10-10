//! Capture parser: extract raw entity data from tree-sitter query matches
//!
//! Provides pure functions that extract typed data from tree-sitter captures
//! without modifying any entity state. All functions are deterministic and
//! depend only on the capture/match data.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// Number of matches that produced no composable signature parts and no
/// usable header fallback. The empty signature itself is the observable
/// failure signal; this counter makes the gap countable in tests.
static SIGNATURE_MISSING_COUNT: AtomicU64 = AtomicU64::new(0);

/// Current value of the signature-missing counter.
pub fn signature_missing_count() -> u64 {
    SIGNATURE_MISSING_COUNT.load(Ordering::Relaxed)
}

fn record_signature_missing() {
    SIGNATURE_MISSING_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Reconstruct signature text from structural sub-captures.
///
/// Signature equals the named structural parts in source order. Each part
/// keeps its source text verbatim (including consecutive spaces inside
/// string defaults); only the separator between parts is a single space.
/// Decorators, comments, and docstrings are never captured so they cannot
/// enter the signature. Two roles are summarized, never cut mid-token by a
/// blind length cap: provenance sources keep their head
/// ([`utils::summarize_provenance_source`]) and argument lists fold embedded
/// block bodies to shape markers ([`collapse_long_brace_blocks`]).
pub fn reconstruct_signature_from_subcaptures(mat: &QueryMatch, source: &str) -> String {
    let mut sub_captures: Vec<&Capture> = mat
        .captures
        .iter()
        .filter(|c| c.name.contains(".signature."))
        .collect();

    if sub_captures.is_empty() {
        return String::new();
    }

    sub_captures.sort_by_key(|c| c.start_byte);

    let parts: Vec<String> = sub_captures
        .iter()
        .map(|c| {
            let raw = utils::extract_text_from_source(source, c.start_byte, c.end_byte);
            let text = if raw.is_empty() { c.text.clone() } else { raw };
            let text = text.trim().to_string();
            // Provenance roles (`for x in <collection>`) bind data, not
            // declarations: keep the collection head, drop the rows.
            // Declaration roles (name, params, types) stay verbatim, except
            // argument lists whose embedded block bodies are folded to shape
            // markers so implementation detail cannot leak into signatures.
            if c.name.ends_with(".signature.source") {
                utils::summarize_provenance_source(&text)
            } else if c.name.ends_with(".signature.arguments") {
                collapse_long_brace_blocks(&text)
            } else {
                text
            }
        })
        .filter(|t| !t.is_empty())
        .collect();
    parts.join(" ")
}

/// Maximum signature length in chars for the bodiless full-text fallback.
///
/// Bodiless entities (variables, fields, imports, and friends) take their
/// whole main span as the signature. Initializers can be arbitrarily large
/// (giant literals, table fixtures, chained builder calls), and that data
/// flows verbatim into NL conversion, retrieval text, and snapshot keys.
/// The cap keeps the declaration head and drops the data tail. Composed
/// and header-sliced signatures are bounded by construction and stay
/// verbatim; only this unbounded branch is capped.
pub const MAX_SIGNATURE_LEN: usize = 500;

/// Truncate a fallback signature to [`MAX_SIGNATURE_LEN`] on a char boundary.
fn truncate_fallback_signature(signature: String) -> String {
    if signature.chars().count() > MAX_SIGNATURE_LEN {
        signature.chars().take(MAX_SIGNATURE_LEN).collect()
    } else {
        signature
    }
}

/// Maximum inline brace-block length in chars for argument lists.
///
/// Blocks at or below this size (small literals, short lambdas) stay
/// verbatim; longer ones are implementation detail inside a declaration
/// role and fold to a shape marker.
const MAX_INLINE_BLOCK_LEN: usize = 100;

/// Fold long brace blocks inside an argument list to shape markers.
///
/// Keeps argument structure, scalar arguments, and lambda parameter lists
/// while replacing block bodies (`{ ... }`) longer than
/// [`MAX_INLINE_BLOCK_LEN`] with `{...}`. The scan is aware of strings,
/// chars, line/block comments, and text blocks so braces inside them never
/// open a fold; unbalanced trailing text passes through verbatim.
fn collapse_long_brace_blocks(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let end = skip_quoted(bytes, i, true);
                out.push_str(&text[i..end]);
                i = end;
            }
            b'\'' => {
                let end = skip_quoted(bytes, i, false);
                out.push_str(&text[i..end]);
                i = end;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                let mut end = i + 2;
                while end < bytes.len() && bytes[end] != b'\n' {
                    end += 1;
                }
                out.push_str(&text[i..end]);
                i = end;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                let mut end = i + 2;
                while end + 1 < bytes.len() && !(bytes[end] == b'*' && bytes[end + 1] == b'/') {
                    end += 1;
                }
                end = (end + 2).min(bytes.len());
                out.push_str(&text[i..end]);
                i = end;
            }
            b'{' => match match_brace(bytes, i) {
                Some(end) if text[i..end].chars().count() > MAX_INLINE_BLOCK_LEN => {
                    out.push_str("{...}");
                    i = end;
                }
                Some(end) => {
                    out.push_str(&text[i..end]);
                    i = end;
                }
                None => {
                    out.push_str(&text[i..]);
                    break;
                }
            },
            _ => {
                let next = next_char_boundary(text, i);
                out.push_str(&text[i..next]);
                i = next;
            }
        }
    }
    out
}

/// Byte index one char past `i`. All scanned delimiters are ASCII, so every
/// produced index is a char boundary.
fn next_char_boundary(text: &str, i: usize) -> usize {
    let mut indices = text[i..].char_indices();
    indices.next();
    match indices.next() {
        Some((offset, _)) => i + offset,
        None => text.len(),
    }
}

/// End index just past a quoted region starting at `i` (which holds the
/// opening quote). `text_block` selects `"""`-style text blocks; otherwise a
/// single `'`/`"` quote with backslash escapes. Unterminated regions run to
/// the end of the text.
fn skip_quoted(bytes: &[u8], i: usize, text_block: bool) -> usize {
    if text_block && i + 2 < bytes.len() && bytes[i + 1] == b'"' && bytes[i + 2] == b'"' {
        let mut j = i + 3;
        while j + 2 < bytes.len() {
            if bytes[j] == b'"' && bytes[j + 1] == b'"' && bytes[j + 2] == b'"' {
                return j + 3;
            }
            j += 1;
        }
        return bytes.len();
    }
    let quote = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == quote {
            return j + 1;
        }
        if bytes[j] == b'\n' && quote != b'"' {
            return j;
        }
        j += 1;
    }
    bytes.len()
}

/// End index just past the `}` balancing the `{` at `i`, or `None` when
/// unbalanced. Nested blocks and quoted/commented regions are honored.
fn match_brace(bytes: &[u8], i: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut j = i;
    while j < bytes.len() {
        match bytes[j] {
            b'"' => j = skip_quoted(bytes, j, true),
            b'\'' => j = skip_quoted(bytes, j, false),
            b'/' if j + 1 < bytes.len() && bytes[j + 1] == b'/' => {
                while j < bytes.len() && bytes[j] != b'\n' {
                    j += 1;
                }
            }
            b'/' if j + 1 < bytes.len() && bytes[j + 1] == b'*' => {
                j += 2;
                while j + 1 < bytes.len() && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                    j += 1;
                }
                j = (j + 2).min(bytes.len());
            }
            b'{' => {
                depth += 1;
                j += 1;
            }
            b'}' => {
                depth -= 1;
                j += 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => j += 1,
        }
    }
    None
}
/// Whether a capture marks the start of an entity body (function block,
/// class block, field list, and their per-language equivalents).
fn is_body_capture(name: &str) -> bool {
    name.ends_with(".body")
}

/// Whether the main capture names a kind that conceptually owns a block
/// body. Inline forms (arrow, lambda, callbacks) keep the full text as
/// their header; block forms without a body capture are query gaps and
/// must surface as empty signatures, never as full-text fallbacks.
fn main_expects_body(main_name: &str) -> bool {
    let lower = main_name.to_lowercase();
    if lower.contains("arrow")
        || lower.contains("callback")
        || lower.contains("lambda")
        || lower.contains("closure")
        || lower.contains("literal")
        || lower.contains("comprehension")
        || lower.contains("iife")
        || lower.contains("top_call")
        || lower.contains("top_if")
        || lower.contains("variant")
        || lower.contains("enum_")
        || lower.contains("enum.")
    {
        return false;
    }
    lower.contains("function")
        || lower.contains("method")
        || lower.contains("constructor")
        || lower.contains("destructor")
        || lower.contains("operator")
        || lower.contains("class")
        || lower.contains("struct")
        || lower.contains("enum")
        || lower.contains("interface")
        || lower.contains("trait")
        || lower.contains(".impl")
        || lower.contains("union")
        || lower.contains("getter")
        || lower.contains("setter")
}

/// Extract full entity signature text from source.
///
/// Priority:
/// 1. Compose from signature sub-captures in source order.
/// 2. Transitional: slice the header before the body capture for patterns
///    that predate signature aliases. No cleaning and no truncation.
/// 3. Bodiless entities (module, import, macro, and friends): the full
///    main text is the header, capped at [`MAX_SIGNATURE_LEN`] chars so
///    giant initializers cannot leak unbounded data downstream.
///    Otherwise return empty and count the gap instead of fabricating text.
pub fn extract_signature(mat: &QueryMatch, source: &str) -> String {
    let composed = reconstruct_signature_from_subcaptures(mat, source);
    if !composed.trim().is_empty() {
        return composed;
    }

    let Some(main) = find_main_capture(mat) else {
        record_signature_missing();
        return String::new();
    };

    let mut header_end = main.end_byte;
    for capture in mat.captures.iter().filter(|c| is_body_capture(&c.name)) {
        if capture.start_byte > main.start_byte
            && capture.start_byte < header_end
            && capture.end_byte <= main.end_byte
        {
            header_end = capture.start_byte;
        }
    }
    if header_end > main.start_byte && header_end < main.end_byte {
        let sliced = utils::extract_text_from_source(source, main.start_byte, header_end);
        let header = sliced.trim();
        if !header.is_empty() {
            return header.to_string();
        }
        record_signature_missing();
        return String::new();
    }

    if main_expects_body(&main.name) {
        record_signature_missing();
        return String::new();
    }
    truncate_fallback_signature(
        utils::extract_text_from_source(source, main.start_byte, main.end_byte)
            .trim()
            .to_string(),
    )
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
                    for (idx, (name, typ, _)) in parse_parameters_text(&capture.text, language)
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

/// Extract parameter defaults from match, returning (name, default) pairs
/// for parameters that declare a default value (e.g. `x: int = 5`).
pub fn extract_parameter_defaults(
    mat: &QueryMatch,
    language: &cce_types::language::Language,
) -> Vec<(String, String)> {
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
            if suffix == "params" {
                return parse_parameters_text(&capture.text, language)
                    .into_iter()
                    .filter_map(|(name, _, default)| {
                        default.map(|d| (name, d)).filter(|(_, d)| !d.is_empty())
                    })
                    .collect();
            }
        }
    }
    Vec::new()
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
) -> Vec<(String, Option<String>, Option<String>)> {
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
) -> (String, Option<String>, Option<String>) {
    let text = text.trim();
    if text.is_empty() {
        return (String::new(), None, None);
    }
    // Rust method receivers (`&mut self`, `&'a self`, `mut self: Type`)
    // carry the reference on the name side; without special handling the
    // generic splitter reports (`self`, `&mut`), which the inferer then
    // wraps in another reference (`&mut &mut`).
    if *language == cce_types::language::Language::Rust
        && let Some((name, ty)) = parse_rust_receiver(text)
    {
        return (name, ty, None);
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
    let default_value: Option<String> = eq_pos
        .map(|epos| text[epos + 1..].trim().to_string())
        .filter(|s| !s.is_empty());
    match colon_pos {
        Some(cpos) => {
            let name = text[..cpos].trim().to_string();
            let type_end = eq_pos.unwrap_or(text.len());
            let typ_raw = text[cpos + 1..type_end].trim();
            let typ = if typ_raw.is_empty() {
                None
            } else {
                Some(typ_raw.to_string())
            };
            (name, typ, default_value)
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
                        return (first.to_string(), None, default_value);
                    }
                    let name = first
                        .trim_start_matches("this.")
                        .trim_start_matches("super.")
                        .trim_start_matches("...")
                        .to_string();
                    let typ = rest.join(" ").trim().to_string();
                    if name.is_empty() {
                        return (before_eq.to_string(), None, default_value);
                    }
                    if typ.is_empty() {
                        return (name, None, default_value);
                    }
                    return (name, Some(typ), default_value);
                }
                return (before_eq.to_string(), None, default_value);
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
                        return (before_eq.to_string(), None, default_value);
                    }
                    if typ.is_empty() {
                        return (name, None, default_value);
                    }
                    return (name, Some(typ), default_value);
                }
            }
            let name = before_eq
                .trim_start_matches("this.")
                .trim_start_matches("super.")
                .to_string();
            (name, None, default_value)
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

/// Extract base class names from `@entity.class.base` captures.
/// Stacked signature aliases share the same span as the plain capture, so
/// duplicates are removed while keeping source order.
pub fn extract_base_classes(mat: &QueryMatch) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut bases = Vec::new();
    let mut ordered: Vec<&Capture> = mat
        .captures
        .iter()
        .filter(|c| c.name.to_lowercase().ends_with(".base"))
        .collect();
    ordered.sort_by_key(|c| c.start_byte);
    for c in ordered {
        if seen.insert((c.start_byte, c.end_byte, c.text.clone())) {
            bases.push(c.text.clone());
        }
    }
    bases
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
    fn test_extract_signature_colon_block_excludes_body() {
        let source = "def send_file(path: str) -> Response:\n    \"\"\"Send docs.\"\"\"\n    return make(path)\n";
        let body_start = source.find('\n').expect("header line") + 1;
        let mat = make_match(vec![
            make_capture("entity.function", source, 0, source.len()),
            make_capture(
                "entity.function.body",
                &source[body_start..],
                body_start,
                source.len(),
            ),
        ]);
        assert_eq!(
            extract_signature(&mat, source),
            "def send_file(path: str) -> Response:"
        );
    }

    #[test]
    fn test_extract_signature_class_excludes_methods() {
        let source = "class FakePath:\n    \"\"\"Fake object.\"\"\"\n    def __fspath__(self):\n        return self.path\n";
        let body_start = source.find('\n').expect("header line") + 1;
        let mat = make_match(vec![
            make_capture("entity.class", source, 0, source.len()),
            make_capture(
                "entity.class.body",
                &source[body_start..],
                body_start,
                source.len(),
            ),
        ]);
        assert_eq!(extract_signature(&mat, source), "class FakePath:");
    }

    #[test]
    fn test_extract_signature_composition_excludes_decorator() {
        let source = "@app.route(\"/read\")\ndef read():\n    return str(x)\n";
        let name_start = source.find("read").expect("name");
        let params_start = source.find("()").expect("params");
        let mat = make_match(vec![
            make_capture("entity.function", source, 0, source.len()),
            make_capture(
                "entity.function.signature.name",
                "read",
                name_start,
                name_start + 4,
            ),
            make_capture(
                "entity.function.signature.params",
                "()",
                params_start,
                params_start + 2,
            ),
        ]);
        assert_eq!(extract_signature(&mat, source), "read ()");
    }

    #[test]
    fn test_extract_signature_brace_block_keeps_header() {
        let source = "int add(int a, int b) {\n    return a + b;\n}";
        let body_start = source.find('{').expect("brace");
        let mat = make_match(vec![
            make_capture("entity.function", source, 0, source.len()),
            make_capture(
                "entity.function.body",
                &source[body_start..],
                body_start,
                source.len(),
            ),
        ]);
        assert_eq!(extract_signature(&mat, source), "int add(int a, int b)");
    }

    #[test]
    fn test_extract_signature_without_body_returns_header() {
        let source = "mod once_box";
        let mat = make_match(vec![make_capture("entity.module", source, 0, source.len())]);
        assert_eq!(extract_signature(&mat, source), "mod once_box");
    }

    #[test]
    fn test_extract_signature_keeps_preprocessor_line() {
        let source = "#define MAX_ITEMS 100";
        let mat = make_match(vec![make_capture("entity.macro", source, 0, source.len())]);
        assert_eq!(extract_signature(&mat, source), "#define MAX_ITEMS 100");
    }

    #[test]
    fn test_extract_signature_preserves_long_header() {
        let header = format!("def f({}):", vec!["arg: int"; 200].join(", "));
        let source = format!("{header}\n    return 1\n");
        let body_start = header.len() + 1;
        let mat = make_match(vec![
            make_capture("entity.function", source.as_str(), 0, source.len()),
            make_capture(
                "entity.function.body",
                &source[body_start..],
                body_start,
                source.len(),
            ),
        ]);
        let signature = extract_signature(&mat, source.as_str());
        assert_eq!(signature, header);
        assert!(!signature.contains("return"));
    }

    #[test]
    fn test_extract_signature_preserves_string_default_spacing() {
        let source = "def greet(name: str = \"a  b\"):\n    return name\n";
        let name_start = source.find("greet").expect("name");
        let params_text = "(name: str = \"a  b\")";
        let params_start = source.find(params_text).expect("params");
        let mat = make_match(vec![
            make_capture("entity.function", source, 0, source.len()),
            make_capture(
                "entity.function.signature.name",
                "greet",
                name_start,
                name_start + 5,
            ),
            make_capture(
                "entity.function.signature.params",
                params_text,
                params_start,
                params_start + params_text.len(),
            ),
        ]);
        assert_eq!(
            extract_signature(&mat, source),
            format!("greet {params_text}")
        );
    }

    #[test]
    fn test_extract_signature_dict_default_not_cut() {
        let source = "def f(opts: dict = {\"a\": 1}):\n    return opts\n";
        let name_start = source.find("f(").expect("name");
        let params_text = "(opts: dict = {\"a\": 1})";
        let params_start = source.find(params_text).expect("params");
        let mat = make_match(vec![
            make_capture("entity.function", source, 0, source.len()),
            make_capture(
                "entity.function.signature.name",
                "f",
                name_start,
                name_start + 1,
            ),
            make_capture(
                "entity.function.signature.params",
                params_text,
                params_start,
                params_start + params_text.len(),
            ),
        ]);
        assert_eq!(extract_signature(&mat, source), format!("f {params_text}"));
    }

    #[test]
    fn test_extract_signature_missing_returns_empty() {
        let source = "def broken(arg):\n    return arg\n";
        let before = signature_missing_count();
        let mat = make_match(vec![make_capture(
            "entity.function",
            source,
            0,
            source.len(),
        )]);
        assert_eq!(extract_signature(&mat, source), "");
        assert!(signature_missing_count() > before, "gap must be counted");
    }

    #[test]
    fn test_extract_signature_enum_variant_returns_full_text() {
        let source = "Some(x)";
        let before = signature_missing_count();
        let mat = make_match(vec![make_capture(
            "entity.enum_variant",
            source,
            0,
            source.len(),
        )]);
        assert_eq!(extract_signature(&mat, source), "Some(x)");
        assert_eq!(
            signature_missing_count(),
            before,
            "variant-like kinds must not count as missing"
        );
    }

    #[test]
    fn test_extract_signature_lambda_returns_full_text() {
        let source = "double = lambda x: x * 2";
        let before = signature_missing_count();
        let mat = make_match(vec![
            make_capture("entity.lambda", source, 0, source.len()),
            make_capture("entity.lambda.name", "double", 0, 6),
        ]);
        assert_eq!(extract_signature(&mat, source), source);
        assert_eq!(
            signature_missing_count(),
            before,
            "inline lambda must keep its full text"
        );
    }

    #[test]
    fn test_extract_signature_bodiless_fallback_truncates() {
        let filler = "x".repeat(MAX_SIGNATURE_LEN + 100);
        let source = format!("data = [{filler}]");
        let before = signature_missing_count();
        let mat = make_match(vec![make_capture(
            "entity.variable",
            source.as_str(),
            0,
            source.len(),
        )]);
        let signature = extract_signature(&mat, source.as_str());
        assert_eq!(signature.chars().count(), MAX_SIGNATURE_LEN);
        assert!(
            source.starts_with(&signature),
            "truncation must keep the declaration head"
        );
        assert_eq!(
            signature_missing_count(),
            before,
            "truncated fallback must not count as missing"
        );
    }

    #[test]
    fn test_extract_signature_source_part_truncates() {
        let big = format!("[{}]", "1,".repeat(MAX_SIGNATURE_LEN));
        let source = format!("for x in {big}:\n    use(x)\n");
        let name_start = source.find('x').expect("name");
        let src_start = source.find('[').expect("source");
        let mat = make_match(vec![
            make_capture("entity.variable.loop", source.as_str(), 0, source.len()),
            make_capture(
                "entity.variable.loop.signature.name",
                "x",
                name_start,
                name_start + 1,
            ),
            make_capture(
                "entity.variable.loop.signature.source",
                big.as_str(),
                src_start,
                src_start + big.len(),
            ),
        ]);
        let signature = extract_signature(&mat, source.as_str());
        assert!(signature.starts_with("x ["));
        assert!(signature.chars().count() <= MAX_SIGNATURE_LEN + 2);
    }

    #[test]
    fn test_extract_signature_source_part_summarizes_multiline_head() {
        let head = "[]struct {\n\tname string\n\tvalue any\n}";
        let rows = "{\n\t{\"base type\", 1},\n\t{\"zero value\", 0},\n}".repeat(10);
        let big = format!("{head}{rows}");
        let source = format!("for _, tt := range {big} {{ use(tt) }}");
        let name_start = source.find("tt").expect("name");
        let src_start = source.find("[]struct").expect("source");
        let mat = make_match(vec![
            make_capture("entity.variable.loop", source.as_str(), 0, source.len()),
            make_capture(
                "entity.variable.loop.signature.name",
                "tt",
                name_start,
                name_start + 2,
            ),
            make_capture(
                "entity.variable.loop.signature.source",
                big.as_str(),
                src_start,
                src_start + big.len(),
            ),
        ]);
        let signature = extract_signature(&mat, source.as_str());
        assert!(
            signature.starts_with("tt []struct {"),
            "summary must keep the collection head, got: {signature}"
        );
        assert!(
            !signature.contains("base type"),
            "summary must drop literal rows, got: {signature}"
        );
        assert!(
            signature.ends_with("..."),
            "summary must be marked, got: {signature}"
        );
        assert!(signature.chars().count() <= MAX_SIGNATURE_LEN);
    }

    #[test]
    fn test_extract_signature_arguments_collapses_lambda_body() {
        let body =
            "JsonParser parser = create();\nfeeder.feed(input);\nreturn parser;\n".repeat(10);
        let args = format!("(\n(String input) -> {{\n{body}}},\ntrue,\nfalse,\ntrue\n)");
        let source = format!("ASYNC{args}\n;");
        let name_end = "ASYNC".len();
        let args_start = source.find('(').expect("args");
        let mat = make_match(vec![
            make_capture("entity.enum_constant", source.as_str(), 0, source.len()),
            make_capture("entity.enum_constant.signature.name", "ASYNC", 0, name_end),
            make_capture(
                "entity.enum_constant.signature.arguments",
                args.as_str(),
                args_start,
                args_start + args.len(),
            ),
        ]);
        let signature = extract_signature(&mat, source.as_str());
        assert!(
            signature.starts_with("ASYNC ("),
            "variant name and argument shape must survive, got: {signature}"
        );
        assert!(
            signature.contains("(String input)"),
            "lambda parameters must survive, got: {signature}"
        );
        assert!(
            signature.contains("true"),
            "scalar arguments must survive, got: {signature}"
        );
        assert!(
            !signature.contains("feed(input)"),
            "lambda implementation must fold away, got: {signature}"
        );
        assert!(signature.chars().count() <= MAX_SIGNATURE_LEN);
    }

    #[test]
    fn test_extract_signature_subcaptures_stay_single_line() {
        let source = "fn demo(value: T) -> T { value }";
        let name_start = source.find("demo").expect("name");
        let params_start = source.find("(value: T)").expect("params");
        let ret_start = source.find("-> T").expect("return");
        let mat = make_match(vec![
            make_capture("entity.function", source, 0, source.len()),
            make_capture(
                "entity.function.signature.name",
                "demo",
                name_start,
                name_start + 4,
            ),
            make_capture(
                "entity.function.signature.params",
                "(value: T)",
                params_start,
                params_start + 10,
            ),
            make_capture(
                "entity.function.signature.return_type",
                "-> T",
                ret_start,
                ret_start + 4,
            ),
        ]);
        assert_eq!(extract_signature(&mat, source), "demo (value: T) -> T");
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
        assert_eq!(
            params[0],
            ("name".to_string(), Some("string".to_string()), None)
        );
        assert_eq!(
            params[1],
            ("age".to_string(), Some("int".to_string()), None)
        );
    }

    #[test]
    fn test_parse_single_param_skips_double_colon() {
        let (name, typ, _) = parse_single_param(
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
            let (name, parsed, _) = parse_single_param(text, &Rust);
            assert_eq!(name, "self", "receiver name for {text:?}");
            assert_eq!(parsed.as_deref(), Some(ty), "receiver type for {text:?}");
        }
        // Ordinary parameters are untouched.
        let (name, typ, _) = parse_single_param("count: i32", &Rust);
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
