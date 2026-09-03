//! JavaScript/TypeScript diagnosis strategy
//!
//! Provides JavaScript/TypeScript specific error refinement logic.

use cce_types::language::Language;
use cce_types::position::Position;

use super::ErrorCandidate;
use super::common;
use crate::tools::ast_diagnosis::{Diagnostic, DiagnosticKind, DiagnosticPrecision};

/// Refine error candidates and generate diagnostics (JavaScript/TypeScript strategy)
pub fn refine(
    candidates: Vec<ErrorCandidate>,
    source: &str,
    language: &Language,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Process each candidate
    for candidate in &candidates {
        // Try JS specific classification first
        if let Some(diagnostic) = classify_error(candidate, source) {
            diagnostics.push(diagnostic);
        } else {
            // Fall back to common strategy
            let diagnostic = common::candidate_to_diagnostic(candidate, source);
            diagnostics.push(diagnostic);
        }
    }

    // Apply JS specific checks
    diagnostics.extend(check_js_patterns(source));

    // Check JSX syntax for TypeScript
    if *language == Language::TypeScript {
        diagnostics.extend(check_jsx_syntax(source));
    }

    // Apply common checks
    diagnostics.extend(common::check_bracket_balance(source));
    diagnostics.extend(common::check_string_closure(source));

    // Deduplicate and sort
    common::deduplicate_and_sort(diagnostics)
}

/// Check for JavaScript-specific patterns
fn check_js_patterns(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Check for common JS/TS issues
    diagnostics.extend(check_template_literal_syntax(source));

    diagnostics
}

/// Check for template literal syntax issues
fn check_template_literal_syntax(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_template = false;
    let mut template_start: Option<(usize, usize)> = None; // (byte_pos, line)
    let mut brace_depth = 0;

    let mut line = 0;

    for (byte_pos, ch) in source.char_indices() {
        if ch == '\n' {
            line += 1;
            continue;
        }

        // Check for backtick (template literal)
        if ch == '`' && brace_depth == 0 {
            if in_template {
                // Closing backtick
                in_template = false;
                template_start = None;
            } else {
                // Opening backtick
                in_template = true;
                template_start = Some((byte_pos, line));
            }
        }

        // Track brace depth for template expressions
        if in_template {
            if ch == '{' {
                // Check if preceded by $
                if byte_pos > 0 && source.as_bytes()[byte_pos - 1] == b'$' {
                    brace_depth += 1;
                }
            } else if ch == '}' && brace_depth > 0 {
                brace_depth -= 1;
            }
        }
    }

    // Report unclosed template literals
    if let Some((byte_pos, template_line)) = template_start {
        let column = byte_pos - source[..byte_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);

        diagnostics.push(Diagnostic::new(
            DiagnosticKind::UnclosedString,
            Position::new(template_line, column),
            "unclosed template literal, expected '`'",
            DiagnosticPrecision::High,
        ));
    }

    diagnostics
}

/// Check for JSX/TSX specific syntax (if applicable)
fn check_jsx_syntax(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut jsx_tag_stack: Vec<(String, usize, usize)> = Vec::new(); // (tag_name, byte_pos, line)

    let mut line = 0;

    let mut chars = source.char_indices().peekable();
    while let Some((byte_pos, ch)) = chars.next() {
        if ch == '\n' {
            line += 1;
            continue;
        }

        // Simple JSX tag detection (this is a simplified check)
        if ch == '<' {
            // Check if this is a JSX tag
            if let Some(&(_, next_ch)) = chars.peek() {
                if next_ch.is_alphabetic() || next_ch == '_' || next_ch == '.' {
                    // This might be a JSX opening tag
                    // Extract tag name
                    let mut tag_name = String::new();
                    while let Some(&(_, c)) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                            tag_name.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    // Check if it's a self-closing tag
                    let mut is_self_closing = false;
                    while let Some(&(_, c)) = chars.peek() {
                        if c == '/' {
                            chars.next();
                            if let Some(&(_, '>')) = chars.peek() {
                                is_self_closing = true;
                                chars.next();
                                break;
                            }
                        } else if c == '>' {
                            chars.next();
                            break;
                        } else if c.is_whitespace() {
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    if !is_self_closing && !tag_name.is_empty() {
                        jsx_tag_stack.push((tag_name, byte_pos, line));
                    }
                } else if next_ch == '/' {
                    // Closing tag
                    chars.next(); // consume '/'
                    let mut tag_name = String::new();
                    while let Some(&(_, c)) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                            tag_name.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    // Check if it matches the last opening tag
                    if let Some((open_tag, _, _)) = jsx_tag_stack.last() {
                        if open_tag == &tag_name {
                            jsx_tag_stack.pop();
                        } else {
                            // Mismatched tags
                            let column = byte_pos
                                - source[..byte_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
                            diagnostics.push(Diagnostic::new(
                                DiagnosticKind::SyntaxError,
                                Position::new(line, column),
                                format!(
                                    "JSX closing tag '{}' does not match opening tag '{}'",
                                    tag_name, open_tag
                                ),
                                DiagnosticPrecision::High,
                            ));
                        }
                    }
                }
            }
        }
    }

    // Report unclosed JSX tags
    for (tag_name, byte_pos, tag_line) in jsx_tag_stack {
        let column = byte_pos - source[..byte_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
        diagnostics.push(Diagnostic::new(
            DiagnosticKind::UnclosedBrace,
            Position::new(tag_line, column),
            format!("unclosed JSX tag '{}'", tag_name),
            DiagnosticPrecision::Medium,
        ));
    }

    diagnostics
}

/// Classify JavaScript-specific error types
fn classify_error(candidate: &ErrorCandidate, source: &str) -> Option<Diagnostic> {
    // Check for missing semicolon (optional in JS, but can indicate issues)
    if looks_like_missing_semicolon(candidate, source) {
        // In JavaScript, semicolons are optional, so this is less critical
        // But it can still indicate issues in some cases
        return Some(Diagnostic::new(
            DiagnosticKind::MissingSemicolon,
            candidate.end,
            "expected ';' (automatic semicolon insertion may apply)",
            DiagnosticPrecision::Medium,
        ));
    }

    None
}

/// Check if error looks like a missing semicolon
fn looks_like_missing_semicolon(candidate: &ErrorCandidate, source: &str) -> bool {
    if candidate.start_byte == 0 {
        return false;
    }

    let before = &source[..candidate.start_byte];
    let before = before.trim_end();

    // In JS, statements that might need semicolons
    if before.ends_with(')')
        || before.ends_with(']')
        || before.ends_with('"')
        || before.ends_with('\'')
        || before.ends_with('`')
    {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_template_literal_valid() {
        let source = r#"const s = `hello`;"#;

        let diagnostics = check_template_literal_syntax(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_check_template_literal_unclosed() {
        let source = r#"const s = `hello"#;

        let diagnostics = check_template_literal_syntax(source);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::UnclosedString)
        );
    }

    #[test]
    fn test_check_template_literal_with_expression() {
        let source = r#"const s = `hello ${name}`;"#;

        let diagnostics = check_template_literal_syntax(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_refine_javascript_code() {
        let source = "function hello() { console.log('hello'); }";

        let candidates = vec![];
        let diagnostics = refine(candidates, source, &Language::JavaScript);

        // Should be valid
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_refine_typescript_code() {
        let source = "const hello: string = 'hello';";

        let candidates = vec![];
        let diagnostics = refine(candidates, source, &Language::TypeScript);

        // Should be valid
        assert!(diagnostics.is_empty());
    }
}
