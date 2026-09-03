//! Rust diagnosis strategy
//!
//! Provides Rust specific error refinement logic.

use cce_types::position::Position;

use super::ErrorCandidate;
use super::common;
use crate::tools::ast_diagnosis::{Diagnostic, DiagnosticKind, DiagnosticPrecision};

/// Refine error candidates and generate diagnostics (Rust strategy)
pub fn refine(candidates: Vec<ErrorCandidate>, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Process each candidate
    for candidate in &candidates {
        // Try Rust specific classification first
        if let Some(diagnostic) = classify_error(candidate, source) {
            diagnostics.push(diagnostic);
        } else {
            // Fall back to common strategy
            let diagnostic = common::candidate_to_diagnostic(candidate, source);
            diagnostics.push(diagnostic);
        }
    }

    // Apply Rust specific checks
    diagnostics.extend(check_rust_patterns(source));
    diagnostics.extend(check_macro_syntax(source));

    // Apply common checks
    diagnostics.extend(common::check_bracket_balance(source));
    diagnostics.extend(common::check_string_closure(source));

    // Deduplicate and sort
    common::deduplicate_and_sort(diagnostics)
}

/// Check for Rust-specific patterns
fn check_rust_patterns(_source: &str) -> Vec<Diagnostic> {
    // Check for common Rust-specific issues
    // Note: Lifetime and macro syntax checks are complex and may produce false positives
    // For now, we rely on tree-sitter to catch most errors

    Vec::new()
}

/// Check for macro syntax issues
fn check_macro_syntax(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut line = 0;
    let mut line_start = 0;

    for (byte_pos, ch) in source.char_indices() {
        if ch == '\n' {
            line += 1;
            line_start = byte_pos + 1;
            continue;
        }

        // Check for macro invocation patterns
        if ch == '!' {
            // Check if this is a macro invocation
            if byte_pos > 0 {
                let before = &source[..byte_pos];
                // Find the identifier before !
                if let Some(ident_end) = before.rfind(|c: char| !c.is_alphanumeric() && c != '_') {
                    let ident = &before[ident_end + 1..];
                    if !ident.is_empty() && ident.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        // This is a macro invocation, check if followed by proper syntax
                        let after = &source[byte_pos + 1..];
                        let after = after.trim_start();
                        if !after.starts_with('(')
                            && !after.starts_with('[')
                            && !after.starts_with('{')
                        {
                            // Macro invocation without proper delimiter
                            diagnostics.push(Diagnostic::new(
                                DiagnosticKind::SyntaxError,
                                Position::new(line, byte_pos - line_start + 1),
                                format!("macro `{}` invocation requires delimiters", ident),
                                DiagnosticPrecision::High,
                            ));
                        }
                    }
                }
            }
        }
    }

    diagnostics
}

/// Classify Rust-specific error types
fn classify_error(candidate: &ErrorCandidate, source: &str) -> Option<Diagnostic> {
    // Check for missing semicolon
    if looks_like_missing_semicolon(candidate, source) {
        return Some(Diagnostic::new(
            DiagnosticKind::MissingSemicolon,
            candidate.end,
            "expected `;` after expression",
            DiagnosticPrecision::High,
        ));
    }

    // Check for unclosed string (Rust uses " for strings)
    if looks_like_unclosed_string(candidate) {
        return Some(Diagnostic::new(
            DiagnosticKind::UnclosedString,
            candidate.start,
            "unclosed string literal",
            DiagnosticPrecision::High,
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

    // Rust statements that should end with semicolon
    // (excluding blocks, if/while/for/loop/match expressions)
    if before.ends_with(')')
        || before.ends_with('}')
        || before.ends_with(']')
        || before.ends_with('"')
    {
        // Check if it's not a block expression
        let line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line = &before[line_start..];

        // These don't need semicolons when used as expressions
        if line.contains("if ")
            || line.contains("while ")
            || line.contains("for ")
            || line.contains("loop ")
            || line.contains("match ")
            || line.trim().starts_with("else")
        {
            return false;
        }

        return true;
    }

    false
}

/// Check if error looks like an unclosed string
fn looks_like_unclosed_string(candidate: &ErrorCandidate) -> bool {
    if let Some(ref text) = candidate.text {
        if text.starts_with('"') && !text.ends_with('"') {
            return true;
        }
    }
    false
}
