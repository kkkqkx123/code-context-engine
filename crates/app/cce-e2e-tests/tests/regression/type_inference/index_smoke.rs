//! Cross-language index smoke tests.
//!
//! Loop-driven tests covering fixtures that do not yet have dedicated
//! per-language snapshot assertions.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};

use crate::helper::{TestFixture, init_minimal_logging};

use super::common::run_index;

#[tokio::test]
async fn test_new_language_fixtures_index() {
    init_minimal_logging();

    for (fixture, extensions) in [
        (
            TestFixture::c_type_inference_declarations().expect("Failed to load c fixture"),
            vec!["c".to_string(), "h".to_string()],
        ),
        (
            TestFixture::c_basic().expect("Failed to load c basic fixture"),
            vec!["c".to_string(), "h".to_string()],
        ),
        (
            TestFixture::bash_basic().expect("Failed to load bash fixture"),
            vec!["sh".to_string()],
        ),
        (
            TestFixture::lua_basic().expect("Failed to load lua fixture"),
            vec!["lua".to_string()],
        ),
        (
            TestFixture::cpp_type_inference_declarations().expect("Failed to load cpp fixture"),
            vec!["cpp".to_string()],
        ),
        (
            TestFixture::kotlin_type_inference_generics().expect("Failed to load kotlin fixture"),
            vec!["kt".to_string()],
        ),
        (
            TestFixture::scala_type_inference_declarations().expect("Failed to load scala fixture"),
            vec!["scala".to_string()],
        ),
        (
            TestFixture::ruby_type_inference_constructors().expect("Failed to load ruby fixture"),
            vec!["rb".to_string()],
        ),
        (
            TestFixture::php_type_inference_phpdoc().expect("Failed to load php fixture"),
            vec!["php".to_string()],
        ),
        (
            TestFixture::dart_type_inference_declarations().expect("Failed to load dart fixture"),
            vec!["dart".to_string()],
        ),
        (
            TestFixture::javascript_type_inference_narrowing().expect("Failed to load js fixture"),
            vec!["js".to_string()],
        ),
        (
            TestFixture::typescript_basic().expect("Failed to load typescript basic fixture"),
            vec!["ts".to_string()],
        ),
        (
            TestFixture::javascript_basic().expect("Failed to load javascript basic fixture"),
            vec!["js".to_string()],
        ),
        (
            TestFixture::cpp_basic().expect("Failed to load cpp basic fixture"),
            vec!["cpp".to_string(), "h".to_string()],
        ),
    ] {
        let orchestrator = run_index(fixture, extensions).await;
        let relation_index = orchestrator
            .get_relation_index()
            .expect("Relation index should be available");
        assert!(
            relation_index.file_count() >= 1,
            "Should have at least one file indexed"
        );
    }
}

#[tokio::test]
async fn test_review_fixtures_index() {
    init_minimal_logging();

    for (fixture, extensions) in [
        (
            TestFixture::kotlin_review_coroutines().expect("Failed to load kotlin review fixture"),
            vec!["kt".to_string()],
        ),
        (
            TestFixture::scala_review_case_class().expect("Failed to load scala review fixture"),
            vec!["scala".to_string()],
        ),
        (
            TestFixture::ruby_review_mixin().expect("Failed to load ruby review fixture"),
            vec!["rb".to_string()],
        ),
        (
            TestFixture::php_review_namespace_trait().expect("Failed to load php review fixture"),
            vec!["php".to_string()],
        ),
        (
            TestFixture::typescript_review_re_export()
                .expect("Failed to load typescript re-export fixture"),
            vec!["ts".to_string()],
        ),
        (
            TestFixture::typescript_review_wildcard()
                .expect("Failed to load typescript wildcard fixture"),
            vec!["ts".to_string()],
        ),
        (
            TestFixture::python_review_re_export()
                .expect("Failed to load python re-export fixture"),
            vec!["py".to_string()],
        ),
        (
            TestFixture::python_review_wildcard().expect("Failed to load python wildcard fixture"),
            vec!["py".to_string()],
        ),
    ] {
        let orchestrator = run_index(fixture, extensions).await;
        let relation_index = orchestrator
            .get_relation_index()
            .expect("Relation index should be available");
        assert!(
            relation_index.file_count() >= 2,
            "review fixture should index at least two files"
        );
        assert!(
            !relation_index.function_index().is_empty(),
            "review fixture should extract function entities"
        );
    }
}

#[tokio::test]
async fn test_re_export_chain_resolves_calls() {
    init_minimal_logging();

    for (fixture, extensions) in [
        (
            TestFixture::typescript_review_re_export()
                .expect("Failed to load typescript re-export fixture"),
            vec!["ts".to_string()],
        ),
        (
            TestFixture::python_review_re_export()
                .expect("Failed to load python re-export fixture"),
            vec!["py".to_string()],
        ),
    ] {
        let orchestrator = run_index(fixture, extensions).await;
        let relation_index = orchestrator
            .get_relation_index()
            .expect("Relation index should be available");
        assert!(
            relation_index.file_count() >= 3,
            "re-export fixture should index origin, middle and consumer"
        );
        assert!(
            relation_index.resolved_relation_count() >= 1,
            "re-export chain should resolve at least one call edge"
        );
    }
}

#[tokio::test]
async fn test_wildcard_import_resolves_calls() {
    init_minimal_logging();

    for (fixture, extensions) in [
        (
            TestFixture::typescript_review_wildcard()
                .expect("Failed to load typescript wildcard fixture"),
            vec!["ts".to_string()],
        ),
        (
            TestFixture::python_review_wildcard().expect("Failed to load python wildcard fixture"),
            vec!["py".to_string()],
        ),
    ] {
        let orchestrator = run_index(fixture, extensions).await;
        let relation_index = orchestrator
            .get_relation_index()
            .expect("Relation index should be available");
        assert!(
            relation_index.file_count() >= 2,
            "wildcard fixture should index utils and consumer"
        );
        assert!(
            relation_index.resolved_relation_count() >= 1,
            "wildcard import should resolve at least one call edge"
        );
    }
}

#[tokio::test]
async fn test_shell_and_c_review_fixtures_index() {
    init_minimal_logging();

    for (fixture, extensions) in [
        (
            TestFixture::bash_type_inference_variables()
                .expect("Failed to load bash type inference fixture"),
            vec!["sh".to_string()],
        ),
        (
            TestFixture::lua_type_inference_variables()
                .expect("Failed to load lua type inference fixture"),
            vec!["lua".to_string()],
        ),
        (
            TestFixture::c_review_macros().expect("Failed to load c review fixture"),
            vec!["c".to_string(), "h".to_string()],
        ),
        (
            TestFixture::cpp_review_templates().expect("Failed to load cpp review fixture"),
            vec!["cpp".to_string(), "h".to_string()],
        ),
        (
            TestFixture::bash_review_pipeline().expect("Failed to load bash review fixture"),
            vec!["sh".to_string()],
        ),
        (
            TestFixture::lua_review_closure().expect("Failed to load lua review fixture"),
            vec!["lua".to_string()],
        ),
        (
            TestFixture::dart_review_mixin_async().expect("Failed to load dart review fixture"),
            vec!["dart".to_string()],
        ),
    ] {
        let orchestrator = run_index(fixture, extensions).await;
        let relation_index = orchestrator
            .get_relation_index()
            .expect("Relation index should be available");
        assert!(
            relation_index.file_count() >= 1,
            "fixture should index at least one file"
        );
        assert!(
            !relation_index.function_index().is_empty(),
            "fixture should extract function entities"
        );
    }
}

#[tokio::test]
async fn test_visibility_and_overload_fixtures_index() {
    init_minimal_logging();

    for (fixture, extensions) in [
        (
            TestFixture::java_type_inference_visibility()
                .expect("Failed to load java visibility fixture"),
            vec!["java".to_string()],
        ),
        (
            TestFixture::java_type_inference_overloads()
                .expect("Failed to load java overloads fixture"),
            vec!["java".to_string()],
        ),
        (
            TestFixture::csharp_type_inference_overloads()
                .expect("Failed to load csharp overloads fixture"),
            vec!["cs".to_string()],
        ),
        (
            TestFixture::typescript_type_inference_overloads()
                .expect("Failed to load typescript overloads fixture"),
            vec!["ts".to_string()],
        ),
    ] {
        let orchestrator = run_index(fixture, extensions).await;
        let relation_index = orchestrator
            .get_relation_index()
            .expect("Relation index should be available");
        assert!(
            relation_index.file_count() >= 1,
            "visibility/overload fixture should index at least one file"
        );
        assert!(
            !relation_index.function_index().is_empty(),
            "visibility/overload fixture should extract function entities"
        );
        assert!(
            relation_index.resolved_relation_count() >= 1,
            "visibility/overload fixture should resolve at least one call edge"
        );
    }
}

#[tokio::test]
async fn test_phase3_new_fixtures_index() {
    init_minimal_logging();

    for (fixture, extensions) in [
        (
            TestFixture::python_type_inference_lambda()
                .expect("Failed to load python lambda fixture"),
            vec!["py".to_string()],
        ),
        (
            TestFixture::typescript_type_inference_lambda()
                .expect("Failed to load typescript lambda fixture"),
            vec!["ts".to_string()],
        ),
        (
            TestFixture::rust_type_inference_closure()
                .expect("Failed to load rust closure fixture"),
            vec!["rs".to_string()],
        ),
        (
            TestFixture::kotlin_type_inference_lambda()
                .expect("Failed to load kotlin lambda fixture"),
            vec!["kt".to_string()],
        ),
        (
            TestFixture::csharp_type_inference_lambda()
                .expect("Failed to load csharp lambda fixture"),
            vec!["cs".to_string()],
        ),
        (
            TestFixture::java_type_inference_lambda().expect("Failed to load java lambda fixture"),
            vec!["java".to_string()],
        ),
        (
            TestFixture::python_type_inference_discriminated_union()
                .expect("Failed to load python discriminated union fixture"),
            vec!["py".to_string()],
        ),
        (
            TestFixture::csharp_type_inference_discriminated_union()
                .expect("Failed to load csharp discriminated union fixture"),
            vec!["cs".to_string()],
        ),
        (
            TestFixture::kotlin_type_inference_discriminated_union()
                .expect("Failed to load kotlin discriminated union fixture"),
            vec!["kt".to_string()],
        ),
        (
            TestFixture::dart_type_inference_discriminated_union()
                .expect("Failed to load dart discriminated union fixture"),
            vec!["dart".to_string()],
        ),
        (
            TestFixture::java_type_inference_discriminated_union()
                .expect("Failed to load java discriminated union fixture"),
            vec!["java".to_string()],
        ),
        (
            TestFixture::rust_type_inference_destructuring()
                .expect("Failed to load rust destructuring fixture"),
            vec!["rs".to_string()],
        ),
        (
            TestFixture::typescript_type_inference_destructuring()
                .expect("Failed to load typescript destructuring fixture"),
            vec!["ts".to_string()],
        ),
        (
            TestFixture::python_type_inference_destructuring()
                .expect("Failed to load python destructuring fixture"),
            vec!["py".to_string()],
        ),
        (
            TestFixture::rust_type_inference_lifetime()
                .expect("Failed to load rust lifetime fixture"),
            vec!["rs".to_string()],
        ),
        (
            TestFixture::rust_type_inference_impl_self()
                .expect("Failed to load rust impl_self fixture"),
            vec!["rs".to_string()],
        ),
        (
            TestFixture::rust_type_inference_reference()
                .expect("Failed to load rust reference fixture"),
            vec!["rs".to_string()],
        ),
        (
            TestFixture::java_type_inference_pattern_matching()
                .expect("Failed to load java pattern matching fixture"),
            vec!["java".to_string()],
        ),
        (
            TestFixture::java_type_inference_var_inference()
                .expect("Failed to load java var inference fixture"),
            vec!["java".to_string()],
        ),
        (
            TestFixture::kotlin_type_inference_null_safety()
                .expect("Failed to load kotlin null safety fixture"),
            vec!["kt".to_string()],
        ),
        (
            TestFixture::kotlin_type_inference_scope_functions()
                .expect("Failed to load kotlin scope functions fixture"),
            vec!["kt".to_string()],
        ),
    ] {
        let orchestrator = run_index(fixture, extensions).await;
        let relation_index = orchestrator
            .get_relation_index()
            .expect("Relation index should be available");
        assert!(
            relation_index.file_count() >= 1,
            "phase3 fixture should index at least one file"
        );
        assert!(
            !relation_index.function_index().is_empty(),
            "phase3 fixture should extract function entities"
        );
    }
}

#[tokio::test]
async fn test_phase4_new_fixtures_index() {
    init_minimal_logging();

    for (fixture, extensions) in [
        (
            TestFixture::python_type_inference_negated_checks()
                .expect("Failed to load python negated checks fixture"),
            vec!["py".to_string()],
        ),
        (
            TestFixture::typescript_type_inference_negated_checks()
                .expect("Failed to load typescript negated checks fixture"),
            vec!["ts".to_string()],
        ),
        (
            TestFixture::typescript_type_inference_control_positions()
                .expect("Failed to load typescript control positions fixture"),
            vec!["ts".to_string()],
        ),
        (
            TestFixture::java_type_inference_negated_checks()
                .expect("Failed to load java negated checks fixture"),
            vec!["java".to_string()],
        ),
        (
            TestFixture::java_type_inference_control_positions()
                .expect("Failed to load java control positions fixture"),
            vec!["java".to_string()],
        ),
        (
            TestFixture::csharp_type_inference_is_not()
                .expect("Failed to load csharp is_not fixture"),
            vec!["cs".to_string()],
        ),
        (
            TestFixture::csharp_type_inference_control_positions()
                .expect("Failed to load csharp control positions fixture"),
            vec!["cs".to_string()],
        ),
        (
            TestFixture::kotlin_type_inference_negated_is()
                .expect("Failed to load kotlin negated is fixture"),
            vec!["kt".to_string()],
        ),
        (
            TestFixture::dart_type_inference_is_negated()
                .expect("Failed to load dart is_negated fixture"),
            vec!["dart".to_string()],
        ),
        (
            TestFixture::dart_type_inference_control_positions()
                .expect("Failed to load dart control positions fixture"),
            vec!["dart".to_string()],
        ),
        (
            TestFixture::scala_type_inference_for_comprehension()
                .expect("Failed to load scala for_comprehension fixture"),
            vec!["scala".to_string()],
        ),
        (
            TestFixture::go_type_inference_type_assertion()
                .expect("Failed to load go type assertion fixture"),
            vec!["go".to_string()],
        ),
    ] {
        let orchestrator = run_index(fixture, extensions).await;
        let relation_index = orchestrator
            .get_relation_index()
            .expect("Relation index should be available");
        assert!(
            relation_index.file_count() >= 1,
            "phase4 fixture should index at least one file"
        );
        assert!(
            !relation_index.function_index().is_empty(),
            "phase4 fixture should extract function entities"
        );
    }
}
