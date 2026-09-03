//! C/C++ diagnosis strategy
//!
//! Provides C/C++ specific error refinement logic.

use cce_types::position::Position;

use super::ErrorCandidate;
use super::common;
use crate::tools::ast_diagnosis::{Diagnostic, DiagnosticKind, DiagnosticPrecision};

/// Refine error candidates and generate diagnostics (C/C++ strategy)
pub fn refine(candidates: Vec<ErrorCandidate>, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Process each candidate
    for candidate in &candidates {
        // Try C/C++ specific classification first
        if let Some(diagnostic) = classify_error(candidate, source) {
            diagnostics.push(diagnostic);
        } else {
            // Fall back to common strategy
            let diagnostic = common::candidate_to_diagnostic(candidate, source);
            diagnostics.push(diagnostic);
        }
    }

    // Apply C/C++ specific checks
    diagnostics.extend(check_preprocessor(source));

    // Apply common checks
    diagnostics.extend(common::check_bracket_balance(source));
    diagnostics.extend(common::check_string_closure(source));

    // Deduplicate and sort
    common::deduplicate_and_sort(diagnostics)
}

/// Check for preprocessor directive errors
fn check_preprocessor(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_preprocessor = false;
    let mut preprocessor_start: Option<(usize, usize)> = None; // (byte_pos, line)

    let mut line = 0;

    for (byte_pos, ch) in source.char_indices() {
        if ch == '\n' {
            if in_preprocessor {
                // Check if preprocessor directive was complete
                if let Some((start_pos, start_line)) = preprocessor_start {
                    let directive = &source[start_pos..byte_pos];
                    if is_incomplete_preprocessor(directive) {
                        let column = 0;
                        diagnostics.push(Diagnostic::new(
                            DiagnosticKind::PreprocessorError,
                            Position::new(start_line, column),
                            "incomplete preprocessor directive",
                            DiagnosticPrecision::High,
                        ));
                    }
                }
            }
            in_preprocessor = false;
            preprocessor_start = None;
            line += 1;
            continue;
        }

        // Check for preprocessor directive start
        if ch == '#' && (byte_pos == 0 || source.as_bytes()[byte_pos - 1] == b'\n') {
            in_preprocessor = true;
            preprocessor_start = Some((byte_pos, line));
        }
    }

    diagnostics
}

/// Check if a preprocessor directive is incomplete
fn is_incomplete_preprocessor(directive: &str) -> bool {
    let directive = directive.trim();

    // Check for common incomplete patterns
    if directive.starts_with("#include") {
        // Check for missing closing angle bracket or quote
        if directive.contains('<') && !directive.contains('>') {
            return true;
        }
        if directive.contains('"') && directive.matches('"').count() < 2 {
            return true;
        }
    }

    if directive.starts_with("#define") {
        // Check for missing macro name
        let parts: Vec<&str> = directive.split_whitespace().collect();
        if parts.len() < 2 {
            return true;
        }
    }

    if directive.starts_with("#if")
        || directive.starts_with("#ifdef")
        || directive.starts_with("#ifndef")
    {
        // These need corresponding #endif (but we can't easily check this here)
        // Just check if there's something after the directive
        let parts: Vec<&str> = directive.split_whitespace().collect();
        if parts.len() < 2 {
            return true;
        }
    }

    false
}

/// Classify C/C++ specific error types
fn classify_error(candidate: &ErrorCandidate, source: &str) -> Option<Diagnostic> {
    // Check for preprocessor error
    if is_preprocessor_context(candidate, source) {
        return Some(Diagnostic::new(
            DiagnosticKind::PreprocessorError,
            candidate.start,
            "preprocessor directive error",
            DiagnosticPrecision::High,
        ));
    }

    // Check for missing semicolon (common in C/C++)
    if looks_like_missing_semicolon(candidate, source) {
        return Some(Diagnostic::new(
            DiagnosticKind::MissingSemicolon,
            candidate.end,
            "expected ';' after expression",
            DiagnosticPrecision::High,
        ));
    }

    None
}

/// Check if error is in preprocessor context
fn is_preprocessor_context(candidate: &ErrorCandidate, source: &str) -> bool {
    // Check if the line starts with #
    let line_start = source[..candidate.start_byte]
        .rfind('\n')
        .map(|p| p + 1)
        .unwrap_or(0);

    let line_end = source[candidate.start_byte..]
        .find('\n')
        .map(|p| candidate.start_byte + p)
        .unwrap_or(source.len());

    if line_end > line_start {
        let line = &source[line_start..line_end.min(source.len())];
        line.trim().starts_with('#')
    } else {
        false
    }
}

/// Check if error looks like a missing semicolon
fn looks_like_missing_semicolon(candidate: &ErrorCandidate, source: &str) -> bool {
    // Check if the text before the error looks like a complete statement
    if candidate.start_byte == 0 {
        return false;
    }

    let before = &source[..candidate.start_byte];
    let before = before.trim_end();

    // Common patterns that should end with semicolon
    if before.ends_with(')')
        || before.ends_with('}')
        || before.ends_with(']')
        || before.ends_with('"')
        || before.ends_with('\'')
    {
        // Check if it's not a control structure
        let line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line = &before[line_start..];

        // Control structures that don't need semicolon
        if line.contains("if ")
            || line.contains("while ")
            || line.contains("for ")
            || line.contains("switch ")
            || line.starts_with("else")
        {
            return false;
        }

        return true;
    }

    false
}
