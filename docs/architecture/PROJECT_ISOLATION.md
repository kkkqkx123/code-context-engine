# Project-Based Multi-Tenant Isolation

## Overview

This document describes the multi-tenant project isolation architecture in CCE (Code Context Engine). The system ensures complete data separation between different projects through multiple layers of isolation mechanisms.

**Key Principle**: Project isolation is enforced through deterministic checks at multiple layers—not assumed or deferred. Failure to isolate is treated as a critical security issue.

## Architecture Layers

### 1. Code Layer (Assertion & Type Safety)

All components that create project-scoped operations enforce project_id validation at construction time:

```rust
// ✓ All entry points validate project_id > 0
QueryOptions::new(query, project_id)  // Panics if project_id <= 0
StorageCoordinator::new(project_id)   // Panics if project_id <= 0
CheckpointManager::new_for_project(project_id)  // Panics if project_id <= 0
```

**Protection**: The type system and assertions prevent accidental creation of unscoped operations.

### 2. Storage Layer (Multi-Backend Isolation)

Each storage backend enforces project_id isolation independently:

#### Qdrant Vector Database
- **Mechanism**: Project isolation through payload fields
- **Field**: `project_group_id` (UUID) + `group_id` in Point Payload
- **Point ID Format**: `"{project_group_id}::{chunk_id}"` — naturally scopes data
- **Query Filtering**: Qdrant's payload filters ensure only matching documents are retrieved

```rust
// Isolation happens at storage time
let point_id = format!("{}::{}", group_id, chunk_id);  // group_id = project_group_id
let payload = Payload::new(file_path)
    .with_group_id(group_id);  // Project identifier in metadata
```

#### SQLite Metadata Database
- **Mechanism**: SQL WHERE clause filtering
- **Field**: `project_id` (i64) column in all relevant tables
- **Query Pattern**: `WHERE project_id = ? AND <other_conditions>`
- **Coverage**: All data retrieval operations include project_id filter

```sql
-- SQLite isolation examples
SELECT * FROM chunks 
WHERE chunk_id IN (...) AND project_id = ?;

DELETE FROM files 
WHERE file_id = ? AND project_id = ?;
```

#### BM25 Full-Text Index (Native Isolation)
- **Mechanism**: BM25 documents store `project_id` field
- **Query Pattern**: Native Tantivy query with project_id term filter
- **Advantage**: Eliminates external SQLite dependency for project scoping
- **Performance**: O(1) filter compared to O(n) SQLite validation

```rust
// Native project_id filtering in Tantivy
if let Some(project_id) = options.project_id {
    let project_term = Term::from_field_text(schema.project_id, &project_id.to_string());
    let project_filter = TermQuery::new(project_term, ...);
    
    // Combine with main query
    query = BooleanQuery::new(vec![
        (Occur::Must, main_query),
        (Occur::Must, project_filter),  // Enforce project scope
    ]);
}
```

### 3. Query Layer (Request-Level Scoping)

All query operations must specify a project_id:

```rust
// ✓ Forced project scoping
let options = QueryOptions::new("search_term", project_id);
let builder = QueryConfigBuilder::new(project_id);
```

**Safety Fail Behavior**: If isolation mechanisms are unavailable, queries are rejected:

```rust
// BM25 strategy requires project_id field or rejects query
if self.sqlite.is_none() && options.project_id.is_some() {
    return Err(QueryError::Config(
        "BM25 requires SQLite for project isolation"
    ));
}
```

### 4. Operational Layer (Version Control)

File update operations use version control to prevent concurrent conflicts:

```rust
pub struct FileUpdateState {
    pub project_id: i64,      // Scoped to project
    pub version: u64,         // Prevents old updates from overwriting new ones
    pub module_states: HashMap<ModuleType, ModuleUpdateRecord>,
    // ...
}
```

**Protection**: Version checks prevent concurrent modification conflicts at the project level.

## Data Flow: Index Operation

```
User: index_files(project_id=123)
    ↓
IndexOrchestrator::new(123)
    ├─ assert!(123 > 0) ✓
    └─ UpdateStateTracker::new(123)
    └─ StorageCoordinator::new(123)
        └─ .with_project_group_id("uuid-for-123")
    ↓
FileUpdateState::for_full_index(..., project_id=123, ...)
    └─ All files tagged with project_id=123
    ↓
StorageCoordinator::store_vectors_batched()
    ├─ Qdrant: point_id = "uuid-for-123::chunk-1"
    │          payload.group_id = "uuid-for-123"
    ├─ SQLite: INSERT chunks(..., project_id=123, ...)
    └─ BM25:   INSERT documents(..., project_id="123", ...)
    ↓
✓ Data stored with project_id=123 across all backends
```

## Data Flow: Query Operation

```
User: search(query="find X", project_id=123)
    ↓
QueryOptions::new(query, 123)
    └─ assert!(123 > 0) ✓
    ↓
Searcher::search(QueryOptions{project_id=123, ...})
    ├─ DenseRetrieval::retrieve(123)
    │   └─ Qdrant: filter(payload.group_id == "uuid-for-123")
    │
    ├─ Bm25Retrieval::retrieve(123)
    │   └─ Tantivy: term_filter(project_id = "123")
    │              (native indexing, no SQLite roundtrip)
    │
    └─ RelationRetrieval::retrieve(123)
        └─ SQLite: SELECT relations WHERE project_id=123 AND ...
    ↓
Result Fusion (only project_id=123 data)
    ↓
✓ User receives isolated results
```

## Isolation Guarantees

### 1. Data Separation
- ✓ Files indexed for project A never appear in project B queries
- ✓ Vector embeddings are scoped via Qdrant payload fields
- ✓ BM25 index filtering prevents keyword leakage
- ✓ SQLite queries always include WHERE project_id = ?

### 2. Concurrent Safety
- ✓ FileUpdateState.version prevents concurrent modification conflicts
- ✓ CheckpointManager isolates recovery by project_id
- ✓ ModuleRetryManager aggregates failures per project

### 3. Explicit Failure on Missing Isolation
- ✓ Queries fail rather than return unfiltered results
- ✓ Operations panic if project_id validation fails
- ✓ SQLite connection failures return empty results (safe fail)

## Key Design Decisions

### Decision 1: Native BM25 Filtering
**Choice**: Store and filter project_id in BM25 index

**Rationale**:
- Reduces SQLite queries from O(n) to O(0) for project scoping
- BM25 filter is part of query execution (not post-processing)
- Improves performance by eliminating external lookups
- Simpler architecture: filter at query parse time

**Tradeoff**: Requires BM25 schema to include project_id field (already implemented)

### Decision 2: Multiple Isolation Layers
**Choice**: Enforce project_id at code, storage, and query layers

**Rationale**:
- **Defense in depth**: Even if one layer fails, others protect data
- **Type safety**: Rust compile-time checks catch many isolation bugs
- **Operational safety**: Queries fail fast rather than leak data

### Decision 3: Safe Failure on Missing Isolation
**Choice**: Reject queries if isolation mechanisms are unavailable

**Rationale**:
- Data integrity > feature availability
- Security-critical failures should not be silent
- Encourages deployment to catch issues early

## Testing Isolation

### Unit Tests
- Project_id validation in constructors
- Version conflict detection
- Module retry aggregation per project

### Integration Tests
- Verify BM25 filtering prevents cross-project results
- Verify vector queries return only scoped data
- Verify relation queries respect project_id
- Verify version control prevents concurrent conflicts

**Example Test**:
```rust
#[tokio::test]
async fn test_bm25_cross_project_isolation() {
    // Index content in project 1 and 2
    index_project_1().await;
    index_project_2().await;
    
    // Query project 1
    let results = search(project_id=1, query="test").await?;
    
    // Assert: only project 1 results returned
    assert!(results.iter().all(|r| r.project_id == 1));
}
```

## Audit & Monitoring

### Logging
- All queries log `project_id` for audit trails
- Isolation filter failures logged as errors
- Cross-project anomalies should trigger alerts

### Metrics
- project_id filter execution time
- Isolation filter rejection rate
- Per-project query latency

### Security Checklist
- [ ] All entry points validate project_id > 0
- [ ] BM25 queries include project_id filter
- [ ] SQLite queries include WHERE project_id = ?
- [ ] Version conflicts are detected and logged
- [ ] Isolation test coverage >= 80%

## Future Improvements

1. **Encryption at Rest**: Encrypt project data using project_id as key component
2. **Audit Logging**: Detailed per-project operation logging
3. **Rate Limiting**: Per-project query rate limits
4. **Quota Management**: Per-project storage/query quotas
5. **Cross-Project Visibility**: Optional explicit sharing with audit trail
