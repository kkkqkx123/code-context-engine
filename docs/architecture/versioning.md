# Versioning Architecture

## Overview

The system uses two independent versioning dimensions to manage data lifecycle:

- **Epoch**: Full re-index version for query isolation
- **Batch ID**: Hot update batch tracking for write-path synchronization

## Epoch

### Purpose

Epoch represents a consistent snapshot of the entire codebase index. It ensures queries only return data from a specific full-indexing cycle.

### Lifecycle

```
Full Re-Index:
  1. Increment epoch in project_meta (e.g., 5 → 6)
  2. Re-index all files with new epoch
  3. Update Qdrant/BM25 payloads with new epoch
  4. Switch active_epoch to new epoch
```

### Storage

| Layer | Field | Usage |
|-------|-------|-------|
| SQLite | `project_meta.value` where `key = 'epoch'` | Project-level version |
| SQLite | `project_meta.value` where `key = 'active_epoch'` | Currently active epoch for queries |
| SQLite | `chunks.epoch`, `entities.epoch` | Per-record epoch |
| Qdrant | `Payload.epoch` | Vector payload filter field |
| BM25 | `IndexSchema.epoch` (STRING field) | Tantivy index filter field |

### Query Filtering

All queries must filter by `epoch = active_epoch`:

```rust
// QueryFilter generates epoch constraints for all backends
let filter = QueryFilter::new(active_epoch);

// SQLite: WHERE epoch = ?
filter.sqlite_where();

// Qdrant: { "must": [{ "key": "epoch", "match": { "value": 5 } }] }
filter.to_search_filter();

// BM25: TermQuery(epoch_field, "5")
// Applied in Bm25SearchOptions.epoch
```

### Design Rationale

- Epoch provides strong version isolation: queries never see data from other epochs
- Switching epoch is atomic: all backends switch simultaneously
- Old epochs can be cleaned up or archived independently

## Batch ID

### Purpose

Batch ID tracks which files were processed during hot updates. It enables the system to detect files that need external storage synchronization after a hot update cycle.

### Lifecycle

```
Hot Update:
  1. Detect changed files (content hash mismatch)
  2. Increment batch_id in project_meta (e.g., 10 → 11)
  3. Re-index changed files with new batch_id
  4. Update Qdrant/BM25 payloads with new batch_id
  5. Unchanged files retain their old batch_id
```

### Storage

| Layer | Field | Usage |
|-------|-------|-------|
| SQLite | `project_meta.value` where `key = 'batch_id'` | Project-level batch counter |
| SQLite | `files.batch_id` | Per-file batch tracking |
| SQLite | `chunks.batch_id`, `entities.batch_id` | Per-record batch |
| Qdrant | `Payload.batch_id` | Vector payload metadata |
| BM25 | `IndexSchema.batch_id` (STRING field) | Tantivy index metadata |

### Usage in Recovery

During startup recovery, batch_id is used for logging and tracking only. The actual file state classification relies on content hash comparison:

```rust
// recovery.rs - File classification logic
let state = match compute_file_hash(&path) {
    Ok(disk_hash) => match &stored_hash {
        Some(stored) if disk_hash != *stored => FileState::Modified,  // Needs re-indexing
        _ => FileState::Consistent,  // Content unchanged
    },
    Err(_) => FileState::Consistent,  // Cannot read, treat as consistent
};
```

Files marked as `FileState::Modified` are re-indexed. The `batch_id` field in `FileClassification` is preserved for diagnostic purposes but does not influence the classification decision.

### Design Rationale

- batch_id is purely a write-path tracking mechanism
- It does NOT participate in query filtering (unchanged files with older batch_id still have valid data)
- Content hash is the source of truth for detecting file changes
- batch_id provides an audit trail for which files were processed in which batch

## Relationship Between Epoch and Batch ID

```
                          Epoch (Full Re-Index)
  ┌─────────────────────────────────────────────────────────┐
  │  Epoch 5                                                │
  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐      │
  │  │ Batch 8 │ │ Batch 9 │ │Batch 10 │ │Batch 11 │      │
  │  │ (init)  │ │ (hot)   │ │ (hot)   │ │ (hot)   │      │
  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘      │
  │                                                         │
  │  Query: epoch = 5 → returns all data regardless of      │
  │         batch_id (unchanged files are still valid)      │
  └─────────────────────────────────────────────────────────┘
```

### Key Principles

1. **Epoch is for queries**: It defines which data version is visible to users
2. **Batch ID is for writes**: It tracks which files were processed when
3. **They are independent**: A hot update increments batch_id but keeps epoch the same
4. **Unchanged files are valid**: Files not touched by a hot update retain their old batch_id but their data is still correct for the current epoch

## Migration and Cleanup

### Epoch Switch

When a full re-index completes:
1. New data is written with `new_epoch`
2. `active_epoch` is atomically updated to `new_epoch`
3. Old epoch data can be cleaned up from Qdrant/BM25

### Batch ID Reset

Batch ID does not need resetting. It monotonically increases and serves as an audit log. If desired, it can to 0 during a full re-index (since all files get re-indexed anyway).
