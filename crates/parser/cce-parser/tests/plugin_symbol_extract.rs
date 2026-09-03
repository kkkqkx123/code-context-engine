//! Integration tests for the `SymbolExtract` plugin capability:
//!
//! - `create_extractor_with_registry` routes custom languages to a plugin.
//! - Plugin imports/exports are converted into the standardized forms.
//! - The relation helpers honor the plugin registry for custom languages.
//! - The stdlib classifier consumes plugin `is_stdlib` metadata.

use std::sync::Arc;

use cce_parser::parser::AstParser;
use cce_parser::parser::create_extractor_with_registry;
use cce_plugin::{PluginBundle, PluginRegistry};
use cce_plugin_runtime::LuaPlugin;
use cce_types::language::Language;

fn zig_symbol_extract_plugin() -> Arc<dyn cce_plugin::CodePlugin> {
    let script = r#"
        plugin = {
            id = "zig_symbol_extract",
            capabilities = { "symbol_extract" },
            extract_imports = function(content, file_path, language)
                local imports = {}
                for line in content:gmatch("[^\r\n]+") do
                    local path = line:match("const%s+%w+%s*=%s*@import%(\"([^\"]+)\"%)")
                    if path then
                        imports[#imports + 1] = { path = path }
                    end
                end
                if #imports == 0 then return nil end
                return imports
            end,
            extract_exports = function(content, file_path, language)
                local exports = {}
                for line in content:gmatch("[^\r\n]+") do
                    local name = line:match("pub%s+fn%s+(%w+)%s*%(")
                    if name then
                        exports[#exports + 1] = { name = name, kind = "function", visibility = "public" }
                    end
                end
                if #exports == 0 then return nil end
                return exports
            end
        }
    "#;
    Arc::new(LuaPlugin::from_script(script).expect("valid lua"))
}

fn zig_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    // File-pattern routing is the primary filter; the language constraint is
    // omitted so `Language::Custom(0).to_string()` (which is `custom_0` unless
    // a process-global plugin language is registered) still matches.
    registry.register_bundle(
        PluginBundle::new(zig_symbol_extract_plugin())
            .with_file_patterns(vec!["*.zig".to_string()]),
    );
    registry
}

/// Parse a trivial built-in source to obtain a valid `Tree`. The plugin
/// extractor ignores the tree, so this only serves as a trait-typed handle.
fn dummy_tree() -> tree_sitter::Tree {
    let mut parser = AstParser::new();
    parser
        .parse_with_tree("fn main() {}", &Language::Rust)
        .expect("rust parses")
        .0
}

#[test]
fn test_create_extractor_with_registry_finds_plugin() {
    let registry = zig_registry();
    let extractor =
        create_extractor_with_registry(Language::Custom(0), Some(&registry), "test.zig", "zig");
    let extractor = extractor.expect("SymbolExtract plugin should be found");
    assert_eq!(extractor.language(), Language::Custom(0));

    let content = r#"
        const std = @import("std");
        const mem = @import("mem.zig");
        fn main() void {}
    "#;
    let imports = extractor.extract_imports(&dummy_tree(), content);
    assert_eq!(imports.len(), 2);
    assert_eq!(imports[0].source, "std");
    assert_eq!(imports[1].source, "mem.zig");
    assert!(!imports[0].is_relative);
}

#[test]
fn test_create_extractor_with_registry_exports() {
    let registry = zig_registry();
    let extractor =
        create_extractor_with_registry(Language::Custom(0), Some(&registry), "test.zig", "zig")
            .expect("plugin found");

    let content = "const std = @import(\"std\");\npub fn hello() void {}";
    let exports = extractor.extract_exports(&dummy_tree(), content);
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].target.name, "hello");
}

#[test]
fn test_create_extractor_with_registry_none_without_plugin() {
    // Empty registry → no plugin → no extractor for a custom language.
    let registry = PluginRegistry::new();
    let extractor =
        create_extractor_with_registry(Language::Custom(0), Some(&registry), "test.zig", "zig");
    assert!(extractor.is_none());

    // Built-in languages never consult the registry.
    let extractor = create_extractor_with_registry(Language::Rust, Some(&registry), "x.rs", "rust");
    assert!(extractor.is_some());
}

/// A Zig-style project indexed through the production `FileProcessor` path
/// (`IndexBuilder::register_file_entities`). The `SymbolExtract` plugin
/// provides imports on raw source text, so no tree-sitter grammar is needed
/// for the custom language; the resulting imports land in each file's import
/// table and the cross-file dependencies form the dependency graph.
#[test]
fn test_relation_index_uses_plugin_imports() {
    use cce_relation::IndexBuilder;
    use cce_relation::index::ImportIndexOps;

    let registry = Arc::new(zig_registry());
    let mut builder = IndexBuilder::new().with_plugin_registry(registry);
    builder.set_symbol_extract_enabled(true);
    builder.set_graph_options(100, true, true);

    let main_source =
        "const std = @import(\"std\");\nconst math = @import(\"math.zig\");\npub fn main() void {}";
    let main_parsed =
        cce_types::ParsedFile::new(Language::Custom(0), "src/main.zig".to_string(), main_source);
    let math_source =
        "const io = @import(\"io.zig\");\npub fn add(a: i32, b: i32) i32 { return a + b; }";
    let math_parsed =
        cce_types::ParsedFile::new(Language::Custom(0), "src/math.zig".to_string(), math_source);

    builder.register_file_entities(&main_parsed);
    builder.register_file_entities(&math_parsed);

    let index = builder.index();

    // main.zig imports std + math.zig through the plugin extractor.
    let main_imports = index
        .get_import_table("src/main.zig")
        .expect("main.zig must have an import table");
    assert_eq!(
        main_imports.import_count(),
        2,
        "plugin imports must land in the import table"
    );
    assert!(
        main_imports
            .standardized_imports
            .iter()
            .any(|i| i.source == "std")
    );
    assert!(
        main_imports
            .standardized_imports
            .iter()
            .any(|i| i.source == "math.zig")
    );

    // math.zig imports io.zig.
    let math_imports = index
        .get_import_table("src/math.zig")
        .expect("math.zig must have an import table");
    assert_eq!(math_imports.import_count(), 1);
    assert!(
        math_imports
            .standardized_imports
            .iter()
            .any(|i| i.source == "io.zig")
    );

    // Cross-file dependencies: main → math, math → io. The dependency graph
    // records the raw import path as the target.
    let deps = builder.dependency_graph();
    assert!(
        deps.has_dependency("src/main.zig", "math.zig"),
        "main.zig must depend on math.zig"
    );
    assert!(
        deps.has_dependency("src/math.zig", "io.zig"),
        "math.zig must depend on io.zig"
    );
}

/// The `symbol_extract_enabled` flag gates plugin import extraction. With it
/// disabled, a custom language without a grammar gets an empty import table
/// instead of plugin imports.
#[test]
fn test_relation_index_disabled_plugin_imports() {
    use cce_relation::IndexBuilder;
    use cce_relation::index::ImportIndexOps;

    let registry = Arc::new(zig_registry());
    let mut builder = IndexBuilder::new().with_plugin_registry(registry);
    builder.set_graph_options(100, true, true);
    // symbol_extract_enabled defaults to false.

    let parsed = cce_types::ParsedFile::new(
        Language::Custom(0),
        "src/lib.zig".to_string(),
        "const std = @import(\"std\");",
    );
    builder.register_file_entities(&parsed);

    let imports = builder
        .index()
        .get_import_table("src/lib.zig")
        .expect("file must be indexed");
    assert_eq!(
        imports.import_count(),
        0,
        "plugin imports must be gated behind symbol_extract_enabled"
    );
}

/// Built-in languages never consult the plugin registry for imports, even
/// when symbol extraction is enabled.
#[test]
fn test_relation_index_plugin_ignores_builtin_languages() {
    use cce_relation::IndexBuilder;
    use cce_relation::index::ImportIndexOps;

    let registry = Arc::new(zig_registry());
    let mut builder = IndexBuilder::new().with_plugin_registry(registry);
    builder.set_symbol_extract_enabled(true);
    builder.set_graph_options(100, true, true);

    let parsed = cce_types::ParsedFile::new(
        Language::Rust,
        "src/main.rs".to_string(),
        "use std::collections::HashMap;",
    );
    builder.register_file_entities(&parsed);

    let imports = builder
        .index()
        .get_import_table("src/main.rs")
        .expect("file must be indexed");
    assert_eq!(
        imports.import_count(),
        1,
        "built-in languages keep their own extractor"
    );
    assert!(
        imports
            .standardized_imports
            .iter()
            .any(|i| i.source == "std::collections::HashMap")
    );
}
