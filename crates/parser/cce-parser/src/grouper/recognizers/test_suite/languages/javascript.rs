//! JavaScript and TypeScript test detection
//!
//! Detects JS/TS test entities using the `describe`/`context`/`it`/`test`/
//! `specify` top-level call blocks, constrained to test files
//! (`.spec.*`/`.test.*` or `__tests__/` directory).

use cce_types::entity::Entity;
use cce_types::language::Language;
use cce_types::test_info::TestInfo;

/// Detect JS/TS test blocks and cases in test files (confidence `High`).
///
/// The file constraint (`.spec`/`.test` suffix or `__tests__/` segment) is
/// part of the rule, so a production `test()` call is never marked.
pub fn detect_conventional(entity: &Entity, file_path: &str) -> Option<TestInfo> {
    let in_test_file = TestInfo::from_path(Some(&Language::TypeScript), file_path).is_test()
        || TestInfo::from_path(Some(&Language::JavaScript), file_path).is_test();
    if !in_test_file {
        return None;
    }

    let name = entity.name.as_str();
    if name == "describe" || name == "context" {
        return Some(TestInfo::test_ast_block());
    }
    if name == "it" || name == "test" || name == "specify" {
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
    fn test_detect_test_block_in_spec_file() {
        assert!(detect_conventional(&function("describe"), "user.spec.ts").is_some());
        assert!(detect_conventional(&function("context"), "user.spec.js").is_some());
    }

    #[test]
    fn test_detect_test_case_in_test_dir() {
        assert!(detect_conventional(&function("it"), "src/__tests__/user.ts").is_some());
        assert!(detect_conventional(&function("test"), "user.test.ts").is_some());
        assert!(detect_conventional(&function("specify"), "user.spec.js").is_some());
    }

    #[test]
    fn test_not_test_file() {
        assert!(detect_conventional(&function("describe"), "src/user.ts").is_none());
        assert!(detect_conventional(&function("test"), "src/contest.ts").is_none());
    }

    #[test]
    fn test_jsx_tsx_test_files() {
        // React component test files (`Foo.test.tsx`/`Foo.spec.jsx`) must hit
        // the conventional detector through the extended path rules.
        assert!(detect_conventional(&function("describe"), "user.spec.tsx").is_some());
        assert!(detect_conventional(&function("it"), "user.test.tsx").is_some());
        assert!(detect_conventional(&function("test"), "user.test.jsx").is_some());
        assert!(detect_conventional(&function("describe"), "user.spec.jsx").is_some());
        // Module variant files
        assert!(detect_conventional(&function("describe"), "user.spec.mts").is_some());
        // Production TSX files stay unmarked
        assert!(detect_conventional(&function("describe"), "src/user.tsx").is_none());
    }

    #[test]
    fn test_ordinary_function_in_test_file() {
        assert!(detect_conventional(&function("fetchUser"), "user.spec.ts").is_none());
    }
}
