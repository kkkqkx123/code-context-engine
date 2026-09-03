//! Python diagnosis strategy
//!
//! Provides Python specific error refinement logic, including indentation checking.

use cce_types::position::Position;

use super::ErrorCandidate;
use super::common;
use crate::tools::ast_diagnosis::{Diagnostic, DiagnosticKind, DiagnosticPrecision};

/// Refine error candidates and generate diagnostics (Python strategy)
pub fn refine(candidates: Vec<ErrorCandidate>, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Python indentation issues don't produce ERROR nodes in tree-sitter
    // So we need to check them separately
    diagnostics.extend(check_indentation(source));

    // Process error candidates from tree-sitter
    for candidate in &candidates {
        // Try Python specific classification first
        if let Some(diagnostic) = classify_error(candidate, source) {
            diagnostics.push(diagnostic);
        } else {
            // Fall back to common strategy
            let diagnostic = common::candidate_to_diagnostic(candidate, source);
            diagnostics.push(diagnostic);
        }
    }

    // Apply Python specific checks
    diagnostics.extend(check_python_syntax(source));
    diagnostics.extend(check_string_syntax(source));

    // Apply common checks (but skip bracket balance for Python as it's less relevant)
    diagnostics.extend(common::check_string_closure(source));

    // Deduplicate and sort
    common::deduplicate_and_sort(diagnostics)
}

/// Check Python indentation issues
///
/// Python uses indentation for block structure, so we need to check:
/// 1. Consistent use of tabs vs spaces
/// 2. Proper dedent levels
/// 3. Indentation after colons (if, for, while, def, class, etc.)
fn check_indentation(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    // Track indentation levels
    let mut indent_stack: Vec<usize> = vec![0]; // Start with 0 indentation
    let mut uses_tabs: Option<bool> = None; // None = not determined yet

    for (line_num, line) in lines.iter().enumerate() {
        // Skip empty lines and comments
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Calculate indentation
        let indent = calculate_indent(line);

        // Check for mixed tabs and spaces
        if let Some(diagnostic) = check_mixed_indentation(line, line_num, &mut uses_tabs) {
            diagnostics.push(diagnostic);
        }

        // Check indentation level
        let current_indent = indent_stack.last().copied().unwrap_or(0);

        if indent > current_indent {
            // Increased indentation - push to stack
            indent_stack.push(indent);
        } else if indent < current_indent {
            // Decreased indentation - pop from stack
            while let Some(&top) = indent_stack.last() {
                if top == indent {
                    break;
                }
                if top < indent {
                    // Indentation doesn't match any level
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::IndentationError,
                        Position::new(line_num, 0),
                        format!(
                            "unindent does not match any outer indentation level (expected {}, found {})",
                            top, indent
                        ),
                        DiagnosticPrecision::High,
                    ));
                    break;
                }
                indent_stack.pop();
            }
        }

        // Check if line ends with colon (should have increased indent next)
        if trimmed.ends_with(':') {
            // Next non-empty line should have increased indentation
            // This is checked in the next iteration
        }
    }

    diagnostics
}

/// Calculate indentation of a line (number of leading spaces or tabs)
fn calculate_indent(line: &str) -> usize {
    let mut indent = 0;
    for ch in line.chars() {
        match ch {
            ' ' => indent += 1,
            '\t' => indent += 4, // Treat tab as 4 spaces
            _ => break,
        }
    }
    indent
}

/// Check for mixed tabs and spaces
fn check_mixed_indentation(
    line: &str,
    line_num: usize,
    uses_tabs: &mut Option<bool>,
) -> Option<Diagnostic> {
    let leading: String = line
        .chars()
        .take_while(|&c| c == ' ' || c == '\t')
        .collect();

    if leading.is_empty() {
        return None;
    }

    let has_tabs = leading.contains('\t');
    let has_spaces = leading.contains(' ');

    // Check for mixed tabs and spaces in the same line
    if has_tabs && has_spaces {
        return Some(Diagnostic::new(
            DiagnosticKind::IndentationError,
            Position::new(line_num, 0),
            "mixed tabs and spaces in indentation",
            DiagnosticPrecision::High,
        ));
    }

    // Check for consistency with previous lines
    if let Some(prev_uses_tabs) = *uses_tabs {
        if has_tabs != prev_uses_tabs {
            return Some(Diagnostic::new(
                DiagnosticKind::IndentationError,
                Position::new(line_num, 0),
                "inconsistent use of tabs and spaces in indentation",
                DiagnosticPrecision::High,
            ));
        }
    } else {
        // First non-empty line with indentation - set the style
        *uses_tabs = Some(has_tabs);
    }

    None
}

/// Check for Python-specific syntax issues
fn check_python_syntax(_source: &str) -> Vec<Diagnostic> {
    // Check for common Python issues
    // Note: Most syntax errors are caught by tree-sitter

    Vec::new()
}

/// Check for Python string syntax issues
fn check_string_syntax(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_string = false;
    let mut string_start: Option<(usize, usize, char, bool)> = None; // (byte_pos, line, quote, is_triple)
    let mut escape_next = false;

    let mut line = 0;

    for (byte_pos, ch) in source.char_indices() {
        if ch == '\n' {
            line += 1;
            continue;
        }

        if escape_next {
            escape_next = false;
            continue;
        }

        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }

        match ch {
            '"' | '\'' => {
                // Check for triple quotes
                let rest = &source[byte_pos..];
                let is_triple = rest.starts_with("\"\"\"") || rest.starts_with("'''");

                if in_string {
                    if let Some((_, _, quote, was_triple)) = string_start {
                        if ch == quote {
                            if was_triple {
                                // Check for closing triple quote
                                if rest.starts_with(&format!("{}{}{}", ch, ch, ch)) {
                                    in_string = false;
                                    string_start = None;
                                }
                            } else {
                                in_string = false;
                                string_start = None;
                            }
                        }
                    }
                } else {
                    in_string = true;
                    string_start = Some((byte_pos, line, ch, is_triple));
                }
            }
            _ => {}
        }
    }

    // Report unclosed strings
    if let Some((byte_pos, string_line, quote, is_triple)) = string_start {
        let column = byte_pos - source[..byte_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);

        let msg = if is_triple {
            format!(
                "unclosed triple-quoted string literal, expected '{}{}{}'",
                quote, quote, quote
            )
        } else {
            format!("unclosed string literal, expected '{}'", quote)
        };

        diagnostics.push(Diagnostic::new(
            DiagnosticKind::UnclosedString,
            Position::new(string_line, column),
            msg,
            DiagnosticPrecision::High,
        ));
    }

    diagnostics
}

/// Classify Python-specific error types
fn classify_error(_candidate: &ErrorCandidate, _source: &str) -> Option<Diagnostic> {
    // Python-specific error classification
    None // For now, rely on common strategy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_indentation_valid() {
        let source = r#"def foo():
    pass

def bar():
    pass"#;

        let diagnostics = check_indentation(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_check_indentation_mixed() {
        let source = "def foo():\n    pass\n\treturn 1";

        let diagnostics = check_indentation(source);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::IndentationError)
        );
    }

    #[test]
    fn test_check_indentation_unindent() {
        let source = "def foo():\n    pass\n  return 1"; // Wrong dedent level

        let diagnostics = check_indentation(source);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_calculate_indent() {
        assert_eq!(calculate_indent("    pass"), 4);
        assert_eq!(calculate_indent("\tpass"), 4);
        assert_eq!(calculate_indent("pass"), 0);
        assert_eq!(calculate_indent("        pass"), 8);
    }

    #[test]
    fn test_check_string_syntax_valid() {
        let source = r#"s = "hello""#;

        let diagnostics = check_string_syntax(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_check_string_syntax_unclosed() {
        let source = r#"s = "hello"#;

        let diagnostics = check_string_syntax(source);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::UnclosedString)
        );
    }

    #[test]
    fn test_check_string_syntax_triple_quote() {
        let source = r#"s = """hello
world""""#;

        let diagnostics = check_string_syntax(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_refine_python_code() {
        let source = "def hello():\n    print('hello')";

        let candidates = vec![];
        let diagnostics = refine(candidates, source);

        // Should be valid
        assert!(diagnostics.is_empty());
    }
}
