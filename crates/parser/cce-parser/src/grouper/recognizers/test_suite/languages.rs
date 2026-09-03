//! Language-specific test detection implementations
//!
//! Coverage matrix:
//!
//! | Language | Annotation/attribute | Naming convention | Path rule |
//! |----------|----------------------|-------------------|-----------|
//! | Rust     | `#[test]`, `#[cfg(test)]` | — (deliberately no naming) | `*_test.rs`, `tests/` |
//! | Python   | `@pytest.mark.*`, `@pytest.fixture` | `test_*` in test files | `test_*.py`, `conftest.py`, `tests/` |
//! | Go       | — | `TestXxx`/`BenchmarkXxx` in `*_test.go` | `*_test.go` |
//! | Java/Kotlin | `@Test`, `@ParameterizedTest` | `*Test`/`*Tests` classes | `*Test.java`/`*Tests.java`, `src/test/` |
//! | JS/TS/JSX/TSX | — | `describe`/`it`/`test` in test files | `.spec.*`/`.test.*`, `__tests__/` |
//! | C#       | `[Test]`, `[TestCase]`, `[Fact]`, `[Theory]` | `*Test`/`*Tests` classes | `*Test.cs`/`*Tests.cs`, `tests/` |
//! | Scala    | `@Test` | `*Test`/`*Tests`/`*Spec` classes | `src/test/`, `tests/` |
//! | PHP      | `#[Test]`, `@test` docblock | `*Test` classes | `*Test.php`, `tests/` |
//! | C++      | — | `TEST()`/`TEST_F()` macro shape | `*_test.cpp`/`*_test.cc`, `tests/` |
//! | C        | — | — (path rules only) | `*_test.c`, `tests/` |
//! | Dart     | — | — (path rules only) | `test_*.dart`, `test/`, `tests/` |
//! | Ruby     | — | — (path rules only) | `spec/`, `tests/` |
//! | Lua      | — | luaunit `test_*`/`test*` in test files | `test_*.lua`, `*_test.lua`, `*_spec.lua`, `test/`, `tests/`, `spec/` |
//! | Bash     | — | — (path rules only) | `*.bats`, `tests/` |
//!
//! **Degradation decisions** (per "no entity, degrade" principle): Dart
//! `test()`/`group()` and Ruby `describe`/`it` are call expressions that the
//! entity queries do not capture as entities, and their signals are file-
//! constrained anyway (a test call outside a test path is not a test), so
//! entity-level detection adds nothing over the path rules — only path rules
//! are implemented. Catch2 `TEST_CASE(...)` parses as a `call_expression`
//! without an entity, so it also degrades to path rules. The same applies to
//! busted `describe`/`it`/`context` (Lua) and bats `@test` blocks (Bash).

mod cpp;
mod csharp;
mod go;
mod java;
mod javascript;
mod lua;
mod php;
mod python;
mod rust;
mod scala;

use cce_types::entity::Entity;
use cce_types::language::Language;
use cce_types::test_info::TestInfo;

/// Level-1 detection: AST attribute adjacency (confidence `High`).
///
/// Inspects the annotation names directly preceding the entity span. Never
/// matches on entity names or whole-source substrings.
pub fn detect_from_annotations(
    entity: &Entity,
    language: &Language,
    adjacent_annotations: &[String],
    source: &str,
) -> Option<TestInfo> {
    match language {
        Language::Rust => rust::detect_from_annotations(entity, adjacent_annotations),
        Language::Java | Language::Kotlin => java::detect_from_annotations(adjacent_annotations),
        Language::CSharp => csharp::detect_from_annotations(adjacent_annotations),
        Language::Scala => scala::detect_from_annotations(adjacent_annotations),
        // PHPUnit 10 `#[Test]` attributes are captured as annotation entities;
        // PHPUnit 4-9 `@test` markers live in the entity doc comment.
        Language::Php => php::detect_from_annotations(entity, adjacent_annotations),
        // Python decorator chains (e.g. `@pytest.mark.parametrize`) are not
        // captured as annotation entities by the parser query, so the
        // decorator block is reconstructed from the source region directly
        // preceding the entity.
        Language::Python => python::detect_from_source_block(entity, source),
        _ => None,
    }
}

/// Level-1 detection: constrained naming conventions (confidence `High`).
///
/// Per-language conventions that require additional file context (test-file
/// constraint) so generic `contains("test")` matching never applies.
pub fn detect_conventional(
    entity: &Entity,
    language: &Language,
    file_path: &str,
    source: &str,
) -> Option<TestInfo> {
    match language {
        Language::Java | Language::Kotlin => java::detect_conventional(entity, language),
        Language::CSharp => csharp::detect_conventional(entity),
        Language::Scala => scala::detect_conventional(entity),
        Language::Php => php::detect_conventional(entity),
        // GoogleTest `TEST()`/`TEST_F()` parse as constructor-shaped
        // definitions; the parameter shape (two bare identifiers) is the
        // strong signal, no file constraint needed.
        Language::Cpp => cpp::detect_conventional(entity, source),
        Language::Go => go::detect_conventional(entity, file_path),
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            javascript::detect_conventional(entity, file_path)
        }
        Language::Python => python::detect_conventional(entity, file_path, source),
        // luaunit `test_*`/`test*` names require the per-language path rule
        // (test directories or test-ish filenames) to fire first.
        Language::Lua => lua::detect_conventional(entity, file_path),
        _ => None,
    }
}
