use std::sync::Arc;
use std::time::Duration;

use cce_storage_qdrant::{
    QdrantControlAction, QdrantProcessConfig, QdrantProcessHandle, QdrantProcessManager,
    QdrantProcessStatus,
};
use tokio::sync::{RwLock, mpsc};

impl super::CodeContextEngine {
    /// Start the Qdrant subprocess manager (controllable by API)
    ///
    /// If auto_start is enabled in the Qdrant config, this method:
    /// 1. Creates a shared status + control channel
    /// 2. Spawns a background task that manages the Qdrant process lifecycle
    /// 3. Stores a `QdrantProcessHandle` in the Engine so that API handlers can
    ///    query status and send start/stop/restart commands
    ///
    /// This method stores the handle and returns immediately.
    pub fn start_qdrant_process_manager(&mut self) -> Option<QdrantProcessHandle> {
        if !self.qdrant.config().is_process_managed() {
            tracing::info!("Qdrant subprocess management is disabled");
            return None;
        }

        let process_config = QdrantProcessConfig::from_config(self.qdrant.config());
        let auto_start = self.qdrant.config().auto_start;
        let auto_restart = process_config.auto_restart;

        tracing::info!(
            auto_start,
            auto_restart,
            "Starting controllable Qdrant process manager"
        );

        // Shared state between background task and API handlers
        let shared_status: Arc<RwLock<QdrantProcessStatus>> =
            Arc::new(RwLock::new(QdrantProcessStatus::Idle));
        let (control_tx, control_rx) = mpsc::unbounded_channel::<QdrantControlAction>();

        let handle = QdrantProcessHandle::new(auto_start, shared_status.clone(), control_tx);
        self.qdrant_control = Some(handle.clone());

        // Spawn the background lifecycle task (passing auto_start/auto_restart separately)
        Self::spawn_qdrant_lifecycle_task(
            process_config,
            auto_start,
            auto_restart,
            shared_status,
            control_rx,
        );

        Some(handle)
    }

    /// Internal helper: spawns the long-lived Qdrant lifecycle task.
    ///
    /// The task:
    /// 1. Auto-starts Qdrant (if `auto_start` is `true`)
    /// 2. Enters a control loop that processes commands from `control_rx`
    /// 3. Periodically checks whether the child process is still alive
    /// 4. If `auto_restart` is enabled, attempts restart with exponential backoff
    ///    when the process crashes unexpectedly
    fn spawn_qdrant_lifecycle_task(
        process_config: QdrantProcessConfig,
        auto_start: bool,
        auto_restart: bool,
        shared_status: Arc<RwLock<QdrantProcessStatus>>,
        mut control_rx: mpsc::UnboundedReceiver<QdrantControlAction>,
    ) {
        tokio::spawn(async move {
            let mut manager = QdrantProcessManager::new(process_config);
            let status_ptr = shared_status.clone();

            // ---- Auto-start on boot ----
            if auto_start {
                *status_ptr.write().await = QdrantProcessStatus::Starting;
                match manager.start().await {
                    Ok(()) => {
                        tracing::info!("Qdrant auto-started successfully");
                        *status_ptr.write().await = QdrantProcessStatus::Running;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Qdrant auto-start failed");
                        *status_ptr.write().await = QdrantProcessStatus::Failed(e.to_string());
                        // Even if auto-start fails, continue listening so that
                        // the user can manually trigger a start via the API later.
                    }
                }
            }

            // ---- Control loop ----
            loop {
                tokio::select! {
                    Some(cmd) = control_rx.recv() => {
                        match cmd {
                            QdrantControlAction::Start => {
                                if *status_ptr.read().await == QdrantProcessStatus::Running {
                                    tracing::debug!("Qdrant already running, ignoring start");
                                    continue;
                                }
                                *status_ptr.write().await = QdrantProcessStatus::Starting;
                                match manager.start().await {
                                    Ok(()) => {
                                        tracing::info!("Qdrant started via API");
                                        *status_ptr.write().await = QdrantProcessStatus::Running;
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "Qdrant start via API failed");
                                        *status_ptr.write().await = QdrantProcessStatus::Failed(e.to_string());
                                    }
                                }
                            }
                            QdrantControlAction::Stop => {
                                if *status_ptr.read().await != QdrantProcessStatus::Running {
                                    tracing::debug!("Qdrant not running, ignoring stop");
                                    continue;
                                }
                                *status_ptr.write().await = QdrantProcessStatus::Stopping;
                                manager.stop().await.ok();
                                tracing::info!("Qdrant stopped via API");
                                *status_ptr.write().await = QdrantProcessStatus::Stopped;
                            }
                            QdrantControlAction::Restart => {
                                *status_ptr.write().await = QdrantProcessStatus::Stopping;
                                manager.stop().await.ok();
                                *status_ptr.write().await = QdrantProcessStatus::Starting;
                                match manager.start().await {
                                    Ok(()) => {
                                        tracing::info!("Qdrant restarted via API");
                                        *status_ptr.write().await = QdrantProcessStatus::Running;
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "Qdrant restart via API failed");
                                        *status_ptr.write().await = QdrantProcessStatus::Failed(e.to_string());
                                    }
                                }
                            }
                        }
                    }
                    // Periodic process liveness check (every 2 seconds)
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {
                        let current_status = status_ptr.read().await.clone();
                        if current_status != QdrantProcessStatus::Running {
                            // Don't check while we're deliberately stopping/starting
                            continue;
                        }

                        match manager.try_wait() {
                            Ok(Some(exit_status)) => {
                                tracing::warn!(
                                    code = exit_status.code().unwrap_or(-1),
                                    "Qdrant process exited unexpectedly"
                                );
                                *status_ptr.write().await = QdrantProcessStatus::Crashed;

                                if auto_restart {
                                    tracing::info!("Auto-restarting Qdrant with backoff");
                                    // Exponential backoff: 2s, 4s, 8s, 16s, 32s
                                    let mut backoff = 2u64;
                                    for attempt in 1u32..=5 {
                                        tokio::time::sleep(Duration::from_secs(backoff)).await;
                                        tracing::info!(
                                            attempt,
                                            backoff_secs = backoff,
                                            "Auto-restart attempt"
                                        );
                                        *status_ptr.write().await = QdrantProcessStatus::Starting;
                                        match manager.start().await {
                                            Ok(()) => {
                                                tracing::info!("Qdrant auto-restarted successfully");
                                                *status_ptr.write().await = QdrantProcessStatus::Running;
                                                break;
                                            }
                                            Err(e) => {
                                                tracing::error!(error = %e, "Auto-restart attempt failed");
                                                backoff = backoff.saturating_mul(2);
                                            }
                                        }
                                    }
                                    let final_status = status_ptr.read().await.clone();
                                    if final_status == QdrantProcessStatus::Starting || final_status == QdrantProcessStatus::Crashed {
                                        tracing::error!("Max auto-restart attempts reached, giving up");
                                        let msg = format!("Failed after {} restart attempts", 5);
                                        *status_ptr.write().await = QdrantProcessStatus::Failed(msg);
                                    }
                                } else {
                                    tracing::warn!("Auto-restart disabled, Qdrant will remain stopped");
                                }
                            }
                            Ok(None) => {
                                // Process still running normally
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Error checking Qdrant process status");
                            }
                        }
                    }
                }
            }
        });
    }

    /// Return a reference to the Qdrant process handle, if available.
    pub fn qdrant_control(&self) -> Option<&QdrantProcessHandle> {
        self.qdrant_control.as_ref()
    }

    /// Start the Qdrant connection health monitor
    ///
    /// Periodically checks the Qdrant health endpoint and logs warnings
    /// when the connection is lost. If the Qdrant process is managed and
    /// auto_restart is enabled, the process manager handles restarts.
    ///
    /// This spawns a background task and returns immediately.
    pub fn start_qdrant_connection_monitor(&self) {
        let qdrant = self.qdrant.clone();
        let check_interval = Duration::from_secs(30);

        tracing::info!(
            interval_secs = check_interval.as_secs(),
            "Starting Qdrant connection health monitor"
        );

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(check_interval);
            let mut consecutive_failures: u32 = 0;

            loop {
                interval.tick().await;

                match qdrant.health().await {
                    Ok(true) => {
                        if consecutive_failures > 0 {
                            tracing::info!(
                                consecutive_failures,
                                "Qdrant connection restored after {} failures",
                                consecutive_failures
                            );
                            consecutive_failures = 0;
                        }
                    }
                    Ok(false) => {
                        consecutive_failures += 1;
                        tracing::warn!(
                            consecutive_failures,
                            "Qdrant health check returned non-success status"
                        );
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        tracing::error!(
                            error = %e,
                            consecutive_failures,
                            "Qdrant health check failed"
                        );
                    }
                }
            }
        });
    }
}
