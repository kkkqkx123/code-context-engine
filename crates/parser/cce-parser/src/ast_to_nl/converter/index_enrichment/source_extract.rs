//! Source extraction and entity lookup for index sidecar building.
//!
//! Control flow and behavior facts carry byte ranges into the original
//! source. This module re-extracts those ranges with indentation normalized
//! and trailing closers trimmed, so the sidecar keeps searchable code while
//! the template keeps the presentation text.

use crate::grouper::ProcessingResult;
use cce_types::EntityId;

/// Whether a behavior fact is a docstring statement.
///
/// Tree-sitter represents a docstring as a plain expression statement, so
/// the behavior query captures its raw literal. The template already emits
/// the cleaned documentation; keeping the raw literal would duplicate the
/// docstring and reintroduce RST markup (`"""`, `:attr:`,
/// `.. versionchanged::`) into the sidecar text.
pub(super) fn is_docstring_statement(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''")
}

pub(super) fn deindent(text: &str) -> String {
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

pub(super) fn extract_clean_source(source: &str, start_byte: usize, end_byte: usize) -> String {
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

pub(super) fn find_entity_span(
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
pub(super) fn find_entity_signature(
    processing_result: &ProcessingResult,
    entity_id: EntityId,
) -> Option<&str> {
    find_entity(processing_result, entity_id).map(|entity| entity.signature.as_str())
}

/// The documentation comment of an entity, taken from the group it belongs to.
pub(super) fn find_entity_doc_comment(
    processing_result: &ProcessingResult,
    entity_id: EntityId,
) -> Option<&str> {
    find_entity(processing_result, entity_id).and_then(|entity| entity.doc_comment.as_deref())
}

/// The entity with the given id, taken from the group tree it belongs to.
pub(super) fn find_entity(
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

pub(super) fn extract_body_without_signature(
    source: &str,
    start_byte: usize,
    end_byte: usize,
) -> String {
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
    let rest = skip_leading_signature_block(rest);
    let rest = skip_leading_docstring_block(rest);
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

fn skip_leading_signature_block(lines: Vec<&str>) -> Vec<&str> {
    let mut index = 0usize;
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }
    while index < lines.len() && lines[index].trim_start().starts_with('@') {
        index += 1;
        while index < lines.len() && lines[index].trim().is_empty() {
            index += 1;
        }
    }
    if index >= lines.len() {
        return Vec::new();
    }
    let trimmed = lines[index].trim_start();
    let is_definition = trimmed.starts_with("def ")
        || trimmed.starts_with("async def ")
        || trimmed.starts_with("class ");
    if is_definition {
        while index < lines.len() && !lines[index].contains(':') {
            index += 1;
        }
        if index < lines.len() {
            index += 1;
        }
        while index < lines.len() && lines[index].trim().is_empty() {
            index += 1;
        }
    }
    lines[index..].to_vec()
}

fn skip_leading_docstring_block(lines: Vec<&str>) -> Vec<&str> {
    let mut index = 0usize;
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }
    if index >= lines.len() {
        return Vec::new();
    }
    let delimiter = docstring_delimiter(lines[index]);
    let Some(delimiter) = delimiter else {
        return lines[index..].to_vec();
    };
    let opener = lines[index].trim_start();
    if single_line_docstring(opener, delimiter) {
        let mut rest = lines[index + 1..].to_vec();
        while !rest.is_empty() && rest[0].trim().is_empty() {
            rest.remove(0);
        }
        return rest;
    }
    let mut end = index + 1;
    while end < lines.len() && !lines[end].contains(delimiter) {
        end += 1;
    }
    if end < lines.len() {
        end += 1;
    }
    let mut rest = lines[end..].to_vec();
    while !rest.is_empty() && rest[0].trim().is_empty() {
        rest.remove(0);
    }
    rest
}

/// First byte of executable code inside an entity span.
///
/// Skips decorators, the definition header (including multi-line
/// signatures), and a leading docstring block. Any shape mismatch falls
/// back to the span start, so filtering based on this offset stays a
/// no-op for languages and layouts the scan does not understand.
pub(super) fn code_body_start(source: &str, span_start: usize, span_end: usize) -> usize {
    let Some(window) = source.get(span_start..span_end) else {
        return span_start;
    };
    let mut offsets: Vec<usize> = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    let mut cursor = span_start;
    for piece in window.split_inclusive('\n') {
        offsets.push(cursor);
        lines.push(piece);
        cursor += piece.len();
    }
    if lines.is_empty() {
        return span_start;
    }
    let mut index = 0usize;
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }
    loop {
        while index < lines.len() && lines[index].trim().is_empty() {
            index += 1;
        }
        if index < lines.len() && lines[index].trim_start().starts_with('@') {
            index += 1;
        } else {
            break;
        }
    }
    if index >= lines.len() {
        return span_start;
    }
    let head = lines[index].trim_start();
    let is_definition = head.starts_with("def ")
        || head.starts_with("def\t")
        || head.starts_with("async def ")
        || head.starts_with("async def\t")
        || head.starts_with("class ")
        || head.starts_with("class\t");
    if !is_definition {
        return span_start;
    }
    let mut sig_end = index;
    loop {
        let line = lines[sig_end];
        let code_part = line.split('#').next().unwrap_or(line);
        if code_part.trim_end().ends_with(':') || code_part.contains("):") {
            break;
        }
        sig_end += 1;
        if sig_end >= lines.len() {
            return span_start;
        }
    }
    let end_code = lines[sig_end].split('#').next().unwrap_or(lines[sig_end]);
    if !end_code.trim_end().ends_with(':') {
        return span_start;
    }
    let mut body = sig_end + 1;
    while body < lines.len() && lines[body].trim().is_empty() {
        body += 1;
    }
    if body >= lines.len() {
        return span_end;
    }
    let opener = lines[body].trim_start();
    let stripped = opener.trim_start_matches(['r', 'R', 'u', 'U', 'b', 'B', 'f', 'F']);
    let delimiter = if stripped.starts_with("\"\"\"") {
        Some("\"\"\"")
    } else if stripped.starts_with("'''") {
        Some("'''")
    } else {
        None
    };
    let Some(delimiter) = delimiter else {
        return offsets[body];
    };
    let after_open = &stripped[delimiter.len()..];
    if after_open.contains(delimiter) {
        return if body + 1 < lines.len() {
            offsets[body + 1]
        } else {
            span_end
        };
    }
    let mut close = body + 1;
    while close < lines.len() && !lines[close].contains(delimiter) {
        close += 1;
    }
    if close >= lines.len() {
        return span_start;
    }
    if close + 1 < lines.len() {
        offsets[close + 1]
    } else {
        span_end
    }
}

fn docstring_delimiter(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("\"\"\"") {
        Some("\"\"\"")
    } else if trimmed.starts_with("'''") {
        Some("'''")
    } else {
        None
    }
}

fn single_line_docstring(trimmed_start: &str, delimiter: &str) -> bool {
    let Some(after_open) = trimmed_start.strip_prefix(delimiter) else {
        return false;
    };
    after_open.contains(delimiter)
}

pub(super) fn is_meaningful_code(text: &str) -> bool {
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

pub(super) fn strip_trailing_closers(text: &str) -> String {
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

    #[test]
    fn extract_clean_source_strips_comments() {
        // Protocol comments and trailing field notes are retrieval signals
        // and must survive into the embedding sidecar.
        let source = "let x = 5; // initialize\nlet y = 10;";
        let result = extract_clean_source(source, 0, source.len());
        assert!(result.contains("// initialize"), "should keep comments");
        assert!(result.contains("let x = 5;"), "should keep code");
        assert!(result.contains("let y = 10;"), "should keep code");
    }

    #[test]
    fn extract_clean_source_handles_out_of_bounds() {
        let source = "short";
        let result = extract_clean_source(source, 0, 100);
        assert_eq!(result, "short");

        let result = extract_clean_source(source, 10, 20);
        assert_eq!(result, "");
    }

    #[test]
    fn extract_clean_source_deindents() {
        let source = "fn demo() {\n    if true {\n        return;\n    }\n}";
        let result = extract_clean_source(source, 12, source.len() - 1);
        assert!(
            !result.lines().next().unwrap().starts_with(' '),
            "first line should be de-indented: {:?}",
            result
        );
    }

    #[test]
    fn deindent_removes_common_leading_whitespace() {
        let input = "    if true {\n        return;\n    }";
        let result = deindent(input);
        assert_eq!(result, "if true {\n    return;\n}");
    }

    #[test]
    fn extract_clean_source_preserves_indentation_when_keyword_is_indented() {
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
    fn code_body_start_skips_decorator_signature_and_docstring() {
        let source = "@setupmethod\ndef register_blueprint(self, blueprint):\n    \"\"\"Register the blueprint.\n\n    Details here.\n    \"\"\"\n    blueprint.register(self)\n";
        let body = code_body_start(source, 0, source.len());
        assert_eq!(&source[body..], "    blueprint.register(self)\n");
    }

    #[test]
    fn code_body_start_handles_multiline_signature() {
        let source = "def f(\n    a,\n    b=1,\n):\n    \"\"\"Doc.\"\"\"\n    return a\n";
        let body = code_body_start(source, 0, source.len());
        assert_eq!(&source[body..], "    return a\n");
    }

    #[test]
    fn code_body_start_falls_back_for_unknown_shapes() {
        let rust = "fn demo() {\n    let x = 1;\n}";
        assert_eq!(code_body_start(rust, 0, rust.len()), 0);
        let one_liner = "def f(): return 1\n";
        assert_eq!(code_body_start(one_liner, 0, one_liner.len()), 0);
        let unterminated = "def f():\n    \"\"\"never closed\n    return 1\n";
        assert_eq!(code_body_start(unterminated, 0, unterminated.len()), 0);
    }
}
