-- plugin/rerank.lua
-- Demo: Rerank — query result reranking.
--
-- Receives the query string and a 1-indexed array of candidate tables:
--   { id, content, file_path, initial_score, entity_type, metadata }
-- Returns { reranked_candidates = { { id, rerank_score, initial_score,
--   final_score, rank_change, reasoning }, ... } }.
--
-- This example boosts candidates whose content mentions the query keyword,
-- then re-sorts by the boosted score.

plugin = {
    id = "rerank_plugin",
    name = "Keyword Reranker",
    version = "0.1.0",
    priority = 5,
    description = "Boosts results containing the query keyword (Rerank demo).",
    capabilities = { "rerank" }
}

function plugin.rerank(query, candidates)
    local reranked = {}
    local keyword = query:lower()
    for i = 1, #candidates do
        local c = candidates[i]
        local boost = 0.0
        local content_lower = (c.content or ""):lower()
        if keyword ~= "" and content_lower:find(keyword, 1, true) then
            boost = 0.15
        end
        local final_score = math.min(1.0, c.initial_score + boost)
        reranked[i] = {
            id = c.id,
            rerank_score = final_score,
            initial_score = c.initial_score,
            final_score = final_score,
            rank_change = 0,
            reasoning = boost > 0 and "contains query keyword" or nil
        }
    end

    -- Sort by final_score descending (stable).
    table.sort(reranked, function(a, b) return a.final_score > b.final_score end)

    -- Compute rank_change vs. the original candidate order.
    for i, r in ipairs(reranked) do
        r.rank_change = i - 1
    end

    return { reranked_candidates = reranked }
end
