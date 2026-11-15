-- plugin/zig_symbol_extract.lua
-- Demo: SymbolExtract — import/export extraction for a custom language.
--
-- Extracts `@import("...")` paths and `pub fn` exports from Zig source so
-- custom-language files (`Language::Custom`) obtain import/export metadata in
-- the relation index (import tables + cross-file dependencies).
--
-- Requires `relation.plugin_symbol_extract_enabled = true` in config, and an
-- `ast_language` plugin providing the Zig grammar for AST parsing.

plugin = {
    id = "zig_symbol_extract_plugin",
    name = "Zig Symbol Extract",
    version = "0.1.0",
    priority = 10,
    description = "Extracts Zig @import paths and pub fn exports (SymbolExtract demo).",
    capabilities = { "symbol_extract" }
}

function plugin.extract_imports(content, file_path, language)
    local imports = {}
    for line in content:gmatch("[^\r\n]+") do
        local path = line:match("const%s+%w+%s*=%s*@import%(\"([^\"]+)\"%)")
        if path then
            imports[#imports + 1] = {
                path = path,
                is_wildcard = false,
                metadata = { kind = "module" }
            }
        end
    end
    if #imports == 0 then
        return nil
    end
    return imports
end

function plugin.extract_exports(content, file_path, language)
    local exports = {}
    for line in content:gmatch("[^\r\n]+") do
        local name = line:match("pub%s+fn%s+(%w+)%s*%(")
        if name then
            exports[#exports + 1] = {
                name = name,
                kind = "function",
                visibility = "public"
            }
        end
    end
    if #exports == 0 then
        return nil
    end
    return exports
end
