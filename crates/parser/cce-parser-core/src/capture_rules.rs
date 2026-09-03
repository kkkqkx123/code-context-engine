//! Structured capture extraction rules
//!
//! Provides a deterministic, table-driven approach to capture selection that
//! replaces the predicate-chain approach in `find_callee_capture` and
//! `find_dependency_capture`.

/// Structured capture extraction rule.
///
/// Defines how to extract a callee or dependency name from a set of captures.
/// Each rule specifies required and optional capture name patterns, and a
/// deterministic extraction function.
pub struct CaptureRule {
    /// Capture name suffixes that MUST be present for this rule to apply.
    pub required: &'static [&'static str],
    /// Capture name suffixes that are optional but used during extraction.
    pub optional: &'static [&'static str],
    /// Deterministic function to extract the name from matching captures.
    pub extract: fn(&[CapturedItem]) -> Option<String>,
}

/// A captured item with its name and text.
#[derive(Debug, Clone)]
pub struct CapturedItem {
    /// Full capture name (e.g., "@entity.function.name")
    pub name: String,
    /// Capture text content
    pub text: String,
}

/// Language-specific capture rules.
///
/// Each language implements this trait to define how captures should be
/// processed for call and dependency extraction.
pub trait LanguageRules {
    /// Get the capture rules for call expressions.
    fn call_rules(&self) -> &[CaptureRule];

    /// Get the capture rules for dependency expressions.
    fn dependency_rules(&self) -> &[CaptureRule];
}

/// Find a capture by suffix match.
pub fn find_capture_by_suffix<'a>(
    captures: &'a [CapturedItem],
    suffix: &str,
) -> Option<&'a CapturedItem> {
    captures.iter().find(|c| c.name.ends_with(suffix))
}

/// Find all captures matching a suffix.
pub fn find_captures_by_suffix<'a>(
    captures: &'a [CapturedItem],
    suffix: &str,
) -> Vec<&'a CapturedItem> {
    captures
        .iter()
        .filter(|c| c.name.ends_with(suffix))
        .collect()
}

/// Apply a capture rule to a set of captures.
///
/// Returns `Some(name)` if all required captures are present and the extraction
/// function succeeds.
pub fn apply_capture_rule(captures: &[CapturedItem], rule: &CaptureRule) -> Option<String> {
    // Check all required captures are present
    for required in rule.required {
        find_capture_by_suffix(captures, required)?;
    }

    // Apply the extraction function
    (rule.extract)(captures)
}

/// Try multiple capture rules in order, returning the first successful result.
pub fn try_capture_rules(captures: &[CapturedItem], rules: &[CaptureRule]) -> Option<String> {
    for rule in rules {
        if let Some(name) = apply_capture_rule(captures, rule) {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_capture_by_suffix() {
        let captures = vec![
            CapturedItem {
                name: "@call.function.name".to_string(),
                text: "foo".to_string(),
            },
            CapturedItem {
                name: "@call.receiver".to_string(),
                text: "obj".to_string(),
            },
        ];

        assert!(find_capture_by_suffix(&captures, ".function.name").is_some());
        assert!(find_capture_by_suffix(&captures, ".receiver").is_some());
        assert!(find_capture_by_suffix(&captures, ".nonexistent").is_none());
    }

    #[test]
    fn test_apply_capture_rule() {
        let captures = vec![CapturedItem {
            name: "@call.function.name".to_string(),
            text: "foo".to_string(),
        }];

        let rule = CaptureRule {
            required: &[".function.name"],
            optional: &[],
            extract: |captures| {
                find_capture_by_suffix(captures, ".function.name").map(|c| c.text.clone())
            },
        };

        assert_eq!(
            apply_capture_rule(&captures, &rule),
            Some("foo".to_string())
        );
    }

    #[test]
    fn test_apply_capture_rule_missing_required() {
        let captures = vec![CapturedItem {
            name: "@call.receiver".to_string(),
            text: "obj".to_string(),
        }];

        let rule = CaptureRule {
            required: &[".function.name"],
            optional: &[],
            extract: |captures| {
                find_capture_by_suffix(captures, ".function.name").map(|c| c.text.clone())
            },
        };

        assert_eq!(apply_capture_rule(&captures, &rule), None);
    }
}
