# Hybrid Fusion Design

## Problem Statement

During indexing, each entity group produces two independent text representations:
- **BM25 path**: keyword-rich text, split by `max_bm25_words`
- **Embedding path**: semantic NL text, split by `max_tokens`

These two paths chunk independently, producing different chunk counts and boundaries for the same logical entity. At query time, both paths are searched in parallel, and their results must be merged into a single ranked list.

The core challenge: **how to align results across paths when chunk boundaries are inherently inconsistent?**

## Design Decision: Entity-Level Alignment

The system avoids chunk-to-chunk alignment entirely. Instead, it aligns at the **entity level** using an alignment key:

| Content Type | Alignment Key | Source |
|---|---|---|
| Code (function/class/method) | `entity_id` | AST parsing → `EntityId` |
| Document (markdown/txt/log) | `segment_id` | Grouping logic → `source_group_id` |

### Why Entity-Level?

Chunk-level alignment between BM25 and Embedding paths is unsolvable in general:
- BM25 splits by word count; Embedding splits by token count
- The same entity may produce 2 chunks in BM25 but 3 in Embedding
- There is no deterministic mapping between chunk N of path A and chunk M of path B

By aligning at the entity level, the system sidesteps this problem: each path contributes its best-matching chunk for a given entity, and the fusion combines their scores.

## Entity vs Segment

These two concepts coexist but serve different roles:

- **Entity**: A code-only semantic unit (function, class, method) with a stable `EntityId`. Provides precise alignment for code content.
- **Segment**: A logical grouping unit (String-based) that owns chunks. For code, it equals the `EntityGroup.group_id`. For documents, it equals the `source_group_id`. **Always populated.**

The fusion priority is `entity_id > segment_id`:
- Code chunks have both; `entity_id` provides fine-grained matching
- Document chunks have only `segment_id`; it serves as a fallback alignment key

This dual-key design ensures consistent fusion logic across all content types without coupling to the presence of code entities.

## Fusion Algorithm

### Step 1: Per-Path Score Normalization

Each path's scores are min-max normalized independently to [0.0, 1.0]. This makes scores from different retrieval algorithms comparable within each path.

**Design rationale**: Min-max is simple and preserves rank order. The configured `vector_weight` and `bm25_weight` provide user control over each path's contribution.

**Limitation**: Single-result paths get normalized to 1.0 (no distribution to normalize against). This is intentional — the path weight fully controls the contribution, rather than imposing an arbitrary penalty. Cross-path score calibration (e.g., vector cosine vs BM25 TF-IDF) is not addressed by min-max; users should tune weights based on empirical results.

**Alternatives considered**: Z-score normalization (requires historical statistics), rank-based fusion (loses score magnitude), learned calibration (requires labeled data). Min-max chosen for simplicity; see `score-normalization-analysis.md` for detailed comparison.

### Step 2: Best-Score-Per-Key Selection

For each alignment key, keep only the best-scoring chunk from each path:

```
vector_by_key: HashMap<AlignmentKey, (index, normalized_score)>
bm25_by_key:  HashMap<AlignmentKey, (index, normalized_score)>
```

This ensures each entity/segment contributes at most one entry per path.

### Step 3: Weighted Linear Combination

For each alignment key present in both paths:
```
fused_score = vector_weight * norm(vector_score) + bm25_weight * norm(bm25_score)
```

For keys present in only one path (if `include_single_path`):
```
fused_score = path_weight * norm(path_score)
```

### Step 4: Sort and Filter

Results are sorted by fused score descending. A `min_score` threshold can filter low-confidence matches.

## Multi-Entity Expansion

A single chunk may contain multiple entities (e.g., a class with methods). Before fusion, such chunks are expanded so each entity gets its own entry with the same score. This enables entity-level alignment instead of chunk-level alignment.

This expansion happens in `Searcher::expand_multi_entity_results()` before calling `fuse_hybrid_results()`.

## Aggregated Search Deduplication

`search_aggregated()` runs multiple sub-queries and merges results. Deduplication uses the same alignment key as fusion:
1. `entity_id` (if present)
2. `segment_id` (fallback)
3. chunk `id` (last resort)

This ensures the same entity from different sub-queries or paths is correctly deduplicated.

## Post-Fusion Assembly (Optional)

When `WithAssembly` strategy is enabled, the fused results undergo additional processing:

1. **SPSR-Graph expansion**: Call chain traversal (forward/backward) from each result's entity
2. **Unit deduplication**: Remove duplicate expanded units
3. **Structure concatenation**: Assemble primary unit + call chain into a coherent code block
4. **Segment aggregation**: Merge adjacent code segments within the same file (gap ≤ `segment_merge_gap`)

The segment aggregation step reduces fragmentation by combining nearby results into contiguous code blocks, which is more useful for LLM consumption.

## Configuration

```toml
[sources.hybrid]
vector_weight = 0.5
bm25_weight = 0.5

[assembly]
enable_segment_merge = true
segment_merge_gap = 3
enable_file_coverage_threshold = true
file_coverage_threshold = 0.6
```

## Known Limitations

1. **Single-chunk-per-entity**: Only the best chunk per entity per path contributes to fusion. If an entity's content spans multiple chunks covering different aspects, only the top-scoring chunk is represented.

2. **No cross-path chunk boundary alignment**: The system intentionally avoids mapping chunk N in BM25 to chunk M in Embedding. This means fine-grained positional correspondence is lost.

3. **Segment aggregation is assembly-only**: The `SegmentAggregator` only runs in the `WithAssembly` path. Standard hybrid results may contain fragmented segments from the same file.
