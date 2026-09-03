//! Rust-specific test detection
//!
//! Detects Rust test entities using:
//! - `#[test]` / `#[tokio::test]` attribute adjacency
//! - `#[cfg(test)]` attribute adjacency (module-level)

use cce_types::entity::Entity;
use cce_types::test_info::TestInfo;

/// Detect Rust test attributes from the annotation nodes directly preceding
/// the entity (confidence `High`).
///
/// - `#[test]` / `#[tokio::test]` on a function → test entity
/// - `#[cfg(test)]` (parsed from the cfg payload, not substring matching)
///   → test module/block
pub fn detect_from_annotations(
    entity: &Entity,
    adjacent_annotations: &[String],
) -> Option<TestInfo> {
    for annotation in adjacent_annotations {
        let name = annotation.as_str();
        if name == "test" || name == "tokio::test" {
            return Some(TestInfo::test_ast());
        }
        if let Some(payload) = cfg_payload(name) {
            if cfg_targets_test(payload) {
                let granularity = if entity.kind.is_module_like() {
                    TestInfo::test_ast_block()
                } else {
                    TestInfo::test_ast()
                };
                return Some(granularity);
            }
        }
    }
    None
}

/// Extract the payload of a `cfg(...)` attribute (also handles `cfg_attr`).
fn cfg_payload(name: &str) -> Option<&str> {
    for prefix in ["cfg(", "cfg_attr("] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return rest.strip_suffix(')');
        }
    }
    None
}

/// Whether a cfg payload targets the `test` configuration.
///
/// Tokenizes on non-alphanumeric characters and checks for an exact `test`
/// token, so `cfg(feature = "contest")` is never misjudged.
fn cfg_targets_test(payload: &str) -> bool {
    payload
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| token == "test")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind};
    use cce_types::test_info::TestSource;

    fn function(name: &str) -> Entity {
        Entity::new(
            EntityId(0),
            EntityKind::Function,
            name.to_string(),
            Span::default(),
        )
    }

    fn module(name: &str) -> Entity {
        Entity::new(
            EntityId(0),
            EntityKind::Module,
            name.to_string(),
            Span::default(),
        )
    }

    #[test]
    fn test_detect_test_attribute() {
        let info = detect_from_annotations(&function("test_login"), &["test".to_string()]);
        assert!(info.is_some());
        let info = info.unwrap();
        assert!(info.is_test());
        assert_eq!(info.source, TestSource::Ast);
    }

    #[test]
    fn test_detect_tokio_test_attribute() {
        assert!(
            detect_from_annotations(&function("async_login"), &["tokio::test".to_string()])
                .is_some()
        );
    }

    #[test]
    fn test_detect_cfg_test_module() {
        let info = detect_from_annotations(&module("tests"), &["cfg(test)".to_string()]);
        assert!(info.is_some());
        assert!(info.unwrap().is_test());
    }

    #[test]
    fn test_cfg_feature_contest_not_test() {
        // `#[cfg(feature = "contest")]` must never be treated as test
        assert!(
            detect_from_annotations(
                &module("feature"),
                &["cfg(feature = \"contest\")".to_string()]
            )
            .is_none()
        );
    }

    #[test]
    fn test_cfg_all_test() {
        assert!(
            detect_from_annotations(&module("m"), &["cfg(all(test, unix))".to_string()]).is_some()
        );
    }

    #[test]
    fn test_no_name_based_matching() {
        // `latest` / `contest` style names never match without attributes
        assert!(detect_from_annotations(&function("latest"), &[]).is_none());
        assert!(detect_from_annotations(&function("test_mode"), &[]).is_none());
    }
}
