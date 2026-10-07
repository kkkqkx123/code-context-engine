//! Shared sync parameters and the business defaults every entry point starts from.

use std::path::PathBuf;

/// Shared sync parameters for one-shot and watch passes.
///
/// This structure is the single source of business defaults: the CLI branch
/// and the standalone daemon only translate their explicit arguments over
/// `with_defaults`, so the three entry points cannot drift apart. Explicit
/// command-line values outrank the environment, which outranks these
/// defaults.
#[derive(Debug, Clone)]
pub struct SyncParams {
    /// Remote project id.
    pub project_id: i64,
    /// Local directory to supply.
    pub path: String,
    /// File extensions to include, comma separated, empty means all text.
    pub extensions: String,
    /// Directories to exclude, comma separated.
    pub exclude: String,
    /// Whether to respect gitignore files.
    pub gitignore: bool,
    /// Whether to run the index commit after staging.
    pub commit: bool,
    /// Whether to compress batch chunks before upload, off by default.
    ///
    /// Compression only applies to the batch path; incremental events always
    /// carry raw bytes so the event entry stays a single whole-file shape.
    /// The default picks up `CCE_GATEWAY_COMPRESS`.
    pub compress: bool,
    /// Poll interval in seconds for watch mode.
    pub interval_secs: u64,
    /// Lower bound applied to the poll interval.
    pub min_interval_secs: u64,
    /// Change count above which a watch pass falls back to a full sync.
    pub full_sync_threshold: usize,
    /// Heartbeat file for supervisors and health probes, if any.
    pub health_file: Option<PathBuf>,
    /// Emit single-line JSON progress records instead of human text.
    pub json_progress: bool,
}

impl SyncParams {
    /// Parameters for one project with the shared business defaults.
    ///
    /// Every entry point starts here and overrides only what its caller
    /// stated explicitly.
    pub fn with_defaults(project_id: i64, path: impl Into<String>) -> Self {
        Self {
            project_id,
            path: path.into(),
            extensions: String::new(),
            exclude: "node_modules,target,.git,vendor".to_string(),
            gitignore: true,
            commit: true,
            compress: compression_enabled(),
            interval_secs: 5,
            min_interval_secs: 1,
            full_sync_threshold: 100,
            health_file: None,
            json_progress: false,
        }
    }
}

/// Whether chunk compression is enabled. Optional negotiation, off by
/// default; the remote decompresses before hash verification.
pub fn compression_enabled() -> bool {
    std::env::var("CCE_GATEWAY_COMPRESS")
        .map(|v| {
            let v = v.trim().to_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_shared_contract_of_every_entry_point() {
        let params = SyncParams::with_defaults(3, "/srv/repo");
        assert_eq!(params.project_id, 3);
        assert_eq!(params.path, "/srv/repo");
        assert_eq!(params.extensions, "");
        assert_eq!(params.exclude, "node_modules,target,.git,vendor");
        assert!(params.gitignore);
        assert!(params.commit);
        assert_eq!(params.compress, compression_enabled());
        assert_eq!(params.interval_secs, 5);
        assert_eq!(params.min_interval_secs, 1);
        assert_eq!(params.full_sync_threshold, 100);
        assert!(params.health_file.is_none());
    }
}
