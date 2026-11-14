//! Logging implementation: initialization, rotation, retention, reload.

use std::path::Path;
use std::sync::Mutex;
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, reload, util::SubscriberInitExt};

use cce_config::global::LoggingConfig;

use crate::capture;
use cce_types::error::ConfigError;

pub mod config;

pub use config::{get_format_string, get_output_string, parse_level, validate_config};

pub type Result<T> = std::result::Result<T, ConfigError>;

/// Global guard to keep the log appenders alive
pub static LOG_GUARD: OnceLock<Vec<WorkerGuard>> = OnceLock::new();

/// Reload handle for runtime level updates
static RELOAD_HANDLE: OnceLock<reload::Handle<EnvFilter, tracing_subscriber::Registry>> =
    OnceLock::new();

/// Initialize the logging system
pub fn init(config: &LoggingConfig) -> Result<()> {
    validate_config(config)?;

    let filter_str = if let Some(f) = &config.filter {
        f.clone()
    } else {
        config.level.to_string()
    };

    let filter = EnvFilter::new(filter_str.clone());

    let (filter_layer, reload_handle) = reload::Layer::new(filter);

    // Initialize log capture for the management API.
    capture::init(config);

    let format = get_format_string(config);
    let output = get_output_string(config);

    match (output, format) {
        ("file", fmt_str) => {
            init_file(config, filter_layer, reload_handle.clone(), fmt_str)?;
        }
        ("stderr", fmt_str) => {
            init_stderr(filter_layer, reload_handle.clone(), fmt_str)?;
        }
        (_, fmt_str) => {
            init_stdout(filter_layer, reload_handle.clone(), fmt_str)?;
        }
    }

    let _ = RELOAD_HANDLE.set(reload_handle);

    Ok(())
}

/// Update the log filter at runtime.
pub fn reload_level(filter: &str) -> Result<()> {
    if let Some(handle) = RELOAD_HANDLE.get() {
        handle
            .reload(EnvFilter::new(filter))
            .map_err(|e| ConfigError::Other(format!("reload failed: {e}")))?;
        Ok(())
    } else {
        Err(ConfigError::Other("logger not initialized".to_string()))
    }
}

/// Cleanup old log files beyond retention limits. Called automatically on init.
pub fn cleanup_logs(config: &LoggingConfig) {
    let file_path = match &config.file {
        Some(p) => std::path::PathBuf::from(p),
        None => return,
    };
    let dir = file_path.parent().unwrap_or(std::path::Path::new("."));
    let prefix = file_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    rotate_retention::cleanup(dir, &prefix, config);
}

mod rotate_retention {
    use super::*;
    use std::fs;
    use std::path::Path;

    pub fn cleanup(dir: &Path, prefix: &str, config: &LoggingConfig) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = match path.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };
            if !name.starts_with(prefix) {
                continue;
            }
            let meta = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            files.push((path, meta.len(), mtime));
        }

        // sort oldest first
        files.sort_by_key(|f| f.2);

        // max_files
        if let Some(max) = config.max_files {
            if max > 0 && files.len() > max {
                let to_remove = files.len() - max;
                for (path, _, _) in files.iter().take(to_remove) {
                    let _ = fs::remove_file(path);
                }
                // rebuild list
                files = files.into_iter().skip(to_remove).collect();
            }
        }

        // max_total_size_mb
        if let Some(max_mb) = config.max_total_size_mb {
            let max_bytes = max_mb * 1024 * 1024;
            let mut total: u64 = files.iter().map(|f| f.1).sum();
            let mut idx = 0;
            while total > max_bytes && idx < files.len() {
                let _ = fs::remove_file(&files[idx].0);
                total -= files[idx].1;
                idx += 1;
            }
        }
    }
}

fn init_file(
    config: &LoggingConfig,
    filter_layer: reload::Layer<EnvFilter, tracing_subscriber::Registry>,
    _reload_handle: reload::Handle<EnvFilter, tracing_subscriber::Registry>,
    format: &str,
) -> Result<()> {
    let file_path = config
        .file
        .as_ref()
        .map(Path::new)
        .ok_or_else(|| ConfigError::Other("Log file path not specified".to_string()))?;

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ConfigError::Other(format!("Failed to create log directory: {}", e)))?;
    }

    cleanup_logs(config);

    // Build rotating file writer honoring size + retention.
    let dir = file_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let prefix = file_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let writer = RotatingFile::new(
        dir,
        prefix,
        config.max_file_size_mb.map(|m| m * 1024 * 1024),
        config.max_files,
        config.max_total_size_mb.map(|m| m * 1024 * 1024),
    );

    let (non_blocking, guard) = tracing_appender::non_blocking(writer);
    let _ = LOG_GUARD.set(vec![guard]);

    let main_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = match format {
        "json" => Box::new(
            fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_target(true)
                .with_level(true),
        ),
        "compact" => Box::new(
            fmt::layer()
                .compact()
                .with_writer(non_blocking)
                .with_target(true)
                .with_level(true),
        ),
        _ => Box::new(
            fmt::layer()
                .with_writer(non_blocking)
                .with_target(true)
                .with_level(true),
        ),
    };

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(crate::capture::CaptureLayer)
        .with(main_layer)
        .try_init()
        .ok();

    Ok(())
}

fn init_stderr(
    filter_layer: reload::Layer<EnvFilter, tracing_subscriber::Registry>,
    _reload_handle: reload::Handle<EnvFilter, tracing_subscriber::Registry>,
    format: &str,
) -> Result<()> {
    let main_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = match format {
        "json" => Box::new(
            fmt::layer()
                .json()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_level(true),
        ),
        "compact" => Box::new(
            fmt::layer()
                .compact()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_level(true),
        ),
        _ => Box::new(
            fmt::layer()
                .pretty()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_level(true),
        ),
    };

    let dummy_guard = create_dummy_guard();
    let _ = LOG_GUARD.set(vec![dummy_guard]);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(crate::capture::CaptureLayer)
        .with(main_layer)
        .try_init()
        .ok();

    Ok(())
}

fn init_stdout(
    filter_layer: reload::Layer<EnvFilter, tracing_subscriber::Registry>,
    _reload_handle: reload::Handle<EnvFilter, tracing_subscriber::Registry>,
    format: &str,
) -> Result<()> {
    let main_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = match format {
        "json" => Box::new(fmt::layer().json().with_target(false).with_level(true)),
        "compact" => Box::new(fmt::layer().compact().with_target(false).with_level(true)),
        _ => Box::new(fmt::layer().pretty().with_target(false).with_level(true)),
    };

    let dummy_guard = create_dummy_guard();
    let _ = LOG_GUARD.set(vec![dummy_guard]);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(crate::capture::CaptureLayer)
        .with(main_layer)
        .try_init()
        .ok();

    Ok(())
}

fn create_dummy_guard() -> WorkerGuard {
    use tracing_appender::rolling;
    let appender = rolling::never(".", ".dummy");
    let (_, guard) = tracing_appender::non_blocking(appender);
    guard
}

// ---- Size-rotating file writer ----

struct RotatingFile {
    inner: Mutex<RotatingInner>,
    dir: std::path::PathBuf,
    prefix: String,
    max_bytes: Option<u64>,
    max_files: Option<usize>,
    max_total_bytes: Option<u64>,
}

struct RotatingInner {
    path: std::path::PathBuf,
    file: Option<std::fs::File>,
    size: u64,
    date: String,
}

impl RotatingFile {
    fn new(
        dir: std::path::PathBuf,
        prefix: String,
        max_bytes: Option<u64>,
        max_files: Option<usize>,
        max_total_bytes: Option<u64>,
    ) -> Self {
        Self {
            inner: Mutex::new(RotatingInner {
                path: std::path::PathBuf::new(),
                file: None,
                size: 0,
                date: String::new(),
            }),
            dir,
            prefix,
            max_bytes,
            max_files,
            max_total_bytes,
        }
    }

    fn today() -> String {
        // UTC date
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Convert to YYYY-MM-DD (UTC) without chrono dependency? use chrono
        chrono::DateTime::<chrono::Utc>::from_timestamp(now as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn ensure_ready(&self, inner: &mut RotatingInner) -> std::io::Result<()> {
        let today = Self::today();
        if inner.file.is_none() || inner.date != today {
            // switch to new date file
            if inner.file.is_some() {
                // drop old handle
                inner.file = None;
            }
            inner.path = self.dir.join(format!("{}.{}", self.prefix, today));
            inner.date = today.clone();
            inner.size = if inner.path.exists() {
                std::fs::metadata(&inner.path).map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };
            let f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&inner.path)?;
            inner.file = Some(f);
            return Ok(());
        }

        // size check
        if let Some(max) = self.max_bytes {
            if inner.size >= max {
                // rotate current file to unique name
                if let Some(f) = inner.file.take() {
                    drop(f);
                }
                if inner.path.exists() {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    let rotated =
                        std::path::PathBuf::from(format!("{}.{}", inner.path.display(), ts));
                    let _ = std::fs::rename(&inner.path, &rotated);
                }
                inner.path = self.dir.join(format!("{}.{}", self.prefix, today));
                inner.size = 0;
                let f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&inner.path)?;
                inner.file = Some(f);
                self.cleanup();
            }
        }
        Ok(())
    }

    fn cleanup(&self) {
        use std::fs;
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = match path.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };
            if !name.starts_with(&self.prefix) {
                continue;
            }
            if path == self.inner.lock().unwrap().path {
                continue;
            }
            let meta = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            files.push((path, meta.len(), mtime));
        }
        files.sort_by_key(|f| f.2);

        // max_files
        if let Some(max) = self.max_files {
            if max > 0 && files.len() > max {
                let to_remove = files.len() - max;
                for (path, _, _) in files.iter().take(to_remove) {
                    let _ = fs::remove_file(path);
                }
                files = files.into_iter().skip(to_remove).collect();
            }
        }

        // max_total_bytes
        if let Some(max_bytes) = self.max_total_bytes {
            let mut total: u64 = files.iter().map(|f| f.1).sum();
            // include current file size
            total += self.inner.lock().unwrap().size;
            let mut idx = 0;
            while total > max_bytes && idx < files.len() {
                let _ = fs::remove_file(&files[idx].0);
                total -= files[idx].1;
                idx += 1;
            }
        }
    }
}

impl std::io::Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| std::io::Error::other("lock poisoned"))?;
        self.ensure_ready(&mut inner)?;
        if let Some(f) = &mut inner.file {
            let n = f.write(buf)?;
            inner.size += n as u64;
            Ok(n)
        } else {
            Err(std::io::Error::other("no open file"))
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| std::io::Error::other("lock poisoned"))?;
        if let Some(f) = &mut inner.file {
            f.flush()
        } else {
            Ok(())
        }
    }
}

impl Clone for RotatingFile {
    fn clone(&self) -> Self {
        // Not truly cloneable; reconstruct with same config but fresh state.
        // RotatingFile is used once in non_blocking, clone not needed.
        Self {
            inner: Mutex::new(RotatingInner {
                path: std::path::PathBuf::new(),
                file: None,
                size: 0,
                date: String::new(),
            }),
            dir: self.dir.clone(),
            prefix: self.prefix.clone(),
            max_bytes: self.max_bytes,
            max_files: self.max_files,
            max_total_bytes: self.max_total_bytes,
        }
    }
}

// Send + Sync
unsafe impl Send for RotatingFile {}
unsafe impl Sync for RotatingFile {}
