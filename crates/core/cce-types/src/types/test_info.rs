//! Test code marking (TestInfo)
//!
//! End-to-end test-code marker propagated from the grouper (AST-level
//! detection) and the file-path rules through the chunker into storage and
//! evaluation. Orthogonal to `GroupType` and `EntityKind`: it never changes
//! grouping or conversion logic, it only tags chunks.
//!
//! # Two-level determination
//!
//! - **AST level** (grouper stage, source `Ast`, highest priority):
//!   `#[cfg(test)]`/`#[test]` (Rust), `@Test`/`@ParameterizedTest` (Java/Kotlin),
//!   `@pytest.mark.*` (Python), `TestXxx` in `*_test.go` (Go), `describe`/`it`/
//!   `test` blocks (JS/TS/JSX/TSX), and constrained naming conventions per
//!   language.
//! - **Path level** (source `Path`): per-language file-path rules.
//!
//! # Merge rule
//!
//! A `Test` signal wins (`Ast` source overrides `Path`; a group is `Test` when
//! any member is `Test`). With no signal at either level the chunk stays
//! `Unknown` — callers must never default it to `Test`. A merge result always
//! carries `Group` granularity (a merged marker spans multiple source
//! entities, the original granularity is only meaningful before merging).
//!
//! # Invariants
//!
//! - `status = Test` requires `source ∈ {Ast, Path}`.
//! - `status = Unknown` requires `source = None`.
//! - Construct via the factory methods; the debug assertions in `new` enforce
//!   the invariants in debug builds.
//! - `granularity` is informational only and never influences logic.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use super::language::Language;

/// Two-state test determination.
///
/// The variant discriminant IS the storage encoding (`Unknown` = 0,
/// `Test` = 1) used by SQLite columns and BM25 numeric fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Archive, RkyvDeserialize, Serialize)]
#[repr(u8)]
pub enum TestStatus {
    /// No signal at either level. Callers may fall back to path rules but
    /// must never default to `Test`.
    #[default]
    Unknown = 0,
    /// The chunk belongs to test code (usable for filtering and statistics).
    Test = 1,
}

impl SerdeSerialize for TestStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.as_u8())
    }
}

impl<'de> SerdeDeserialize<'de> for TestStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = <u8 as SerdeDeserialize>::deserialize(deserializer)?;
        Ok(Self::from_u8(code))
    }
}

impl TestStatus {
    /// Storage encoding of this status.
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Decode a storage-encoded status. Unknown codes degrade to `Unknown`.
    #[inline]
    pub const fn from_u8(code: u8) -> Self {
        match code {
            1 => Self::Test,
            _ => Self::Unknown,
        }
    }
}

/// Source of a test determination, ordered by trust: `Ast` > `Path` > `None`.
///
/// `Ast`-level signals (attribute adjacency or constrained conventions) are
/// stronger than file-path conventions. The ordering only decides which
/// marker survives a merge; it never influences behavioral branches.
///
/// The variant discriminant IS the storage encoding, aligned with the trust
/// rank (`None` = 0, `Path` = 1, `Ast` = 2) for SQLite columns and Qdrant
/// payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Archive, RkyvDeserialize, Serialize)]
#[repr(u8)]
pub enum TestSource {
    /// No signal.
    #[default]
    None = 0,
    /// File-path rule determination.
    Path = 1,
    /// AST-level determination (attribute adjacency or constrained convention).
    Ast = 2,
}

impl SerdeSerialize for TestSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.as_u8())
    }
}

impl<'de> SerdeDeserialize<'de> for TestSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = <u8 as SerdeDeserialize>::deserialize(deserializer)?;
        Ok(Self::from_u8(code))
    }
}

impl TestSource {
    /// Storage encoding of this source.
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Decode a storage-encoded source. Unknown codes degrade to `None`.
    #[inline]
    pub const fn from_u8(code: u8) -> Self {
        match code {
            2 => Self::Ast,
            1 => Self::Path,
            _ => Self::None,
        }
    }
}

impl PartialOrd for TestSource {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TestSource {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        rank(*self).cmp(&rank(*other))
    }
}

const fn rank(source: TestSource) -> u8 {
    match source {
        TestSource::Ast => 3,
        TestSource::Path => 2,
        TestSource::None => 1,
    }
}

/// Granularity of a test determination.
///
/// Informational only; never influences logic. A merge result always carries
/// `Group` (see `merge`).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Default,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub enum TestGranularity {
    /// File-level marker, propagated to every chunk in the file.
    File,
    /// Entity-level marker.
    Entity,
    /// Group-level marker (aggregated from members or a merge result).
    #[default]
    Group,
}

/// End-to-end test marker for a chunk/group/entity.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Default,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub struct TestInfo {
    pub status: TestStatus,
    pub source: TestSource,
    pub granularity: TestGranularity,
}

impl TestInfo {
    /// Create a new test info with explicit fields.
    ///
    /// Legal combinations (enforced by debug assertions):
    /// - `Test` + `Ast`/`Path`
    /// - `Unknown` + `None`
    pub fn new(status: TestStatus, source: TestSource, granularity: TestGranularity) -> Self {
        let info = Self::construct(status, source, granularity);
        debug_assert!(
            matches!(
                (info.status, info.source),
                (TestStatus::Test, TestSource::Ast | TestSource::Path)
                    | (TestStatus::Unknown, TestSource::None)
            ),
            "illegal TestInfo combination: status={:?} source={:?}",
            info.status,
            info.source
        );
        info
    }

    /// Const constructor used by the factory methods; skips the debug
    /// assertions (the factories only build legal combinations).
    const fn construct(
        status: TestStatus,
        source: TestSource,
        granularity: TestGranularity,
    ) -> Self {
        Self {
            status,
            source,
            granularity,
        }
    }

    /// No signal at either level.
    pub const fn unknown() -> Self {
        Self::construct(
            TestStatus::Unknown,
            TestSource::None,
            TestGranularity::Group,
        )
    }

    /// AST-level `Test` at entity granularity.
    pub const fn test_ast() -> Self {
        Self::construct(TestStatus::Test, TestSource::Ast, TestGranularity::Entity)
    }

    /// AST-level `Test` at block (suite) granularity.
    pub const fn test_ast_block() -> Self {
        Self::construct(TestStatus::Test, TestSource::Ast, TestGranularity::Group)
    }

    /// Path-level `Test` at file granularity.
    pub const fn test_path() -> Self {
        Self::construct(TestStatus::Test, TestSource::Path, TestGranularity::File)
    }

    /// Whether this marker says the chunk is test code.
    #[inline]
    pub const fn is_test(&self) -> bool {
        matches!(self.status, TestStatus::Test)
    }

    /// Whether no signal is available.
    #[inline]
    pub const fn is_unknown(&self) -> bool {
        matches!(self.status, TestStatus::Unknown)
    }

    /// Merge two markers using the documented priority rules.
    ///
    /// - `Test` wins over `Unknown` (a group is `Test` when any member is
    ///   `Test`); within `Test`, the `Ast` source overrides `Path`.
    /// - Both `Unknown` → `Unknown`.
    /// - The result always carries `Group` granularity: a merged marker spans
    ///   multiple source entities, so the original granularity is no longer
    ///   meaningful.
    ///
    /// Order-independent by construction (takes the maximum along
    /// `(status, source)`), so the merge result never depends on iteration
    /// order of groups or chunks.
    pub fn merge(&self, other: &TestInfo) -> TestInfo {
        let winner = match (self.status, other.status) {
            (TestStatus::Test, TestStatus::Test) => {
                if other.source > self.source {
                    other
                } else {
                    self
                }
            }
            (TestStatus::Test, _) => self,
            (_, TestStatus::Test) => other,
            (TestStatus::Unknown, TestStatus::Unknown) => return TestInfo::unknown(),
        };
        TestInfo {
            granularity: TestGranularity::Group,
            ..*winner
        }
    }

    /// Two-level path determination for a file, dispatched per language.
    ///
    /// Every language shares one unified test-directory rule set (exact
    /// segment match): `tests`, `test`, `__tests__`, `e2e-tests`,
    /// `integration-tests`, `testdata`. Language-specific rules add file-name
    /// conventions:
    ///
    /// | Language | Path rule |
    /// |----------|-----------|
    /// | Rust     | generic test dirs, `*_test.rs` |
    /// | Python   | generic test dirs, `test_*.py`, `conftest.py` |
    /// | Go       | generic test dirs, `*_test.go` |
    /// | JS/TS/JSX/TSX | generic test dirs, `*.spec.*`/`*.test.*` (`js`, `ts`, `mjs`, `cjs`, `mts`, `cts`, `jsx`, `tsx`) |
    /// | Java     | generic test dirs, `*Test.java`, `*Tests.java` |
    /// | Kotlin   | generic test dirs, `*Test.kt`, `*Tests.kt`, `*Spec.kt` |
    /// | C#       | generic test dirs, `*Test.cs`, `*Tests.cs` |
    /// | Dart     | generic test dirs, `test_*.dart` |
    /// | Scala    | generic test dirs |
    /// | PHP      | generic test dirs, `*Test.php` |
    /// | Ruby     | generic test dirs, `spec/` segment |
    /// | C        | generic test dirs, `*_test.c` |
    /// | C++      | generic test dirs, `*_test.cpp`, `*_test.cc` |
    /// | Lua      | generic test dirs, `spec/` segment, `test_*.lua`, `*_test.lua`, `*_spec.lua` |
    /// | Bash     | generic test dirs, `*.bats` |
    /// | Other    | generic test dirs only |
    ///
    /// `language = None` applies the generic test-dir rule only (document and
    /// plain-text files).
    pub fn from_path(language: Option<&Language>, path: &str) -> TestInfo {
        use crate::path::{file_name_str, segments};

        // Separator-agnostic parsing via the shared path helpers, so
        // `\` and `/` are handled identically.
        let file_name = file_name_str(path);
        let segments: Vec<&str> = segments(path);

        // Unified test-directory rules shared by every language. Directory
        // names are matched exactly so `e2e-tests`/`testdata`/`__tests__` are
        // caught while `test_data`/`testsuite` are not.
        const TEST_DIR_SEGMENTS: [&str; 6] = [
            "tests",
            "test",
            "__tests__",
            "e2e-tests",
            "integration-tests",
            "testdata",
        ];
        let matches_generic_test_dir = || segments.iter().any(|s| TEST_DIR_SEGMENTS.contains(s));

        let hit = match language {
            Some(Language::Rust) => file_name.ends_with("_test.rs") || matches_generic_test_dir(),
            Some(Language::Python) => {
                (file_name.starts_with("test_") && file_name.ends_with(".py"))
                    || file_name == "conftest.py"
                    || matches_generic_test_dir()
            }
            Some(Language::Go) => file_name.ends_with("_test.go") || matches_generic_test_dir(),
            Some(Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx) => {
                const TEST_SUFFIXES: [&str; 16] = [
                    ".spec.js",
                    ".spec.ts",
                    ".spec.mjs",
                    ".spec.cjs",
                    ".spec.mts",
                    ".spec.cts",
                    ".spec.jsx",
                    ".spec.tsx",
                    ".test.js",
                    ".test.ts",
                    ".test.mjs",
                    ".test.cjs",
                    ".test.mts",
                    ".test.cts",
                    ".test.jsx",
                    ".test.tsx",
                ];
                TEST_SUFFIXES
                    .iter()
                    .any(|suffix| file_name.ends_with(suffix))
                    || matches_generic_test_dir()
            }
            Some(Language::Java) => {
                (file_name.ends_with("Test.java") || file_name.ends_with("Tests.java"))
                    || matches_generic_test_dir()
            }
            Some(Language::Kotlin) => {
                (file_name.ends_with("Test.kt")
                    || file_name.ends_with("Tests.kt")
                    || file_name.ends_with("Spec.kt"))
                    || matches_generic_test_dir()
            }
            Some(Language::CSharp) => {
                (file_name.ends_with("Test.cs") || file_name.ends_with("Tests.cs"))
                    || matches_generic_test_dir()
            }
            Some(Language::Dart) => {
                (file_name.starts_with("test_") && file_name.ends_with(".dart"))
                    || matches_generic_test_dir()
            }
            Some(Language::Scala) => matches_generic_test_dir(),
            Some(Language::Php) => file_name.ends_with("Test.php") || matches_generic_test_dir(),
            Some(Language::Ruby) => segments.contains(&"spec") || matches_generic_test_dir(),
            Some(Language::C) => file_name.ends_with("_test.c") || matches_generic_test_dir(),
            Some(Language::Cpp) => {
                (file_name.ends_with("_test.cpp") || file_name.ends_with("_test.cc"))
                    || matches_generic_test_dir()
            }
            Some(Language::Lua) => {
                (file_name.starts_with("test_") && file_name.ends_with(".lua"))
                    || file_name.ends_with("_test.lua")
                    || file_name.ends_with("_spec.lua")
                    || segments.contains(&"spec")
                    || matches_generic_test_dir()
            }
            Some(Language::Bash) => file_name.ends_with(".bats") || matches_generic_test_dir(),
            _ => matches_generic_test_dir(),
        };

        if hit {
            TestInfo::test_path()
        } else {
            TestInfo::unknown()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_unknown() {
        let info = TestInfo::default();
        assert!(info.is_unknown());
        assert_eq!(info.source, TestSource::None);
    }

    #[test]
    fn test_path_rules_rust() {
        assert!(TestInfo::from_path(Some(&Language::Rust), "tests/foo.rs").is_test());
        assert!(TestInfo::from_path(Some(&Language::Rust), "src/foo_test.rs").is_test());
        assert!(TestInfo::from_path(Some(&Language::Rust), "crates/a/tests/b.rs").is_test());
        assert!(TestInfo::from_path(Some(&Language::Rust), "src/lib.rs").is_unknown());
        // `latest` / `contest` style names must never match
        assert!(TestInfo::from_path(Some(&Language::Rust), "src/contest.rs").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Rust), "src/testutils.rs").is_unknown());
    }

    #[test]
    fn test_unified_test_directory_rules() {
        // Unified test-dir segments apply to every language
        for (lang, ext) in [
            (Language::Rust, "rs"),
            (Language::Go, "go"),
            (Language::TypeScript, "ts"),
            (Language::CSharp, "cs"),
        ] {
            for dir in [
                "tests",
                "test",
                "__tests__",
                "e2e-tests",
                "integration-tests",
                "testdata",
            ] {
                let path = format!("{dir}/foo.{ext}");
                assert!(
                    TestInfo::from_path(Some(&lang), &path).is_test(),
                    "expected {path} to be a test file"
                );
            }
        }
        // Documents (language = None) share the generic rule
        assert!(TestInfo::from_path(None, "docs/e2e-tests/guide.md").is_test());
        assert!(TestInfo::from_path(None, "docs/tests/README.md").is_test());
        // Non-test directory names must not match
        assert!(TestInfo::from_path(Some(&Language::Rust), "test_data/foo.rs").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Rust), "testsuite/foo.rs").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Python), "docs/spec/design.md").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Python), "test_runner/foo.py").is_unknown());
    }

    #[test]
    fn test_path_rules_python() {
        assert!(TestInfo::from_path(Some(&Language::Python), "tests/test_user.py").is_test());
        assert!(TestInfo::from_path(Some(&Language::Python), "test_user.py").is_test());
        assert!(TestInfo::from_path(Some(&Language::Python), "tests/conftest.py").is_test());
        assert!(TestInfo::from_path(Some(&Language::Python), "src/util.py").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Python), "src/user_test.py").is_unknown());
    }

    #[test]
    fn test_path_rules_go() {
        assert!(TestInfo::from_path(Some(&Language::Go), "user_test.go").is_test());
        assert!(TestInfo::from_path(Some(&Language::Go), "src/user.go").is_unknown());
    }

    #[test]
    fn test_path_rules_js_ts() {
        assert!(TestInfo::from_path(Some(&Language::TypeScript), "user.spec.ts").is_test());
        assert!(TestInfo::from_path(Some(&Language::JavaScript), "user.test.js").is_test());
        assert!(
            TestInfo::from_path(Some(&Language::TypeScript), "src/__tests__/user.ts").is_test()
        );
        assert!(TestInfo::from_path(Some(&Language::TypeScript), "src/user.ts").is_unknown());
    }

    #[test]
    fn test_path_rules_js_ts_extension_variants() {
        // Module variants (`mts`/`cts`/`mjs`/`cjs`)
        assert!(TestInfo::from_path(Some(&Language::TypeScript), "user.spec.mts").is_test());
        assert!(TestInfo::from_path(Some(&Language::TypeScript), "user.test.cts").is_test());
        assert!(TestInfo::from_path(Some(&Language::JavaScript), "user.spec.mjs").is_test());
        assert!(TestInfo::from_path(Some(&Language::JavaScript), "user.test.cjs").is_test());
        // JSX/TSX components
        assert!(TestInfo::from_path(Some(&Language::Tsx), "user.spec.tsx").is_test());
        assert!(TestInfo::from_path(Some(&Language::Tsx), "user.test.tsx").is_test());
        assert!(TestInfo::from_path(Some(&Language::Jsx), "user.spec.jsx").is_test());
        assert!(TestInfo::from_path(Some(&Language::Jsx), "user.test.jsx").is_test());
        // Negative cases
        assert!(TestInfo::from_path(Some(&Language::Tsx), "user.tsx").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Jsx), "user.jsx").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::TypeScript), "user.testutil.ts").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::TypeScript), "src/contest.ts").is_unknown());
    }

    #[test]
    fn test_path_rules_java() {
        assert!(TestInfo::from_path(Some(&Language::Java), "UserServiceTest.java").is_test());
        assert!(TestInfo::from_path(Some(&Language::Java), "UserServiceTests.java").is_test());
        assert!(
            TestInfo::from_path(Some(&Language::Java), "src/test/java/UserService.java").is_test()
        );
        assert!(
            TestInfo::from_path(Some(&Language::Java), "src/main/UserService.java").is_unknown()
        );
        assert!(TestInfo::from_path(Some(&Language::Java), "src/contest.java").is_unknown());
    }

    #[test]
    fn test_path_rules_csharp() {
        assert!(TestInfo::from_path(Some(&Language::CSharp), "CalculatorTests.cs").is_test());
        assert!(TestInfo::from_path(Some(&Language::CSharp), "CalculatorTest.cs").is_test());
        assert!(TestInfo::from_path(Some(&Language::CSharp), "tests/Calculator.cs").is_test());
        // `TestRunner` / `Contest` style names must never match
        assert!(TestInfo::from_path(Some(&Language::CSharp), "src/TestRunner.cs").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::CSharp), "src/Contest.cs").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::CSharp), "src/Calculator.cs").is_unknown());
    }

    #[test]
    fn test_path_rules_dart() {
        assert!(TestInfo::from_path(Some(&Language::Dart), "test/user_test.dart").is_test());
        assert!(TestInfo::from_path(Some(&Language::Dart), "test/user.dart").is_test());
        assert!(TestInfo::from_path(Some(&Language::Dart), "lib/user.dart").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Dart), "lib/testMode.dart").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Dart), "lib/tester.dart").is_unknown());
    }

    #[test]
    fn test_path_rules_scala() {
        assert!(TestInfo::from_path(Some(&Language::Scala), "src/test/scala/Foo.scala").is_test());
        assert!(TestInfo::from_path(Some(&Language::Scala), "src/test/FooSpec.scala").is_test());
        assert!(TestInfo::from_path(Some(&Language::Scala), "tests/Foo.scala").is_test());
        assert!(TestInfo::from_path(Some(&Language::Scala), "src/main/Foo.scala").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Scala), "src/contest.scala").is_unknown());
    }

    #[test]
    fn test_path_rules_lua() {
        assert!(TestInfo::from_path(Some(&Language::Lua), "test/test_calc.lua").is_test());
        assert!(TestInfo::from_path(Some(&Language::Lua), "test_calc.lua").is_test());
        assert!(TestInfo::from_path(Some(&Language::Lua), "tests/test_calc.lua").is_test());
        assert!(TestInfo::from_path(Some(&Language::Lua), "spec/calc_spec.lua").is_test());
        assert!(TestInfo::from_path(Some(&Language::Lua), "calc_spec.lua").is_test());
        assert!(TestInfo::from_path(Some(&Language::Lua), "calc_test.lua").is_test());
        // `latest` / `contest` / `testMode` style names must never match
        assert!(TestInfo::from_path(Some(&Language::Lua), "src/calc.lua").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Lua), "src/testMode.lua").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Lua), "src/contest.lua").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Lua), "src/latest.lua").is_unknown());
    }

    #[test]
    fn test_path_rules_bash() {
        // bats files are 100% test files; `.sh` relies on the generic rule
        assert!(TestInfo::from_path(Some(&Language::Bash), "math.bats").is_test());
        assert!(TestInfo::from_path(Some(&Language::Bash), "tests/math.bats").is_test());
        assert!(TestInfo::from_path(Some(&Language::Bash), "tests/math.sh").is_test());
        assert!(TestInfo::from_path(Some(&Language::Bash), "src/math.sh").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Bash), "src/contest.sh").is_unknown());
    }

    #[test]
    fn test_path_rules_kotlin() {
        assert!(TestInfo::from_path(Some(&Language::Kotlin), "UserServiceTest.kt").is_test());
        assert!(TestInfo::from_path(Some(&Language::Kotlin), "UserServiceTests.kt").is_test());
        assert!(TestInfo::from_path(Some(&Language::Kotlin), "UserServiceSpec.kt").is_test());
        assert!(
            TestInfo::from_path(Some(&Language::Kotlin), "src/test/kotlin/UserService.kt")
                .is_test()
        );
        assert!(
            TestInfo::from_path(Some(&Language::Kotlin), "src/main/UserService.kt").is_unknown()
        );
        assert!(TestInfo::from_path(Some(&Language::Kotlin), "src/contest.kt").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Kotlin), "src/TestRunner.kt").is_unknown());
        // Java and Kotlin suffixes must not leak across languages
        assert!(
            TestInfo::from_path(Some(&Language::Kotlin), "src/UserServiceTest.java").is_unknown()
        );
        assert!(TestInfo::from_path(Some(&Language::Java), "src/UserServiceSpec.kt").is_unknown());
    }

    #[test]
    fn test_path_rules_php() {
        assert!(TestInfo::from_path(Some(&Language::Php), "CalculatorTest.php").is_test());
        assert!(TestInfo::from_path(Some(&Language::Php), "tests/Calculator.php").is_test());
        assert!(TestInfo::from_path(Some(&Language::Php), "src/TestRunner.php").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Php), "src/Contest.php").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Php), "src/Calculator.php").is_unknown());
    }

    #[test]
    fn test_path_rules_ruby() {
        assert!(TestInfo::from_path(Some(&Language::Ruby), "spec/models/user_spec.rb").is_test());
        assert!(TestInfo::from_path(Some(&Language::Ruby), "tests/user_test.rb").is_test());
        assert!(TestInfo::from_path(Some(&Language::Ruby), "lib/user.rb").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Ruby), "lib/contest.rb").is_unknown());
    }

    #[test]
    fn test_path_rules_c_cpp() {
        assert!(TestInfo::from_path(Some(&Language::C), "math_test.c").is_test());
        assert!(TestInfo::from_path(Some(&Language::C), "src/math.c").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::C), "src/latest.c").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Cpp), "math_test.cpp").is_test());
        assert!(TestInfo::from_path(Some(&Language::Cpp), "math_test.cc").is_test());
        assert!(TestInfo::from_path(Some(&Language::Cpp), "tests/math.cpp").is_test());
        assert!(TestInfo::from_path(Some(&Language::Cpp), "src/math.cpp").is_unknown());
        assert!(TestInfo::from_path(Some(&Language::Cpp), "src/latest.cpp").is_unknown());
    }

    #[test]
    fn test_generic_path_rule() {
        assert!(TestInfo::from_path(None, "tests/readme.md").is_test());
        assert!(TestInfo::from_path(None, "docs/readme.md").is_unknown());
    }

    #[test]
    fn test_merge_test_wins_over_unknown() {
        let path = TestInfo::test_path();
        let unknown = TestInfo::unknown();
        assert!(path.merge(&unknown).is_test());
        assert!(unknown.merge(&path).is_test());
        assert_eq!(path.merge(&unknown).source, TestSource::Path);
        assert_eq!(unknown.merge(&path).source, TestSource::Path);
    }

    #[test]
    fn test_merge_ast_overrides_path() {
        let path = TestInfo::test_path();
        let ast = TestInfo::test_ast();
        let merged = path.merge(&ast);
        assert!(merged.is_test());
        assert_eq!(merged.source, TestSource::Ast);
        let merged = ast.merge(&path);
        assert!(merged.is_test());
        assert_eq!(merged.source, TestSource::Ast);
    }

    #[test]
    fn test_merge_is_order_independent() {
        let path = TestInfo::test_path();
        let ast = TestInfo::test_ast();
        let unknown = TestInfo::unknown();
        assert_eq!(path.merge(&ast), ast.merge(&path));
        assert_eq!(path.merge(&unknown), unknown.merge(&path));
        assert_eq!(unknown.merge(&unknown), TestInfo::unknown());
    }

    #[test]
    fn test_merge_granularity_always_group() {
        let file = TestInfo::test_path();
        let entity = TestInfo::test_ast();
        assert_eq!(
            file.merge(&unknown_test_info()).granularity,
            TestGranularity::Group
        );
        assert_eq!(file.merge(&entity).granularity, TestGranularity::Group);
        assert_eq!(
            file.merge(&TestInfo::unknown()).granularity,
            TestGranularity::Group
        );
    }

    fn unknown_test_info() -> TestInfo {
        TestInfo::unknown()
    }

    #[test]
    fn test_factories_carry_expected_fields() {
        assert_eq!(TestInfo::test_ast().granularity, TestGranularity::Entity);
        assert_eq!(
            TestInfo::test_ast_block().granularity,
            TestGranularity::Group
        );
        assert_eq!(TestInfo::test_path().granularity, TestGranularity::File);
        assert_eq!(TestInfo::test_ast().source, TestSource::Ast);
        assert_eq!(TestInfo::test_path().source, TestSource::Path);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let info = TestInfo::test_ast();
        let json = serde_json::to_string(&info).unwrap();
        let back: TestInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }
}
