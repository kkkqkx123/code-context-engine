-- plugin/proto_format.lua
-- Demo: FormatParse — parse a custom document format via regex patterns.
--
-- This plugin declares `parse_document` as a Lua function. It is a
-- minimal `.proto`-style parser that extracts message/service entities.
-- The host routes `.proto` files here (when this plugin is enabled and
-- matched by file_patterns) before the built-in document pipelines.

plugin = {
    id = "proto_format_plugin",
    name = "Proto Format Parser",
    version = "0.1.0",
    priority = 5,
    description = "Parses .proto files into message/service entities (FormatParse demo).",
    capabilities = { "format_parse" }
}

--- Parse a document into { title, language, entities }.
-- `entities` is a 1-indexed array of entity tables with fields:
--   id, kind, name, signature, doc_comment, metadata, span, children
function plugin.parse_document(content, file_path)
    local entities = {}
    local index = 0

    -- Messages
    for match in content:gmatch("message%s+([%w_]+)%s*{") do
        index = index + 1
        entities[index] = {
            id = "message_" .. index,
            kind = "message",
            name = match,
            signature = "message " .. match,
            doc_comment = "Protocol buffer message definition."
        }
    end

    -- Services
    for match in content:gmatch("service%s+([%w_]+)%s*{") do
        index = index + 1
        entities[index] = {
            id = "service_" .. index,
            kind = "service",
            name = match,
            signature = "service " .. match,
            doc_comment = "Protocol buffer RPC service definition."
        }
    end

    if index == 0 then
        return nil -- decline; built-in pipeline handles it
    end

    return {
        title = file_path,
        language = "proto",
        entities = entities
    }
end
