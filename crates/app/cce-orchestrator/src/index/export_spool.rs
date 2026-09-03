//! Disk spool for NL document export inputs.
//!
//! The full-index export phase runs only after the relation index is
//! finalized, so per-file chunks must survive the whole batch loop.
//! Previously every project chunk was cloned into memory
//! (`ctx.export_chunks_by_file`), making peak memory scale with project size.
//! This spool persists each file's chunks as a compressed JSON payload
//! (serde_json + zstd) and replays them one file at a time during export,
//! keeping memory bounded by a single file's chunks.
//!
//! The spool lives at `<project_root>/.cce/export-spool-{project_id}` and is
//! removed on drop; a leftover directory from a crashed run is cleaned up on
//! the next full index.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use cce_types::ChunkedResult;

use crate::error::OrchestratorError;

/// Per-file chunk payload directory spool.
pub struct ExportSpool {
    directory: PathBuf,
    /// (encoded file path, payload file path) in append order
    entries: Vec<(String, PathBuf)>,
}

impl ExportSpool {
    /// Create an empty spool under the project data directory.
    ///
    /// A leftover directory from a crashed previous run is removed first
    /// (full indexes are exclusive per project, so a fresh spool supersedes
    /// any earlier in-flight one).
    pub(crate) fn new(project_id: i64, project_root: &Path) -> Result<Self, OrchestratorError> {
        let directory = project_root
            .join(".cce")
            .join(format!("export-spool-{project_id}"));
        if directory.exists() {
            if let Err(error) = fs::remove_dir_all(&directory) {
                tracing::warn!(
                    path = %directory.display(),
                    %error,
                    "Failed to remove orphaned export spool"
                );
            }
        }
        fs::create_dir_all(&directory).map_err(|error| {
            OrchestratorError::index(
                "export_spool",
                format!("Failed to create export spool directory: {error}"),
            )
        })?;
        Ok(Self {
            directory,
            entries: Vec::new(),
        })
    }

    /// Persist one file's chunks for the final export replay.
    pub(crate) fn append(
        &mut self,
        file_path: &str,
        chunks: Vec<ChunkedResult>,
    ) -> Result<(), OrchestratorError> {
        if chunks.is_empty() {
            return Ok(());
        }
        let payload_path = self
            .directory
            .join(format!("{:06}.bin", self.entries.len()));
        let encoded = encode_chunks(&chunks).map_err(|error| {
            OrchestratorError::index(
                "export_spool",
                format!("Failed to serialize chunks for {file_path}: {error}"),
            )
        })?;
        let file = fs::File::create(&payload_path).map_err(|error| {
            OrchestratorError::index(
                "export_spool",
                format!("Failed to create spool payload for {file_path}: {error}"),
            )
        })?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&encoded).map_err(|error| {
            OrchestratorError::index(
                "export_spool",
                format!("Failed to write spool payload for {file_path}: {error}"),
            )
        })?;
        writer.flush().map_err(|error| {
            OrchestratorError::index(
                "export_spool",
                format!("Failed to flush spool payload for {file_path}: {error}"),
            )
        })?;
        self.entries.push((file_path.to_string(), payload_path));
        Ok(())
    }

    /// Load one file's chunks back for export.
    pub(crate) fn load_chunks(
        &self,
        file_path: &str,
    ) -> Result<Vec<ChunkedResult>, OrchestratorError> {
        let payload = self
            .entries
            .iter()
            .find(|(path, _)| path == file_path)
            .map(|(_, payload)| payload)
            .ok_or_else(|| {
                OrchestratorError::index(
                    "export_spool",
                    format!("No spooled chunks for {file_path}"),
                )
            })?;
        let encoded = fs::read(payload).map_err(|error| {
            OrchestratorError::index(
                "export_spool",
                format!("Failed to read spool payload for {file_path}: {error}"),
            )
        })?;
        decode_chunks(&encoded).map_err(|error| {
            OrchestratorError::index(
                "export_spool",
                format!("Failed to decode spool payload for {file_path}: {error}"),
            )
        })
    }

    /// All spooled file paths (used for stale-document cleanup).
    pub(crate) fn file_paths(&self) -> Vec<String> {
        self.entries.iter().map(|(path, _)| path.clone()).collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn cleanup(&mut self) {
        if self.directory.exists()
            && let Err(error) = fs::remove_dir_all(&self.directory)
        {
            tracing::warn!(
                path = %self.directory.display(),
                %error,
                "Failed to remove export spool directory"
            );
        }
    }
}

impl Drop for ExportSpool {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn encode_chunks(chunks: &[ChunkedResult]) -> Result<Vec<u8>, OrchestratorError> {
    let json = serde_json::to_vec(chunks).map_err(|error| {
        OrchestratorError::index("export_spool", format!("JSON encoding failed: {error}"))
    })?;
    zstd::encode_all(&*json, 3).map_err(|error| {
        OrchestratorError::index("export_spool", format!("Zstd encoding failed: {error}"))
    })
}

fn decode_chunks(encoded: &[u8]) -> Result<Vec<ChunkedResult>, OrchestratorError> {
    let json = zstd::decode_all(encoded).map_err(|error| {
        OrchestratorError::index("export_spool", format!("Zstd decoding failed: {error}"))
    })?;
    serde_json::from_slice(&json).map_err(|error| {
        OrchestratorError::index("export_spool", format!("JSON decoding failed: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_chunks() -> Vec<ChunkedResult> {
        vec![
            ChunkedResult {
                chunk_id: "g_0_src/main.rs_0".to_string(),
                source_group_id: "g_0".to_string(),
                path: cce_types::ChunkPath::Embedding,
                group_type: cce_types::grouper::GroupType::Standalone,
                chunk_index: 0,
                total_chunks: 1,
                text: "fn alpha does something useful".to_string(),
                bm25_title: None,
                bm25_keywords: Vec::new(),
                token_count: 8,
                start_byte: 0,
                end_byte: 24,
                prev_overlap: None,
                next_overlap: None,
                related_groups: Vec::new(),
                self_contained: true,
                metadata: Default::default(),
            },
            ChunkedResult {
                chunk_id: "g_1_src/main.rs_0".to_string(),
                source_group_id: "g_1".to_string(),
                path: cce_types::ChunkPath::Embedding,
                group_type: cce_types::grouper::GroupType::Standalone,
                chunk_index: 0,
                total_chunks: 1,
                text: "fn beta computes values".to_string(),
                bm25_title: None,
                bm25_keywords: Vec::new(),
                token_count: 6,
                start_byte: 30,
                end_byte: 50,
                prev_overlap: None,
                next_overlap: None,
                related_groups: Vec::new(),
                self_contained: false,
                metadata: Default::default(),
            },
        ]
    }

    #[test]
    fn chunks_round_trip_through_spool() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut spool = ExportSpool::new(1, dir.path()).expect("spool creation");
        let chunks = sample_chunks();
        spool.append("src/main.rs", chunks.clone()).expect("append");
        assert_eq!(spool.file_paths(), vec!["src/main.rs".to_string()]);
        let restored = spool.load_chunks("src/main.rs").expect("load");
        assert_eq!(restored.len(), chunks.len());
        for (a, b) in restored.iter().zip(chunks.iter()) {
            assert_eq!(a.chunk_id, b.chunk_id);
            assert_eq!(a.text, b.text);
            assert_eq!(a.metadata.file_path, b.metadata.file_path);
        }
    }

    #[test]
    fn spool_new_removes_orphaned_directory_from_crashed_run() {
        let dir = tempfile::tempdir().expect("tempdir");
        let spool_dir = dir.path().join(".cce").join("export-spool-7");
        fs::create_dir_all(&spool_dir).expect("create leftover dir");
        fs::write(spool_dir.join("000000.bin"), b"leftover").expect("write leftover");
        let _spool = ExportSpool::new(7, dir.path()).expect("spool creation");
        let entries: Vec<_> = fs::read_dir(&spool_dir)
            .expect("spool dir recreated")
            .collect();
        assert!(
            entries.is_empty(),
            "orphaned spool content must be removed before reuse"
        );
    }

    #[test]
    fn spool_drop_cleans_up_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let spool_dir = dir.path().join(".cce").join("export-spool-9");
        {
            let mut spool = ExportSpool::new(9, dir.path()).expect("spool creation");
            spool
                .append("src/main.rs", sample_chunks())
                .expect("append");
            assert!(spool_dir.exists());
        }
        assert!(
            !spool_dir.exists(),
            "spool directory must be removed on drop"
        );
    }
}
