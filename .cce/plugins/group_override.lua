-- plugin/group_override.lua
-- Demo: GroupOverride — full replacement of the built-in grouper.
--
-- Receives a GroupPluginContext carrying the serialized parsed entities and
-- raw relations, and returns a list of EntityGroup tables. When a non-empty
-- list is returned, the built-in grouping stages are skipped entirely.
--
-- This example creates one group per top-level entity annotated with a
-- "@module:" metadata marker, mimicking module-based grouping.

plugin = {
    id = "group_override_plugin",
    name = "Module-Based Group Override",
    version = "0.1.0",
    priority = 9,
    description = "Fully replaces built-in grouping with per-module groups (GroupOverride demo).",
    capabilities = { "group_override" }
}

function plugin.group(context)
    -- context: { file_path, language, source, entities = {PluginEntity,...},
    --            relations = {PluginRelation,...} }
    if not context.entities then
        return nil
    end
    local groups = {}
    local group_by_module = {}
    for i = 1, #context.entities do
        local entity = context.entities[i]
        local module = "default"
        if entity.metadata then
            local m = entity.metadata["module"]
            if m and m ~= "" then
                module = m
            end
        end
        local group = group_by_module[module]
        if not group then
            group = {
                group_id = "override_" .. module,
                group_type = "Module",
                name = module,
                kind = "Module",
                language = context.language,
                header = {
                    id = 0,
                    name = module,
                    kind = "Module",
                    metadata = {}
                },
                members = {},
                metadata = { override = "true" }
            }
            group_by_module[module] = group
            groups[#groups + 1] = group
        end
        group.members[#group.members + 1] = {
            id = entity.id,
            name = entity.name,
            kind = entity.kind,
            signature = entity.signature or "",
            metadata = entity.metadata or {}
        }
    end
    if #groups == 0 then
        return nil
    end
    return groups
end
