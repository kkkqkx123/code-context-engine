-- plugin/flask_routes.lua
-- Demo: Flask Route NL Template Extensions + pattern-based route extraction

plugin = {
    id = "flask_route_plugin",
    name = "Flask Route Templater",
    version = "0.1.0",
    priority = 10, -- Prioritized over built-in logic
    description = "Generates semantic descriptions for Flask route handlers and extracts route entities via regex patterns.",
    capabilities = { "text_gen", "entity_extract" }
}

--- 1. BM25 Template generation (AST-to-NL layer)
-- Generate keyword-inclusive descriptions for search engines
function plugin.generate_bm25(group)
    local desc = string.format("Flask route handler function %s.", group.name)

    -- Simulating Endpoint and Method Extraction from Metadata
    -- In the actual Rust implementation, these fields are passed in via Entity.metadata
    if group.metadata and group.metadata.endpoint then
        desc = desc .. string.format(" Endpoint: %s.", group.metadata.endpoint)
    end
    if group.metadata and group.metadata.methods then
        desc = desc .. string.format(" HTTP Methods: %s.", group.metadata.methods)
    end

    return desc
end

--- 3. Embedding template generation (AST-to-NL layer)
-- Generating pure semantic descriptions for vector models
function plugin.generate_embedding(group)
    local desc = string.format("Handles web requests for the %s endpoint.", group.name)

    if group.metadata and group.metadata.methods then
        desc = desc .. string.format(" Supports %s operations.", group.metadata.methods)
    end

    return desc
end

--- 4. EntityExtract: pattern-based supplementary entity extraction.
-- The host compiles these Rust regexes and maps named captures to fields:
--   name        -> entity name
--   signature   -> entity signature
--   meta_<key>  -> metadata["<key>"]
-- Each match becomes a standalone "route" group that flows through the
-- grouper -> NL -> chunker pipeline alongside the tree-sitter entities.
plugin.patterns = {
    {
        name = "route",
        regex = "@app\\.route\\(['\"](?P<name>[^'\"]+)['\"]\\)[\\s\\S]*?\\n\\s*def\\s+(?P<signature>[\\w_]+)\\(",
        kind = "route"
    }
}
