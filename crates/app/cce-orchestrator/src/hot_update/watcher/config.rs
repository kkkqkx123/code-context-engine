//! Configuration types for file watching
//!
//! This module provides configuration types for the file watching system.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Watch strategy
///
/// Note: Debounce is handled by HotUpdateCoordinator's GlobalDebounce
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchStrategy {
    /// Primary mode: file watching
    FileWatch,

    /// Fallback mode: periodic scanning
    PeriodicScan {
        /// Scan interval in seconds
        interval_secs: u64,
    },

    /// Hybrid mode: file watching + periodic verification
    Hybrid {
        /// Use file watching
        use_file_watch: bool,
        /// Event storm threshold (events per second)
        event_threshold: usize,
        /// Fallback scan interval in seconds
        fallback_interval_secs: u64,
        /// Verification interval in seconds
        verification_interval_secs: u64,
    },
}

impl Default for WatchStrategy {
    fn default() -> Self {
        Self::Hybrid {
            use_file_watch: true,
            event_threshold: 100,
            fallback_interval_secs: 30,
            verification_interval_secs: 600,
        }
    }
}

/// Watch configuration
///
/// Note: Debounce is handled by HotUpdateCoordinator's GlobalDebounce,
/// not by WatchCoordinator. This config only controls file watching behavior.
///
/// Configuration parameters are now unified with ModeSwitchConfig to eliminate duplication.
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// Event storm threshold (events per second)
    /// Unified parameter: used by both WatchCoordinator and ModeStateMachine
    pub event_threshold: usize,

    /// Fallback scan interval in seconds
    /// Used when file watching is degraded (unified with degraded_scan_interval_secs)
    pub fallback_interval_secs: u64,

    /// Verification interval in seconds
    /// Periodic check that watcher is still active
    pub verification_interval_secs: u64,

    /// File extensions to watch
    pub extensions: Vec<String>,

    /// Path patterns to ignore (glob syntax)
    pub ignore_patterns: Vec<String>,

    /// Pre-compiled glob patterns for efficient matching
    compiled_globs: Vec<cce_utils::glob::Glob>,

    /// Enable config file watching
    pub watch_config_files: bool,

    /// Maximum events to buffer
    pub max_event_buffer: usize,

    // === Additional unified parameters ===
    /// Storm duration threshold (seconds) - how long storm must persist before degradation
    pub storm_duration_secs: u64,

    /// Recovery threshold (events per second) - threshold to recover to file watch mode
    pub recovery_threshold: usize,

    /// Recovery duration threshold (seconds) - how long recovery condition must persist
    pub recovery_duration_secs: u64,
}

impl Default for WatchConfig {
    fn default() -> Self {
        // Patterns use glob syntax: ** matches any number of path components,
        // * matches within a single component. Literal names without wildcards
        // are wrapped to match any occurrence in the path.
        let ignore_patterns = vec![
            "**/node_modules/**".to_string(),
            "**/target/**".to_string(),
            "**/.git/**".to_string(),
            "**/dist/**".to_string(),
            "**/build/**".to_string(),
            "**/.idea/**".to_string(),
            "**/.vscode/**".to_string(),
            "**/__pycache__/**".to_string(),
            "**/.venv/**".to_string(),
            "**/venv/**".to_string(),
        ];

        let compiled_globs = ignore_patterns
            .iter()
            .map(|p| {
                cce_utils::glob::Glob::new(p).expect(
                    "default ignore patterns are hard-coded glob literals; \
                     reaching this path means the literal is invalid (unreachable)",
                )
            })
            .collect();

        Self {
            event_threshold: 100,
            fallback_interval_secs: 30,
            verification_interval_secs: 600,
            extensions: vec![
                "rs".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "jsx".to_string(),
                "tsx".to_string(),
                "py".to_string(),
                "java".to_string(),
                "go".to_string(),
                "c".to_string(),
                "cpp".to_string(),
                "h".to_string(),
                "hpp".to_string(),
                "cs".to_string(),
                "rb".to_string(),
                "php".to_string(),
                "kt".to_string(),
                "kts".to_string(),
                "swift".to_string(),
                "vue".to_string(),
                "svelte".to_string(),
                "html".to_string(),
                "css".to_string(),
                "scss".to_string(),
                "json".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
                "toml".to_string(),
                "md".to_string(),
                // Document pipeline extensions (plain text / config / schema)
                "txt".to_string(),
                "log".to_string(),
                "ini".to_string(),
                "xml".to_string(),
                "csv".to_string(),
                "rst".to_string(),
                "adoc".to_string(),
                "proto".to_string(),
                "graphql".to_string(),
                "thrift".to_string(),
                "avsc".to_string(),
            ],
            ignore_patterns,
            compiled_globs,
            watch_config_files: true,
            max_event_buffer: 1000,
            // Unified parameters with ModeSwitchConfig defaults
            storm_duration_secs: 10,
            recovery_threshold: 50,
            recovery_duration_secs: 30,
        }
    }
}

impl WatchConfig {
    /// Create a new watch config with custom extensions
    pub fn with_extensions(extensions: Vec<String>) -> Self {
        Self {
            extensions,
            ..Default::default()
        }
    }

    /// Create a new watch config with custom ignore patterns
    ///
    /// Patterns use glob syntax (e.g., "**/node_modules/**")
    pub fn with_ignore_patterns(ignore_patterns: Vec<String>) -> Result<Self, String> {
        let compiled_globs = ignore_patterns
            .iter()
            .map(|p| cce_utils::glob::Glob::new(p))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            ignore_patterns,
            compiled_globs,
            ..Default::default()
        })
    }

    /// Create a new watch config with custom parameters and ignore patterns
    #[allow(clippy::too_many_arguments)]
    pub fn with_params(
        event_threshold: usize,
        fallback_interval_secs: u64,
        verification_interval_secs: u64,
        extensions: Vec<String>,
        ignore_patterns: Vec<String>,
        watch_config_files: bool,
        max_event_buffer: usize,
        storm_duration_secs: u64,
        recovery_threshold: usize,
        recovery_duration_secs: u64,
    ) -> Result<Self, String> {
        let compiled_globs = ignore_patterns
            .iter()
            .map(|p| cce_utils::glob::Glob::new(p))
            .collect::<Result<Vec<_>, _>>()?;

        // An empty extension list means "watch the default extensions" so
        // callers that pass `vec![]` do not silently filter out every file
        // event in `should_process`.
        let extensions = if extensions.is_empty() {
            Self::default().extensions
        } else {
            extensions
        };

        Ok(Self {
            event_threshold,
            fallback_interval_secs,
            verification_interval_secs,
            extensions,
            ignore_patterns,
            compiled_globs,
            watch_config_files,
            max_event_buffer,
            storm_duration_secs,
            recovery_threshold,
            recovery_duration_secs,
        })
    }

    /// Check if a file extension should be watched
    ///
    /// Case-insensitive: the on-disk extension is compared against the
    /// configured list ignoring ASCII case, so `main.RS` triggers the same
    /// re-index as `main.rs`.
    pub fn should_watch_extension(&self, ext: &str) -> bool {
        self.extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(ext))
    }

    /// Check if a path should be ignored using glob matching
    pub fn should_ignore_path(&self, path: &str) -> bool {
        let path_obj = Path::new(path);
        for glob in &self.compiled_globs {
            if glob.is_match(path_obj) {
                return true;
            }
        }
        false
    }

    /// Convert to ModeSwitchConfig (unified configuration)
    pub fn to_mode_switch_config(&self) -> super::super::mode_switch::ModeSwitchConfig {
        super::super::mode_switch::ModeSwitchConfig {
            storm_threshold: self.event_threshold,
            storm_duration_secs: self.storm_duration_secs,
            recovery_threshold: self.recovery_threshold,
            recovery_duration_secs: self.recovery_duration_secs,
            degraded_scan_interval_secs: self.fallback_interval_secs,
        }
    }
}

/// Watch mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WatchMode {
    /// File watching mode
    #[default]
    FileWatch,

    /// Periodic scan mode (degraded)
    PeriodicScan,
}
