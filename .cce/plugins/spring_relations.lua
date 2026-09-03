-- plugin/spring_relations.lua
-- Demo: RelationExtract — supplementary symbols + explicit relations.
--
-- Extracts Spring-style `@Service` / `@Repository` bean symbols and the
-- `@Autowired`/constructor injection edges between them. The host registers
-- the symbols into the project symbol table and resolves the relation edges
-- into the relation index (call chains / dependency queries).
--
-- Requires `relation.plugin_symbols_enabled = true` in config.

plugin = {
    id = "spring_relations_plugin",
    name = "Spring Bean Relations",
    version = "0.1.0",
    priority = 10,
    description = "Extracts Spring @Service/@Repository beans and injection edges (RelationExtract demo).",
    capabilities = { "relation_extract" }
}

function plugin.extract_symbols(content, file_path, language)
    local symbols = {}
    local pending_service = false
    local pending_repo = false
    for line in content:gmatch("[^\r\n]+") do
        if line:match("@Service") then
            pending_service = true
        end
        if line:match("@Repository") then
            pending_repo = true
        end
        local name = line:match("class%s+([%w_]+)") or line:match("interface%s+([%w_]+)")
        if name and (pending_service or pending_repo) then
            symbols[#symbols + 1] = {
                id = name,
                name = name,
                kind = pending_service and "service" or "repository",
                visibility = "public",
                metadata = { bean = "true" }
            }
            pending_service = false
            pending_repo = false
        end
    end
    if #symbols == 0 then
        return nil
    end
    return symbols
end

function plugin.extract_relations(content, file_path, language)
    local relations = {}
    local autowired_pending = false
    for line in content:gmatch("[^\r\n]+") do
        if line:match("@Autowired") then
            autowired_pending = true
        end
        if autowired_pending then
            local field = line:match("private%s+[%w_]+%s+([%w_]+)")
            if not field then
                field = line:match("([%w_]+)%s+%w+%s*;")
            end
            if field then
                relations[#relations + 1] = {
                    from = field,
                    to = field,
                    relation_type = "injects",
                    metadata = { framework = "spring" }
                }
                autowired_pending = false
            end
        end
    end
    if #relations == 0 then
        return nil
    end
    return relations
end
