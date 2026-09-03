-- plugin/group_hook.lua
-- Demo: Group — post-grouping hook.
--
-- Runs after the built-in grouper and before combined-source generation.
-- Receives the full group list plus a context table, and returns a
-- (possibly modified) group list. This example merges adjacent standalone
-- groups that share the same `topic` metadata into one composite group.

plugin = {
    id = "group_hook_plugin",
    name = "Group Post-Processor",
    version = "0.1.0",
    priority = 8,
    description = "Merges standalone groups sharing a topic metadata key (Group hook demo).",
    capabilities = { "group" }
}

function plugin.post_group(groups, context)
    -- groups: 1-indexed array of EntityGroup tables
    -- context: { file_path, language, source }
    local merged = {}
    local seen = {}
    for i = 1, #groups do
        local group = groups[i]
        local topic = nil
        if group.metadata then
            topic = group.metadata["topic"]
        end
        if topic and seen[topic] then
            -- Merge this group's members into the first group with the same topic.
            local target = merged[seen[topic]]
            for j = 1, #group.members do
                target.members[#target.members + 1] = group.members[j]
            end
            if group.header and not target.header then
                target.header = group.header
                target.header_id = group.header_id
            end
        else
            merged[#merged + 1] = group
            if topic then
                seen[topic] = #merged
            end
        end
    end
    return merged
end
