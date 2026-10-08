//! Qdrant subprocess manager

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::fs::File;
use tokio::process::{Child, Command};
use tokio::sync::{RwLock, mpsc};
use tokio::time::sleep;
use tracing::{error, info, trace, warn};

use crate::error::QdrantError;

/// Default port for Qdrant HTTP endpoint
const DEFAULT_QDRANT_PORT: u16 = 6333;

/// Common Qdrant binary names to search for
const BINARY_NAMES: &[&str] = &["qdrant", "qdrant.exe"];

/// Common installation paths for Qdrant binary discovery
const COMMON_PATHS: &[&str] = &[
    "/usr/bin",
    "/usr/local/bin",
    "/opt/homebrew/bin",
    "C:\\Program Files\\qdrant",
    "C:\\qdrant",
];

/// Configuration for the Qdrant process manager
#[derive(Debug, Clone)]
pub struct QdrantProcessConfig {
    /// Path to the Qdrant binary (None = auto-discover)
    pub binary_path: Option<PathBuf>,
    /// Data directory for Qdrant storage
    pub data_dir: Option<PathBuf>,
    /// Port for the HTTP endpoint
    pub port: u16,
    /// Startup timeout
    pub startup_timeout: Duration,
    /// Whether to auto-restart on unexpected exit
    pub auto_restart: bool,
}

impl Default for QdrantProcessConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            data_dir: None,
            port: DEFAULT_QDRANT_PORT,
            startup_timeout: Duration::from_secs(60),
            auto_restart: false,
        }
    }
}

impl QdrantProcessConfig {
    /// Create config from QdrantConfig's process management fields
    pub fn from_config(config: &cce_config::modules::QdrantConfig) -> Self {
        Self {
            binary_path: config.resolved_binary_path().map(PathBuf::from),
            data_dir: config.resolved_data_dir().map(PathBuf::from),
            port: Self::extract_port(&config.url),
            startup_timeout: Duration::from_secs(config.startup_timeout_secs),
            auto_restart: config.auto_restart,
        }
    }

    fn extract_port(raw_url: &str) -> u16 {
        let after_scheme = if let Some(pos) = raw_url.find("://") {
            &raw_url[pos + 3..]
        } else {
            raw_url
        };

        let after_auth = if let Some(pos) = after_scheme.rfind('@') {
            &after_scheme[pos + 1..]
        } else {
            after_scheme
        };

        if after_auth.starts_with('[') {
            if let Some(close) = after_auth.find(']') {
                let after_bracket = &after_auth[close + 1..];
                if let Some(port_str) = after_bracket
                    .strip_prefix(':')
                    .and_then(|s| s.split('/').next())
                    .filter(|&s| !s.is_empty())
                {
                    return port_str.parse().unwrap_or(DEFAULT_QDRANT_PORT);
                }
            }
            return DEFAULT_QDRANT_PORT;
        }

        if let Some(port_start) = after_auth.rfind(':') {
            let after_colon = &after_auth[port_start + 1..];
            let port_str = after_colon.split('/').next().unwrap_or(after_colon);
            if let Ok(port) = port_str.parse::<u16>() {
                return port;
            }
        }

        DEFAULT_QDRANT_PORT
    }
}

/// Status of the managed Qdrant process
pub use cce_api::models::QdrantProcessStatus;

/// Manager for Qdrant child process lifecycle
pub struct QdrantProcessManager {
    config: QdrantProcessConfig,
    process: Option<Child>,
    status: QdrantProcessStatus,
}

impl QdrantProcessManager {
    /// Create a new process manager with the given configuration
    pub fn new(config: QdrantProcessConfig) -> Self {
        Self {
            config,
            process: None,
            status: QdrantProcessStatus::Idle,
        }
    }

    /// Get the current process status
    pub fn status(&self) -> &QdrantProcessStatus {
        &self.status
    }

    /// Check if the process is currently running
    pub fn is_running(&self) -> bool {
        self.status == QdrantProcessStatus::Running
    }

    /// Start the Qdrant process
    pub async fn start(&mut self) -> Result<(), QdrantError> {
        info!("Starting Qdrant process manager");

        if self.is_running() {
            warn!("Qdrant process is already running");
            return Ok(());
        }

        self.status = QdrantProcessStatus::Starting;

        let binary_path = self.resolve_binary().await?;
        info!(binary = %binary_path.display(), "Resolved Qdrant binary");

        let mut cmd = Command::new(&binary_path);

        let stderr_log_path;
        if let Some(ref data_dir) = self.config.data_dir {
            tokio::fs::create_dir_all(data_dir)
                .await
                .map_err(|e| QdrantError::Io(e.into()))?;
            info!(data_dir = %data_dir.display(), "Using custom data directory");

            let config_path = data_dir.join("config.yaml");
            let config_content = format!(
                "storage:\n  storage_path: \"{}\"\n",
                data_dir.display().to_string().replace('\\', "/")
            );
            tokio::fs::write(&config_path, config_content)
                .await
                .map_err(|e| QdrantError::Io(e.into()))?;

            cmd.arg("--config-path").arg(&config_path);

            stderr_log_path = Some(data_dir.join("qdrant_stderr.log"));
            if let Some(ref log_path) = stderr_log_path {
                let log_file = File::create(log_path)
                    .await
                    .map_err(|e| QdrantError::Io(cce_types::error::common::IoError::from(e)))?;
                cmd.stderr(std::process::Stdio::from(log_file.into_std().await));
            } else {
                cmd.stderr(std::process::Stdio::null());
            }
        } else {
            stderr_log_path = None;
            cmd.stderr(std::process::Stdio::null());
        }

        cmd.stdout(std::process::Stdio::null());

        let process = cmd.spawn().map_err(|e| {
            error!(error = %e, binary = %binary_path.display(), "Failed to spawn Qdrant process");
            QdrantError::connection(format!(
                "Failed to spawn Qdrant at {}: {}",
                binary_path.display(),
                e
            ))
        })?;

        info!(pid = process.id().unwrap_or(0), "Qdrant process spawned");
        self.process = Some(process);

        match self.wait_for_health().await {
            Ok(()) => {
                self.status = QdrantProcessStatus::Running;
                info!("Qdrant process is ready");
                Ok(())
            }
            Err(e) => {
                let stderr_info = if let Some(ref log_path) = stderr_log_path {
                    match tokio::fs::read_to_string(log_path).await {
                        Ok(content) if !content.is_empty() => {
                            let truncated = if content.len() > 2048 {
                                format!("... (truncated)\n{}", &content[content.len() - 2048..])
                            } else {
                                content.clone()
                            };
                            format!("\nQdrant stderr:\n{}", truncated)
                        }
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };

                error!(error = %e, "Qdrant failed to become healthy");
                self.stop().await.ok();
                let enhanced_msg = format!("{}{}", e, stderr_info);
                self.status = QdrantProcessStatus::Failed(enhanced_msg.clone());
                Err(QdrantError::connection(enhanced_msg))
            }
        }
    }

    /// Stop the Qdrant process gracefully
    pub async fn stop(&mut self) -> Result<(), QdrantError> {
        if let Some(mut process) = self.process.take() {
            info!(pid = process.id().unwrap_or(0), "Stopping Qdrant process");

            let _ = process.start_kill();

            tokio::select! {
                result = process.wait() => {
                    match result {
                        Ok(status) => info!("Qdrant process exited with status: {}", status),
                        Err(e) => warn!(error = %e, "Error waiting for Qdrant process exit"),
                    }
                }
                _ = sleep(Duration::from_secs(5)) => {
                    warn!("Qdrant process did not exit in 5s, killing");
                    let _ = process.kill().await;
                }
            }

            self.status = QdrantProcessStatus::Stopped;
            info!("Qdrant process stopped");
        }
        Ok(())
    }

    /// Monitor the process for unexpected exits and auto-restart
    pub async fn monitor_and_restart(&mut self) -> Result<(), QdrantError> {
        loop {
            let exit_status = match self.process.as_mut() {
                Some(process) => process
                    .wait()
                    .await
                    .map_err(|e| QdrantError::connection(format!("Process wait error: {}", e)))?,
                None => {
                    self.status = QdrantProcessStatus::Stopped;
                    info!("No Qdrant process to monitor");
                    return Ok(());
                }
            };

            warn!("Qdrant process exited with: {}", exit_status);

            if self.config.auto_restart {
                info!("Auto-restarting Qdrant process...");
                self.process = None;
                self.status = QdrantProcessStatus::Idle;

                for attempt in 1..=5 {
                    sleep(Duration::from_secs(2u64.pow(attempt))).await;
                    match self.start().await {
                        Ok(()) => {
                            info!("Qdrant restarted successfully after exit");
                            break;
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                attempt = attempt,
                                "Failed to restart Qdrant, retrying..."
                            );
                        }
                    }
                }

                if !self.is_running() {
                    error!("Failed to restart Qdrant after multiple attempts");
                    self.status =
                        QdrantProcessStatus::Failed("Max restart attempts reached".into());
                    return Err(QdrantError::connection(
                        "Failed to restart Qdrant after max attempts",
                    ));
                }
            } else {
                self.status = QdrantProcessStatus::Crashed;
                info!("Auto-restart is disabled, not restarting Qdrant");
                return Err(QdrantError::connection(format!(
                    "Qdrant process exited unexpectedly with: {}",
                    exit_status
                )));
            }
        }
    }

    async fn resolve_binary(&self) -> Result<PathBuf, QdrantError> {
        if let Some(ref path) = self.config.binary_path {
            if path.exists() {
                return Ok(path.clone());
            }
            warn!(path = %path.display(), "Configured binary path does not exist, searching alternatives");
        }

        if let Some(path) = Self::search_path() {
            return Ok(path);
        }

        for base in COMMON_PATHS {
            let base_path = PathBuf::from(base);
            for name in BINARY_NAMES {
                let candidate = base_path.join(name);
                if candidate.exists() {
                    info!(binary = %candidate.display(), "Found Qdrant in common location");
                    return Ok(candidate);
                }
            }
        }

        Err(QdrantError::connection(
            "Qdrant binary not found. Install qdrant or set binary_path in config",
        ))
    }

    fn search_path() -> Option<PathBuf> {
        let path_var = std::env::var_os("PATH")?;
        let separator = if cfg!(windows) { ';' } else { ':' };

        for dir in path_var.to_string_lossy().split(separator) {
            let dir = dir.trim();
            if dir.is_empty() {
                continue;
            }
            for name in BINARY_NAMES {
                let candidate = PathBuf::from(dir).join(name);
                if candidate.exists() {
                    info!(binary = %candidate.display(), "Found Qdrant in PATH");
                    return Some(candidate);
                }
            }
        }
        None
    }

    async fn wait_for_health(&self) -> Result<(), QdrantError> {
        let port = self.config.port;
        let url = format!("http://127.0.0.1:{}/healthz", port);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| {
                QdrantError::connection(format!("Failed to build health client: {}", e))
            })?;

        let deadline = tokio::time::Instant::now() + self.config.startup_timeout;
        let mut delay = Duration::from_millis(200);

        while tokio::time::Instant::now() < deadline {
            match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    return Ok(());
                }
                Ok(response) => {
                    warn!(
                        status = response.status().as_u16(),
                        "Qdrant health check returned non-success status"
                    );
                }
                Err(e) => {
                    trace!("Qdrant health check failed: {}", e);
                }
            }

            sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(5));
        }

        Err(QdrantError::connection(format!(
            "Qdrant did not become healthy within {:.0?} on port {}",
            self.config.startup_timeout, port
        )))
    }

    /// Non-blocking check whether the Qdrant process has exited.
    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>, QdrantError> {
        match self.process.as_mut() {
            Some(child) => child.try_wait().map_err(|e| {
                QdrantError::connection(format!("Failed to check process status: {}", e))
            }),
            None => Ok(None),
        }
    }
}

impl Drop for QdrantProcessManager {
    fn drop(&mut self) {
        if self.process.is_some() {
            info!("QdrantProcessManager dropped, killing child process");
            if let Some(ref mut process) = self.process {
                let _ = process.start_kill();
            }
        }
    }
}

/// Commands that can be sent to the Qdrant process background task
#[derive(Debug, Clone)]
pub enum QdrantControlAction {
    /// Start the Qdrant process (idempotent if already running)
    Start,
    /// Gracefully stop the Qdrant process
    Stop,
    /// Restart the Qdrant process (stop + start)
    Restart,
}

/// Shared handle for controlling a managed Qdrant process from API handlers.
#[derive(Clone)]
pub struct QdrantProcessHandle {
    /// Shared process status (updated by the background task)
    pub status: Arc<RwLock<QdrantProcessStatus>>,
    /// Whether subprocess management is enabled (auto_start in config)
    pub managed: bool,
    /// Command channel to the background task
    control_tx: mpsc::UnboundedSender<QdrantControlAction>,
}

impl QdrantProcessHandle {
    /// Create a new handle wrapping the given status + channel.
    pub fn new(
        managed: bool,
        status: Arc<RwLock<QdrantProcessStatus>>,
        control_tx: mpsc::UnboundedSender<QdrantControlAction>,
    ) -> Self {
        Self {
            status,
            managed,
            control_tx,
        }
    }

    /// Send a start command to the background task (fire-and-forget).
    pub fn start(&self) {
        let _ = self.control_tx.send(QdrantControlAction::Start);
    }

    /// Send a stop command to the background task (fire-and-forget).
    pub fn stop(&self) {
        let _ = self.control_tx.send(QdrantControlAction::Stop);
    }

    /// Send a restart command to the background task (fire-and-forget).
    pub fn restart(&self) {
        let _ = self.control_tx.send(QdrantControlAction::Restart);
    }

    /// Get the current shared status (snapshot).
    pub async fn current_status(&self) -> QdrantProcessStatus {
        self.status.read().await.clone()
    }

    /// Quick check without awaiting.
    pub fn current_status_sync(&self) -> QdrantProcessStatus {
        self.status
            .try_read()
            .map(|s| s.clone())
            .unwrap_or(QdrantProcessStatus::Idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_config_default() {
        let config = QdrantProcessConfig::default();
        assert_eq!(config.port, 6333);
        assert_eq!(config.startup_timeout.as_secs(), 60);
        assert!(!config.auto_restart);
    }

    #[test]
    fn test_process_status_initial() {
        let manager = QdrantProcessManager::new(QdrantProcessConfig::default());
        assert_eq!(manager.status(), &QdrantProcessStatus::Idle);
        assert!(!manager.is_running());
    }

    #[test]
    fn test_extract_port() {
        assert_eq!(
            QdrantProcessConfig::extract_port("http://localhost:6333"),
            6333
        );
        assert_eq!(
            QdrantProcessConfig::extract_port("http://127.0.0.1:6333/"),
            6333
        );
        assert_eq!(
            QdrantProcessConfig::extract_port("http://localhost:7000"),
            7000
        );
        assert_eq!(QdrantProcessConfig::extract_port("http://localhost"), 6333);
    }

    #[test]
    fn test_extract_port_with_auth() {
        assert_eq!(
            QdrantProcessConfig::extract_port("http://user:pass@localhost:6333"),
            6333
        );
        assert_eq!(
            QdrantProcessConfig::extract_port("http://admin:secret@192.168.1.1:7000/"),
            7000
        );
    }

    #[test]
    fn test_extract_port_ipv6() {
        assert_eq!(QdrantProcessConfig::extract_port("http://[::1]:6333"), 6333);
        assert_eq!(
            QdrantProcessConfig::extract_port("http://[2001:db8::1]:7000/"),
            7000
        );
        assert_eq!(QdrantProcessConfig::extract_port("http://[::1]"), 6333);
    }

    #[test]
    fn test_from_config() {
        let config = cce_config::QdrantConfig {
            auto_start: true,
            startup_timeout_secs: 30,
            auto_restart: true,
            binary_path: Some("/usr/local/bin/qdrant".to_string()),
            data_dir: Some("/var/qdrant/data".to_string()),
            ..Default::default()
        };

        let process_config = QdrantProcessConfig::from_config(&config);
        assert_eq!(process_config.port, 6333);
        assert_eq!(process_config.startup_timeout.as_secs(), 30);
        assert!(process_config.auto_restart);
        assert_eq!(
            process_config.binary_path,
            Some(PathBuf::from("/usr/local/bin/qdrant"))
        );
        assert_eq!(
            process_config.data_dir,
            Some(PathBuf::from("/var/qdrant/data"))
        );
    }
}
