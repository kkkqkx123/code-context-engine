//! Exclude rules for filtering files during hot update
//!
//! This module provides flexible file exclusion based on:
//! - Path patterns (glob matching)
//! - File size limits
//! - Language types
//! - Composite conditions (AND logic)

use serde::{Deserialize, Serialize};

use cce_scanner::FileEntry;
use cce_utils::glob::Glob;

/// Exclude rule type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExcludeRuleType {
    /// Path pattern matching (glob)
    PathPattern {
        /// Glob pattern (e.g., "**/*.pb.rs")
        pattern: String,
    },
    /// File size filter
    FileSize {
        /// Maximum file size in bytes
        max_bytes: u64,
    },
    /// Language filter
    Language {
        /// List of languages to exclude
        languages: Vec<String>,
    },
    /// Composite rule (AND logic)
    Composite {
        /// List of sub-conditions (all must match)
        conditions: Vec<ExcludeRule>,
    },
}

/// Single exclude rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExcludeRule {
    /// Rule type and parameters
    #[serde(flatten)]
    pub rule_type: ExcludeRuleType,
}

impl ExcludeRule {
    /// Create a path pattern rule
    pub fn path_pattern(pattern: impl Into<String>) -> Self {
        Self {
            rule_type: ExcludeRuleType::PathPattern {
                pattern: pattern.into(),
            },
        }
    }

    /// Create a file size rule
    pub fn file_size(max_bytes: u64) -> Self {
        Self {
            rule_type: ExcludeRuleType::FileSize { max_bytes },
        }
    }

    /// Create a language rule
    pub fn language(languages: Vec<String>) -> Self {
        Self {
            rule_type: ExcludeRuleType::Language { languages },
        }
    }

    /// Create a composite rule
    pub fn composite(conditions: Vec<ExcludeRule>) -> Self {
        Self {
            rule_type: ExcludeRuleType::Composite { conditions },
        }
    }
}

/// Exclude rules configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludeRulesConfig {
    /// Enable exclude rules
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// List of exclude rules
    #[serde(default)]
    pub rules: Vec<ExcludeRule>,
}

fn default_enabled() -> bool {
    true
}

impl Default for ExcludeRulesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: vec![],
        }
    }
}

/// Exclude rules engine with pre-compiled glob patterns
pub struct ExcludeRules {
    config: ExcludeRulesConfig,
    compiled_globs: Vec<(String, Glob)>, // (pattern, compiled_glob)
}

impl std::fmt::Debug for ExcludeRules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExcludeRules")
            .field("enabled", &self.config.enabled)
            .field("rule_count", &self.config.rules.len())
            .finish()
    }
}

impl ExcludeRules {
    /// Create new exclude rules from config
    pub fn new(config: ExcludeRulesConfig) -> Result<Self, String> {
        let mut compiled_globs = Vec::new();

        // Pre-compile all glob patterns
        for rule in &config.rules {
            if let ExcludeRuleType::PathPattern { pattern } = &rule.rule_type {
                let glob = Glob::new(pattern)
                    .map_err(|e| format!("Failed to compile glob pattern '{}': {}", pattern, e))?;
                compiled_globs.push((pattern.clone(), glob));
            } else if let ExcludeRuleType::Composite { conditions } = &rule.rule_type {
                // Compile globs in composite rules
                for cond in conditions {
                    if let ExcludeRuleType::PathPattern { pattern } = &cond.rule_type {
                        let glob = Glob::new(pattern).map_err(|e| {
                            format!("Failed to compile glob pattern '{}': {}", pattern, e)
                        })?;
                        compiled_globs.push((pattern.clone(), glob));
                    }
                }
            }
        }

        Ok(Self {
            config,
            compiled_globs,
        })
    }

    /// Check if a file should be excluded
    pub fn should_exclude(&self, entry: &FileEntry) -> bool {
        if !self.config.enabled {
            return false;
        }

        for rule in &self.config.rules {
            if self.matches_rule(rule, entry) {
                return true;
            }
        }

        false
    }

    /// Check if a file matches a specific rule
    fn matches_rule(&self, rule: &ExcludeRule, entry: &FileEntry) -> bool {
        match &rule.rule_type {
            ExcludeRuleType::PathPattern { pattern } => {
                // Match against the root-relative path, the same base the
                // scanner uses for include/exclude globs; matching the
                // absolute `entry.path` made the same pattern text behave
                // differently between scan and hot-update.
                self.compiled_globs
                    .iter()
                    .any(|(p, g)| p == pattern && g.is_match(&entry.relative_path))
            }
            ExcludeRuleType::FileSize { max_bytes } => entry.size > *max_bytes,
            ExcludeRuleType::Language { languages } => entry
                .language_info
                .as_ref()
                .map(|lang| {
                    let lang_name = format!("{:?}", lang.language);
                    languages.contains(&lang_name)
                })
                .unwrap_or(false),
            ExcludeRuleType::Composite { conditions } => {
                // AND logic: all sub-conditions must match
                conditions.iter().all(|cond| self.matches_rule(cond, entry))
            }
        }
    }

    /// Get the number of configured rules
    pub fn rule_count(&self) -> usize {
        self.config.rules.len()
    }

    /// Check if exclude rules are enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_entry(path: &str, size: u64, language: Option<&str>) -> FileEntry {
        use cce_types::language::{FileType, Language, LanguageInfo};

        FileEntry {
            path: PathBuf::from(path),
            relative_path: PathBuf::from(path),
            size,
            modified: chrono::Utc::now(),
            content_hash: None,
            language_info: language.map(|lang| {
                let language_type = match lang {
                    "rust" => Language::Rust,
                    _ => Language::Unknown,
                };
                LanguageInfo {
                    language: language_type,
                    file_type: FileType::Source,
                    extensions: vec![lang.to_string()],
                }
            }),
        }
    }

    #[test]
    fn test_path_pattern_exclusion() {
        let config = ExcludeRulesConfig {
            enabled: true,
            rules: vec![ExcludeRule::path_pattern("**/*.pb.rs")],
        };

        let rules = ExcludeRules::new(config).expect("Failed to create rules");

        // Should match
        let entry = create_test_entry("src/generated/test.pb.rs", 1000, Some("rust"));
        assert!(rules.should_exclude(&entry));

        // Should not match
        let entry = create_test_entry("src/main.rs", 1000, Some("rust"));
        assert!(!rules.should_exclude(&entry));
    }

    #[test]
    fn test_file_size_exclusion() {
        let config = ExcludeRulesConfig {
            enabled: true,
            rules: vec![ExcludeRule::file_size(1048576)], // 1MB
        };

        let rules = ExcludeRules::new(config).expect("Failed to create rules");

        // Should exclude (larger than 1MB)
        let entry = create_test_entry("large.bin", 2_000_000, None);
        assert!(rules.should_exclude(&entry));

        // Should not exclude (smaller than 1MB)
        let entry = create_test_entry("small.txt", 500_000, None);
        assert!(!rules.should_exclude(&entry));
    }

    #[test]
    fn test_language_exclusion() {
        let config = ExcludeRulesConfig {
            enabled: true,
            rules: vec![ExcludeRule::language(vec![
                "Rust".to_string(),
                "Unknown".to_string(),
            ])],
        };

        let rules = ExcludeRules::new(config).expect("Failed to create rules");

        // Should exclude (proto -> Unknown)
        let entry = create_test_entry("schema.proto", 1000, Some("proto"));
        assert!(rules.should_exclude(&entry));

        // Should exclude (rust -> Rust)
        let entry = create_test_entry("main.rs", 1000, Some("rust"));
        assert!(rules.should_exclude(&entry));
    }

    #[test]
    fn test_composite_rule() {
        let config = ExcludeRulesConfig {
            enabled: true,
            rules: vec![ExcludeRule::composite(vec![
                ExcludeRule::path_pattern("**/bindings/*.rs"),
                ExcludeRule::file_size(524288), // 512KB
            ])],
        };

        let rules = ExcludeRules::new(config).expect("Failed to create rules");

        // Should exclude (matches both conditions)
        let entry = create_test_entry("src/bindings/large.rs", 600_000, Some("rust"));
        assert!(rules.should_exclude(&entry));

        // Should not exclude (matches path but not size)
        let entry = create_test_entry("src/bindings/small.rs", 1000, Some("rust"));
        assert!(!rules.should_exclude(&entry));

        // Should not exclude (matches size but not path)
        let entry = create_test_entry("src/other/large.rs", 600_000, Some("rust"));
        assert!(!rules.should_exclude(&entry));
    }

    #[test]
    fn test_disabled_rules() {
        let config = ExcludeRulesConfig {
            enabled: false,
            rules: vec![ExcludeRule::path_pattern("**/*.rs")],
        };

        let rules = ExcludeRules::new(config).expect("Failed to create rules");

        // Should not exclude even if pattern matches (rules disabled)
        let entry = create_test_entry("src/main.rs", 1000, Some("rust"));
        assert!(!rules.should_exclude(&entry));
    }

    #[test]
    fn test_multiple_rules() {
        let config = ExcludeRulesConfig {
            enabled: true,
            rules: vec![
                ExcludeRule::path_pattern("**/*.pb.rs"),
                ExcludeRule::file_size(1048576),
            ],
        };

        let rules = ExcludeRules::new(config).expect("Failed to create rules");

        // Should exclude by path pattern
        let entry = create_test_entry("gen/test.pb.rs", 1000, Some("rust"));
        assert!(rules.should_exclude(&entry));

        // Should exclude by file size
        let entry = create_test_entry("large.bin", 2_000_000, None);
        assert!(rules.should_exclude(&entry));

        // Should not exclude (matches neither)
        let entry = create_test_entry("main.rs", 1000, Some("rust"));
        assert!(!rules.should_exclude(&entry));
    }
}
