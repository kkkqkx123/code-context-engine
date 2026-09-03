//! Common diagnosis strategy
//!
//! Provides generic error refinement logic applicable to most languages.

use cce_types::position::{Position, Span};

use super::ErrorCandidate;
use crate::tools::ast_diagnosis::{
    Diagnostic, DiagnosticKind, DiagnosticPrecision, ErrorCandidateKind,
};

/// Refine error candidates and generate diagnostics (common strategy)
///
/// This is the baseline refinement logic applicable to most languages.
pub fn refine(candidates: Vec<ErrorCandidate>, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Convert error candidates to diagnostics
    for candidate in &candidates {
        let diagnostic = candidate_to_diagnostic(candidate, source);
        diagnostics.push(diagnostic);
    }

    // Apply common checks
    diagnostics.extend(check_bracket_balance(source));
    diagnostics.extend(check_string_closure(source));

    // Deduplicate and sort
    deduplicate_and_sort(diagnostics)
}

/// Convert an error candidate to a diagnostic
pub fn candidate_to_diagnostic(candidate: &ErrorCandidate, source: &str) -> Diagnostic {
    let kind = infer_diagnostic_kind(candidate, source);
    let precision = infer_precision(candidate);
    let message = generate_message(&kind, candidate);

    let span = Span::new(
        candidate.start_byte,
        candidate.end_byte,
        candidate.start.row,
        candidate.start.column,
        candidate.end.row,
        candidate.end.column,
    );

    Diagnostic::new(kind, candidate.start, message, precision).with_span(span)
}

/// Infer diagnostic kind from error candidate
fn infer_diagnostic_kind(candidate: &ErrorCandidate, _source: &str) -> DiagnosticKind {
    // Check for specific patterns based on node kind
    if let Some(ref node_kind) = candidate.node_kind {
        match node_kind.as_str() {
            "string_literal" | "string" => return DiagnosticKind::UnclosedString,
            "\"" | "'" => return DiagnosticKind::UnclosedString,
            _ => {}
        }
    }

    // Check text content for patterns
    if let Some(ref text) = candidate.text {
        // Check for unclosed string
        if (text.starts_with('"') || text.starts_with('\''))
            && !text.ends_with('"')
            && !text.ends_with('\'')
        {
            return DiagnosticKind::UnclosedString;
        }

        // Check for unclosed brackets
        let open_brackets: usize = text.chars().filter(|&c| c == '{').count();
        let close_brackets: usize = text.chars().filter(|&c| c == '}').count();
        if open_brackets > close_brackets {
            return DiagnosticKind::UnclosedBrace;
        }

        let open_parens: usize = text.chars().filter(|&c| c == '(').count();
        let close_parens: usize = text.chars().filter(|&c| c == ')').count();
        if open_parens > close_parens {
            return DiagnosticKind::UnclosedParenthesis;
        }

        let open_brackets_sq: usize = text.chars().filter(|&c| c == '[').count();
        let close_brackets_sq: usize = text.chars().filter(|&c| c == ']').count();
        if open_brackets_sq > close_brackets_sq {
            return DiagnosticKind::UnclosedBracket;
        }
    }

    // Check for missing token
    if candidate.kind == ErrorCandidateKind::Missing {
        return DiagnosticKind::SyntaxError;
    }

    // Default to syntax error
    DiagnosticKind::SyntaxError
}

/// Infer positioning precision from error candidate
fn infer_precision(candidate: &ErrorCandidate) -> DiagnosticPrecision {
    // Small ERROR nodes (less than 20 bytes) are high precision
    if candidate.len() <= 20 {
        return DiagnosticPrecision::High;
    }

    // Medium sized ERROR nodes (20-100 bytes) are medium precision
    if candidate.len() <= 100 {
        return DiagnosticPrecision::Medium;
    }

    // Large ERROR nodes are low precision
    DiagnosticPrecision::Low
}

/// Generate human-readable error message
fn generate_message(kind: &DiagnosticKind, candidate: &ErrorCandidate) -> String {
    match kind {
        DiagnosticKind::MissingSemicolon => "expected ';' after expression".to_string(),
        DiagnosticKind::UnclosedString => "unclosed string literal".to_string(),
        DiagnosticKind::UnclosedParenthesis => "unclosed parenthesis, expected ')'".to_string(),
        DiagnosticKind::UnclosedBrace => "unclosed brace, expected '}'".to_string(),
        DiagnosticKind::UnclosedBracket => "unclosed bracket, expected ']'".to_string(),
        DiagnosticKind::IncompleteExpression => "incomplete expression".to_string(),
        DiagnosticKind::IllegalToken => "illegal token".to_string(),
        DiagnosticKind::IncompleteDeclaration => "incomplete declaration".to_string(),
        DiagnosticKind::PreprocessorError => "preprocessor directive error".to_string(),
        DiagnosticKind::IndentationError => "indentation error".to_string(),
        DiagnosticKind::SyntaxError => {
            if let Some(ref text) = candidate.text {
                if text.len() > 50 {
                    format!("syntax error near '{}...'", &text[..50])
                } else {
                    format!("syntax error near '{}'", text)
                }
            } else {
                "syntax error".to_string()
            }
        }
    }
}

/// Check bracket balance in source code
///
/// Returns diagnostics for unbalanced brackets.
pub fn check_bracket_balance(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut stack: Vec<(char, usize, usize)> = Vec::new(); // (char, byte_pos, line)

    let mut line = 0;
    let mut line_start = 0;

    for (byte_pos, ch) in source.char_indices() {
        if ch == '\n' {
            line += 1;
            line_start = byte_pos + 1;
            continue;
        }

        match ch {
            '(' | '{' | '[' => {
                stack.push((ch, byte_pos, line));
            }
            ')' => {
                if let Some((open, _, _)) = stack.last() {
                    if *open == '(' {
                        stack.pop();
                    } else {
                        // Mismatched closing
                        diagnostics.push(Diagnostic::new(
                            DiagnosticKind::UnclosedParenthesis,
                            Position::new(line, byte_pos - line_start),
                            "unexpected ')', no matching '('",
                            DiagnosticPrecision::High,
                        ));
                    }
                } else {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::UnclosedParenthesis,
                        Position::new(line, byte_pos - line_start),
                        "unexpected ')', no matching '('",
                        DiagnosticPrecision::High,
                    ));
                }
            }
            '}' => {
                if let Some((open, _, _)) = stack.last() {
                    if *open == '{' {
                        stack.pop();
                    } else {
                        diagnostics.push(Diagnostic::new(
                            DiagnosticKind::UnclosedBrace,
                            Position::new(line, byte_pos - line_start),
                            "unexpected '}', no matching '{'",
                            DiagnosticPrecision::High,
                        ));
                    }
                } else {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::UnclosedBrace,
                        Position::new(line, byte_pos - line_start),
                        "unexpected '}', no matching '{'",
                        DiagnosticPrecision::High,
                    ));
                }
            }
            ']' => {
                if let Some((open, _, _)) = stack.last() {
                    if *open == '[' {
                        stack.pop();
                    } else {
                        diagnostics.push(Diagnostic::new(
                            DiagnosticKind::UnclosedBracket,
                            Position::new(line, byte_pos - line_start),
                            "unexpected ']', no matching '['",
                            DiagnosticPrecision::High,
                        ));
                    }
                } else {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::UnclosedBracket,
                        Position::new(line, byte_pos - line_start),
                        "unexpected ']', no matching '['",
                        DiagnosticPrecision::High,
                    ));
                }
            }
            _ => {}
        }
    }

    // Report unclosed brackets
    for (ch, byte_pos, bracket_line) in stack {
        let kind = match ch {
            '(' => DiagnosticKind::UnclosedParenthesis,
            '{' => DiagnosticKind::UnclosedBrace,
            '[' => DiagnosticKind::UnclosedBracket,
            _ => continue,
        };

        let column = byte_pos - source[..byte_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);

        diagnostics.push(Diagnostic::new(
            kind,
            Position::new(bracket_line, column),
            format!("unclosed '{}', expected closing bracket", ch),
            DiagnosticPrecision::Medium,
        ));
    }

    diagnostics
}

/// Check for unclosed strings in source code
pub fn check_string_closure(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_string = false;
    let mut string_start: Option<(usize, usize, char)> = None; // (byte_pos, line, quote)
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
                if in_string {
                    if let Some((_, _, quote)) = string_start {
                        if ch == quote {
                            in_string = false;
                            string_start = None;
                        }
                    }
                } else {
                    in_string = true;
                    string_start = Some((byte_pos, line, ch));
                }
            }
            _ => {}
        }
    }

    // Report unclosed strings
    if let Some((byte_pos, string_line, quote)) = string_start {
        let column = byte_pos - source[..byte_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);

        diagnostics.push(Diagnostic::new(
            DiagnosticKind::UnclosedString,
            Position::new(string_line, column),
            format!("unclosed string literal, expected '{}'", quote),
            DiagnosticPrecision::High,
        ));
    }

    diagnostics
}

/// Deduplicate and sort diagnostics
pub fn deduplicate_and_sort(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    // Sort by position
    diagnostics.sort_by(|a, b| match a.position.row.cmp(&b.position.row) {
        std::cmp::Ordering::Equal => a.position.column.cmp(&b.position.column),
        other => other,
    });

    // Remove duplicates (same position and kind)
    diagnostics.dedup_by(|a, b| {
        a.position.row == b.position.row
            && a.position.column == b.position.column
            && a.kind == b.kind
    });

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_diagnostic_kind_unclosed_string() {
        let candidate = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 10),
            start_byte: 0,
            end_byte: 10,
            text: Some("\"unclosed".to_string()),
            node_kind: Some("string_literal".to_string()),
        };

        let kind = infer_diagnostic_kind(&candidate, "\"unclosed");
        assert_eq!(kind, DiagnosticKind::UnclosedString);
    }

    #[test]
    fn test_infer_precision() {
        let small = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 10),
            start_byte: 0,
            end_byte: 10,
            text: None,
            node_kind: None,
        };
        assert_eq!(infer_precision(&small), DiagnosticPrecision::High);

        let medium = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 50),
            start_byte: 0,
            end_byte: 50,
            text: None,
            node_kind: None,
        };
        assert_eq!(infer_precision(&medium), DiagnosticPrecision::Medium);

        let large = ErrorCandidate {
            kind: ErrorCandidateKind::Error,
            start: Position::new(0, 0),
            end: Position::new(0, 150),
            start_byte: 0,
            end_byte: 150,
            text: None,
            node_kind: None,
        };
        assert_eq!(infer_precision(&large), DiagnosticPrecision::Low);
    }

    #[test]
    fn test_check_bracket_balance_valid() {
        let source = "fn main() { let x = [1, 2, 3]; }";

        let diagnostics = check_bracket_balance(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_check_bracket_balance_unclosed_brace() {
        let source = "fn main() {";

        let diagnostics = check_bracket_balance(source);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::UnclosedBrace)
        );
    }

    #[test]
    fn test_check_string_closure_valid() {
        let source = r#"let s = "hello";"#;

        let diagnostics = check_string_closure(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_check_string_closure_unclosed() {
        let source = r#"let s = "hello"#;

        let diagnostics = check_string_closure(source);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::UnclosedString)
        );
    }

    #[test]
    fn test_deduplicate_and_sort() {
        let d1 = Diagnostic::new(
            DiagnosticKind::SyntaxError,
            Position::new(2, 0),
            "error 1",
            DiagnosticPrecision::High,
        );

        let d2 = Diagnostic::new(
            DiagnosticKind::SyntaxError,
            Position::new(0, 5),
            "error 2",
            DiagnosticPrecision::High,
        );

        let d3 = Diagnostic::new(
            DiagnosticKind::SyntaxError,
            Position::new(0, 5),
            "error 3",
            DiagnosticPrecision::High,
        );

        let diagnostics = vec![d1, d2, d3];
        let result = deduplicate_and_sort(diagnostics);

        // Should be sorted and deduplicated
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].position.row, 0);
        assert_eq!(result[1].position.row, 2);
    }
}
