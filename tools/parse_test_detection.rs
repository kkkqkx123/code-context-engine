//! Test-detection AST shape verification tool
//!
//! Dumps tree-sitter parse trees for the test-related constructs of the
//! languages covered by `docs/plan/language_test_coverage.md`, to verify
//! node shapes before writing detection code:
//!
//! - C#: `[Test]` / `[TestCase(...)]` / `[Fact]` / `[Theory]` attributes
//! - Scala: `@Test` annotations (bare and with arguments)
//! - PHP: `#[Test]` attributes (PHP 8+) and `@test` docblocks
//! - Ruby: `describe` / `it` calls (RSpec)
//! - Dart: top-level `test()` / `group()` calls (package:test)
//! - C++: `TEST()` / `TEST_F()` / `TEST_CASE()` macro invocations
//! - C: plain functions named like `test_*`
//!
//! ## Usage
//!
//! ```bash
//! # Standalone (with the tree-sitter rlibs on the extern path), e.g.:
//! rustc --edition 2024 tools/parse_test_detection.rs -L target/debug/deps \
//!   --extern tree_sitter=$(ls target/debug/deps/libtree_sitter-*.rlib | head -1) \
//!   --extern tree_sitter_c_sharp=... && ./parse_test_detection [case_name]
//! ```
//!
//! Without arguments all cases are dumped.

use std::env;

struct Case {
    name: &'static str,
    lang_name: &'static str,
    code: &'static str,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "csharp_attributes",
            lang_name: "csharp",
            code: r#"
using NUnit.Framework;

public class CalculatorTests
{
    [Test]
    public void Add() {}

    [TestCase(1, 2, 3)]
    public void Add_Parameterized(int a, int b, int expected) {}

    [Fact]
    public void Multiply() {}
}
"#,
        },
        Case {
            name: "scala_annotations",
            lang_name: "scala",
            code: r#"
import org.junit.Test

class CalculatorSpec {
  @Test
  def add(): Unit = ()

  @Test(timeout = 100)
  def sub(): Unit = ()

  @tailrec
  def helper(n: Int): Int = n
}
"#,
        },
        Case {
            name: "php_attributes_and_docblock",
            lang_name: "php",
            code: r#"<?php
namespace App\Tests;

use PHPUnit\Framework\Attributes\Test;

class CalculatorTest
{
    #[Test]
    public function testAdd(): void {}

    #[PHPUnit\Framework\Attributes\Test]
    public function testQualified(): void {}

    /**
     * @test
     * Long description line.
     */
    public function docTest(): void {}

    /** @testdox Something not a test marker. */
    public function testdoxCase(): void {}
}
"#,
        },
        Case {
            name: "ruby_rspec_calls",
            lang_name: "ruby",
            code: r#"
describe 'Calculator' do
  it 'adds' do
    expect(2 + 2).to eq(4)
  end

  it 'subtracts' do
    expect(4 - 2).to eq(2)
  end
end

it 'top level case' do
  expect(1).to eq(1)
end

items.each { |it| puts it }
"#,
        },
        Case {
            name: "dart_test_calls",
            lang_name: "dart",
            code: r#"
import 'package:test/test.dart';

void main() {
  test('adds numbers', () {
    expect(2 + 2, 4);
  });

  group('math', () {
    test('subtracts', () {
      expect(4 - 2, 2);
    });
  });
}
"#,
        },
        Case {
            name: "cpp_gtest_macros",
            lang_name: "cpp",
            code: r#"
#include <gtest/gtest.h>

TEST(MathTest, Add) {
  EXPECT_EQ(4, 2 + 2);
}

TEST_F(Fixture, Subtract) {
  EXPECT_EQ(2, 4 - 2);
}

TEST_CASE("math", "[basic]") {
  REQUIRE(4 == 4);
}

TEST(MathTest) {}

TEST(A, B, C) {}

int TEST(int a, int b) {
  return a + b;
}

int latest(int x) {
  return x + 1;
}
"#,
        },
        Case {
            name: "c_test_functions",
            lang_name: "c",
            code: r#"
void test_runner(void) {}

int latest(int x) {
  return x + 1;
}
"#,
        },
        Case {
            name: "lua_luaunit",
            lang_name: "lua",
            code: r#"
require("luaunit")

function test_addition()
  assertEquals(4, 2 + 2)
end

TestCalculator = {
  test_add = function(self)
    assertEquals(4, 2 + 2)
  end,
}

function TestCalculator:testSubtract()
  assertEquals(2, 4 - 2)
end

function helper()
  return 1
end

local testify = function()
  return "fake"
end
"#,
        },
        Case {
            name: "lua_busted",
            lang_name: "lua",
            code: r#"
describe("addition", function()
  it("adds two numbers", function()
    assert.are.equal(4, 2 + 2)
  end)
end)

context("multiplication", function()
  it("multiplies", function()
    assert.are.equal(6, 2 * 3)
  end)
end)
"#,
        },
        Case {
            name: "bash_bats",
            lang_name: "bash",
            code: r#"
#!/usr/bin/env bats

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

function test_helper() {
  echo "shunit2 style"
}
"#,
        },
        Case {
            name: "kotlin_annotations",
            lang_name: "kotlin",
            code: r#"
import org.junit.Test

class UserService {
    @Test
    fun shouldLogin() {}

    @Test(timeout = 100)
    fun shouldLogout() {}

    @Suppress("UNUSED")
    fun helper(): Int = 1
}
"#,
        },
        Case {
            name: "kotlin_kotest",
            lang_name: "kotlin",
            code: r#"
import io.kotest.core.spec.style.FunSpec

class CalculatorSpec : FunSpec({
    test("addition") {
        4 shouldBe (2 + 2)
    }
})

class UserService {
    fun helper(): Int = 1
}

class TestRunner {
    fun run() {}
}
"#,
        },
    ]
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let filter = if args.len() > 1 { Some(args[1].as_str()) } else { None };

    for case in cases() {
        if let Some(f) = filter {
            if !case.name.contains(f) {
                continue;
            }
        }

        println!("\n========================================");
        println!("Case: {} [{}]", case.name, case.lang_name);
        println!("Code: {}", case.code);
        println!("========================================\n");

        let language = lang(case.lang_name);
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .expect("Failed to set language");

        let tree = parser
            .parse(case.code, None)
            .expect("Failed to parse");
        print_tree(tree.root_node(), case.code, 0);
    }
}

fn lang(name: &'static str) -> tree_sitter::Language {
    match name {
        "csharp" => tree_sitter_c_sharp::LANGUAGE.into(),
        "scala" => tree_sitter_scala::LANGUAGE.into(),
        "php" => tree_sitter_php::LANGUAGE_PHP.into(),
        "ruby" => tree_sitter_ruby::LANGUAGE.into(),
        "dart" => tree_sitter_dart::LANGUAGE.into(),
        "cpp" => tree_sitter_cpp::LANGUAGE.into(),
        "c" => tree_sitter_c::LANGUAGE.into(),
        "lua" => tree_sitter_lua::LANGUAGE.into(),
        "bash" => tree_sitter_bash::LANGUAGE.into(),
        "kotlin" => tree_sitter_kotlin_ng::LANGUAGE.into(),
        other => panic!("unknown language: {other}"),
    }
}

fn print_tree(node: tree_sitter::Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = if node.child_count() == 0 {
        let start = node.start_byte();
        let end = node.end_byte();
        let end = end.min(source.len());
        &source[start.min(end)..end]
    } else {
        ""
    };

    println!("{}{}: {:?}", indent, node.kind(), text);

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            if let Some(field_name) = node.field_name_for_child(i as u32) {
                println!("{}  [field: {}]", indent, field_name);
            }
            print_tree(child, source, depth + 1);
        }
    }
}
