//! Integration tests for the plugin-extension pipeline capabilities:
//!
//! - M2 `FormatParse` in `PipelineRouter` (document plugin formats)
//! - M3 `EntityExtract` in `PreprocessingPipeline` (regex supplementary entities)
//! - M4 `Group` post-grouping hook in `PreprocessingPipeline`
//! - M5 `Chunk` override in `GroupChunker`

use std::sync::Arc;

use cce_config::modules::ChunkingConfig;
use cce_parser::document::PipelineRouter;
use cce_parser::grouper::PreprocessingPipeline;
use cce_plugin::{PluginBundle, PluginRegistry};
use cce_plugin_runtime::LuaPlugin;
use cce_types::ParsedFile;
use cce_types::ast_to_nl::options::OutputMode;
use cce_types::language::Language;

fn pattern_extract_plugin(script: &str) -> Arc<dyn cce_plugin::CodePlugin> {
    Arc::new(LuaPlugin::from_script(script).expect("valid lua"))
}

/// M3: `EntityExtract` pattern plugin injects route entities as groups.
#[test]
fn test_entity_extract_injects_plugin_groups() {
    let script = r#"
        plugin = {
            id = "flask_route",
            patterns = {
                { name = "route", regex = "@app\\.route\\('(?P<name>[^']+)'\\)[\\s\\S]*?\\n\\s*def\\s+(?P<signature>[\\w_]+)\\(", kind = "route" }
            }
        }
    "#;
    let mut registry = PluginRegistry::new();
    registry.register(pattern_extract_plugin(script));

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(Arc::new(registry));
    let content = "@app.route('/users')\ndef users():\n    pass\n\n@app.route('/items')\ndef items():\n    pass\n";
    let parsed = ParsedFile::new(Language::Python, "app.py".to_string(), content);
    let result = pipeline.process(&parsed);

    // The two route entities are injected as plugin groups; the small-fragment
    // merger may fold them into a single MergedFragments group. Either way the
    // route names must be present in the final groups.
    let names: Vec<String> = result
        .groups
        .iter()
        .flat_map(|g| {
            let mut names = Vec::new();
            if let Some(ref h) = g.header {
                names.push(h.name.clone());
            }
            names.extend(g.members.iter().map(|m| m.name.clone()));
            names
        })
        .collect();
    assert!(
        names.iter().any(|n| n == "/users"),
        "route /users must be injected"
    );
    assert!(
        names.iter().any(|n| n == "/items"),
        "route /items must be injected"
    );
    assert!(
        result.groups.iter().any(|g| g.group_id.contains("plugin_")),
        "a plugin-origin group must exist"
    );
}

/// M4: `Group` post-grouping hook renames groups and annotates metadata.
#[test]
fn test_post_group_hook_applies() {
    let script = r#"
        plugin = {
            id = "group_hook",
            post_group = function(groups, context)
                local out = {}
                for i = 1, #groups do
                    local g = groups[i]
                    g.metadata = { plugin_touched = "yes" }
                    out[i] = g
                end
                return out
            end
        }
    "#;
    let mut registry = PluginRegistry::new();
    registry.register(pattern_extract_plugin(script));

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(Arc::new(registry));
    let mut parsed = ParsedFile::new(Language::Rust, "src/main.rs".to_string(), "fn main() {}");
    parsed.add_entity(cce_types::Entity::new(
        cce_types::EntityId(1),
        cce_types::EntityKind::Function,
        "main".to_string(),
        cce_types::Span::new(0, 12, 0, 0, 0, 12),
    ));
    let result = pipeline.process(&parsed);

    assert!(!result.groups.is_empty());
    let all_touched = result
        .groups
        .iter()
        .all(|g| g.metadata.get("plugin_touched").map(String::as_str) == Some("yes"));
    assert!(all_touched, "every group should be touched by the hook");
}

/// M2: `FormatParse` plugin parses `.proto` documents into chunks.
#[test]
fn test_plugin_document_format_routing() {
    let script = r#"
        plugin = {
            id = "proto_format",
            parse_document = function(content, file_path)
                local entities = {}
                local n = 0
                for m in content:gmatch("message%s+([%w_]+)%s*{") do
                    n = n + 1
                    entities[n] = { id = "m" .. n, kind = "message", name = m,
                                    signature = "message " .. m, doc_comment = "proto message" }
                end
                if n == 0 then return nil end
                return { title = file_path, language = "proto", entities = entities }
            end
        }
    "#;
    let mut registry = PluginRegistry::new();
    registry.register_bundle(
        PluginBundle::new(pattern_extract_plugin(script))
            .with_file_patterns(vec!["*.proto".to_string()]),
    );

    let router = PipelineRouter::new();
    let config = ChunkingConfig::default();
    let (chunks, _) = router
        .process_with_plugins(
            "message User {}\nmessage Order {}\n",
            "api.proto",
            &config,
            OutputMode::Both,
            &registry,
        )
        .expect("plugin format processed");

    assert!(
        !chunks.is_empty(),
        "plugin document format should produce chunks"
    );
    assert!(
        chunks.iter().all(|c| c.metadata.is_document()),
        "plugin document chunks must have ContentType::Document"
    );
    let combined: String = chunks
        .iter()
        .map(|c| c.text.clone())
        .collect::<Vec<_>>()
        .join("");
    assert!(combined.contains("User"));
    assert!(combined.contains("Order"));
}

/// M2: built-in pipeline still handles unknown formats when no plugin matches.
#[test]
fn test_plugin_document_format_falls_back() {
    let script = r#"
        plugin = {
            id = "proto_format",
            parse_document = function(content, file_path) return nil end
        }
    "#;
    let mut registry = PluginRegistry::new();
    registry.register_bundle(
        PluginBundle::new(pattern_extract_plugin(script))
            .with_file_patterns(vec!["*.proto".to_string()]),
    );

    let router = PipelineRouter::new();
    let config = ChunkingConfig::default();
    // A .md file should not be intercepted by the *.proto plugin.
    let (chunks, _) = router
        .process_with_plugins(
            "# Hello\n\ntext\n",
            "README.md",
            &config,
            OutputMode::Both,
            &registry,
        )
        .expect("built-in markdown pipeline");
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|c| c.metadata.is_document()));
}

/// `GroupOverride` full-override tier replaces built-in grouping.
#[test]
fn test_group_override_replaces_builtin_grouping() {
    let script = r#"
        plugin = {
            id = "module_override",
            group = function(context)
                local groups = {}
                local by_module = {}
                for i = 1, #context.entities do
                    local e = context.entities[i]
                    local module = "core"
                    if e.metadata and e.metadata["module"] then
                        module = e.metadata["module"]
                    end
                    local g = by_module[module]
                    if not g then
                        g = {
                            group_id = "override_" .. module,
                            name = module,
                            kind = "Module",
                            language = context.language,
                            members = { { id = e.id, name = e.name, kind = e.kind } },
                            metadata = { override = "true" }
                        }
                        by_module[module] = g
                        groups[#groups + 1] = g
                    else
                        g.members[#g.members + 1] = { id = e.id, name = e.name, kind = e.kind }
                    end
                end
                if #groups == 0 then return nil end
                return groups
            end
        }
    "#;
    let mut registry = PluginRegistry::new();
    registry.register(pattern_extract_plugin(script));

    let pipeline = PreprocessingPipeline::new().with_plugin_registry(Arc::new(registry));
    let mut parsed = ParsedFile::new(
        Language::Python,
        "mod.py".to_string(),
        "def a():\n    pass\n\ndef b():\n    pass\n",
    );
    parsed.add_entity(cce_types::Entity::new(
        cce_types::EntityId(1),
        cce_types::EntityKind::Function,
        "a".to_string(),
        cce_types::Span::new(0, 12, 0, 0, 0, 12),
    ));
    parsed.add_entity(cce_types::Entity::new(
        cce_types::EntityId(2),
        cce_types::EntityKind::Function,
        "b".to_string(),
        cce_types::Span::new(14, 26, 0, 0, 0, 14),
    ));
    let result = pipeline.process(&parsed);

    // All groups come from the override plugin.
    assert!(!result.groups.is_empty());
    assert!(
        result
            .groups
            .iter()
            .all(|g| g.group_id.starts_with("override_")),
        "all groups must originate from the override plugin"
    );
    assert!(
        result
            .groups
            .iter()
            .all(|g| g.metadata.get("override").map(String::as_str) == Some("true")),
        "override metadata must be preserved"
    );
}

/// `RelationExtract` symbols are registered into the project
/// symbol table and relations resolve to entity ids.
#[test]
fn test_relation_extract_registers_symbols_and_relations() {
    let script = r#"
        plugin = {
            id = "spring_relations",
            extract_symbols = function(content, file_path, language)
                return {
                    { id = "svc", name = "UserService", kind = "service", visibility = "public" },
                    { id = "repo", name = "UserRepository", kind = "repository", visibility = "public" }
                }
            end,
            extract_relations = function(content, file_path, language)
                return {
                    { from = "UserService", to = "UserRepository", relation_type = "injects" }
                }
            end
        }
    "#;
    let mut registry = PluginRegistry::new();
    registry.register(pattern_extract_plugin(script));
    let registry = Arc::new(registry);

    let builder = cce_relation::IndexBuilder::new().with_plugin_registry(registry.clone());
    let mut parsed = ParsedFile::new(
        Language::Java,
        "UserService.java".to_string(),
        "class UserService {}",
    );
    parsed.add_entity(cce_types::Entity::new(
        cce_types::EntityId(1),
        cce_types::EntityKind::Class,
        "UserService".to_string(),
        cce_types::Span::new(0, 20, 0, 0, 0, 20),
    ));
    parsed.add_entity(cce_types::Entity::new(
        cce_types::EntityId(2),
        cce_types::EntityKind::Class,
        "UserRepository".to_string(),
        cce_types::Span::new(0, 20, 0, 0, 0, 20),
    ));

    let symbols = builder.create_project_symbol_table(".");
    builder.add_file_symbols(&parsed, &symbols);
    builder.register_file_plugin_symbols(&parsed, &symbols);
    builder.register_file_entities(&parsed);
    builder.inject_plugin_relations(&parsed, &symbols);

    // Both plugin symbols must be registered into the project global index
    // (the resolver fallback consumes `get_by_qualified_name` with the
    // `file::name` form).
    assert!(
        symbols
            .get_by_qualified_name("UserService.java::UserService")
            .is_some(),
        "plugin symbol UserService must be registered"
    );
    assert!(
        symbols
            .get_by_qualified_name("UserService.java::UserRepository")
            .is_some(),
        "plugin symbol UserRepository must be registered"
    );
    // The injected relation must exist in the index.
    use cce_relation::index::RelationQueryOps;
    assert!(
        builder.index().resolved_relation_count() >= 1,
        "plugin relation must be injected into the index"
    );
}

/// LangHeuristics: a Lua plugin maps an unknown capture name to an entity
/// kind, classifies a stdlib module path, and flags a test file.
#[test]
fn test_lang_heuristics_full_chain() {
    use cce_config::project::{GrammarAbiPolicy, LanguageExtensionConflictPolicy};
    use cce_parser::parser::ast_parser::AstParser;
    use cce_parser::parser::extractor::EntityExtractor;
    use cce_types::EntityKind;

    let script = r#"
        plugin = {
            id = "lang_heuristics",
            name = "Lang Heuristics",
            language_name = "tplx",
            language_extensions = { "tplx" },
            remap_grammar_language = "JavaScript",
            query_schemes = {
                entity = [[(function_declaration
                              name: (identifier) @entity.tpl_block.name
                            ) @entity.tpl_block]]
            }
        }

        function plugin.classify_stdlib(module_path)
            if module_path:match("^core%.") then return "Utility" end
            return nil
        end

        function plugin.is_test_file(file_path, content)
            if file_path:match("_spec%.") then return true end
            return nil
        end

        function plugin.entity_kind(capture_name)
            if capture_name == "entity.tpl_block" then return "function" end
            return nil
        end
    "#;
    let mut registry = PluginRegistry::new();
    registry.register(pattern_extract_plugin(script));
    let registry = Arc::new(registry);

    let registered = cce_parser::tree_sitter_init::register_ast_language_plugins(
        &registry,
        LanguageExtensionConflictPolicy::Allow,
        GrammarAbiPolicy::Deny,
    );
    assert_eq!(registered, 1, "remap language must register");
    let index =
        cce_types::language::plugin_language_for_extension("tplx").expect("tplx extension routed");
    let custom = Language::Custom(index);

    // 0. Facade-level checks: the three heuristics answer in priority order.
    use cce_types::StdlibCategory;
    assert_eq!(
        cce_parser::plugin::heuristics::classify_stdlib(&registry, "core.utils.fmt"),
        Some(StdlibCategory::Utility)
    );
    assert_eq!(
        cce_parser::plugin::heuristics::classify_stdlib(&registry, "my.lib"),
        None
    );
    assert_eq!(
        cce_parser::plugin::heuristics::is_test_file(&registry, "src/app_spec.tplx", ""),
        Some(true)
    );
    assert_eq!(
        cce_parser::plugin::heuristics::is_test_file(&registry, "src/app.rs", ""),
        None
    );
    assert_eq!(
        cce_parser::plugin::heuristics::entity_kind(&registry, "entity.tpl_block"),
        Some(EntityKind::Function)
    );
    assert_eq!(
        cce_parser::plugin::heuristics::entity_kind(&registry, "entity.unknown.thing"),
        None
    );

    // 1. Entity-kind heuristic: the custom capture maps to Function.
    let content = "function greet(name) { return \"hi \" + name; }";
    let (tree, _) = AstParser::new()
        .parse_with_tree(content, &custom)
        .expect("parse via remapped grammar");
    let extractor = EntityExtractor::new().with_heuristics_registry(registry.clone());
    let entities = extractor
        .extract(&tree, content, &custom)
        .expect("extract entities");
    assert!(
        entities
            .iter()
            .any(|e| e.kind == EntityKind::Function && e.name == "greet"),
        "plugin entity-kind mapping must classify entity.tpl_block as a function"
    );

    // 2. Test-file heuristic via the grouper pipeline (path rule leaves
    // `.tplx` unknown; the plugin marks `_spec.` files as tests).
    let mut parsed = ParsedFile::new(custom, "src/app_spec.tplx".to_string(), content);
    for e in entities {
        parsed.add_entity(e);
    }
    let pipeline = PreprocessingPipeline::new().with_plugin_registry(registry);
    let result = pipeline.process(&parsed);
    assert!(
        !result.groups.is_empty(),
        "groups must be produced from the remapped language"
    );
    assert!(
        result.groups.iter().all(|g| g.test_info.is_test()),
        "plugin is_test_file must mark the file as a test file: {:?}",
        result
            .groups
            .iter()
            .map(|g| g.test_info)
            .collect::<Vec<_>>()
    );

    cce_parser::tree_sitter_init::clear_plugin_languages_all();
}
