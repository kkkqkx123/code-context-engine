-- .cce/plugins/tpl_heuristics.lua
-- Demo: LangHeuristics — language-specific heuristics for the remapped
-- `tmpl` language (see `tmpl_remap.lua`).
--
-- Three independent hooks, each optional:
--   classify_stdlib(module_path) -> category name or nil: marks entities
--     whose module path belongs to the language's standard library.
--   is_test_file(file_path, content) -> true/false or nil: decides
--     test-file status when the built-in path rule has no signal.
--   entity_kind(capture_name) -> kind name or nil: maps tree-sitter capture
--     names unknown to the built-in mapping onto entity kinds.
--
-- All hooks return nil to defer to the built-in logic.

plugin = {
    id = "tpl_heuristics_plugin",
    name = "Template Language Heuristics",
    version = "0.1.0",
    priority = 10,
    description = "LangHeuristics demo for the 'tmpl' language (stdlib / test-file / entity-kind).",
    capabilities = { "lang_heuristics" }
}

function plugin.classify_stdlib(module_path)
    if module_path:match("^tmpl%.") then
        return "Utility"
    end
    return nil
end

function plugin.is_test_file(file_path, content)
    if file_path:match("_spec%.") then
        return true
    end
    return nil
end

function plugin.entity_kind(capture_name)
    if capture_name == "entity.tpl_block" then
        return "function"
    end
    return nil
end
