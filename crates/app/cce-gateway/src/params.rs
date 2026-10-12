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
    /// Scan cache file for cross-restart hash reuse, if any.
    ///
    /// Disabled by default; when set, successful scans persist their entries
    /// so the next start can skip re-hashing unchanged files. The cache is a
    /// pure read optimization and never decides change detection.
    pub cache_file: Option<PathBuf>,
    /// Whether the watch interval adapts to the baseline file count.
    ///
    /// Disabled by default; when enabled, medium and large trees wait longer
    /// between polls. The default picks up `CCE_GATEWAY_ADAPTIVE_INTERVAL`.
    pub adaptive_interval: bool,
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
            cache_file: None,
            adaptive_interval: adaptive_enabled(),
            json_progress: false,
        }
    }

    /// Effective poll interval for the given baseline size.
    ///
    /// Small trees keep the configured interval; medium and large trees wait
    /// longer when adaptation is enabled. The configured lower bound always
    /// applies first, so an explicit interval stays a floor rather than a
    /// ceiling.
    pub fn effective_interval(&self, file_count: usize) -> u64 {
        Self::adaptive_interval_for(
            file_count,
            self.interval_secs.max(self.min_interval_secs),
            self.adaptive_interval,
        )
    }

    /// Resolve one poll interval from a file count and the adaptation flag.
    pub fn adaptive_interval_for(file_count: usize, base_secs: u64, adaptive: bool) -> u64 {
        if !adaptive {
            return base_secs;
        }
        if file_count < 1000 {
            base_secs
        } else if file_count < 10_000 {
            base_secs.max(10)
        } else {
            base_secs.max(30)
        }
    }

    /// Apply explicit caller overrides over the shared defaults.
    ///
    /// Every entry point translates its arguments into one override bundle
    /// and funnels through here, so the override semantics cannot drift
    /// between the CLI branch and the standalone daemon. Explicit values
    /// outrank the environment-backed defaults; anything left unset keeps
    /// the shared value.
    pub fn apply_overrides(&mut self, overrides: SyncParamOverrides) {
        if let Some(extensions) = overrides.extensions {
            self.extensions = extensions;
        }
        if let Some(exclude) = overrides.exclude {
            self.exclude = exclude;
        }
        if let Some(gitignore) = overrides.gitignore {
            self.gitignore = gitignore;
        }
        if let Some(commit) = overrides.commit {
            self.commit = commit;
        }
        if let Some(compress) = overrides.compress {
            self.compress = compress;
        }
        if let Some(interval_secs) = overrides.interval_secs {
            self.interval_secs = interval_secs;
        }
        if let Some(cache_file) = overrides.cache_file {
            self.cache_file = Some(cache_file);
        }
        if let Some(adaptive_interval) = overrides.adaptive_interval {
            self.adaptive_interval = adaptive_interval;
        }
        self.health_file = overrides.health_file;
        self.json_progress = overrides.json_progress;
    }
}

/// Explicit caller overrides translated over the shared defaults.
///
/// Each field mirrors one `SyncParams` knob; `None` keeps the shared value.
/// Entry points build one bundle from their own argument shapes instead of
/// reimplementing the override logic.
#[derive(Debug, Clone, Default)]
pub struct SyncParamOverrides {
    /// File extensions to include, comma separated.
    pub extensions: Option<String>,
    /// Directories to exclude, comma separated.
    pub exclude: Option<String>,
    /// Whether to respect gitignore files.
    pub gitignore: Option<bool>,
    /// Whether to run the index commit after staging.
    pub commit: Option<bool>,
    /// Whether to compress batch chunks before upload.
    pub compress: Option<bool>,
    /// Poll interval in seconds for watch mode.
    pub interval_secs: Option<u64>,
    /// Scan cache file for cross-restart hash reuse.
    pub cache_file: Option<PathBuf>,
    /// Whether the watch interval adapts to the baseline file count.
    pub adaptive_interval: Option<bool>,
    /// Heartbeat file for supervisors and health probes.
    pub health_file: Option<PathBuf>,
    /// Emit single-line JSON progress records instead of human text.
    pub json_progress: bool,
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

/// Whether watch interval adaptation is enabled. Opt-in, off by default.
pub fn adaptive_enabled() -> bool {
    std::env::var("CCE_GATEWAY_ADAPTIVE_INTERVAL")
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
        assert!(params.cache_file.is_none());
        assert_eq!(params.adaptive_interval, adaptive_enabled());
    }

    #[test]
    fn adaptive_interval_keeps_small_trees_and_relaxes_large_ones() {
        assert_eq!(SyncParams::adaptive_interval_for(10, 5, false), 5);
        assert_eq!(SyncParams::adaptive_interval_for(50_000, 5, false), 5);
        assert_eq!(SyncParams::adaptive_interval_for(10, 5, true), 5);
        assert_eq!(SyncParams::adaptive_interval_for(5_000, 5, true), 10);
        assert_eq!(SyncParams::adaptive_interval_for(50_000, 5, true), 30);
        assert_eq!(SyncParams::adaptive_interval_for(50_000, 60, true), 60);
    }

    #[test]
    fn overrides_replace_defaults_and_none_keeps_shared_values() {
        let mut params = SyncParams::with_defaults(3, "/srv/repo");
        params.apply_overrides(SyncParamOverrides {
            extensions: Some("rs,py".to_string()),
            exclude: None,
            gitignore: Some(false),
            commit: Some(false),
            compress: Some(true),
            interval_secs: Some(11),
            cache_file: None,
            adaptive_interval: Some(true),
            health_file: Some(std::path::PathBuf::from("/tmp/h.json")),
            json_progress: true,
        });
        assert_eq!(params.extensions, "rs,py");
        assert_eq!(params.exclude, "node_modules,target,.git,vendor");
        assert!(!params.gitignore);
        assert!(!params.commit);
        assert!(params.compress);
        assert_eq!(params.interval_secs, 11);
        assert!(params.cache_file.is_none());
        assert!(params.adaptive_interval);
        assert_eq!(
            params.health_file,
            Some(std::path::PathBuf::from("/tmp/h.json"))
        );
        assert!(params.json_progress);
    }

    #[test]
    fn effective_interval_applies_floor_before_adaptation() {
        let mut params = SyncParams::with_defaults(3, "/srv/repo");
        params.interval_secs = 2;
        params.min_interval_secs = 7;
        params.adaptive_interval = false;
        assert_eq!(params.effective_interval(50_000), 7);
        params.adaptive_interval = true;
        assert_eq!(params.effective_interval(50_000), 30);
    }
}
