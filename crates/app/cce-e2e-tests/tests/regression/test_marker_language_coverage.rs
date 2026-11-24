//! Language-coverage test-marker regression tests (C# / Dart / Scala / PHP /
//! C++)
//!
//! Offline verification of the benchmark evaluation machinery (`NoTest`
//! variant filtering + `TestDiagnostics` statistics) against chunk data
//! generated from C#/Dart/C++/Scala/PHP sources — no embedding service or
//! Qdrant needed (BM25-only dataset; the embedding path is left empty and
//! `collect_test_diagnostics` reports it as-is).
//!
//! Covers the plan's per-language counter-examples:
//! - C#: `TestRunner.cs` must never be marked; `[Test]`/`[TestCase]` methods
//!   and the `*Tests` class must be marked.
//! - Dart: `test/` path rule marks the file; `testMode.dart` must not.
//! - Scala: `@Test` defs and `*Spec` classes must be marked.
//! - PHP: `#[Test]` methods and `@test` docblocks must be marked.
//! - C++: `TEST(A, B)`/`TEST_F(A, B)` macros must be marked; wrong arity and
//!   typed parameter shapes must not.

use cce_e2e_tests::bench_data::{
    BenchmarkData, Bm25DocRecord, ChunkData, QueryData, QueryType, RelevanceJudgment,
    RelevanceLevel, RetrieverDataset, SourceRange,
};
use cce_e2e_tests::bench_gen::chunk_data_from_result;
use cce_e2e_tests::direct_chunker::EntityChunker;
use cce_e2e_tests::judgments::evaluate::{
    EvalVariant, RangeMode, TestDiagnostics, collect_test_diagnostics, evaluate_bm25,
};
use cce_parser::ast_to_nl::chunker::ChunkedResult;

const C_SHARP_PROJECT: &[(&str, &str)] = &[
    (
        "src/Calculator.cs",
        r#"
namespace MyApp
{
    public class Calculator
    {
        public int Add(int a, int b)
        {
            return a + b;
        }
    }
}
"#,
    ),
    (
        "src/CalculatorTests.cs",
        r#"
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
}
"#,
    ),
    (
        "src/TestRunner.cs",
        r#"
namespace MyApp
{
    public class TestRunner
    {
        public void Run()
        {
        }
    }
}
"#,
    ),
];

const DART_PROJECT: &[(&str, &str)] = &[
    (
        "lib/calc.dart",
        r#"
int add(int a, int b) {
  return a + b;
}
"#,
    ),
    (
        "test/calc_test.dart",
        r#"
import 'package:test/test.dart';

void main() {
  test('adds numbers', () {
    expect(add(2, 2), 4);
  });

  group('math', () {
    test('subtracts', () {
      expect(4 - 2, 2);
    });
  });
}
"#,
    ),
    (
        "lib/testMode.dart",
        r#"
bool testMode() {
  return true;
}
"#,
    ),
];

fn chunk_files(files: &[(&str, &str)]) -> Vec<ChunkData> {
    let chunker = EntityChunker::new();
    let owned: Vec<(String, String)> = files
        .iter()
        .map(|(p, c)| (p.to_string(), c.to_string()))
        .collect();
    let results: Vec<ChunkedResult> = chunker.chunk_files(&owned);
    results.iter().map(chunk_data_from_result).collect()
}

fn build_bm25_bench(chunks: Vec<ChunkData>, queries: Vec<QueryData>) -> BenchmarkData {
    let texts: Vec<String> = chunks
        .iter()
        .map(|c| {
            let line_range = format!("{}:{}-{}", c.file_path, c.start_line, c.end_line);
            format!("{} {}", c.entity_name, line_range)
        })
        .collect();
    let query_texts: Vec<String> = queries.iter().map(|q| q.text.clone()).collect();
    let bm25_documents: Vec<Bm25DocRecord> = chunks
        .iter()
        .zip(texts.iter())
        .map(|(c, content)| Bm25DocRecord {
            title: c.entity_name.clone(),
            keywords: String::new(),
            content: content.clone(),
        })
        .collect();
    BenchmarkData {
        queries,
        query_texts,
        embedding: RetrieverDataset {
            chunks: vec![],
            texts: vec![],
            vectors: vec![],
            query_vectors: vec![],
            dimension: 0,
        },
        bm25: RetrieverDataset {
            chunks,
            texts,
            vectors: vec![],
            query_vectors: vec![],
            dimension: 0,
        },
        bm25_documents,
        call_edges: Vec::new(),
    }
}

fn query(id: &str, text: &str) -> QueryData {
    QueryData {
        id: id.into(),
        text: text.into(),
        query_type: QueryType::Qualified,
        relevant_names: vec![id.into()],
        irrelevant_names: vec![],
    }
}

fn judgment(id: &str, text: &str, relevant_file: &str) -> RelevanceJudgment {
    RelevanceJudgment {
        id: id.into(),
        query_text: text.into(),
        query_type: QueryType::Qualified,
        fuzzy_subtype: None,
        relevant_ranges: vec![(
            SourceRange {
                file: relevant_file.into(),
                start_line: 1,
                end_line: 999,
            },
            RelevanceLevel::Strong,
        )],
    }
}

fn assert_chunks_marked(chunks: &[ChunkData], file: &str, expected: bool) {
    let per_file: Vec<&ChunkData> = chunks.iter().filter(|c| c.file_path == file).collect();
    assert!(!per_file.is_empty(), "expected chunks from {file}");
    for chunk in per_file {
        assert_eq!(
            chunk.test_info.is_test(),
            expected,
            "chunk {} in {file} must be marked test={expected}",
            chunk.chunk_id
        );
    }
}

fn assert_unknown(chunks: &[ChunkData], file: &str) {
    let per_file: Vec<&ChunkData> = chunks.iter().filter(|c| c.file_path == file).collect();
    assert!(!per_file.is_empty(), "expected chunks from {file}");
    for chunk in per_file {
        assert!(chunk.test_info.is_unknown(), "{} must stay Unknown", file);
    }
}

/// A mixed file holds both test entities and production/counter-example
/// entities (e.g. C++ `TEST(A, B)` macros next to wrong-arity shapes and
/// plain functions). It must yield at least one `Test` chunk and at least
/// one `Unknown` chunk, and no chunk may carry a third state.
fn assert_file_has_test_and_unknown_chunks(chunks: &[ChunkData], file: &str) {
    let per_file: Vec<&ChunkData> = chunks.iter().filter(|c| c.file_path == file).collect();
    assert!(!per_file.is_empty(), "expected chunks from {file}");
    assert!(
        per_file.iter().any(|c| c.test_info.is_test()),
        "expected a test chunk in {file}"
    );
    assert!(
        per_file.iter().any(|c| c.test_info.is_unknown()),
        "expected an unknown chunk in {file}"
    );
    for chunk in per_file {
        assert!(
            chunk.test_info.is_test() || chunk.test_info.is_unknown(),
            "chunk {} in {file} must be test or unknown, got {:?}",
            chunk.chunk_id,
            chunk.test_info,
        );
    }
}

fn bm25_diagnostics(diags: &[TestDiagnostics]) -> &TestDiagnostics {
    diags
        .iter()
        .find(|d| d.retriever == "BM25")
        .expect("BM25 diagnostics missing")
}

#[test]
fn test_csharp_dart_markers_reach_chunks() {
    let mut chunks = chunk_files(C_SHARP_PROJECT);
    chunks.extend(chunk_files(DART_PROJECT));

    // C#: `[Test]`/`[TestCase]` methods + `*Tests` class convention
    assert_chunks_marked(&chunks, "src/CalculatorTests.cs", true);
    // C#: production file must stay Unknown
    assert_unknown(&chunks, "src/Calculator.cs");
    // C# counter-example: `TestRunner.cs` never matches
    assert_unknown(&chunks, "src/TestRunner.cs");
    // Dart: `test/` path rule marks the whole file
    assert_chunks_marked(&chunks, "test/calc_test.dart", true);
    // Dart counter-example: `testMode.dart` never matches
    assert_unknown(&chunks, "lib/testMode.dart");
    // Dart production file stays Unknown
    assert_unknown(&chunks, "lib/calc.dart");
}

#[test]
fn test_scala_php_cpp_markers_reach_chunks() {
    let scala = vec![
        (
            "src/main/scala/Foo.scala",
            r#"
import org.junit.Test

class CalculatorSpec {
  @Test
  def add(): Unit = ()
}

class UserService {
  def helper(): Int = 1
}
"#,
        ),
        (
            "src/Contest.scala",
            r#"
class Contest {
  def run(): Unit = ()
}
"#,
        ),
    ];
    let php = vec![
        (
            "src/Calculator.php",
            r#"<?php
namespace App;

class CalculatorTest
{
    #[Test]
    public function testAdd(): void {}

    /**
     * @test
     */
    public function docTest(): void {}
}
"#,
        ),
        (
            "src/Contest.php",
            r#"<?php
namespace App;

class Contest
{
    public function run(): void {}
}
"#,
        ),
    ];
    let cpp = vec![(
        "src/math.cpp",
        r#"
#include <gtest/gtest.h>

TEST(MathTest, Add) {
  EXPECT_EQ(4, 2 + 2);
}

TEST_F(Fixture, Subtract) {
  EXPECT_EQ(2, 4 - 2);
}

TEST(SingleArg) {}

int TEST(int a, int b) { return a + b; }

int latest(int x) { return x + 1; }
"#,
    )];

    let mut chunks = chunk_files(&scala);
    chunks.extend(chunk_files(&php));
    chunks.extend(chunk_files(&cpp));

    // Scala: `@Test` def + `*Spec` class marked, while the production
    // `UserService` class in the same file stays Unknown.
    assert_file_has_test_and_unknown_chunks(&chunks, "src/main/scala/Foo.scala");
    assert_unknown(&chunks, "src/Contest.scala");
    // PHP: `#[Test]` method + `@test` docblock + `*Test` class marked
    assert_chunks_marked(&chunks, "src/Calculator.php", true);
    assert_unknown(&chunks, "src/Contest.php");
    // C++: `TEST(A, B)` / `TEST_F(A, B)` marked; wrong shapes
    // (`TEST(SingleArg)`, `int TEST(...)`) and production code stay Unknown.
    assert_file_has_test_and_unknown_chunks(&chunks, "src/math.cpp");
}

#[test]
fn test_no_test_variant_filtering_and_diagnostics() {
    let mut chunks = chunk_files(C_SHARP_PROJECT);
    chunks.extend(chunk_files(DART_PROJECT));

    let queries = vec![
        query("csharp_add", "Calculator Add method returns sum"),
        query("csharp_test", "NUnit test for Calculator Add"),
        query("dart_test", "package:test case for add"),
    ];
    let judgments = vec![
        judgment(
            "csharp_add",
            "Calculator Add method returns sum",
            "src/Calculator.cs",
        ),
        judgment(
            "csharp_test",
            "NUnit test for Calculator Add",
            "src/CalculatorTests.cs",
        ),
        judgment(
            "dart_test",
            "package:test case for add",
            "test/calc_test.dart",
        ),
    ];

    let bench = build_bm25_bench(chunks, queries);

    // TestDiagnostics must count the C#/Dart test-file chunks.
    let diags = collect_test_diagnostics(&bench);
    let bm25 = bm25_diagnostics(&diags);
    assert!(bm25.total_chunks > 0, "BM25 dataset must have chunks");
    assert!(bm25.test_chunks > 0, "C#/Dart test chunks must be counted");
    assert_eq!(
        bm25.filtered_chunks, bm25.test_chunks,
        "NoTest variant removes exactly the test chunks"
    );
    let test_files = ["src/CalculatorTests.cs", "test/calc_test.dart"];
    let expected_test_chunks = bench
        .bm25
        .chunks
        .iter()
        .filter(|c| test_files.contains(&c.file_path.as_str()))
        .count();
    assert_eq!(
        bm25.test_chunks, expected_test_chunks,
        "only the test-file chunks may be counted"
    );

    // The `All` variant sees test chunks in the top-5 for the test queries.
    assert!(
        bm25.top_k_test_occurrences > 0,
        "All variant must surface test chunks in top-5"
    );

    // NoTest variant: every strong/related chunk location must come from a
    // non-test file.
    let mut no_test_results = Vec::new();
    let mut no_test_relevance = Vec::new();
    evaluate_bm25(
        "language_coverage",
        &bench,
        &judgments,
        &mut no_test_results,
        &mut no_test_relevance,
        EvalVariant::NoTest,
        RangeMode::Precise,
    );
    assert!(
        !no_test_results.is_empty(),
        "NoTest evaluation must produce results"
    );
    for info in &no_test_relevance {
        for (_, loc, _) in info.strong_chunks.iter().chain(&info.related_chunks) {
            let file = loc.split(':').next().unwrap_or(loc);
            assert!(
                !test_files.contains(&file),
                "NoTest variant returned test-file chunk: {loc}"
            );
        }
    }
}

#[test]
fn test_unknown_chunks_never_default_to_test() {
    let chunks = chunk_files(C_SHARP_PROJECT);
    // Production + counter-example files must be `Unknown`, and the Unknown
    // fallback (`is_test_chunk`) must not turn them into tests.
    for chunk in &chunks {
        if chunk.file_path == "src/Calculator.cs" || chunk.file_path == "src/TestRunner.cs" {
            assert!(chunk.test_info.is_unknown());
            assert!(
                !cce_e2e_tests::judgments::evaluate::is_test_chunk(chunk),
                "{} must never be treated as test",
                chunk.file_path
            );
        }
    }
}
