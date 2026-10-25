//! Shared local process conventions for lifecycle commands.
//!
//! The CLI stays a pure HTTP client for business commands. Only the explicit
//! `server` lifecycle commands and the `mcp` launcher manage local processes,
//! and both share the discovery, PID file, and signal behavior defined here.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Binary name of the managed server process.
pub const SERVER_BINARY_NAME: &str = "cce";
/// Binary name of the standalone MCP process.
pub const MCP_BINARY_NAME: &str = "cce-mcp";

/// Resolve a sibling or installed binary with explicit override winning.
///
/// Order: explicit `--binary`, then the directory holding the current
/// executable, then every directory on `PATH`.
pub fn resolve_binary(explicit: Option<&str>, name: &str) -> Result<PathBuf> {
    if let Some(path) = explicit.filter(|p| !p.trim().is_empty()) {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Ok(candidate);
        }
        anyhow::bail!("binary override does not exist: {}", candidate.display());
    }
    let file_name = exe_file_name(name);
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let sibling = dir.join(&file_name);
            if sibling.exists() {
                return Ok(sibling);
            }
        }
    }
    if let Some(found) = search_path(&file_name) {
        return Ok(found);
    }
    anyhow::bail!(
        "could not locate '{name}'; pass --binary <path> or install it beside cce-cli or on PATH"
    )
}

/// Search `PATH` for the given file name.
pub fn search_path(file_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let separator = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.to_string_lossy().split(separator) {
        let dir = dir.trim();
        if dir.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(dir).join(file_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Append the platform executable suffix when needed.
pub fn exe_file_name(name: &str) -> String {
    if cfg!(windows) && !name.to_lowercase().ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Base runtime directory holding PID and log files.
pub fn default_runtime_dir() -> PathBuf {
    if let Some(dir) = dirs::data_dir() {
        return dir.join("cce");
    }
    std::env::temp_dir().join("cce")
}

/// Extract the port from a server URL for per-instance file names.
pub fn server_port(server_url: &str) -> u16 {
    let after_scheme = server_url
        .find("://")
        .map(|pos| &server_url[pos + 3..])
        .unwrap_or(server_url);
    let after_auth = after_scheme
        .rfind('@')
        .map_or(after_scheme, |pos| after_scheme[pos + 1..].as_ref());
    let host_port = after_auth.split('/').next().unwrap_or(after_auth);
    if host_port.starts_with('[') {
        if let Some(close) = host_port.find(']') {
            let rest = &host_port[close + 1..];
            if let Some(port_str) = rest.strip_prefix(':') {
                if let Ok(port) = port_str.parse::<u16>() {
                    return port;
                }
            }
        }
        return 9000;
    }
    if let Some(colon) = host_port.rfind(':') {
        let port_str = &host_port[colon + 1..];
        if let Ok(port) = port_str.parse::<u16>() {
            return port;
        }
    }
    9000
}

/// Default PID file for the given server URL unless overridden.
pub fn default_pid_file(server_url: &str) -> PathBuf {
    default_runtime_dir().join(format!("server-{}.pid", server_port(server_url)))
}

/// Default log file for the given server URL unless overridden.
pub fn default_log_file(server_url: &str) -> PathBuf {
    default_runtime_dir().join(format!("server-{}.log", server_port(server_url)))
}

/// Resolve an optional override against the matching default.
pub fn resolve_override(override_path: Option<&str>, default: PathBuf) -> PathBuf {
    override_path
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or(default)
}

/// Read a PID file, returning `None` when it does not exist.
pub fn read_pid_file(path: &Path) -> Result<Option<u32>> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                anyhow::bail!("PID file is empty: {}", path.display());
            }
            let pid: u32 = trimmed
                .parse()
                .with_context(|| format!("PID file is corrupt: {}", path.display()))?;
            Ok(Some(pid))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!(
            "failed to read PID file {}: {e}",
            path.display()
        )),
    }
}

/// Check whether a PID currently maps to a live process.
pub fn process_alive(pid: u32) -> bool {
    use sysinfo::ProcessesToUpdate;
    let pid = sysinfo::Pid::from_u32(pid);
    let mut system = sysinfo::System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).is_some()
}

/// Terminate a process gracefully, escalating to force after the grace period.
///
/// Unix sends the termination signal first; Windows terminates directly.
/// Returns whether the process is gone afterwards.
pub fn terminate_process(pid: u32, grace_secs: u64) -> Result<bool> {
    use sysinfo::ProcessesToUpdate;
    let target = sysinfo::Pid::from_u32(pid);
    let mut system = sysinfo::System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[target]), true);
    let Some(process) = system.process(target) else {
        return Ok(false);
    };
    #[cfg(unix)]
    {
        if process.kill_with(sysinfo::Signal::Term) != Some(true) {
            return Ok(false);
        }
    }
    #[cfg(not(unix))]
    {
        if !process.kill() {
            return Ok(false);
        }
    }
    if wait_for_exit(&mut system, target, grace_secs.max(1)) {
        return Ok(true);
    }
    system.refresh_processes(ProcessesToUpdate::Some(&[target]), true);
    if let Some(process) = system.process(target) {
        if !process.kill() {
            return Ok(false);
        }
    } else {
        return Ok(true);
    }
    Ok(wait_for_exit(&mut system, target, 5))
}

/// Poll until the PID disappears from the process table or time runs out.
fn wait_for_exit(system: &mut sysinfo::System, target: sysinfo::Pid, timeout_secs: u64) -> bool {
    use sysinfo::ProcessesToUpdate;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
        system.refresh_processes(ProcessesToUpdate::Some(&[target]), true);
        if system.process(target).is_none() {
            return true;
        }
    }
    false
}

/// Keep the last `n` lines of log text.
pub fn tail_lines(content: &str, n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_binary_wins_over_discovery() {
        let dir = std::env::temp_dir();
        let marker = dir.join(format!("cce-test-binary-{}", std::process::id()));
        std::fs::write(&marker, "x").expect("marker writes");
        let resolved = resolve_binary(Some(marker.to_str().expect("utf8")), "does-not-exist")
            .expect("explicit resolves");
        assert_eq!(resolved, marker);
        std::fs::remove_file(&marker).ok();
    }

    #[test]
    fn missing_explicit_binary_errors_without_fallback() {
        let result = resolve_binary(Some("/definitely/missing/cce-binary-xyz"), "cce");
        assert!(result.is_err());
    }

    #[test]
    fn sibling_directory_outranks_path_entries() {
        let current = std::env::current_exe().expect("current exe");
        let name = current
            .file_name()
            .expect("exe name")
            .to_string_lossy()
            .to_string();
        let resolved = resolve_binary(None, &name).expect("self resolves");
        let dir = current.parent().expect("exe dir");
        assert_eq!(resolved, dir.join(exe_file_name(&name)));
    }

    #[test]
    fn server_port_parses_common_urls() {
        assert_eq!(server_port("http://127.0.0.1:9000"), 9000);
        assert_eq!(server_port("http://127.0.0.1:9001/"), 9001);
        assert_eq!(server_port("http://localhost"), 9000);
        assert_eq!(server_port("http://[::1]:9010"), 9010);
        assert_eq!(server_port("not a url"), 9000);
    }

    #[test]
    fn default_files_carry_the_server_port() {
        let pid = default_pid_file("http://127.0.0.1:9001");
        let log = default_log_file("http://127.0.0.1:9001");
        assert!(pid.to_string_lossy().contains("9001"));
        assert!(log.to_string_lossy().contains("9001"));
        assert_ne!(
            default_pid_file("http://127.0.0.1:9001"),
            default_pid_file("http://127.0.0.1:9002")
        );
    }

    #[test]
    fn corrupt_pid_file_reports_instead_of_panicking() {
        let path = std::env::temp_dir().join(format!("cce-pid-{}.tmp", std::process::id()));
        std::fs::write(&path, "not-a-pid\n").expect("pid fixture writes");
        assert!(read_pid_file(&path).is_err());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_pid_file_maps_to_none() {
        let path = std::env::temp_dir().join(format!("cce-missing-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(read_pid_file(&path).expect("missing maps to none"), None);
    }

    #[test]
    fn tail_keeps_only_the_requested_suffix() {
        let content = "a\nb\nc\nd";
        assert_eq!(tail_lines(content, 2), "c\nd");
        assert_eq!(tail_lines(content, 10), content);
        assert_eq!(tail_lines(content, 0), "");
    }

    #[test]
    fn current_process_counts_as_alive() {
        assert!(process_alive(std::process::id()));
    }
}
