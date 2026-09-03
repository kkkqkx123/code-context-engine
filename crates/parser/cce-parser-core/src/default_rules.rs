//! Default capture rules for common languages.
//!
//! Provides default implementations of `LanguageRules` for languages that
//! share similar capture patterns.

use crate::capture_rules::{CaptureRule, LanguageRules, find_capture_by_suffix};

/// Default capture rules for most languages.
///
/// These rules define the standard capture selection logic that was previously
/// implemented as predicate chains in `find_callee_capture` and
/// `find_dependency_capture`.
pub struct DefaultLanguageRules;

impl LanguageRules for DefaultLanguageRules {
    fn call_rules(&self) -> &[CaptureRule] {
        &[
            // Function calls: foo()
            CaptureRule {
                required: &[".function.name"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".function.name").map(|c| c.text.clone())
                },
            },
            // Method calls: obj.method()
            CaptureRule {
                required: &[".method.function"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".method.function").map(|c| c.text.clone())
                },
            },
            CaptureRule {
                required: &[".method.name"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".method.name").map(|c| c.text.clone())
                },
            },
            // Generic function calls: foo::<T>()
            CaptureRule {
                required: &[".generic.function.name"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".generic.function.name")
                        .map(|c| c.text.clone())
                },
            },
            // Higher-order function calls: arr.map(x => x)
            CaptureRule {
                required: &[".hof.name"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".hof.name").map(|c| c.text.clone())
                },
            },
            CaptureRule {
                required: &[".hof.method.name"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".hof.method.name").map(|c| c.text.clone())
                },
            },
            // Closure calls: (|| {})()
            CaptureRule {
                required: &[".closure"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".closure").map(|c| c.text.clone())
                },
            },
            // Closure variable calls: let f = || {}; f()
            CaptureRule {
                required: &[".closure_variable.name"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".closure_variable.name")
                        .map(|c| c.text.clone())
                },
            },
            // Generic function reference
            CaptureRule {
                required: &[".function"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".function").map(|c| c.text.clone())
                },
            },
            // Method references: ClassName::methodName
            CaptureRule {
                required: &[".reference"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".reference").map(|c| c.text.clone())
                },
            },
            // Generic .name capture (excluding type names, paths, etc.)
            CaptureRule {
                required: &[".name"],
                optional: &[],
                extract: |captures| {
                    captures
                        .iter()
                        .find(|c| {
                            c.name.ends_with(".name")
                                && !c.name.ends_with(".type.name")
                                && !c.name.ends_with(".path")
                                && !c.name.ends_with(".scoped")
                                && !c.name.ends_with(".scoped_type")
                        })
                        .map(|c| c.text.clone())
                },
            },
            // Chained method calls: a.b.method()
            CaptureRule {
                required: &[".method.chained.to"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".method.chained.to").map(|c| c.text.clone())
                },
            },
            CaptureRule {
                required: &[".method.chained.to.name"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".method.chained.to.name")
                        .map(|c| c.text.clone())
                },
            },
        ]
    }

    fn dependency_rules(&self) -> &[CaptureRule] {
        &[
            // Scoped references: std::collections::HashMap
            CaptureRule {
                required: &[".scoped"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".scoped").map(|c| c.text.clone())
                },
            },
            CaptureRule {
                required: &[".scoped_type"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".scoped_type").map(|c| c.text.clone())
                },
            },
            // Import targets
            CaptureRule {
                required: &[".target"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".target").map(|c| c.text.clone())
                },
            },
            // Trait references
            CaptureRule {
                required: &[".trait"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".trait").map(|c| c.text.clone())
                },
            },
            // Type bounds
            CaptureRule {
                required: &[".bound"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".bound").map(|c| c.text.clone())
                },
            },
            // Super references
            CaptureRule {
                required: &[".super"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".super").map(|c| c.text.clone())
                },
            },
            // Module names
            CaptureRule {
                required: &[".module.name"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".module.name").map(|c| c.text.clone())
                },
            },
            CaptureRule {
                required: &[".module"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".module").map(|c| c.text.clone())
                },
            },
            // Paths
            CaptureRule {
                required: &[".path"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".path").map(|c| c.text.clone())
                },
            },
            CaptureRule {
                required: &[".module.path"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".module.path").map(|c| c.text.clone())
                },
            },
            // Generic name
            CaptureRule {
                required: &[".name"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".name").map(|c| c.text.clone())
                },
            },
            // Type references
            CaptureRule {
                required: &[".type"],
                optional: &[],
                extract: |captures| {
                    find_capture_by_suffix(captures, ".type").map(|c| c.text.clone())
                },
            },
        ]
    }
}

/// Get the default language rules.
pub fn default_language_rules() -> impl LanguageRules {
    DefaultLanguageRules
}
