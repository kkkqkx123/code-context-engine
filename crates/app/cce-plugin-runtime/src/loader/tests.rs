use crate::error::PluginError;
use crate::loader::lua_plugin::LuaPlugin;
use cce_plugin::CodePlugin;
use cce_types::ast_to_nl::RerankCandidate;
use cce_types::plugin::GroupPluginContext;
use std::time::Duration;

fn empty_group() -> cce_types::grouper::EntityGroup {
    cce_types::grouper::EntityGroup::default()
}

#[test]
fn test_load_valid_plugin() {
    let script = r#"
            plugin = {
                id = "test_plugin",
                name = "Test Plugin",
                version = "1.0.0",
                priority = 10,
                description = "A test plugin"
            }
        "#;

    let plugin = LuaPlugin::from_script(script);
    assert!(plugin.is_ok());
    let plugin = plugin.expect("Failed to load valid plugin");
    assert_eq!(plugin.metadata().id, "test_plugin");
    assert_eq!(plugin.metadata().name, "Test Plugin");
    assert_eq!(plugin.metadata().version, "1.0.0");
    assert_eq!(plugin.metadata().priority, 10);
    assert_eq!(
        plugin.metadata().description.as_deref(),
        Some("A test plugin")
    );
}

#[test]
fn test_priority_defaults_to_zero_when_absent() {
    let script = r#"
            plugin = { id = "no_priority" }
        "#;
    let plugin = LuaPlugin::from_script(script).expect("plugin without priority loads");
    assert_eq!(plugin.metadata().priority, 0);
}

#[test]
fn test_language_remap_declaration_probed() {
    let script = r#"
            plugin = {
                id = "remap_probe",
                language_name = "mytpl",
                language_extensions = { "tplx", "mtpl" },
                remap_grammar_language = "JavaScript",
                query_schemes = { entity = "(function_declaration) @entity" }
            }
        "#;
    let plugin = LuaPlugin::from_script(script).expect("remap plugin loads");
    assert!(plugin.supports_language_remap());
    assert_eq!(plugin.language_name().as_deref(), Some("mytpl"));
    assert_eq!(plugin.language_extensions(), vec!["tplx", "mtpl"]);
    assert_eq!(
        plugin.remap_grammar_language().as_deref(),
        Some("JavaScript")
    );
    assert_eq!(
        plugin.query_scheme(cce_types::QueryType::Entity).as_deref(),
        Some("(function_declaration) @entity")
    );
    assert!(
        !plugin.supports_ast_language(),
        "Lua remap plugins must not report AstLanguage (no FFI)"
    );
}

#[test]
fn test_language_remap_missing_declaration_disables_capability() {
    let script = r#"
            plugin = { id = "no_remap" }
        "#;
    let plugin = LuaPlugin::from_script(script).expect("plugin loads");
    assert!(!plugin.supports_language_remap());
    assert!(plugin.language_name().is_none());
    assert!(plugin.remap_grammar_language().is_none());
}

#[test]
fn test_lang_heuristics_functions_probed() {
    let script = r#"
            plugin = {
                id = "heuristics_probe",
                classify_stdlib = function(module_path)
                    if module_path:match("^core%.") then return "Utility" end
                    return nil
                end,
                is_test_file = function(file_path, content)
                    return file_path:match("_spec%.") and true or nil
                end,
                entity_kind = function(capture_name)
                    if capture_name == "entity.tpl_block" then return "function" end
                    return nil
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).expect("heuristics plugin loads");
    assert!(plugin.supports_any_heuristic());
    assert!(plugin.supports_stdlib_heuristic());
    assert!(plugin.supports_test_file_heuristic());
    assert!(plugin.supports_entity_kind_heuristic());
    assert_eq!(
        plugin
            .classify_stdlib("core.utils.fmt")
            .expect("classify call"),
        Some("Utility".to_string())
    );
    assert_eq!(
        plugin.classify_stdlib("my.lib").expect("decline call"),
        None
    );
    assert_eq!(
        plugin
            .is_test_file("src/app_spec.tplx", "")
            .expect("test call"),
        Some(true)
    );
    assert_eq!(
        plugin
            .is_test_file("src/app.rs", "fn main() {}")
            .expect("defer call"),
        None
    );
    assert_eq!(
        plugin.entity_kind("entity.tpl_block").expect("kind call"),
        Some("function".to_string())
    );
    assert_eq!(
        plugin.entity_kind("entity.other").expect("defer call"),
        None
    );
}

#[test]
fn test_lang_heuristics_absent_by_default() {
    let plugin = LuaPlugin::from_script("plugin = { id = \"plain\" }").expect("loads");
    assert!(!plugin.supports_any_heuristic());
    assert!(!plugin.supports_stdlib_heuristic());
    assert!(!plugin.supports_test_file_heuristic());
    assert!(!plugin.supports_entity_kind_heuristic());
}

#[test]
fn test_priority_wrong_type_is_load_error() {
    let script = r#"
            plugin = { id = "bad_priority", priority = "high" }
        "#;
    let result = LuaPlugin::from_script(script);
    assert!(
        result.is_err(),
        "string priority must fail loading instead of silently defaulting to 0"
    );
}

#[test]
fn test_priority_negative_accepted_as_fallback_tier() {
    let script = r#"
            plugin = { id = "neg_priority", priority = -1 }
        "#;
    let plugin = LuaPlugin::from_script(script)
        .expect("negative priority loads (below-builtin fallback tier)");
    assert_eq!(plugin.metadata().priority, -1);
}

#[test]
fn test_priority_out_of_i32_range_is_load_error() {
    let script = r#"
            plugin = { id = "huge_priority", priority = 4294967296 }
        "#;
    let result = LuaPlugin::from_script(script);
    assert!(result.is_err(), "overflowing priority must fail loading");
}

#[test]
fn test_priority_float_integral_accepted() {
    let script = r#"
            plugin = { id = "float_priority", priority = 30.0 }
        "#;
    let plugin = LuaPlugin::from_script(script).expect("integral float priority loads");
    assert_eq!(plugin.metadata().priority, 30);
}

#[test]
fn test_load_invalid_lua_syntax() {
    let script = r#"
            plugin = {
                id = "bad_plugin",
                -- Missing closing bracket causes syntax error
        "#;

    let result = LuaPlugin::from_script(script);
    assert!(result.is_err());
}

#[test]
fn test_generate_bm25_with_and_without_function() {
    let script_with = r#"
            plugin = {
                id = "bm25_plugin",
                description = "BM25 generator",
                generate_bm25 = function(group)
                    return "bm25 text for " .. group.name
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script_with).unwrap();
    assert!(plugin.supports_bm25());

    let result = plugin.generate_bm25(&empty_group()).unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "bm25 text for ");

    let script_without = r#"
            plugin = { id = "no_bm25" }
        "#;
    let plugin = LuaPlugin::from_script(script_without).unwrap();
    assert!(!plugin.supports_bm25());

    let result = plugin.generate_bm25(&empty_group()).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_generate_embedding_with_function() {
    let script = r#"
            plugin = {
                id = "emb_plugin",
                generate_embedding = function(group)
                    return "embedding:" .. group.name .. " kind=" .. group.kind
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_embedding());

    let result = plugin.generate_embedding(&empty_group()).unwrap();
    assert!(result.is_some());
    assert!(result.unwrap().contains("embedding:"));
}

#[test]
fn test_has_methods() {
    let all_three = r#"
            plugin = {
                id = "full",
                generate_bm25 = function(g) return nil end,
                generate_embedding = function(g) return nil end,
            }
        "#;
    let plugin = LuaPlugin::from_script(all_three).unwrap();
    assert!(plugin.supports_bm25());
    assert!(plugin.supports_embedding());

    let only_bm25 = r#"
            plugin = { id = "bm25_only", generate_bm25 = function(g) return nil end }
        "#;
    let plugin = LuaPlugin::from_script(only_bm25).unwrap();
    assert!(plugin.supports_bm25());
    assert!(!plugin.supports_embedding());

    let only_emb = r#"
            plugin = { id = "emb_only", generate_embedding = function(g) return nil end }
        "#;
    let plugin = LuaPlugin::from_script(only_emb).unwrap();
    assert!(!plugin.supports_bm25());
    assert!(plugin.supports_embedding());
}

#[test]
fn test_generate_bm25_batch_with_function() {
    let script = r#"
            plugin = {
                id = "batch_bm25",
                generate_bm25_batch = function(groups)
                    local results = {}
                    for i, group in ipairs(groups) do
                        results[i] = "item:" .. group.name
                    end
                    return results
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    let group1 = empty_group();
    let group2 = empty_group();
    let groups = vec![&group1, &group2];
    let results = plugin.generate_bm25_batch(&groups).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[0].is_some());
    assert!(results[1].is_some());
}

#[test]
fn test_generate_bm25_batch_fallback_to_single() {
    let script = r#"
            plugin = {
                id = "fallback_bm25",
                generate_bm25 = function(group)
                    return "single:" .. group.name
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    let group1 = empty_group();
    let group2 = empty_group();
    let groups = vec![&group1, &group2];
    let results = plugin.generate_bm25_batch(&groups).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[0].is_some());
    assert!(results[1].is_some());
}

#[test]
fn test_generate_bm25_batch_without_function() {
    let script = r#"plugin = { id = "no_batch" }"#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    let group1 = empty_group();
    let group2 = empty_group();
    let group3 = empty_group();
    let groups = vec![&group1, &group2, &group3];
    let results = plugin.generate_bm25_batch(&groups).unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.is_none()));
}

#[test]
fn test_generate_embedding_batch_with_function() {
    let script = r#"
            plugin = {
                id = "batch_emb",
                generate_embedding_batch = function(groups)
                    local results = {}
                    for i, group in ipairs(groups) do
                        results[i] = "emb:" .. group.kind
                    end
                    return results
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    let group1 = empty_group();
    let groups = vec![&group1];
    let results = plugin.generate_embedding_batch(&groups).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].is_some());
}

#[test]
fn test_load_without_plugin_table() {
    let script = r#"local x = 42"#;
    let result = LuaPlugin::from_script(script);
    assert!(result.is_err());
}

#[test]
fn test_metadata_defaults_when_missing() {
    let script = r#"plugin = {}"#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.metadata().id.starts_with("lua_plugin_"));
    assert_eq!(plugin.metadata().name, plugin.metadata().id);
    assert_eq!(plugin.metadata().version, "0.1.0");
    assert_eq!(plugin.metadata().priority, 0);
}

#[test]
fn test_metadata_unique_ids_for_unnamed_plugins() {
    let script = r#"plugin = {}"#;
    let p1 = LuaPlugin::from_script(script).unwrap();
    let p2 = LuaPlugin::from_script(script).unwrap();
    assert_ne!(p1.metadata().id, p2.metadata().id);
}

#[test]
fn test_empty_batch_input() {
    let script = r#"
            plugin = {
                id = "empty_batch",
                generate_bm25 = function(groups)
                    return {}
                end,
                generate_embedding = function(groups)
                    return {}
                end,
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    let empty: Vec<&cce_types::grouper::EntityGroup> = vec![];

    let bm25_results = plugin.generate_bm25_batch(&empty).unwrap();
    assert!(bm25_results.is_empty());

    let emb_results = plugin.generate_embedding_batch(&empty).unwrap();
    assert!(emb_results.is_empty());
}

#[test]
fn test_generate_bm25_batch_lua_list_input() {
    let script = r#"
            plugin = {
                id = "batch_list",
                generate_bm25_batch = function(groups)
                    local results = {}
                    for i, group in ipairs(groups) do
                        results[i] = "result:" .. group.name
                    end
                    return results
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    let group1 = empty_group();
    let group2 = empty_group();
    let groups = vec![&group1, &group2];
    let results = plugin.generate_bm25_batch(&groups).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].as_deref(), Some("result:"));
    assert_eq!(results[1].as_deref(), Some("result:"));
}

#[test]
fn test_concurrent_generate_reuses_vm_pool() {
    use std::sync::Arc;
    use std::thread;

    let script = r#"
            plugin = {
                id = "conc",
                generate_bm25 = function(group)
                    return "bm25:" .. group.name
                end
            }
        "#;
    let plugin = Arc::new(LuaPlugin::from_script(script).unwrap());
    let mut handles = Vec::new();
    for _ in 0..8 {
        let plugin = plugin.clone();
        handles.push(thread::spawn(move || {
            let group = empty_group();
            plugin.generate_bm25(&group)
        }));
    }
    for h in handles {
        let result = h.join().unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }
}

// ── Capability tests ────────────────────────────────────────────

#[test]
fn test_extract_entities_via_patterns() {
    let script = r#"
            plugin = {
                id = "pat",
                patterns = {
                    {
                        name = "route",
                        regex = "@app\\.route\\('(?P<name>[^']+)'\\)[\\s\\S]*?\\n\\s*def\\s+(?P<signature>[\\w_]+)\\(",
                        kind = "route"
                    }
                }
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_extract());
    let content = "@app.route('/users')\ndef users():\n    pass\n@app.route('/items')\ndef items():\n    pass\n";
    let entities = plugin
        .extract_entities(content, "app.py", "python")
        .unwrap()
        .expect("entities extracted");
    assert_eq!(entities.len(), 2);
    assert_eq!(entities[0].name, "/users");
    assert_eq!(entities[0].kind, "route");
    assert_eq!(entities[0].id, "route_0");
    assert!(entities[0].span.is_some());
}

#[test]
fn test_parse_document_function() {
    let script = r#"
            plugin = {
                id = "doc",
                parse_document = function(content, file_path)
                    return {
                        title = file_path,
                        language = "proto",
                        entities = {
                            { id = "m1", kind = "message", name = "User",
                              signature = "message User", doc_comment = "A user." }
                        }
                    }
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_parse());
    let doc = plugin
        .parse_document("message User {}", "api.proto")
        .unwrap()
        .expect("document parsed");
    assert_eq!(doc.language.as_deref(), Some("proto"));
    assert_eq!(doc.entities.len(), 1);
    assert_eq!(doc.entities[0].name, "User");
    assert_eq!(doc.entities[0].kind, "message");
}

#[test]
fn test_post_group_hook() {
    let script = r#"
            plugin = {
                id = "hook",
                post_group = function(groups, context)
                    -- Rename every group and annotate metadata.
                    local out = {}
                    for i = 1, #groups do
                        local g = groups[i]
                        g.name = g.name .. "_x"
                        g.metadata = { touched = context.file_path }
                        out[i] = g
                    end
                    return out
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_group());
    let mut group = empty_group();
    group.name = "fn".into();
    let context = GroupPluginContext {
        file_path: "src/app.py".to_string(),
        language: "python".to_string(),
        source: "source".to_string(),
        entities: Vec::new(),
        relations: Vec::new(),
    };
    let out = plugin
        .post_group(vec![group], context)
        .unwrap()
        .expect("groups returned");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].name, "fn_x");
    assert_eq!(
        out[0].metadata.get("touched").map(String::as_str),
        Some("src/app.py")
    );
}

#[test]
fn test_rerank_function() {
    let script = r#"
            plugin = {
                id = "rr",
                rerank = function(query, candidates)
                    local out = {}
                    for i = 1, #candidates do
                        out[i] = {
                            id = candidates[i].id,
                            rerank_score = 0.9,
                            initial_score = candidates[i].initial_score,
                            final_score = 0.9,
                            rank_change = 0,
                            reasoning = "demo"
                        }
                    end
                    return { reranked_candidates = out }
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_rerank());
    let candidates = vec![RerankCandidate {
        id: "c1".to_string(),
        content: "fn handle()".to_string(),
        file_path: "app.py".to_string(),
        initial_score: 0.5,
        entity_type: Some("function".to_string()),
        metadata: Default::default(),
    }];
    let result = plugin
        .rerank("users", candidates)
        .unwrap()
        .expect("rerank result");
    assert_eq!(result.reranked_candidates.len(), 1);
    assert_eq!(result.reranked_candidates[0].id, "c1");
    assert_eq!(result.reranked_candidates[0].final_score, 0.9);
}

#[test]
fn test_capabilities_declared_from_table() {
    let script = r#"
            plugin = {
                id = "caps",
                capabilities = { "text_gen", "entity_extract" }
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert_eq!(
        plugin.metadata.capabilities,
        vec!["text_gen".to_string(), "entity_extract".to_string()]
    );
}

#[test]
fn test_chunk_function() {
    let script = r#"
            plugin = {
                id = "ck",
                chunk = function(conversions, file_path)
                    local out = {}
                    for i = 1, #conversions do
                        out[i] = {
                            chunk_id = "plugin_chunk_" .. i,
                            source_group_id = tostring(conversions[i].group.group_id),
                            path = "bm25",
                            group_type = "Standalone",
                            chunk_index = 0,
                            total_chunks = 1,
                            text = "plugin-chunked " .. conversions[i].group.name,
                            token_count = 3,
                            start_byte = 0,
                            end_byte = 0,
                            self_contained = false,
                            content_type = "document",
                            file_path = file_path,
                            segment_id = tostring(conversions[i].group.group_id)
                        }
                    end
                    return out
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_chunk());

    let mut group = empty_group();
    group.group_id = "group_1".into();
    group.name = "hello".into();
    let conversions = vec![cce_types::GroupConversions {
        group,
        header_conversion: None,
        member_conversions: Vec::new(),
    }];
    let chunks = plugin
        .chunk(conversions, "src/app.py")
        .unwrap()
        .expect("chunks returned");
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].chunk_id, "plugin_chunk_1");
    assert_eq!(chunks[0].source_group_id, "group_1");
    assert!(chunks[0].text.contains("hello"));
}

// ── Extended capabilities ────────────────────────────────────────

#[test]
fn test_group_override_detection_and_call() {
    let script = r#"
            plugin = {
                id = "override_plugin",
                group = function(context)
                    local groups = {}
                    for i = 1, #context.entities do
                        groups[i] = { group_id = "g" .. i, name = context.entities[i].name }
                    end
                    return groups
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_group_override());

    let context = GroupPluginContext {
        file_path: "mod.py".into(),
        language: "python".into(),
        source: "def a(): pass".into(),
        entities: vec![cce_types::PluginEntity::new("1", "function", "a")],
        relations: Vec::new(),
    };
    let groups = plugin.group(context).unwrap().expect("groups returned");
    assert_eq!(groups.len(), 1);
    assert!(groups[0].group_id.starts_with("g"));
}

#[test]
fn test_relation_extract_symbols_and_relations() {
    let script = r#"
            plugin = {
                id = "rel_plugin",
                extract_symbols = function(content, file_path, language)
                    return { { id = "svc", name = "Svc", kind = "service", visibility = "public" } }
                end,
                extract_relations = function(content, file_path, language)
                    return { { from = "Svc", to = "Repo", relation_type = "injects" } }
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_relation_extract());

    let symbols = plugin
        .extract_symbols("class Svc {}", "Svc.java", "java")
        .unwrap()
        .expect("symbols returned");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Svc");

    let relations = plugin
        .extract_relations("class Svc {}", "Svc.java", "java")
        .unwrap()
        .expect("relations returned");
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].relation_type, "injects");
}

#[test]
fn test_query_rewrite_and_fusion_and_filter() {
    let script = r#"
            plugin = {
                id = "query_plugin",
                rewrite_query = function(query)
                    return { rewritten_query = query .. " extended", expansion_terms = { "extra" } }
                end,
                fusion_weights = function(query, vector_count, bm25_count)
                    return { vector_weight = 0.7, bm25_weight = 0.3 }
                end,
                filter_results = function(query, results)
                    local out = {}
                    for i = 1, #results do
                        if results[i].id ~= "noise" then
                            out[#out + 1] = { id = results[i].id, remove = false }
                        end
                    end
                    return out
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_query_rewrite());
    assert!(plugin.supports_fusion());
    assert!(plugin.supports_result_filter());

    let rw = plugin
        .rewrite_query("vector search")
        .unwrap()
        .expect("rewrite returned");
    assert!(rw.rewritten_query.contains("extended"));
    assert_eq!(rw.expansion_terms, vec!["extra".to_string()]);

    let weights = plugin
        .fusion_weights("vector search", 10, 5)
        .unwrap()
        .expect("weights returned");
    assert_eq!(weights.vector_weight, Some(0.7));
    assert_eq!(weights.bm25_weight, Some(0.3));

    let candidates = vec![
        cce_types::RerankCandidate {
            id: "noise".into(),
            content: "x".into(),
            file_path: "n/f".into(),
            initial_score: 0.5,
            entity_type: None,
            metadata: Default::default(),
        },
        cce_types::RerankCandidate {
            id: "keep".into(),
            content: "y".into(),
            file_path: "k/f".into(),
            initial_score: 0.4,
            entity_type: None,
            metadata: Default::default(),
        },
    ];
    let entries = plugin
        .filter_results("query", candidates)
        .unwrap()
        .expect("entries returned");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "keep");
}

#[test]
fn test_file_filter_decision() {
    let script = r#"
            plugin = {
                id = "file_filter_plugin",
                filter_file = function(file_path, is_directory, size)
                    if file_path:find("scratch") then return "exclude" end
                    if file_path:find("%.cconf$") then return "include" end
                    return nil
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_file_filter());

    assert_eq!(
        plugin.filter_file("a/scratch/x.txt", false, 10).unwrap(),
        Some(cce_types::FileFilterDecision::Exclude)
    );
    assert_eq!(
        plugin.filter_file("a/app.cconf", false, 10).unwrap(),
        Some(cce_types::FileFilterDecision::Include)
    );
    assert_eq!(plugin.filter_file("a/readme.md", false, 10).unwrap(), None);
}

// ── SymbolExtract ────────────────────────────────────────────────

#[test]
fn test_symbol_extract_imports_and_exports() {
    let script = r#"
            plugin = {
                id = "zig_symbol_extract",
                capabilities = { "symbol_extract" },
                extract_imports = function(content, file_path, language)
                    local imports = {}
                    for line in content:gmatch("[^\r\n]+") do
                        local path = line:match("const%s+%w+%s*=%s*@import%(\"([^\"]+)\"%)")
                        if path then
                            imports[#imports + 1] = { path = path, is_wildcard = false }
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
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(plugin.supports_symbol_extract());

    let content = "const std = @import(\"std\");\npub fn main() void {}";
    let imports = plugin
        .extract_imports(content, "lib.zig", "zig")
        .unwrap()
        .expect("imports returned");
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].path, "std");

    let exports = plugin
        .extract_exports(content, "lib.zig", "zig")
        .unwrap()
        .expect("exports returned");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "main");
    assert_eq!(exports[0].kind, "function");
}

#[test]
fn test_symbol_extract_declines_without_functions() {
    let script = r#"plugin = { id = "no_symbol_extract" }"#;
    let plugin = LuaPlugin::from_script(script).unwrap();
    assert!(!plugin.supports_symbol_extract());
    assert_eq!(
        plugin
            .extract_imports("const std = @import(\"std\");", "lib.zig", "zig")
            .unwrap(),
        None
    );
    assert_eq!(
        plugin
            .extract_exports("pub fn main() void {}", "lib.zig", "zig")
            .unwrap(),
        None
    );
}

// ── Safety guards: timeout / memory limit / pool reuse ─────────────

#[test]
fn test_infinite_loop_times_out() {
    let script = r#"
            plugin = {
                id = "infinite",
                generate_bm25 = function(group)
                    while true do end
                end
            }
        "#;
    let plugin = LuaPlugin::with_timeout(script, Duration::from_millis(300)).expect("plugin loads");
    let start = std::time::Instant::now();
    let result = plugin.generate_bm25(&empty_group());
    assert!(
        matches!(result, Err(PluginError::Timeout)),
        "infinite loop must surface as Timeout, got: {result:?}"
    );
    // The caller gives up within the configured budget.
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn test_vm_pool_usable_after_timeout() {
    let script = r#"
            plugin = {
                id = "flaky",
                generate_bm25 = function(group)
                    if group.name == "slow" then
                        while true do end
                    end
                    return "ok:" .. group.name
                end
            }
        "#;
    let plugin = LuaPlugin::with_timeout(script, Duration::from_millis(200)).expect("plugin loads");

    let mut slow = empty_group();
    slow.name = "slow".into();
    let result = plugin.generate_bm25(&slow);
    assert!(matches!(result, Err(PluginError::Timeout)));

    // A subsequent call must still succeed (new VM created; the timed-out
    // worker may linger but must not poison the pool).
    let mut fast = empty_group();
    fast.name = "fast".into();
    let result = plugin.generate_bm25(&fast).expect("call after timeout");
    assert_eq!(result.as_deref(), Some("ok:fast"));
}

#[test]
fn test_memory_limit_interrupts_allocation() {
    let script = r#"
            plugin = {
                id = "hog",
                generate_bm25 = function(group)
                    local t = {}
                    local i = 0
                    while true do
                        i = i + 1
                        t[i] = string.rep("x", 1024)
                    end
                end
            }
        "#;
    // Tight memory budget: the allocation loop must be interrupted by the
    // debug hook long before it exhausts host memory.
    let plugin =
        LuaPlugin::with_options(script, Duration::from_secs(5), 256).expect("plugin loads");
    let start = std::time::Instant::now();
    let result = plugin.generate_bm25(&empty_group());
    match result {
        Err(PluginError::ScriptError(msg)) => {
            assert!(
                msg.to_lowercase().contains("memory"),
                "error must mention the memory limit, got: {msg}"
            );
        }
        Err(PluginError::Timeout) => {
            panic!("memory limit must interrupt allocation before the timeout");
        }
        other => panic!("expected memory-limit ScriptError, got: {other:?}"),
    }
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn test_memory_limit_budget_honored_for_small_scripts() {
    let script = r#"
            plugin = {
                id = "tiny",
                generate_bm25 = function(group)
                    return "small"
                end
            }
        "#;
    let plugin =
        LuaPlugin::with_options(script, Duration::from_secs(1), 256).expect("plugin loads");
    let result = plugin.generate_bm25(&empty_group()).expect("call succeeds");
    assert_eq!(result.as_deref(), Some("small"));
}

#[test]
fn test_script_error_is_reported_without_poisoning_pool() {
    let script = r#"
            plugin = {
                id = "erroring",
                generate_bm25 = function(group)
                    error("boom")
                end
            }
        "#;
    let plugin = LuaPlugin::from_script(script).expect("plugin loads");
    let result = plugin.generate_bm25(&empty_group());
    assert!(
        matches!(result, Err(PluginError::ScriptError(_))),
        "runtime error must surface as ScriptError, got: {result:?}"
    );
    // The VM returns to the pool in a usable state.
    let again = plugin.generate_bm25(&empty_group());
    assert!(matches!(again, Err(PluginError::ScriptError(_))));
}
