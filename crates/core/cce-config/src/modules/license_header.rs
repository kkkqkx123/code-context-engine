//! License header filtering configuration
//!
//! Controls how the parser strips license/copyright notice comments from the
//! file header (the region before the first code entity). Detection uses
//! literal prefix/suffix matching only: a license block starts at a comment
//! line beginning with `prefix` and, when the rule declares a `suffix`, ends
//! at the first line ending with that suffix. If the suffix is never found
//! within the contiguous comment run, the block is kept (miss is preferred
//! over false deletion). Matching is case-insensitive and ignores leading and
//! trailing comment marker characters.

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

/// One license-header matching rule.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct LicenseHeaderRule {
    /// Literal prefix (case-insensitive) of the line that starts a license
    /// block. Required.
    pub prefix: String,
    /// Literal suffix (case-insensitive) of the line that ends the block.
    /// When `None`, the entire contiguous comment run starting at the
    /// matched line is treated as the license block.
    pub suffix: Option<String>,
}

/// License header filtering configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LicenseHeaderConfig {
    /// Whether license header filtering is applied at all.
    pub enabled: bool,
    /// Maximum number of header comments considered per file; bounds the
    /// blast radius of an over-broad rule.
    pub max_comments: usize,
    /// Matching rules, evaluated in declaration order. Providing this list
    /// replaces the built-in default rules entirely.
    pub rules: Vec<LicenseHeaderRule>,
}

impl Default for LicenseHeaderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_comments: 30,
            rules: Self::builtin_rules(),
        }
    }
}

impl LicenseHeaderConfig {
    /// High-confidence built-in rules covering common fixed-format headers.
    /// Deliberately narrow; anything less certain must be user-provided.
    pub fn builtin_rules() -> Vec<LicenseHeaderRule> {
        [
            ("copyright", Some("all rights reserved")),
            ("spdx-license-identifier:", None),
            // MIT: the block ends with "...OR THE USE OR OTHER DEALINGS IN
            // THE SOFTWARE."
            ("copyright", Some("in the software")),
            // Apache: the block ends with "...limitations under the License."
            ("copyright", Some("under the license")),
            // GPL family: the block ends with the "...General Public License
            // for more details." referral line.
            ("copyright", Some("public license for more details")),
        ]
        .into_iter()
        .map(|(prefix, suffix)| LicenseHeaderRule {
            prefix: prefix.to_string(),
            suffix: suffix.map(|s| s.to_string()),
        })
        .collect()
    }
}

impl Validate for LicenseHeaderConfig {
    fn validate_structured(&self) -> ValidationResult {
        if self.max_comments == 0 {
            return Err(ConfigValidationError::invalid_field(
                "license_header.max_comments",
                "must be at least 1",
            ));
        }
        for (idx, rule) in self.rules.iter().enumerate() {
            validate_needle(&rule.prefix).map_err(|e| {
                ConfigValidationError::invalid_field(
                    format!("license_header.rules[{idx}].prefix"),
                    e,
                )
            })?;
            if let Some(suffix) = &rule.suffix {
                validate_needle(suffix).map_err(|e| {
                    ConfigValidationError::invalid_field(
                        format!("license_header.rules[{idx}].suffix"),
                        e,
                    )
                })?;
            }
        }
        Ok(())
    }
}

fn validate_needle(needle: &str) -> Result<(), String> {
    if needle.chars().count() < 4 {
        return Err(format!(
            "literal must be at least 4 characters (got {needle:?}); shorter \
             patterns are too broad for header filtering"
        ));
    }
    if needle.contains(['\n', '\r']) {
        return Err(
            "literal must not contain newlines; prefix/suffix match within a \
             single comment line"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_enabled_with_builtin_rules() {
        let config = LicenseHeaderConfig::default();
        assert!(config.enabled);
        assert!(!config.rules.is_empty());
        assert!(config.validate_structured().is_ok());
    }

    #[test]
    fn rejects_short_needle() {
        let config = LicenseHeaderConfig {
            rules: vec![LicenseHeaderRule {
                prefix: "abc".to_string(),
                suffix: None,
            }],
            ..Default::default()
        };
        assert!(config.validate_structured().is_err());
    }

    #[test]
    fn rejects_multiline_needle() {
        let config = LicenseHeaderConfig {
            rules: vec![LicenseHeaderRule {
                prefix: "line1\nline2".to_string(),
                suffix: None,
            }],
            ..Default::default()
        };
        assert!(config.validate_structured().is_err());
    }

    #[test]
    fn rejects_zero_max_comments() {
        let config = LicenseHeaderConfig {
            max_comments: 0,
            ..Default::default()
        };
        assert!(config.validate_structured().is_err());
    }
}
