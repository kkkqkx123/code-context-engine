//! Go test detection
//!
//! Detects Go test entities using the `TestXxx`/`BenchmarkXxx` conventions
//! inside `*_test.go` files (Go has no attribute-based test markers).

use cce_types::entity::Entity;
use cce_types::test_info::TestInfo;

/// Detect Go test functions (`TestXxx`/`BenchmarkXxx`) inside `*_test.go`
/// files (confidence `High`; the file constraint is part of the rule, so a
/// `TestXxx` function in a production file is never marked).
pub fn detect_conventional(entity: &Entity, file_path: &str) -> Option<TestInfo> {
    if !file_path.ends_with("_test.go") {
        return None;
    }
    let name = entity.name.as_str();
    let is_test_function =
        (name.starts_with("Test") && name.len() > 4) || name.starts_with("Benchmark");
    if is_test_function {
        return Some(TestInfo::test_ast());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind};

    fn function(name: &str) -> Entity {
        Entity::new(
            EntityId(0),
            EntityKind::Function,
            name.to_string(),
            Span::default(),
        )
    }

    #[test]
    fn test_detect_go_test_function() {
        assert!(detect_conventional(&function("TestUser"), "user_test.go").is_some());
        assert!(detect_conventional(&function("TestLogin"), "service_test.go").is_some());
        assert!(detect_conventional(&function("BenchmarkAdd"), "bench_test.go").is_some());
    }

    #[test]
    fn test_not_test_file() {
        assert!(detect_conventional(&function("TestUser"), "user.go").is_none());
    }

    #[test]
    fn test_helper_in_test_file_not_detected() {
        assert!(detect_conventional(&function("helper"), "user_test.go").is_none());
    }
}
