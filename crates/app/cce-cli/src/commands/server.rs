//! Server lifecycle commands for local process management.
//!
//! Business commands stay pure HTTP clients. Only this command group spawns,
//! stops, or inspects the local `cce` server process, and every action is
//! explicit: nothing is auto-started as a side effect of another command.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::Serialize;
use tokio::process::Command;

use crate::cli::OutputFormat;
use crate::output::{print_error, print_output, print_success, print_warning};

use super::process as proc;

/// Server lifecycle state combining local PID liveness and remote health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ServerState {
    Stopped,
    UnmanagedRunning,
    Starting,
    Healthy,
    Degraded,
    StalePid,
}

/// JSON snapshot for `server status --format json`.
#[derive(Debug, Serialize)]
struct ServerStatusSnapshot {
    server_url: String,
    state: ServerState,
    pid: Option<u32>,
    pid_alive: bool,
    pid_file: String,
    log_file: String,
    reachable: bool,
    healthy: Option<bool>,
}

/// Server lifecycle commands (local process management only).
#[derive(Debug, Subcommand)]
pub enum ServerCommands {
    /// Run the server in the foreground (inherits stdio, no PID file)
    Run {
        /// Config file path, forwarded as CCE_CONFIG
        #[arg(short, long)]
        config: Option<String>,
        /// Explicit server binary path (default: auto-discovered `cce`)
        #[arg(long)]
        binary: Option<String>,
    },
    /// Start the server in the background (writes PID and log files)
    Start {
        /// Config file path, forwarded as CCE_CONFIG
        #[arg(short, long)]
        config: Option<String>,
        /// Explicit server binary path (default: auto-discovered `cce`)
        #[arg(long)]
        binary: Option<String>,
        /// PID file path (default: derived from the data dir and server port)
        #[arg(long)]
        pid_file: Option<String>,
        /// Log file path (default: derived from the data dir and server port)
        #[arg(long)]
        log_file: Option<String>,
        /// Seconds to wait for the health endpoint
        #[arg(long, default_value = "30")]
        wait_timeout_secs: u64,
    },
    /// Stop the background server via its PID file
    Stop {
        /// PID file path (default: derived from the data dir and server port)
        #[arg(long)]
        pid_file: Option<String>,
        /// Seconds to wait for graceful exit before force termination
        #[arg(long, default_value = "10")]
        grace_secs: u64,
    },
    /// Restart the background server (stop then start)
    Restart {
        /// Config file path, forwarded as CCE_CONFIG
        #[arg(short, long)]
        config: Option<String>,
        /// Explicit server binary path (default: auto-discovered `cce`)
        #[arg(long)]
        binary: Option<String>,
        /// PID file path (default: derived from the data dir and server port)
        #[arg(long)]
        pid_file: Option<String>,
        /// Log file path (default: derived from the data dir and server port)
        #[arg(long)]
        log_file: Option<String>,
        /// Seconds to wait for the health endpoint
        #[arg(long, default_value = "30")]
        wait_timeout_secs: u64,
        /// Seconds to wait for graceful exit before force termination
        #[arg(long, default_value = "10")]
        grace_secs: u64,
    },
    /// Check local process state plus remote health
    Status {
        /// PID file path (default: derived from the data dir and server port)
        #[arg(long)]
        pid_file: Option<String>,
    },
    /// Show or follow the background server log file
    Logs {
        /// Log file path (default: derived from the data dir and server port)
        #[arg(long)]
        log_file: Option<String>,
        /// Number of trailing lines to show
        #[arg(long, default_value = "200")]
        tail: usize,
        /// Follow new output until interrupted
        #[arg(short, long)]
        follow: bool,
    },
}

/// Execute a server lifecycle command.
pub async fn execute(
    cmd: &ServerCommands,
    server_url: &str,
    verbose: bool,
    format: OutputFormat,
) -> Result<()> {
    match cmd {
        ServerCommands::Run { config, binary } => {
            run_foreground(config.clone(), binary.clone()).await
        }
        ServerCommands::Start {
            config,
            binary,
            pid_file,
            log_file,
            wait_timeout_secs,
        } => {
            start_background(
                server_url,
                config.clone(),
                binary.clone(),
                pid_file.clone(),
                log_file.clone(),
                *wait_timeout_secs,
                verbose,
            )
            .await
        }
        ServerCommands::Stop {
            pid_file,
            grace_secs,
        } => stop_background(server_url, pid_file.clone(), *grace_secs, verbose).await,
        ServerCommands::Restart {
            config,
            binary,
            pid_file,
            log_file,
            wait_timeout_secs,
            grace_secs,
        } => {
            stop_background(server_url, pid_file.clone(), *grace_secs, verbose).await?;
            start_background(
                server_url,
                config.clone(),
                binary.clone(),
                pid_file.clone(),
                log_file.clone(),
                *wait_timeout_secs,
                verbose,
            )
            .await
        }
        ServerCommands::Status { pid_file } => {
            status(server_url, pid_file.clone(), verbose, format).await
        }
        ServerCommands::Logs {
            log_file,
            tail,
            follow,
        } => logs(server_url, log_file.clone(), *tail, *follow).await,
    }
}

/// Foreground run with inherited stdio and exit-code propagation.
async fn run_foreground(config: Option<String>, binary: Option<String>) -> Result<()> {
    let binary_path = proc::resolve_binary(binary.as_deref(), proc::SERVER_BINARY_NAME)?;
    let mut command = Command::new(&binary_path);
    if let Some(path) = config {
        command.env("CCE_CONFIG", path);
    }
    let mut child = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to launch {}", binary_path.display()))?;
    let status = child.wait().await.context("server exited abnormally")?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Background start with PID file, log redirect, and readiness wait.
#[allow(clippy::too_many_arguments)]
async fn start_background(
    server_url: &str,
    config: Option<String>,
    binary: Option<String>,
    pid_file: Option<String>,
    log_file: Option<String>,
    wait_timeout_secs: u64,
    verbose: bool,
) -> Result<()> {
    let pid_path = proc::resolve_override(pid_file.as_deref(), proc::default_pid_file(server_url));
    let log_path = proc::resolve_override(log_file.as_deref(), proc::default_log_file(server_url));
    if let Some(existing) = proc::read_pid_file(&pid_path)? {
        if proc::process_alive(existing) {
            anyhow::bail!("server is already running (pid {existing})");
        }
    }
    if proc::read_pid_file(&pid_path)?.is_some() {
        print_warning("removing stale PID file");
        let _ = std::fs::remove_file(&pid_path);
    }
    let binary_path = proc::resolve_binary(binary.as_deref(), proc::SERVER_BINARY_NAME)?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory {}", parent.display()))?;
    }
    let log_file_handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open log file {}", log_path.display()))?;
    let stdout: Stdio = log_file_handle
        .try_clone()
        .map(Stdio::from)
        .with_context(|| format!("failed to redirect server output to {}", log_path.display()))?;
    let stderr: Stdio = log_file_handle.into();
    let mut command = Command::new(&binary_path);
    if let Some(path) = config {
        command.env("CCE_CONFIG", path);
    }
    let child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .with_context(|| format!("failed to launch {}", binary_path.display()))?;
    let pid = child.id().context("spawned server has no process id")?;
    std::mem::forget(child);
    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&pid_path, format!("{pid}\n"))
        .with_context(|| format!("failed to write PID file {}", pid_path.display()))?;
    if verbose {
        println!("started server pid {pid}; waiting for {server_url}");
    }
    match wait_for_ready(server_url, wait_timeout_secs).await {
        Ok(health) => {
            if health.map(|h| h.healthy).unwrap_or(true) {
                print_success(&format!("server is ready (pid {pid})"));
            } else {
                print_warning("server is reachable but reports degraded health");
                print_success(&format!("server is ready (pid {pid})"));
            }
            Ok(())
        }
        Err(e) => {
            print_error(&format!("server did not become ready: {e:#}"));
            print_error(&format!("see log tail: {}", log_path.display()));
            let _ = proc::terminate_process(pid, 5);
            let _ = std::fs::remove_file(&pid_path);
            Err(anyhow::anyhow!("server failed to become ready"))
        }
    }
}

/// Background stop keyed by the PID file; never kills unknown processes.
async fn stop_background(
    server_url: &str,
    pid_file: Option<String>,
    grace_secs: u64,
    verbose: bool,
) -> Result<()> {
    let pid_path = proc::resolve_override(pid_file.as_deref(), proc::default_pid_file(server_url));
    match proc::read_pid_file(&pid_path)? {
        None => {
            if is_reachable(server_url).await {
                print_warning(&format!(
                    "server at {server_url} is reachable but has no PID file {}; refusing to terminate an unmanaged process",
                    pid_path.display()
                ));
            } else {
                print_success("server is already stopped");
            }
            Ok(())
        }
        Some(pid) => {
            if !proc::process_alive(pid) {
                let _ = std::fs::remove_file(&pid_path);
                if verbose {
                    println!("removed stale PID file for dead pid {pid}");
                }
                print_success("server is already stopped (stale PID file removed)");
                return Ok(());
            }
            if verbose {
                println!("stopping server pid {pid}");
            }
            let exited = proc::terminate_process(pid, grace_secs)?;
            let _ = std::fs::remove_file(&pid_path);
            if exited {
                print_success("server stopped");
            } else {
                print_warning(&format!(
                    "termination signal sent to pid {pid}, exit not confirmed"
                ));
            }
            Ok(())
        }
    }
}

/// Combined local plus remote status without mutating any state.
async fn status(
    server_url: &str,
    pid_file: Option<String>,
    verbose: bool,
    format: OutputFormat,
) -> Result<()> {
    let pid_path = proc::resolve_override(pid_file.as_deref(), proc::default_pid_file(server_url));
    let log_path = proc::default_log_file(server_url);
    let pid = proc::read_pid_file(&pid_path)?;
    let pid_alive = pid.map(proc::process_alive).unwrap_or(false);
    let health_opt = probe_health(server_url).await?;
    let reachable = health_opt.is_some();
    let healthy = health_opt.map(|h| h.healthy);
    let state = match (pid, pid_alive, reachable, healthy) {
        (None, _, false, _) => ServerState::Stopped,
        (None, _, true, _) => ServerState::UnmanagedRunning,
        (Some(_), false, false, _) => ServerState::StalePid,
        (Some(_), false, true, _) => ServerState::UnmanagedRunning,
        (Some(_), true, false, _) => ServerState::Starting,
        (Some(_), true, true, Some(true)) => ServerState::Healthy,
        (Some(_), true, true, _) => ServerState::Degraded,
    };
    if verbose {
        println!("PID file: {}", pid_path.display());
        println!("Log file: {}", log_path.display());
    }
    if matches!(format, OutputFormat::Json) {
        print_output(
            format,
            &ServerStatusSnapshot {
                server_url: server_url.to_string(),
                state,
                pid,
                pid_alive,
                pid_file: pid_path.display().to_string(),
                log_file: log_path.display().to_string(),
                reachable,
                healthy,
            },
        );
        return Ok(());
    }
    match state {
        ServerState::Healthy => {
            print_success(&format!("server is healthy at {server_url}"));
            if let Some(pid) = pid {
                println!("  PID:      {pid}");
            }
        }
        ServerState::Degraded => {
            print_warning(&format!("server at {server_url} is reachable but degraded"));
            if let Some(pid) = pid {
                println!("  PID:      {pid}");
            }
        }
        ServerState::Starting => {
            print_warning(&format!(
                "server process {} exists but is not reachable yet",
                pid.unwrap_or(0)
            ));
        }
        ServerState::Stopped => print_success("server is stopped"),
        ServerState::UnmanagedRunning => print_warning(&format!(
            "server at {server_url} is reachable but not managed by {}",
            pid_path.display()
        )),
        ServerState::StalePid => print_warning(&format!(
            "stale PID file {} (process gone, server unreachable); run `server start` to recover",
            pid_path.display()
        )),
    }
    Ok(())
}

/// Show the tail of the background log file, optionally following it.
async fn logs(server_url: &str, log_file: Option<String>, tail: usize, follow: bool) -> Result<()> {
    let log_path: PathBuf =
        proc::resolve_override(log_file.as_deref(), proc::default_log_file(server_url));
    if !log_path.exists() {
        anyhow::bail!(
            "log file does not exist: {}; is the background server running?",
            log_path.display()
        );
    }
    let content = std::fs::read_to_string(&log_path)
        .with_context(|| format!("failed to read log file {}", log_path.display()))?;
    let suffix = proc::tail_lines(&content, tail);
    if !suffix.is_empty() {
        println!("{suffix}");
    }
    if !follow {
        return Ok(());
    }
    let mut position = std::fs::metadata(&log_path)
        .map(|m| m.len())
        .unwrap_or_default();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
        let len = std::fs::metadata(&log_path)
            .map(|m| m.len())
            .unwrap_or(position);
        if len < position {
            position = 0;
        }
        if len > position {
            let bytes = std::fs::read(&log_path).unwrap_or_default();
            let start = (position as usize).min(bytes.len());
            let chunk = String::from_utf8_lossy(&bytes[start..]);
            if !chunk.is_empty() {
                print!("{chunk}");
            }
            position = len;
        }
    }
    Ok(())
}

/// Wait until the health endpoint answers successfully or the timeout lapses.
///
/// Reachability alone counts as ready; a degraded `healthy` flag only warns.
async fn wait_for_ready(
    server_url: &str,
    timeout_secs: u64,
) -> Result<Option<cce_api::models::HealthStatus>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs.max(1));
    let mut last_error = String::from("no attempts made");
    while tokio::time::Instant::now() < deadline {
        match probe_once(server_url).await {
            Ok(health) => return Ok(Some(health)),
            Err(e) => last_error = e,
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(anyhow::anyhow!(
        "timed out waiting for health: {last_error}"
    ))
}

/// Single health probe bounded to two seconds.
async fn probe_once(
    server_url: &str,
) -> std::result::Result<cce_api::models::HealthStatus, String> {
    let client = crate::client::ApiClient::new(server_url).map_err(|e| format!("{e:#}"))?;
    match tokio::time::timeout(Duration::from_secs(2), client.get(cce_api::paths::HEALTH)).await {
        Ok(Ok(health)) => Ok(health),
        Ok(Err(e)) => Err(format!("{e:#}")),
        Err(_) => Err("health probe timed out".to_string()),
    }
}

/// Best-effort reachability used by stop and status paths.
async fn probe_health(server_url: &str) -> Result<Option<cce_api::models::HealthStatus>> {
    let client = crate::client::ApiClient::new(server_url)?;
    match tokio::time::timeout(
        Duration::from_secs(5),
        client.get::<cce_api::models::HealthStatus>(cce_api::paths::HEALTH),
    )
    .await
    {
        Ok(Ok(health)) => Ok(Some(health)),
        Ok(Err(_)) | Err(_) => Ok(None),
    }
}

/// Whether the health endpoint answers at all.
async fn is_reachable(server_url: &str) -> bool {
    probe_health(server_url).await.unwrap_or(None).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_matrix_covers_mixed_conditions() {
        assert_eq!(
            classify_for_test(None, false, false, None),
            ServerState::Stopped
        );
        assert_eq!(
            classify_for_test(None, false, true, Some(true)),
            ServerState::UnmanagedRunning
        );
        assert_eq!(
            classify_for_test(Some(10), true, false, None),
            ServerState::Starting
        );
        assert_eq!(
            classify_for_test(Some(10), true, true, Some(true)),
            ServerState::Healthy
        );
        assert_eq!(
            classify_for_test(Some(10), true, true, Some(false)),
            ServerState::Degraded
        );
        assert_eq!(
            classify_for_test(Some(10), false, false, None),
            ServerState::StalePid
        );
    }

    fn classify_for_test(
        pid: Option<u32>,
        alive: bool,
        reachable: bool,
        healthy: Option<bool>,
    ) -> ServerState {
        match (pid, alive, reachable, healthy) {
            (None, _, false, _) => ServerState::Stopped,
            (None, _, true, _) => ServerState::UnmanagedRunning,
            (Some(_), false, false, _) => ServerState::StalePid,
            (Some(_), false, true, _) => ServerState::UnmanagedRunning,
            (Some(_), true, false, _) => ServerState::Starting,
            (Some(_), true, true, Some(true)) => ServerState::Healthy,
            (Some(_), true, true, _) => ServerState::Degraded,
        }
    }

    #[test]
    fn status_snapshot_serializes_state_as_snake_case() {
        let snapshot = ServerStatusSnapshot {
            server_url: "http://127.0.0.1:9000".to_string(),
            state: ServerState::Healthy,
            pid: Some(42),
            pid_alive: true,
            pid_file: "/tmp/server-9000.pid".to_string(),
            log_file: "/tmp/server-9000.log".to_string(),
            reachable: true,
            healthy: Some(true),
        };
        let value = serde_json::to_value(&snapshot).expect("snapshot serializes");
        assert_eq!(value["state"], "healthy");
        assert_eq!(value["pid"], 42);
    }
}
