//! Generation and manifest lifecycle.
//!
//! A generation is a versioned snapshot of a project's indexed data spread
//! across Qdrant, BM25 and SQLite. This module owns how generations are
//! started, cloned, published and garbage collected.
//!
//! Responsibility-scoped submodules:
//! - `lifecycle`: manifest start/publish/fail transitions and reads
//! - `gc`: retention-based garbage collection of stale generations
//! - `row_copy`: single-file cross-generation row copies (drift sweeps)
//! - `sqlite_compaction`: SQLite-side materialization of inherited generations
//! - `external_compaction`: Qdrant/BM25-side materialization

use super::StorageCoordinator;

pub(crate) mod external_compaction;
pub(crate) mod gc;
pub(crate) mod lifecycle;
pub(crate) mod row_copy;
pub(crate) mod sqlite_compaction;
