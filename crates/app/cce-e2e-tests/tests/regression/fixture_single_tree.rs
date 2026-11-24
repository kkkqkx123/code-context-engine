//! Single fixture tree regression guard.
//!
//! The crate keeps exactly one fixture tree at the crate root `fixtures/`.
//! Integration tests reuse the library loaders instead of a separate copy.
//! These tests lock that invariant using only fixtures that are already
//! referenced by existing tests.

use std::path::PathBuf;

use cce_e2e_tests::fixture::TestFixture as LibTestFixture;

use crate::helper::{TestFixture, init_minimal_logging};

fn sorted_relative_files(fixture: &TestFixture) -> Vec<String> {
    let root = fixture.root_path().to_path_buf();
    let mut files = fixture
        .list_files()
        .expect("fixture files should be listable")
        .into_iter()
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

#[test]
fn no_second_fixture_tree_under_tests() {
    let second_tree = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    assert!(
        !second_tree.exists(),
        "tests/fixtures must not exist; tests load the single crate-root fixtures tree"
    );
}

#[test]
fn helper_loader_matches_library_loader() {
    init_minimal_logging();

    for spec in [
        cce_e2e_tests::fixture::FixtureSpec::rust_basic(),
        cce_e2e_tests::fixture::FixtureSpec::python_type_inference_cross_file(),
        cce_e2e_tests::fixture::FixtureSpec::rust_relation_diamond(),
    ] {
        let via_helper =
            TestFixture::load(spec.clone()).expect("helper loader should resolve the fixture");
        let via_lib =
            LibTestFixture::load(spec.clone()).expect("library loader should resolve the fixture");
        assert_eq!(
            sorted_relative_files(&via_helper),
            sorted_relative_files(&via_lib),
            "helper and library loaders must serve the same tree for {:?}",
            spec.subdirectory,
        );
        assert!(
            !sorted_relative_files(&via_helper).is_empty(),
            "fixture {:?} must not be empty",
            spec.subdirectory,
        );
    }
}

#[test]
fn merged_diamond_fixture_loads_with_marker_files() {
    init_minimal_logging();

    let fixture =
        TestFixture::rust_relation_diamond().expect("merged relation_diamond fixture should load");
    for marker in [
        "Cargo.toml",
        "src/lib.rs",
        "src/main.rs",
        "src/repository.rs",
        "src/service_a.rs",
        "src/service_b.rs",
    ] {
        assert!(
            fixture.file(marker).exists(),
            "relation_diamond should contain {marker}"
        );
    }
}

#[test]
fn already_used_fixtures_load_with_marker_files() {
    init_minimal_logging();

    let rust = TestFixture::rust_basic().expect("rust_basic should load");
    assert!(rust.file("Cargo.toml").exists());
    assert!(rust.file("src/main.rs").exists());

    let python = TestFixture::python_type_inference_cross_file()
        .expect("python cross-file fixture should load");
    assert!(python.file("models.py").exists());
    assert!(python.file("service.py").exists());

    let documents = TestFixture::documents().expect("documents should load");
    assert!(documents.file("README.md").exists());
}
