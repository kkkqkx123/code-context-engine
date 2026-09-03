//! Python test detection
//!
//! Detects Python test entities using:
//! - `@pytest.mark.*` / `@pytest.fixture` decorator blocks (reconstructed
//!   from the source region directly preceding the entity, since the parser
//!   query does not capture attribute-chain decorators)
//! - `test_` prefix functions inside test directories

use cce_types::entity::Entity;
use cce_types::language::Language;
use cce_types::test_info::TestInfo;

/// Reconstruct the decorator block directly above the entity and check for
/// pytest markers (confidence `High`).
///
/// Walks source lines upward from the entity start, collecting contiguous
/// `@`-prefixed lines (blank lines allowed). Stops at the first non-blank
/// non-decorator line so decorators of a previous entity are never included.
pub fn detect_from_source_block(entity: &Entity, source: &str) -> Option<TestInfo> {
    const MAX_DECORATOR_LINES: usize = 16;

    let entity_start = entity.span.start_byte;
    if entity_start > source.len() || entity_start == 0 {
        return None;
    }
    let before = &source[..entity_start];
    let mut decorators: Vec<&str> = Vec::new();
    for line in before.lines().rev().take(MAX_DECORATOR_LINES) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('@') {
            decorators.push(trimmed);
            continue;
        }
        break;
    }
    decorators.reverse();

    if decorators.iter().any(|d| is_pytest_decorator(d)) {
        Some(TestInfo::test_ast())
    } else {
        None
    }
}

/// Whether a decorator line is a pytest test marker (`@pytest.mark.*`,
/// `@pytest.fixture`).
fn is_pytest_decorator(decorator: &str) -> bool {
    let body = decorator.trim_start_matches('@');
    body.starts_with("pytest.mark")
        || body == "pytest.fixture"
        || body.starts_with("pytest.fixture(")
}

/// Detect `test_` prefix functions inside test files (confidence `High`,
/// requires the per-language path rule so business functions in non-test
/// files are never misjudged).
pub fn detect_conventional(entity: &Entity, file_path: &str, _source: &str) -> Option<TestInfo> {
    let name = entity.name.as_str();
    let in_test_file = TestInfo::from_path(Some(&Language::Python), file_path).is_test();
    if in_test_file && name.starts_with("test_") {
        return Some(TestInfo::test_ast());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind};

    fn function(name: &str, start_byte: usize) -> Entity {
        let mut entity = Entity::new(
            EntityId(0),
            EntityKind::Function,
            name.to_string(),
            Span::default(),
        );
        entity.span.start_byte = start_byte;
        entity.span.end_byte = start_byte + 10;
        entity
    }

    #[test]
    fn test_detect_pytest_mark_decorator() {
        let source = "@pytest.mark.parametrize(\"x\", [1, 2])\ndef test_foo(x):\n    pass";
        let entity = function("test_foo", source.find("def test_foo").unwrap());
        let info = detect_from_source_block(&entity, source);
        assert!(info.is_some());
        assert!(info.unwrap().is_test());
    }

    #[test]
    fn test_detect_pytest_fixture_decorator() {
        let source = "@pytest.fixture\ndef client():\n    pass";
        let entity = function("client", source.find("def client").unwrap());
        assert!(detect_from_source_block(&entity, source).is_some());
    }

    #[test]
    fn test_detect_pytest_mark_simple() {
        let source = "@pytest.mark.skip\ndef test_skip():\n    pass";
        let entity = function("test_skip", source.find("def test_skip").unwrap());
        assert!(detect_from_source_block(&entity, source).is_some());
    }

    #[test]
    fn test_plain_decorator_not_test() {
        let source = "@app.route(\"/\")\ndef index():\n    pass";
        let entity = function("index", source.find("def index").unwrap());
        assert!(detect_from_source_block(&entity, source).is_none());
    }

    #[test]
    fn test_decorator_of_previous_entity_not_attributed() {
        let source = "@pytest.fixture\ndef a():\n    pass\n\ndef b():\n    pass";
        let entity = function("b", source.find("def b").unwrap());
        assert!(detect_from_source_block(&entity, source).is_none());
    }

    #[test]
    fn test_conventional_requires_test_file() {
        let entity = function("test_login", 0);
        assert!(detect_conventional(&entity, "tests/test_user.py", "").is_some());
        assert!(detect_conventional(&entity, "src/user.py", "").is_none());
    }

    #[test]
    fn test_conventional_no_test_prefix_in_test_file() {
        let entity = function("helper", 0);
        assert!(detect_conventional(&entity, "tests/test_user.py", "").is_none());
    }
}
