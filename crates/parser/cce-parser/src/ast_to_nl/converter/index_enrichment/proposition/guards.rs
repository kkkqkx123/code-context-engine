//! Worthiness guards for proposition rendering.
//!
//! Every guard is a named table plus a predicate: adding coverage means
//! adding a table row or one predicate, never editing the render branches.
//! Removing a guard means deleting its single call site in the renderer.

use super::vocab::{BOILERPLATE_GUARDS, ERROR_MARKERS, NULL_WORDS};

/// Whether a guard condition is template boilerplate rather than intent.
///
/// A Go error guard repeats on nearly every function; rendering it would
/// add a shared sentence to every entity instead of signal.
pub(super) fn is_boilerplate_guard(cond: &str) -> bool {
    BOILERPLATE_GUARDS.contains(&cond.trim())
}

/// Whether an effect fact names an error handling shape.
///
/// Blocks that carry none of these words (such as an unsafe wrapper, which
/// the extractor files under effects for lack of a better home) must not
/// be labeled as error reports.
pub(super) fn mentions_error(line: &str) -> bool {
    ERROR_MARKERS.iter().any(|m| line.contains(m))
}

/// Whether a return expression is a bare status code rather than a value.
///
/// Zero, negative numbers, C error macros and null words all say "failed"
/// without saying what was produced; the paired condition sentence already
/// carries the failure. Non-negative values (`1`, `404`), booleans and
/// symbols stay: they may be the answer, not the status.
pub(super) fn is_bare_error_return(expr: &str) -> bool {
    let expr = expr.trim();
    if expr.is_empty() {
        return false;
    }
    if NULL_WORDS.contains(&expr) {
        return true;
    }
    let (negative, digits) = match expr.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, expr),
    };
    if negative {
        // Any negative number or error macro is a status code, never a value.
        return is_error_macro(digits) || parse_plain_number(digits).is_some();
    }
    // Only zero is bare among non-negative numbers: `1`, `404` and floats
    // may be the answer rather than the status.
    match parse_plain_number(digits) {
        Some(v) => v == 0.0,
        None => false,
    }
}

/// Whether a non-call right-hand side is too vacuous to speak.
///
/// Counter initializations, null stores and empty strings add a sentence
/// but no signal. Values that may carry meaning (timeouts, status codes,
/// string contents, casts over identifiers) are outside this narrow list
/// and stay.
pub(super) fn is_vacuous_rhs(rhs: &str) -> bool {
    let rhs = rhs.trim().trim_end_matches(';').trim();
    if rhs.is_empty() {
        return false;
    }
    if NULL_WORDS.contains(&rhs) {
        return true;
    }
    if rhs == "\"\"" {
        return true;
    }
    let (negative, digits) = match rhs.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, rhs),
    };
    if negative {
        return is_error_macro(digits) || parse_plain_number(digits).is_some();
    }
    match parse_plain_number(digits) {
        Some(v) => v == 0.0 || v == 1.0,
        None => false,
    }
}

/// Whether a condition is a bare truthiness check with no computation.
///
/// A lone identifier, negation or pointer path (`p`, `!ok`, `*tag->name`)
/// names no comparison and no call, so it cannot rank one entity above
/// another. Anything with an operator, a call, a subscript or whitespace
/// beyond the path itself stays.
pub(super) fn is_vacuous_condition(cond: &str) -> bool {
    let mut rest = cond.trim();
    if rest.is_empty() {
        return false;
    }
    rest = rest.trim_start_matches(['!', '*']).trim_start();
    if rest.is_empty() {
        return false;
    }
    let mut chars = rest.chars().peekable();
    let mut seen_ident = false;
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() || c == '_' {
            seen_ident = true;
            continue;
        }
        match c {
            '.' => continue,
            ':' => {
                if chars.peek() == Some(&':') {
                    chars.next();
                    continue;
                }
                return false;
            }
            '-' => {
                if chars.peek() == Some(&'>') {
                    chars.next();
                    continue;
                }
                return false;
            }
            _ => return false,
        }
    }
    seen_ident
}

/// Whether the text is a C-style error macro such as `ENOMEM`.
fn is_error_macro(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    text.chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Parse a plain decimal or hexadecimal number with type suffixes.
///
/// Returns `None` for anything that is not a bare numeric literal, so
/// identifiers, calls and expressions always fall through to kept.
fn parse_plain_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        let hex = hex.trim_end_matches(['u', 'U', 'l', 'L']);
        if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        return i64::from_str_radix(hex, 16).ok().map(|v| v as f64);
    }
    let digits = text.trim_end_matches(['u', 'U', 'l', 'L', 'f', 'F']);
    if digits.is_empty() {
        return None;
    }
    let mut dot = 0;
    for c in digits.chars() {
        if c == '.' {
            dot += 1;
            if dot > 1 {
                return None;
            }
        } else if !c.is_ascii_digit() {
            return None;
        }
    }
    digits.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_bare_but_values_stay() {
        for bare in ["-ENOMEM", "-1", "0", "NULL", "nil", "None", "0.0"] {
            assert!(is_bare_error_return(bare), "bare: {bare}");
        }
        for kept in ["1", "404", "true", "false", "Ok", "hdrlen + n", "x", ""] {
            assert!(!is_bare_error_return(kept), "kept: {kept}");
        }
    }

    #[test]
    fn vacuous_rhs_covers_inits_only() {
        for bare in ["0", "1", "NULL", "nil", "-1", "\"\""] {
            assert!(is_vacuous_rhs(bare), "bare: {bare}");
        }
        for kept in [
            "60000",
            "404",
            "\"onRequest\"",
            "(size_t)(p - field)",
            "cbb",
            "x + 1",
        ] {
            assert!(!is_vacuous_rhs(kept), "kept: {kept}");
        }
    }

    #[test]
    fn vacuous_conditions_are_paths_only() {
        for bare in [
            "p",
            "!ptr1",
            "*tag->name",
            "period->event_streams",
            "a::b",
            "self.x",
        ] {
            assert!(is_vacuous_condition(bare), "bare: {bare}");
        }
        for kept in [
            "err != nil",
            "hdrlen < 10",
            "x in y",
            "mpd && *mpd",
            "f(x)",
            "x[0]",
            "a.b(c)",
        ] {
            assert!(!is_vacuous_condition(kept), "kept: {kept}");
        }
    }

    #[test]
    fn error_shapes_still_report_but_with_stays_silent() {
        assert!(mentions_error("throw ConfigError;"));
        assert!(mentions_error("try {"));
        assert!(!mentions_error("with open(path) as f:"));
    }

    #[test]
    fn repeated_error_returns_all_drop() {
        let mut dropped = 0;
        for _ in 0..27 {
            if is_bare_error_return("-ENOMEM") {
                dropped += 1;
            }
        }
        assert_eq!(dropped, 27, "every repeated error code must drop");
    }
}
