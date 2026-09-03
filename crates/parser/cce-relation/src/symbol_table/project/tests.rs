use super::*;
use crate::symbol::SymbolLocation;
use crate::symbol::SymbolMetadata;
use crate::symbol_table::PackageSymbolTable;
use cce_types::Span;
use cce_types::entity::EntityKind;
use cce_types::language::Language;
use std::path::PathBuf;
use std::sync::Arc;

fn create_test_package(id: &str, name: &str) -> Arc<PackageSymbolTable> {
    Arc::new(PackageSymbolTable::new(
        id.to_string(),
        name.to_string(),
        format!("/project/{}", id),
        Language::Rust,
    ))
}

fn create_test_metadata(name: &str, package: &str) -> SymbolMetadata {
    let location = SymbolLocation::new(
        format!("src/{}/lib.rs", package),
        Span {
            start_byte: 0,
            end_byte: 10,
            start_position: Default::default(),
            end_position: Default::default(),
        },
        Language::Rust,
    );
    SymbolMetadata::new(name.to_string(), EntityKind::Function, location)
}

#[test]
fn test_add_and_get_package() {
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));

    let package = create_test_package("pkg-1", "crate-a");
    project.add_package(package);

    assert!(project.has_package("pkg-1"));
    assert!(!project.has_package("pkg-2"));

    let found = project.get_package("pkg-1");
    assert!(found.is_some());
    assert_eq!(found.unwrap().package_name, "crate-a");
}

#[test]
fn test_cross_package_resolution() {
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));

    let package_a = create_test_package("pkg-a", "crate-a");
    package_a.add_public_export(
        "shared_func".to_string(),
        create_test_metadata("shared_func", "crate-a"),
    );
    project.add_package(package_a);

    let package_b = create_test_package("pkg-b", "crate-b");
    project.add_package(package_b);

    // Package b can access public exports from package a
    let result = project.resolve_cross_package("shared_func", "pkg-b");
    assert!(result.is_some());
}

#[test]
fn test_project_stats() {
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));

    let package_a = create_test_package("pkg-a", "crate-a");
    package_a.add_public_export(
        "func1".to_string(),
        create_test_metadata("func1", "crate-a"),
    );
    package_a.add_public_export(
        "func2".to_string(),
        create_test_metadata("func2", "crate-a"),
    );
    project.add_package(package_a);

    let package_b = create_test_package("pkg-b", "crate-b");
    project.add_package(package_b);

    let stats = project.project_stats();
    assert_eq!(stats.package_count, 2);
    assert_eq!(stats.total_symbols, 2);
}

fn module_with_export(
    package: &Arc<PackageSymbolTable>,
    module_path: &str,
    file_path: &str,
    export_name: &str,
) {
    let mut module = crate::symbol_table::module::ModuleSymbolTable::new(
        module_path.to_string(),
        file_path.to_string(),
        Language::Rust,
        "crate-a".to_string(),
    );
    let location = SymbolLocation::new(file_path.to_string(), Span::default(), Language::Rust);
    let metadata = SymbolMetadata::new(export_name.to_string(), EntityKind::Function, location);
    module.add_export(
        export_name.to_string(),
        metadata,
        crate::symbol::Visibility::Public,
    );
    package.add_module(module);
}

fn module_with_reexport(
    package: &Arc<PackageSymbolTable>,
    module_path: &str,
    file_path: &str,
    local_name: &str,
    original_module: &str,
    original_name: &str,
    chain_depth: u8,
) {
    let module = crate::symbol_table::module::ModuleSymbolTable::new(
        module_path.to_string(),
        file_path.to_string(),
        Language::Rust,
        "crate-a".to_string(),
    );
    module.add_reexport(crate::symbol_table::module::ReexportBinding {
        local_name: local_name.to_string(),
        original_module: original_module.to_string(),
        original_name: original_name.to_string(),
        chain_depth,
        resolved_symbol: None,
    });
    package.add_module(module);
}

fn context_for(file_path: &str) -> ResolutionContext {
    ResolutionContext {
        file_path: file_path.to_string(),
        module_path: Vec::new(),
        scope_chain: Vec::new(),
    }
}

#[test]
fn reexport_resolves_target_across_modules_and_caches_result() {
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));
    let package = create_test_package("pkg-a", "crate-a");
    module_with_export(&package, "a", "src/a.rs", "Item");
    module_with_reexport(&package, "lib", "src/lib.rs", "Item", "a", "Item", 0);
    project.add_package(package);

    // Level 1 (simple name) cannot resolve "Item" from lib.rs, so Level 3
    // re-export resolution must find the symbol in module "a".
    let symbol = project
        .resolve_enhanced("Item", &context_for("src/lib.rs"))
        .expect("re-export resolves the original symbol");
    assert_eq!(symbol.name(), "Item");
    assert_eq!(&*symbol.metadata.location.file_path, "src/a.rs");

    // The caller module caches the resolved symbol on first hit.
    let lib_module = project
        .get_package("pkg-a")
        .expect("package present")
        .get_module("src/lib.rs")
        .expect("lib module present");
    assert!(
        lib_module.lookup_reexport("Item").is_some(),
        "resolved re-export must be cached"
    );
}

#[test]
fn reexport_chain_within_depth_cap_resolves() {
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));
    let package = create_test_package("pkg-a", "crate-a");
    module_with_export(&package, "a", "src/a.rs", "Item");
    module_with_reexport(&package, "b", "src/b.rs", "Item", "a", "Item", 0);
    module_with_reexport(&package, "c", "src/c.rs", "Item", "b", "Item", 1);
    module_with_reexport(&package, "d", "src/d.rs", "Item", "c", "Item", 2);
    project.add_package(package);

    let symbol = project
        .resolve_enhanced("Item", &context_for("src/d.rs"))
        .expect("three-hop re-export chain resolves");
    assert_eq!(&*symbol.metadata.location.file_path, "src/a.rs");
}

#[test]
fn reexport_chain_beyond_depth_cap_and_cycles_terminate() {
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));
    let package = create_test_package("pkg-a", "crate-a");

    // Five-hop chain: resolution must stop at the depth cap instead of
    // following the chain to its (present) source.
    module_with_export(&package, "f", "src/f.rs", "Item");
    module_with_reexport(&package, "e", "src/e.rs", "Item", "f", "Item", 3);
    module_with_reexport(&package, "d", "src/d.rs", "Item", "e", "Item", 3);
    module_with_reexport(&package, "c", "src/c.rs", "Item", "d", "Item", 2);
    module_with_reexport(&package, "b", "src/b.rs", "Item", "c", "Item", 1);
    module_with_reexport(&package, "a", "src/a.rs", "Item", "b", "Item", 0);

    // Mutual cycle between modules "x" and "y": must terminate with no
    // resolution rather than looping forever.
    module_with_reexport(&package, "x", "src/x.rs", "Item", "y", "Item", 0);
    module_with_reexport(&package, "y", "src/y.rs", "Item", "x", "Item", 0);
    project.add_package(package);

    assert!(
        project
            .resolve_enhanced("Item", &context_for("src/a.rs"))
            .is_none(),
        "chain longer than the depth cap must not resolve"
    );
    assert!(
        project
            .resolve_enhanced("Item", &context_for("src/x.rs"))
            .is_none(),
        "re-export cycle must terminate without resolving"
    );
}

#[test]
fn qualified_name_resolves_last_segment_with_prefix_match() {
    // Regression: `models::User` must resolve to the `User` export in
    // the module whose path matches the `models` prefix.
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));
    let package = create_test_package("pkg-a", "crate-a");
    module_with_export(&package, "models", "src/models.rs", "User");
    project.add_package(package);

    let symbol = project
        .resolve_enhanced("models::User", &context_for("src/main.rs"))
        .expect("prefix-matched last segment resolves");
    assert_eq!(symbol.name(), "User");
    assert_eq!(&*symbol.metadata.location.file_path, "src/models.rs");
}

#[test]
fn qualified_name_last_segment_fallback_preserves_simple_name_behavior() {
    // Regression: an unresolved qualified name (`obj.method`) falls
    // back to the plain simple-name search rather than failing outright.
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));
    let package = create_test_package("pkg-a", "crate-a");
    module_with_export(&package, "models", "src/models.rs", "User");
    project.add_package(package);

    // Register the file symbol in the simple-name index so the fallback
    // can resolve the last segment.
    project.insert_symbol(
        "src/models.rs::User".to_string(),
        EntityId(1),
        "src/models.rs".to_string(),
        "models".to_string(),
    );

    // `other::User` does not match the `models` prefix, but the last
    // segment `User` still resolves through the plain simple-name search.
    let symbol = project
        .resolve_enhanced("other::User", &context_for("src/main.rs"))
        .expect("last-segment fallback resolves via simple name");
    assert_eq!(symbol.name(), "User");
}

#[test]
fn stdlib_qualified_name_does_not_fallback_to_local_segment() {
    // Regression: `Vec::new` must not resolve to a project-local
    // `new`; the strict path skips the last-segment fallback.
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));
    let package = create_test_package("pkg-a", "crate-a");
    module_with_export(&package, "models", "src/models.rs", "new");
    project.add_package(package);

    project.insert_symbol(
        "src/models.rs::new".to_string(),
        EntityId(1),
        "src/models.rs".to_string(),
        "models".to_string(),
    );

    let context = context_for("src/main.rs");
    assert!(
        project
            .resolve_enhanced_strict("Vec::new", &context)
            .is_none(),
        "stdlib-qualified name must not fall back to a local `new`"
    );
}

#[test]
fn non_strict_qualified_name_falls_back_to_local_segment() {
    // WITHOUT the strict guard, `Vec::new` would fall
    // back to a project-local `new` via the last-segment search. The
    // resolver layer guards stdlib targets by calling the strict variant.
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));
    let package = create_test_package("pkg-a", "crate-a");
    module_with_export(&package, "models", "src/models.rs", "new");
    project.add_package(package);

    project.insert_symbol(
        "src/models.rs::new".to_string(),
        EntityId(1),
        "src/models.rs".to_string(),
        "models".to_string(),
    );

    let context = context_for("src/main.rs");
    let symbol = project
        .resolve_enhanced("Vec::new", &context)
        .expect("non-strict resolution falls back to the local `new`");
    assert_eq!(symbol.name(), "new");
}

#[test]
fn insert_symbol_invalidates_positive_resolution_cache() {
    // Regression: `resolve_enhanced` caches positive results per
    // (caller file, name). Registering a new symbol with the same name
    // (incremental hot-update path) must invalidate that cache — the
    // stale hit would otherwise shadow the newly registered symbol
    // forever (until the bounded cache evicts it by chance).
    let project = ProjectSymbolTable::new(PathBuf::from("/project"));
    let package = create_test_package("pkg-a", "crate-a");
    // The caller's own module exports "Item" too; without a registered
    // file symbol the package-level public export wins simple-name
    // resolution from src/lib.rs.
    module_with_export(&package, "lib", "src/lib.rs", "Item");
    package.add_public_export("Item".to_string(), create_test_metadata("Item", "crate-a"));
    project.add_package(package);

    let _first = project
        .resolve_enhanced("Item", &context_for("src/lib.rs"))
        .expect("package export resolves");
    assert!(
        !project.resolution_cache.try_lock().unwrap().is_empty(),
        "positive result must be cached before the mutation"
    );

    // Registering a caller-local symbol with the same name: simple-name
    // semantics prefer the caller's own file, so resolution must now
    // return src/lib.rs's symbol instead of the stale cached src/a.rs.
    project.insert_symbol(
        "Item".to_string(),
        EntityId(4242),
        "src/lib.rs".to_string(),
        "lib".to_string(),
    );

    let second = project
        .resolve_enhanced("Item", &context_for("src/lib.rs"))
        .expect("caller-local symbol resolves after cache invalidation");
    assert_eq!(
        &*second.metadata.location.file_path, "src/lib.rs",
        "stale positive cache must not shadow the newly registered symbol"
    );
}
