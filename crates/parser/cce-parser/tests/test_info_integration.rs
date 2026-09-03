//! End-to-end test detection integration
//!
//! Exercises the real chain: AstParser → EntityExtractor → PreprocessingPipeline
//! and verifies `test_info` markers are produced from AST attributes
//! (`#[test]`, `#[cfg(test)]`) — not only from file-path rules.

use cce_types::language::Language;
use cce_types::test_info::{TestSource, TestStatus};

use cce_parser::grouper::pipeline::PreprocessingPipeline;
use cce_parser::grouper::types::GroupType;
use cce_parser::parser::ast_parser::AstParser;
use cce_parser::parser::extractor::EntityExtractor;

fn parse_entities(code: &str, language: Language, path: &str) -> cce_types::entity::ParsedFile {
    use cce_types::entity::ParsedFile;

    let mut ast_parser = AstParser::new();
    let extractor = EntityExtractor::new();
    let (tree, _) = ast_parser
        .parse_with_tree(code, &language)
        .expect("failed to parse");

    let mut parsed = ParsedFile::new(language, path.to_string(), code);
    let entities = extractor
        .extract(&tree, code, &language)
        .expect("failed to extract");
    for e in entities {
        parsed.add_entity(e);
    }
    parsed
}

#[test]
fn test_ast_detection_rust_mod_tests() {
    let code = r#"
#[cfg(test)]
mod tests {
    #[test]
    fn test_login() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_logout() {
        assert!(true);
    }
}

fn latest() -> u32 {
    42
}
"#;
    let parsed = parse_entities(code, Language::Rust, "src/lib.rs");
    assert!(!parsed.entities.is_empty(), "should extract entities");

    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let tests_group = result
        .groups
        .iter()
        .filter(|g| g.test_info.status == TestStatus::Test)
        .collect::<Vec<_>>();
    assert!(
        !tests_group.is_empty(),
        "AST-level detection must mark test groups; got {} groups: {:?}",
        result.groups.len(),
        result
            .groups
            .iter()
            .map(|g| (g.name.as_str(), g.group_type))
            .collect::<Vec<_>>()
    );

    for g in &tests_group {
        assert_eq!(
            g.test_info.source,
            TestSource::Ast,
            "group {} must be AST-marked",
            g.name
        );
    }

    // `latest` is production code, must NOT be marked test.
    let latest = result
        .groups
        .iter()
        .find(|g| g.name == "latest")
        .expect("latest group should exist");
    assert!(
        !latest.test_info.is_test(),
        "production function `latest` must not be marked test"
    );
}

#[test]
fn test_ast_detection_rust_standalone_test_fn() {
    let code = r#"
#[test]
fn test_direct_attr() {
    assert!(true);
}

#[tokio::test]
async fn test_async_attr() {
    assert!(true);
}

#[async_std::test]
async fn test_async_std_attr() {
    assert!(true);
}

fn normal_fn() -> i32 {
    1
}
"#;
    let parsed = parse_entities(code, Language::Rust, "src/helpers.rs");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_groups: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .collect();
    assert!(
        !test_groups.is_empty(),
        "at least one test group expected, got: {:?}",
        result
            .groups
            .iter()
            .map(|g| (g.name.as_str(), g.test_info))
            .collect::<Vec<_>>()
    );
    let test_entity_ids: Vec<_> = test_groups
        .iter()
        .flat_map(|g| g.all_entity_ids())
        .collect();
    let test_names: Vec<_> = result
        .groups
        .iter()
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .filter(|e| test_entity_ids.contains(&e.id))
        .map(|e| e.name.as_str())
        .collect();
    for name in ["test_direct_attr", "test_async_attr", "test_async_std_attr"] {
        assert!(
            test_names.contains(&name),
            "`{}` must end up in a test group, got: {:?}",
            name,
            test_names
        );
    }

    // Production function must be isolated from test groups — a test group
    // containing it would be filtered out by the no-test evaluation variant.
    let normal_group = result
        .groups
        .iter()
        .find(|g| g.all_entity_ids().contains(&parsed.entities[3].id))
        .expect("normal_fn group should exist");
    assert!(
        !normal_group.test_info.is_test(),
        "production function `normal_fn` must not share a test group"
    );
}

#[test]
fn test_cfg_test_on_non_module_item() {
    // `#[cfg(test)]` on a plain function (not `mod tests`) must still be
    // detected via the preserved metadata.
    let code = r#"
#[cfg(test)]
fn test_helper() -> u32 {
    9
}

#[cfg(test)]
struct TestFixture {
    value: u32,
}

fn production() -> u32 {
    1
}
"#;
    let parsed = parse_entities(code, Language::Rust, "src/util.rs");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_groups: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .collect();
    let test_names: Vec<_> = test_groups
        .iter()
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        test_names.contains(&"test_helper"),
        "`#[cfg(test)]` fn must be marked test, got: {:?}",
        test_names
    );
    assert!(
        test_names.contains(&"TestFixture"),
        "`#[cfg(test)]` struct must be marked test, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"production"),
        "production fn must not be marked test"
    );
}

#[test]
fn test_inner_attribute_not_leaked() {
    // `#![cfg(test)]` is a file-level inner attribute; it must never be
    // attributed to the first entity of the file.
    let code = r#"
#![cfg(test)]

fn production() -> u32 {
    1
}
"#;
    let parsed = parse_entities(code, Language::Rust, "src/lib.rs");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let production = result
        .groups
        .iter()
        .find(|g| g.name == "production")
        .expect("production group should exist");
    assert!(
        !production.test_info.is_test(),
        "file-level `#![cfg(test)]` must not leak onto `production`"
    );
}

#[test]
fn test_cfg_test_mod_becomes_test_suite_group() {
    let code = r#"
#[cfg(test)]
mod tests {
    #[test]
    fn test_a() {
        assert!(true);
    }

    #[test]
    fn test_b() {
        assert!(true);
    }
}
"#;
    let parsed = parse_entities(code, Language::Rust, "src/lib.rs");

    let suite_entities = parsed
        .entities
        .iter()
        .filter(|e| e.kind == cce_types::entity::EntityKind::TestSuite)
        .collect::<Vec<_>>();
    assert_eq!(
        suite_entities.len(),
        1,
        "`#[cfg(test)] mod tests` must be promoted to TestSuite"
    );
    assert_eq!(suite_entities[0].name, "tests");

    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let suite_group = result
        .groups
        .iter()
        .find(|g| g.group_type == GroupType::TestSuiteWithCases);
    assert!(
        suite_group.is_some(),
        "TestSuiteProcessor must group the suite with its cases, got: {:?}",
        result
            .groups
            .iter()
            .map(|g| (g.name.as_str(), g.group_type))
            .collect::<Vec<_>>()
    );
    let suite_group = suite_group.expect("suite group");
    assert!(suite_group.test_info.is_test(), "suite group must be test");
    assert_eq!(suite_group.members.len(), 2, "suite must contain 2 cases");
}

#[test]
fn test_java_test_annotation_detection() {
    // Java `@Test` methods must be detected through the buffered annotation
    // mechanism (previously the annotation entities were dropped entirely).
    let code = r#"
import org.junit.jupiter.api.Test;

class UserService {
    @Test
    void shouldLogin() {
        assertTrue(true);
    }

    void helper() {
    }
}

class UserServiceTest {
    @Test
    void shouldLogout() {
        assertTrue(true);
    }
}
"#;
    let parsed = parse_entities(
        code,
        Language::Java,
        "src/main/java/com/acme/UserService.java",
    );
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_groups: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .collect();
    let test_names: Vec<_> = test_groups
        .iter()
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        test_names.contains(&"shouldLogin"),
        "Java `@Test` method must be marked test via annotation buffering, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"helper"),
        "plain helper in a non-test class must not be marked test, got: {:?}",
        test_names
    );
    assert!(
        test_names.contains(&"UserServiceTest"),
        "`*Test` class must be marked test via naming convention, got: {:?}",
        test_names
    );
}

#[test]
fn test_path_rule_alone_detects_tests_dir() {
    // No AST attributes at all — the file-path rule must still fire.
    let code = "pub fn helper() -> u32 { 7 }\npub fn other() -> u32 { 8 }";
    let parsed = parse_entities(code, Language::Rust, "tests/integration.rs");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    assert!(
        result.groups.iter().all(|g| g.test_info.is_test()),
        "all groups in tests/ dir must be path-marked test"
    );
    assert!(
        result
            .groups
            .iter()
            .all(|g| g.test_info.source == TestSource::Path),
        "path-rule markers must be TestSource::Path"
    );
}

#[test]
fn test_csharp_test_annotation_detection() {
    // C# `[Test]`/`[TestCase]`/`[Fact]`/`[Theory]` methods must be detected
    // through the buffered attribute mechanism; `*Tests` classes via naming.
    let code = r#"
using NUnit.Framework;

namespace MyApp.Tests
{
    [TestFixture]
    public class CalculatorTests
    {
        [Test]
        public void Add_ReturnsSum()
        {
            Assert.AreEqual(4, 2 + 2);
        }

        [TestCase(1, 2, 3)]
        public void Add_Parameterized(int a, int b, int expected)
        {
        }
    }

    public class TestRunner
    {
        public void Run()
        {
        }
    }
}
"#;
    let parsed = parse_entities(code, Language::CSharp, "src/Calculator.cs");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_names: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        test_names.contains(&"Add_ReturnsSum"),
        "C# `[Test]` method must be marked test, got: {:?}",
        test_names
    );
    assert!(
        test_names.contains(&"Add_Parameterized"),
        "C# `[TestCase]` method must be marked test, got: {:?}",
        test_names
    );
    assert!(
        test_names.contains(&"CalculatorTests"),
        "C# `*Tests` class must be marked test via naming convention, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"TestRunner"),
        "`TestRunner` must never be marked test, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"Run"),
        "plain method in `TestRunner` must not be marked test, got: {:?}",
        test_names
    );
}

#[test]
fn test_csharp_path_rule_detects_test_file() {
    // C# test files without any attribute/convention signal still get
    // path-level marking via the `*Tests.cs`/`tests/` rules.
    let code = "public class Helper { public int Add(int a, int b) { return a + b; } }";
    let parsed = parse_entities(code, Language::CSharp, "tests/CalculatorHelper.cs");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);
    assert!(
        result.groups.iter().all(|g| g.test_info.is_test()),
        "all groups in C# tests/ dir must be path-marked test"
    );
}

#[test]
fn test_scala_test_annotation_and_spec_class_detection() {
    let code = r#"
import org.junit.Test

class CalculatorSpec {
  @Test
  def add(): Unit = ()
}

class UserService {
  @tailrec
  def helper(n: Int): Int = n
}
"#;
    let parsed = parse_entities(code, Language::Scala, "src/main/scala/Foo.scala");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_names: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        test_names.contains(&"add"),
        "Scala `@Test` def must be marked test, got: {:?}",
        test_names
    );
    assert!(
        test_names.contains(&"CalculatorSpec"),
        "Scala `*Spec` class must be marked test via naming convention, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"UserService"),
        "production class must not be marked test, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"helper"),
        "`@tailrec` helper must not be marked test, got: {:?}",
        test_names
    );
}

#[test]
fn test_php_test_attribute_and_docblock_detection() {
    let code = r#"<?php
namespace App\Tests;

use PHPUnit\Framework\Attributes\Test;

class CalculatorTest
{
    #[Test]
    public function testAdd(): void {}

    /**
     * @test
     * Long description.
     */
    public function docTest(): void {}
}

class ProductionService
{
    public function helper(): void {}
}
"#;
    let parsed = parse_entities(code, Language::Php, "src/Calculator.php");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_names: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        test_names.contains(&"testAdd"),
        "PHP `#[Test]` method must be marked test, got: {:?}",
        test_names
    );
    assert!(
        test_names.contains(&"docTest"),
        "PHP `@test` docblock method must be marked test, got: {:?}",
        test_names
    );
    assert!(
        test_names.contains(&"CalculatorTest"),
        "PHP `*Test` class must be marked test via naming convention, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"helper"),
        "plain helper must not be marked test, got: {:?}",
        test_names
    );
}

#[test]
fn test_cpp_test_macro_detection() {
    let code = r#"
#include <gtest/gtest.h>

TEST(MathTest, Add) {
  EXPECT_EQ(4, 2 + 2);
}

TEST_F(Fixture, Subtract) {
  EXPECT_EQ(2, 4 - 2);
}

TEST(SingleArg) {}

TEST(A, B, C) {}

int TEST(int a, int b) {
  return a + b;
}

int latest(int x) {
  return x + 1;
}
"#;
    let parsed = parse_entities(code, Language::Cpp, "src/math.cpp");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_names: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        test_names.contains(&"TEST"),
        "`TEST(MathTest, Add)` must be marked test, got: {:?}",
        test_names
    );
    assert!(
        test_names.contains(&"TEST_F"),
        "`TEST_F(Fixture, Subtract)` must be marked test, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"latest"),
        "production function `latest` must not be marked test, got: {:?}",
        test_names
    );
}

#[test]
fn test_cpp_test_macro_wrong_shapes_not_detected() {
    let code = r#"
TEST(SingleArg) {}
TEST(A, B, C) {}
int TEST(int a, int b) { return a + b; }
"#;
    let parsed = parse_entities(code, Language::Cpp, "src/math.cpp");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_groups: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .collect();
    assert!(
        test_groups.is_empty(),
        "wrong-shape `TEST` invocations must never be marked test, got: {:?}",
        result
            .groups
            .iter()
            .map(|g| (g.name.as_str(), g.test_info))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_cpp_path_rule_detects_test_file() {
    let code = "int helper() { return 1; }";
    let parsed = parse_entities(code, Language::Cpp, "src/math_test.cpp");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);
    assert!(
        result.groups.iter().all(|g| g.test_info.is_test()),
        "all groups in C++ `*_test.cpp` file must be path-marked test"
    );
}

#[test]
fn test_lua_luaunit_naming_detection() {
    // luaunit `test_*` functions, `test*` table fields and `test*` methods in
    // a test directory must be AST-marked via the constrained naming
    // convention; non-test names stay path-marked only.
    let code = r#"
require("luaunit")

function test_addition()
  assertEquals(4, 2 + 2)
end

TestCalculator = {
  test_add = function(self)
    assertEquals(2, 1 + 1)
  end,
}

function TestCalculator:test1()
  assertEquals(2, 4 - 2)
end

function testMode()
  return "flag"
end

function helper()
  return 1
end
"#;
    let parsed = parse_entities(code, Language::Lua, "tests/test_calc.lua");
    assert!(
        parsed.entities.iter().any(|e| e.name == "test_addition"),
        "luaunit function must be extracted, got: {:?}",
        parsed
            .entities
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>()
    );
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_groups: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .collect();
    let test_names: Vec<_> = test_groups
        .iter()
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .map(|e| e.name.as_str())
        .collect();
    for name in ["test_addition", "test_add", "test1"] {
        assert!(
            test_names.contains(&name),
            "luaunit `{name}` must be marked test, got: {:?}",
            test_names
        );
    }
    let ast_group_names: Vec<_> = test_groups
        .iter()
        .filter(|g| g.test_info.source == TestSource::Ast)
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .map(|e| e.name.as_str())
        .collect();
    for name in ["test_addition", "test_add", "test1"] {
        assert!(
            ast_group_names.contains(&name),
            "luaunit `{name}` must be AST-marked via naming, got: {:?}",
            ast_group_names
        );
    }
    // Every entity of a `tests/`-dir file is path-marked test; the whole file
    // ends up in test groups (small fragments merge with adjacent test
    // groups, correctly keeping the `Test` marker).
    assert!(
        result.groups.iter().all(|g| g.test_info.is_test()),
        "all groups in a `tests/` Lua file must be test, got: {:?}",
        result
            .groups
            .iter()
            .map(|g| (g.name.as_str(), g.test_info))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_lua_naming_not_detected_outside_test_file() {
    // The same luaunit-shaped names in a regular source file must not be
    // marked test at all.
    let code = r#"
function test_addition()
  return 2 + 2
end

function testify()
  return "fake"
end
"#;
    let parsed = parse_entities(code, Language::Lua, "src/math.lua");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);
    assert!(
        result.groups.iter().all(|g| !g.test_info.is_test()),
        "production Lua functions must not be marked test, got: {:?}",
        result
            .groups
            .iter()
            .map(|g| (g.name.as_str(), g.test_info))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_bash_bats_path_rule_detects_test_file() {
    // `@test` blocks parse as plain commands without entities (verified AST
    // shape); the `*.bats` path rule must mark every entity in the file.
    let code = r#"
@test "addition using bc" {
  result="$(echo 1+1 | bc)"
  [ "$result" -eq 2 ]
}

@test "invoking foo with no arguments exits with status 2" {
  run foo
  [ "$status" -eq 2 ]
}

setup() {
  load helpers
}

function parse_flags() {
  echo "x"
}
"#;
    let parsed = parse_entities(code, Language::Bash, "test_math.bats");
    assert!(
        parsed.entities.iter().any(|e| e.name == "parse_flags"),
        "bats file functions must be extracted, got: {:?}",
        parsed
            .entities
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>()
    );
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);
    assert!(
        result.groups.iter().all(|g| g.test_info.is_test()),
        "all groups in a `.bats` file must be path-marked test, got: {:?}",
        result
            .groups
            .iter()
            .map(|g| (g.name.as_str(), g.test_info))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_bash_sh_not_test_outside_tests_dir() {
    let code = "function parse_flags() { echo \"x\"; }\nfunction latest() { echo \"y\"; }";
    let parsed = parse_entities(code, Language::Bash, "src/math.sh");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);
    assert!(
        result.groups.iter().all(|g| !g.test_info.is_test()),
        "production bash functions must not be marked test, got: {:?}",
        result
            .groups
            .iter()
            .map(|g| (g.name.as_str(), g.test_info))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_kotlin_spec_class_and_annotation_detection() {
    // Kotest `*Spec` classes via the Kotlin-only naming convention; JUnit
    // `@Test` methods via annotation buffering.
    let code = r#"
import org.junit.Test
import io.kotest.core.spec.style.FunSpec

class CalculatorSpec : FunSpec({
    test("addition") {
        4 shouldBe 4
    }
})

class UserService {
    @Test
    fun shouldLogin() {
        assert(true)
    }

    fun helper(): Int = 1
}

class TestRunner {
    fun run() {}
}
"#;
    let parsed = parse_entities(code, Language::Kotlin, "src/main/kotlin/acme/Foo.kt");
    assert!(
        parsed.entities.iter().any(|e| e.name == "CalculatorSpec"),
        "Kotest class must be extracted, got: {:?}",
        parsed
            .entities
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>()
    );
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);

    let test_names: Vec<_> = result
        .groups
        .iter()
        .filter(|g| g.test_info.is_test())
        .flat_map(|g| g.all_entity_ids())
        .filter_map(|id| parsed.entities.iter().find(|e| e.id == id))
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        test_names.contains(&"CalculatorSpec"),
        "Kotest `*Spec` class must be marked test via naming convention, got: {:?}",
        test_names
    );
    assert!(
        test_names.contains(&"shouldLogin"),
        "Kotlin `@Test` method must be marked test via annotation buffering, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"helper"),
        "plain helper must not be marked test, got: {:?}",
        test_names
    );
    assert!(
        !test_names.contains(&"TestRunner"),
        "`TestRunner` must never be marked test, got: {:?}",
        test_names
    );
}

#[test]
fn test_kotlin_path_rule_detects_spec_file() {
    let code = "class Helper { fun add(a: Int, b: Int): Int = a + b }";
    let parsed = parse_entities(code, Language::Kotlin, "src/CalculatorSpec.kt");
    let pipeline = PreprocessingPipeline::new();
    let result = pipeline.process(&parsed);
    assert!(
        result.groups.iter().all(|g| g.test_info.is_test()),
        "all groups in a `*Spec.kt` file must be path-marked test"
    );
}
