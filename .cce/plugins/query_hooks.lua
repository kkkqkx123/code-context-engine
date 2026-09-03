-- plugin/query_hooks.lua
-- Demo: QueryRewrite + Fusion + ResultFilter — query-side hooks.
--
-- - rewrite_query: maps technical abbreviations to their full forms so BM25
--   recall matches the indexed natural language (e.g. "tf" -> "tensorflow").
-- - fusion_weights: shifts weight toward BM25 for keyword-like queries and
--   toward the vector path for long queries.
-- - filter_results: removes candidates whose file path matches a noise
--   pattern and boosts results whose content mentions the query token.

plugin = {
    id = "query_hooks_plugin",
    name = "Query Hooks",
    version = "0.1.0",
    priority = 10,
    description = "Query rewriting, fusion weight override, and result filtering (query-side demo).",
    capabilities = { "query_rewrite", "fusion", "result_filter" }
}

local ALIASES = {
    ["tf"] = "tensorflow",
    ["nlp"] = "natural language processing",
    ["dl"] = "deep learning",
    ["ml"] = "machine learning",
    ["db"] = "database",
}

function plugin.rewrite_query(query)
    local rewritten = query
    local expansions = {}
    for word in query:gmatch("[%w_]+") do
        local full = ALIASES[word:lower()]
        if full and full ~= word:lower() then
            rewritten = rewritten:gsub("%f[%w]" .. word .. "%f[%W]", full, 1)
            expansions[#expansions + 1] = full
        end
    end
    if rewritten == query and #expansions == 0 then
        return nil
    end
    return { rewritten_query = rewritten, expansion_terms = expansions }
end

function plugin.fusion_weights(query, vector_count, bm25_count)
    if #query <= 3 then
        -- Short keyword-like query: favor BM25.
        return { vector_weight = 0.3, bm25_weight = 0.7 }
    end
    if #query > 40 then
        -- Long semantic query: favor the vector path.
        return { vector_weight = 0.8, bm25_weight = 0.2 }
    end
    return nil
end

local NOISE_PATTERNS = { "generated-", "vendor/", "node_modules/" }

function plugin.filter_results(query, results)
    local entries = {}
    for i = 1, #results do
        local r = results[i]
        local remove = false
        for _, p in ipairs(NOISE_PATTERNS) do
            if r.file_path and r.file_path:find(p, 1, true) then
                remove = true
                break
            end
        end
        local boost = nil
        if not remove and query and r.content and r.content:find(query, 1, true) then
            boost = 0.1
        end
        if remove or boost then
            entries[#entries + 1] = { id = r.id, remove = remove, boost = boost }
        end
    end
    if #entries == 0 then
        return nil
    end
    return entries
end
