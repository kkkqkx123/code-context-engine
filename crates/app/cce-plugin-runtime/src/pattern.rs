//! Host-side pattern extractor (regex-based)
//!
//! Lua plugins declare regex patterns instead of (or alongside) custom
//! functions. The host compiles them with the Rust `regex` crate — faster
//! and safer than Lua string patterns — and produces [`PluginEntity`]s.
//!
//! # Capture convention
//!
//! Each match produces one [`PluginEntity`]. The regex's **named capture
//! groups** map to entity fields:
//!
//! | Capture group   | Entity field                  |
//! |-----------------|-------------------------------|
//! | `name` (or group 1) | entity `name`             |
//! | `signature`     | `signature`                    |
//! | `doc_comment`   | `doc_comment`                  |
//! | `kind`          | overrides the pattern's `kind` |
//! | `meta_<key>`    | `metadata["<key>"]`            |
//!
//! The match byte range becomes the entity's optional [`Span`].

use std::collections::HashMap;

use cce_types::{PluginEntity, Position, Span};
use regex::Regex;
use serde::Deserialize;

/// A pattern declaration as provided by a plugin (e.g. the `plugin.patterns`
/// table of a Lua plugin).
#[derive(Debug, Clone, Deserialize)]
pub struct PatternDeclaration {
    /// Human-readable pattern name (also used for entity ID prefixing).
    pub name: String,
    /// Rust regex (named captures per the convention above).
    pub regex: String,
    /// Entity kind produced by this pattern (defaults to "entity").
    #[serde(default)]
    pub kind: String,
}

/// A compiled pattern ready for extraction.
#[derive(Debug, Clone)]
pub struct CompiledPattern {
    /// Human-readable pattern name.
    pub name: String,
    /// Compiled regex.
    pub regex: Regex,
    /// Entity kind produced by this pattern.
    pub kind: String,
}

impl CompiledPattern {
    /// Compile a declaration.
    pub fn compile(decl: &PatternDeclaration) -> Result<Self, String> {
        let regex =
            Regex::new(&decl.regex).map_err(|e| format!("Invalid regex '{}': {e}", decl.regex))?;
        Ok(Self {
            name: decl.name.clone(),
            regex,
            kind: if decl.kind.is_empty() {
                "entity".to_string()
            } else {
                decl.kind.clone()
            },
        })
    }
}

/// Compile a list of pattern declarations.
pub fn compile_patterns(decls: &[PatternDeclaration]) -> Result<Vec<CompiledPattern>, String> {
    decls.iter().map(CompiledPattern::compile).collect()
}

/// Run all patterns against `content` and collect the resulting entities.
pub fn extract_entities(content: &str, patterns: &[CompiledPattern]) -> Vec<PluginEntity> {
    let mut entities = Vec::new();
    for pattern in patterns {
        let mut match_index = 0usize;
        for caps in pattern.regex.captures_iter(content) {
            let Some(m) = caps.get(0) else {
                continue;
            };
            let name = caps
                .name("name")
                .map(|c| c.as_str().to_string())
                .or_else(|| caps.get(1).map(|c| c.as_str().to_string()))
                .unwrap_or_else(|| m.as_str().to_string());
            let kind = caps
                .name("kind")
                .map(|c| c.as_str().to_string())
                .unwrap_or_else(|| pattern.kind.clone());
            let signature = caps.name("signature").map(|c| c.as_str().to_string());
            let doc_comment = caps.name("doc_comment").map(|c| c.as_str().to_string());

            let mut metadata = HashMap::new();
            for (group_index, group_name) in pattern.regex.capture_names().enumerate() {
                if let Some(capture_name) = group_name {
                    if let Some(rest) = capture_name.strip_prefix("meta_") {
                        if let Some(g) = caps.get(group_index) {
                            metadata.insert(rest.to_string(), g.as_str().to_string());
                        }
                    }
                }
            }

            entities.push(PluginEntity {
                id: format!("{}_{}", pattern.name, match_index),
                kind,
                name,
                signature,
                doc_comment,
                metadata,
                span: Some(compute_span(content, m.start(), m.end())),
                children: Vec::new(),
            });
            match_index += 1;
        }
    }
    entities
}

/// Compute a [`Span`] for the byte range `[start_byte, end_byte)` in `content`.
pub fn compute_span(content: &str, start_byte: usize, end_byte: usize) -> Span {
    Span {
        start_byte,
        end_byte,
        start_position: position_at(content, start_byte),
        end_position: position_at(content, end_byte),
    }
}

/// Compute the (row, column) position of `byte` in `content`.
fn position_at(content: &str, byte: usize) -> Position {
    let byte = byte.min(content.len());
    let prefix = &content[..byte];
    let row = prefix.bytes().filter(|&b| b == b'\n').count();
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    Position {
        row,
        column: byte - line_start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_entities_from_named_captures() {
        let content = "@app.route('/users')\ndef users():\n    pass\n\n@app.route('/items')\ndef items():\n    pass\n";
        let decl = PatternDeclaration {
            name: "route".to_string(),
            regex: r#"@app\.route\('(?P<name>[^']+)'\)(?:\s*\n\s*(?P<signature>def\s+\w+))"#
                .to_string(),
            kind: "route".to_string(),
        };
        let patterns = compile_patterns(&[decl]).unwrap();
        let entities = extract_entities(content, &patterns);
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].name, "/users");
        assert_eq!(entities[0].kind, "route");
        assert_eq!(entities[0].id, "route_0");
        assert!(entities[0].span.is_some());
    }

    #[test]
    fn test_meta_capture_maps_to_metadata() {
        let content = "event(id=42, name='create')";
        let decl = PatternDeclaration {
            name: "event".to_string(),
            regex: r#"event\(id=(?P<meta_id>\d+), name='(?P<name>[^']+)'\)"#.to_string(),
            kind: "event".to_string(),
        };
        let patterns = compile_patterns(&[decl]).unwrap();
        let entities = extract_entities(content, &patterns);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].name, "create");
        assert_eq!(
            entities[0].metadata.get("id").map(String::as_str),
            Some("42")
        );
    }

    #[test]
    fn test_compute_span() {
        let content = "abc\ndef\nghi";
        let span = compute_span(content, 4, 7);
        assert_eq!(span.start_position.row, 1);
        assert_eq!(span.start_position.column, 0);
        assert_eq!(span.end_position.row, 1);
        assert_eq!(span.end_position.column, 3);
    }
}
