//! Lightweight file supply gateway for remote hosting.
//!
//! The gateway runs where the source files live. It enumerates locally,
//! pushes manifests and contents to the remote ingest entries, and polls
//! for incremental changes. It links only the scanner and shared contract:
//! no vector store, embedder, parser, or orchestrator is involved, and the
//! remote side never pulls from this machine.
//!
//! Polling reuses the previous round's hashes for unchanged files as a pure
//! read optimization, with a periodic forced full scan bounding the mtime
//! staleness window; change detection always compares fingerprints.
//! `SyncParams` carries every business default, so the CLI branch and the
//! daemon only translate their explicit arguments over it.
//!
//! The same core backs the one-shot CLI branch and the standalone resident
//! daemon; both share this crate so behavior cannot drift.
//!
//! The implementation is split by responsibility across the modules below:
//! transport, parameters, scanning, full sync, health, and the watch loop.
//! This module re-exports the public surface so callers keep one import path.

mod client;
mod health;
mod params;
mod scan;
mod sync;
mod watch;

pub use client::GatewayClient;
pub use health::{GatewayHealth, write_health_file};
pub use params::{SyncParams, adaptive_enabled, compression_enabled};
pub use scan::{
    BaselineFingerprint, ScanOutcome, ScanSnapshot, ScannedFile, fingerprints_match,
    load_cached_entries, manifest_version_for_snapshot, save_cached_entries, scan_local,
};
pub use sync::{chunk_hash, maybe_compress, sync_once, upload_paths};
pub use watch::watch_loop;
