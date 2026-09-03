//! Tree-sitter query schemes for different languages

use crate::tree_sitter_query::capture::Domain;
use crate::tree_sitter_query::parser_types::CaptureName;

pub mod bash;
pub mod c;
pub mod common;
pub mod cpp;
pub mod csharp;
pub mod css;
pub mod dart;
pub mod go;
pub mod html;
pub mod java;
pub mod javascript;
pub mod kotlin;
pub mod lua;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod scala;
pub mod svelte;
pub mod tsx;
pub mod typescript;
pub mod vue;

/// Check if a capture name belongs to entity domain
///
/// # Arguments
///
/// * `capture` - The capture name to check
///
/// # Returns
///
/// Returns `true` if the capture belongs to entity domain, `false` otherwise.
///
/// # Example
///
/// ```
/// use cce_parser::tree_sitter_query::is_entity_capture;
///
/// assert!(is_entity_capture("@entity.class.name"));
/// assert!(!is_entity_capture("@call.function.name"));
/// ```
pub fn is_entity_capture(capture: &str) -> bool {
    CaptureName::parse(capture)
        .map(|c| c.domain == Domain::Entity)
        .unwrap_or(false)
}

/// Check if a capture name belongs to call domain
///
/// # Arguments
///
/// * `capture` - The capture name to check
///
/// # Returns
///
/// Returns `true` if the capture belongs to call domain, `false` otherwise.
///
/// # Example
///
/// ```
/// use cce_parser::tree_sitter_query::is_call_capture;
///
/// assert!(is_call_capture("@call.function.name"));
/// assert!(!is_call_capture("@entity.class.name"));
/// ```
pub fn is_call_capture(capture: &str) -> bool {
    CaptureName::parse(capture)
        .map(|c| c.domain == Domain::Call)
        .unwrap_or(false)
}

/// Check if a capture name belongs to dependency domain
///
/// # Arguments
///
/// * `capture` - The capture name to check
///
/// # Returns
///
/// Returns `true` if the capture belongs to dependency domain, `false` otherwise.
pub fn is_dependency_capture(capture: &str) -> bool {
    CaptureName::parse(capture)
        .map(|c| c.domain == Domain::Dependency)
        .unwrap_or(false)
}

/// Check if a capture name belongs to comment domain
///
/// # Arguments
///
/// * `capture` - The capture name to check
///
/// # Returns
///
/// Returns `true` if the capture belongs to comment domain, `false` otherwise.
pub fn is_comment_capture(capture: &str) -> bool {
    CaptureName::parse(capture)
        .map(|c| c.domain == Domain::Comment)
        .unwrap_or(false)
}

/// Extract entity category from a capture name
///
/// # Arguments
///
/// * `capture` - The capture name to extract from
///
/// # Returns
///
/// Returns `Some(String)` with the category if the capture belongs to entity domain,
/// or `None` otherwise.
///
/// # Example
///
/// ```
/// use cce_parser::tree_sitter_query::extract_entity_category;
///
/// let category = extract_entity_category("@entity.class.name");
/// assert_eq!(category, Some("class".to_string()));
/// ```
pub fn extract_entity_category(capture: &str) -> Option<String> {
    CaptureName::parse(capture)
        .ok()
        .filter(|c| c.domain == Domain::Entity)
        .and_then(|c| c.category)
}

/// Extract call category from a capture name
///
/// # Arguments
///
/// * `capture` - The capture name to extract from
///
/// # Returns
///
/// Returns `Some(String)` with the category if the capture belongs to call domain,
/// or `None` otherwise.
///
/// # Example
///
/// ```
/// use cce_parser::tree_sitter_query::extract_call_category;
///
/// let category = extract_call_category("@call.method.function");
/// assert_eq!(category, Some("method".to_string()));
/// ```
pub fn extract_call_category(capture: &str) -> Option<String> {
    CaptureName::parse(capture)
        .ok()
        .filter(|c| c.domain == Domain::Call)
        .and_then(|c| c.category)
}

/// Extract dependency category from a capture name
///
/// # Arguments
///
/// * `capture` - The capture name to extract from
///
/// # Returns
///
/// Returns `Some(String)` with the category if the capture belongs to dependency domain,
/// or `None` otherwise.
pub fn extract_dependency_category(capture: &str) -> Option<String> {
    CaptureName::parse(capture)
        .ok()
        .filter(|c| c.domain == Domain::Dependency)
        .and_then(|c| c.category)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_sitter_query::capture::{behavior, control};

    fn extract_and_validate_captures(
        query: &str,
        language: &str,
    ) -> Result<Vec<String>, Vec<String>> {
        let mut errors = Vec::new();
        let mut valid_captures = std::collections::HashSet::new();

        for line in query.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(';') || trimmed.starts_with('#') || trimmed.starts_with("//") {
                continue;
            }

            let captures: Vec<&str> = line
                .split('@')
                .skip(1)
                .filter_map(|s| s.split_whitespace().next())
                .collect();

            for capture in captures {
                if capture.is_empty() || !capture.chars().next().unwrap().is_alphabetic() {
                    continue;
                }

                let full_capture = format!("@{}", capture);

                if valid_captures.contains(&full_capture) {
                    continue;
                }

                match CaptureName::parse(&full_capture) {
                    Ok(_) => {
                        valid_captures.insert(full_capture);
                    }
                    Err(e) => errors.push(format!(
                        "{}: invalid capture '{}' - {}",
                        language, full_capture, e
                    )),
                }
            }
        }

        if errors.is_empty() {
            Ok(valid_captures.into_iter().collect())
        } else {
            Err(errors)
        }
    }

    #[test]
    fn test_is_entity_capture() {
        assert!(is_entity_capture("@entity.class.name"));
        assert!(is_entity_capture("@entity.function.name"));
        assert!(!is_entity_capture("@call.function.name"));
        assert!(!is_entity_capture("@dependency.include.path"));
    }

    #[test]
    fn test_is_call_capture() {
        assert!(is_call_capture("@call.function.name"));
        assert!(is_call_capture("@call.method.function"));
        assert!(!is_call_capture("@entity.class.name"));
        assert!(!is_call_capture("@dependency.include.path"));
    }

    #[test]
    fn test_is_dependency_capture() {
        assert!(is_dependency_capture("@dependency.include.path"));
        assert!(is_dependency_capture("@dependency.using.namespace.name"));
        assert!(!is_dependency_capture("@entity.class.name"));
        assert!(!is_dependency_capture("@call.function.name"));
    }

    #[test]
    fn test_is_control_capture() {
        assert!(control::is_main_control_capture("@control.flow.if"));
        assert!(control::is_main_control_capture("@control.flow.match"));
        assert!(!control::is_main_control_capture("@entity.class.name"));
        assert!(!control::is_main_control_capture("@behavior.data.bind"));
    }

    #[test]
    fn test_extract_entity_category() {
        assert_eq!(
            extract_entity_category("@entity.class.name"),
            Some("class".to_string())
        );
        assert_eq!(
            extract_entity_category("@entity.function.name"),
            Some("function".to_string())
        );
        assert_eq!(extract_entity_category("@call.function.name"), None);
    }

    #[test]
    fn test_extract_call_category() {
        assert_eq!(
            extract_call_category("@call.function.name"),
            Some("function".to_string())
        );
        assert_eq!(
            extract_call_category("@call.method.function"),
            Some("method".to_string())
        );
        assert_eq!(extract_call_category("@entity.class.name"), None);
    }

    #[test]
    fn test_extract_dependency_category() {
        assert_eq!(
            extract_dependency_category("@dependency.include.path"),
            Some("include".to_string())
        );
        assert_eq!(
            extract_dependency_category("@dependency.using.namespace.name"),
            Some("using".to_string())
        );
        assert_eq!(extract_dependency_category("@entity.class.name"), None);
    }

    #[test]
    fn test_parse_control_capture() {
        let capture = CaptureName::parse("@control.flow.if").expect("should parse control capture");
        assert_eq!(capture.domain, Domain::Control);
        assert_eq!(capture.category.as_deref(), Some("flow"));
        assert_eq!(capture.subtype.as_deref(), Some("if"));

        let capture =
            CaptureName::parse("@control.flow.match").expect("should parse control capture");
        assert_eq!(capture.domain, Domain::Control);
        assert_eq!(capture.category.as_deref(), Some("flow"));
        assert_eq!(capture.subtype.as_deref(), Some("match"));

        assert!(
            CaptureName::parse("@entity.class.name")
                .expect("should parse entity capture")
                .domain
                != Domain::Control
        );
    }

    #[test]
    fn test_is_behavior_capture() {
        assert!(behavior::is_main_behavior_capture("@behavior.data.bind"));
        assert!(behavior::is_main_behavior_capture("@behavior.effect.error"));
        assert!(!behavior::is_main_behavior_capture("@entity.class.name"));
        assert!(!behavior::is_main_behavior_capture("@call.function.name"));
    }

    #[test]
    fn test_parse_behavior_capture() {
        let capture =
            CaptureName::parse("@behavior.data.bind").expect("should parse behavior capture");
        assert_eq!(capture.domain, Domain::Behavior);
        assert_eq!(capture.category.as_deref(), Some("data"));
        assert_eq!(capture.subtype.as_deref(), Some("bind"));

        let capture =
            CaptureName::parse("@behavior.effect.error").expect("should parse behavior capture");
        assert_eq!(capture.domain, Domain::Behavior);
        assert_eq!(capture.category.as_deref(), Some("effect"));
        assert_eq!(capture.subtype.as_deref(), Some("error"));

        assert!(
            CaptureName::parse("@entity.class.name")
                .expect("should parse entity capture")
                .domain
                != Domain::Behavior
        );
    }

    #[test]
    fn test_validate_all_query_schemes() {
        // Validate C language queries
        let c_queries = vec![
            ("entity", c::entity_query()),
            ("call", c::call_query()),
            ("dependency", c::dependency_query()),
        ];

        for (name, query) in &c_queries {
            let result = extract_and_validate_captures(query, "C");
            assert!(
                result.is_ok(),
                "C {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate C# language queries
        let csharp_queries = vec![
            ("entity", csharp::entity_query()),
            ("call", csharp::call_query()),
            ("dependency", csharp::dependency_query()),
        ];

        for (name, query) in &csharp_queries {
            let result = extract_and_validate_captures(query, "C#");
            assert!(
                result.is_ok(),
                "C# {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate C++ language queries
        let cpp_entity_query = cpp::entity_query();
        let cpp_call_query = cpp::call_query();
        let cpp_dependency_query = cpp::dependency_query();

        let cpp_queries = vec![
            ("entity", cpp_entity_query.as_str()),
            ("call", cpp_call_query.as_str()),
            ("dependency", cpp_dependency_query.as_str()),
        ];

        for (name, query) in &cpp_queries {
            let result = extract_and_validate_captures(query, "C++");
            assert!(
                result.is_ok(),
                "C++ {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate HTML language queries
        let html_queries = vec![
            ("entity", html::entity_query()),
            ("dependency", html::dependency_query()),
        ];

        for (name, query) in &html_queries {
            let result = extract_and_validate_captures(query, "HTML");
            assert!(
                result.is_ok(),
                "HTML {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate CSS language queries
        let css_queries = vec![
            ("entity", css::entity_query()),
            ("structural", css::structural_query()),
            ("dependency", css::dependency_query()),
        ];

        for (name, query) in &css_queries {
            let result = extract_and_validate_captures(query, "CSS");
            assert!(
                result.is_ok(),
                "CSS {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Vue language queries
        let vue_entity_query = vue::entity_query();
        let vue_structural_query = vue::structural_query();
        let vue_dependency_query = vue::dependency_query();

        let vue_queries: Vec<(&str, &str)> = vec![
            ("entity", vue_entity_query),
            ("structural", vue_structural_query),
            ("dependency", vue_dependency_query),
        ];

        for (name, query) in &vue_queries {
            let result = extract_and_validate_captures(query, "Vue");
            assert!(
                result.is_ok(),
                "Vue {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Svelte language queries
        let svelte_entity_query = svelte::entity_query();
        let svelte_structural_query = svelte::structural_query();
        let svelte_dependency_query = svelte::dependency_query();

        let svelte_queries: Vec<(&str, &str)> = vec![
            ("entity", svelte_entity_query),
            ("structural", svelte_structural_query),
            ("dependency", svelte_dependency_query),
        ];

        for (name, query) in &svelte_queries {
            let result = extract_and_validate_captures(query, "Svelte");
            assert!(
                result.is_ok(),
                "Svelte {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate TSX language queries
        let tsx_entity_query = tsx::entity_query();
        let tsx_structural_query = tsx::structural_query();
        let tsx_dependency_query = tsx::dependency_query();

        let tsx_queries: Vec<(&str, &str)> = vec![
            ("entity", &tsx_entity_query),
            ("structural", &tsx_structural_query),
            ("dependency", &tsx_dependency_query),
        ];

        for (name, query) in &tsx_queries {
            let result = extract_and_validate_captures(query, "TSX");
            assert!(
                result.is_ok(),
                "TSX {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Dart language queries
        let dart_queries = vec![
            ("entity", dart::entity_query()),
            ("call", dart::call_query()),
            ("dependency", dart::dependency_query()),
        ];

        for (name, query) in &dart_queries {
            let result = extract_and_validate_captures(query, "Dart");
            assert!(
                result.is_ok(),
                "Dart {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Bash language queries
        let bash_queries = vec![
            ("entity", bash::entity_query()),
            ("call", bash::call_query()),
            ("dependency", bash::dependency_query()),
        ];

        for (name, query) in &bash_queries {
            let result = extract_and_validate_captures(query, "Bash");
            assert!(
                result.is_ok(),
                "Bash {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Lua language queries
        let lua_queries = vec![
            ("entity", lua::entity_query()),
            ("call", lua::call_query()),
            ("dependency", lua::dependency_query()),
        ];

        for (name, query) in &lua_queries {
            let result = extract_and_validate_captures(query, "Lua");
            assert!(
                result.is_ok(),
                "Lua {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Rust language queries
        let rust_queries = vec![
            ("entity", rust::entity_query()),
            ("call", rust::call_query()),
            ("dependency", rust::dependency_query()),
        ];

        for (name, query) in &rust_queries {
            let result = extract_and_validate_captures(query, "Rust");
            assert!(
                result.is_ok(),
                "Rust {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Python language queries
        let python_queries = vec![
            ("entity", python::entity_query()),
            ("call", python::call_query()),
            ("dependency", python::dependency_query()),
        ];

        for (name, query) in &python_queries {
            let result = extract_and_validate_captures(query, "Python");
            assert!(
                result.is_ok(),
                "Python {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate JavaScript language queries
        let js_entity_query = javascript::entity_query();
        let js_call_query = javascript::call_query();
        let js_dependency_query = javascript::dependency_query();

        let javascript_queries = vec![
            ("entity", js_entity_query.as_str()),
            ("call", js_call_query),
            ("dependency", js_dependency_query),
        ];

        for (name, query) in &javascript_queries {
            let result = extract_and_validate_captures(query, "JavaScript");
            assert!(
                result.is_ok(),
                "JavaScript {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate TypeScript language queries
        let ts_entity_query = typescript::entity_query();
        let ts_call_query = typescript::call_query();
        let ts_dependency_query = typescript::dependency_query();

        let typescript_queries = vec![
            ("entity", ts_entity_query.as_str()),
            ("call", ts_call_query.as_str()),
            ("dependency", ts_dependency_query.as_str()),
        ];

        for (name, query) in &typescript_queries {
            let result = extract_and_validate_captures(query, "TypeScript");
            assert!(
                result.is_ok(),
                "TypeScript {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Go language queries
        let go_queries = vec![
            ("entity", go::entity_query()),
            ("call", go::call_query()),
            ("dependency", go::dependency_query()),
        ];

        for (name, query) in &go_queries {
            let result = extract_and_validate_captures(query, "Go");
            assert!(
                result.is_ok(),
                "Go {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Java language queries
        let java_queries = vec![
            ("entity", java::entity_query()),
            ("call", java::call_query()),
            ("dependency", java::dependency_query()),
        ];

        for (name, query) in &java_queries {
            let result = extract_and_validate_captures(query, "Java");
            assert!(
                result.is_ok(),
                "Java {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate PHP language queries
        let php_queries = vec![
            ("entity", php::entity_query()),
            ("call", php::call_query()),
            ("dependency", php::dependency_query()),
        ];

        for (name, query) in &php_queries {
            let result = extract_and_validate_captures(query, "PHP");
            assert!(
                result.is_ok(),
                "PHP {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Ruby language queries
        let ruby_queries = vec![
            ("entity", ruby::entity_query()),
            ("call", ruby::call_query()),
            ("dependency", ruby::dependency_query()),
        ];

        for (name, query) in &ruby_queries {
            let result = extract_and_validate_captures(query, "Ruby");
            assert!(
                result.is_ok(),
                "Ruby {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Kotlin language queries
        let kotlin_queries = vec![
            ("entity", kotlin::entity_query()),
            ("call", kotlin::call_query()),
            ("dependency", kotlin::dependency_query()),
        ];

        for (name, query) in &kotlin_queries {
            let result = extract_and_validate_captures(query, "Kotlin");
            assert!(
                result.is_ok(),
                "Kotlin {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }

        // Validate Scala language queries
        let scala_queries = vec![
            ("entity", scala::entity_query()),
            ("call", scala::call_query()),
            ("dependency", scala::dependency_query()),
        ];

        for (name, query) in &scala_queries {
            let result = extract_and_validate_captures(query, "Scala");
            assert!(
                result.is_ok(),
                "Scala {} query has invalid captures: {:?}",
                name,
                result.err()
            );
        }
    }
}
