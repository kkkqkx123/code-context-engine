//! Resident health heartbeat for supervisors and health probes.

use anyhow::{Context, Result};

use super::params::SyncParams;

/// Schema version of the heartbeat record written by this build.
const GATEWAY_HEALTH_VERSION: u32 = 1;

/// Resident health state written as a heartbeat file.
///
/// The format only ever gains fields: supervisors parse leniently, treating
/// an absent `version` as the earliest shape.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayHealth {
    /// Heartbeat schema version; absent in records written before
    /// versioning was introduced.
    #[serde(default)]
    pub version: u32,
    /// Local path being supplied.
    pub path: String,
    /// Remote project id.
    pub project_id: i64,
    /// Last successful sync in RFC3339, if any.
    pub last_sync: Option<String>,
    /// Files in the last baseline.
    pub baseline_files: usize,
    /// Daemon start time in RFC3339.
    pub started_at: String,
    /// Last forced full scan in RFC3339, if any.
    #[serde(default)]
    pub last_full_scan: Option<String>,
}

/// Build one heartbeat record for the current watch state.
fn health_record(
    params: &SyncParams,
    baseline_files: usize,
    started_at: &str,
    last_full_scan: Option<String>,
) -> GatewayHealth {
    GatewayHealth {
        version: GATEWAY_HEALTH_VERSION,
        path: params.path.clone(),
        project_id: params.project_id,
        last_sync: Some(chrono::Utc::now().to_rfc3339()),
        baseline_files,
        started_at: started_at.to_string(),
        last_full_scan,
    }
}

/// Write the heartbeat file for process supervisors and health probes.
pub fn write_health_file(path: &std::path::Path, health: &GatewayHealth) -> Result<()> {
    let content = serde_json::to_string_pretty(health).context("health must serialize")?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).context("health parent must be creatable")?;
    }
    std::fs::write(path, content).context("health file must be writable")?;
    Ok(())
}

/// Persist the heartbeat when one is configured, warning otherwise.
pub(crate) fn write_heartbeat(
    params: &SyncParams,
    baseline_files: usize,
    started_at: &str,
    last_full_scan: Option<String>,
) {
    let Some(path) = params.health_file.as_deref() else {
        return;
    };
    let health = health_record(params, baseline_files, started_at, last_full_scan);
    if let Err(e) = write_health_file(path, &health) {
        eprintln!("warning: health write failed: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_is_versioned_and_parses_legacy_records_leniently() {
        let params = SyncParams::with_defaults(9, "/srv/repo");
        let record = health_record(&params, 12, "2026-01-01T00:00:00Z", None);
        assert_eq!(record.version, GATEWAY_HEALTH_VERSION);
        assert_eq!(
            record.version,
            serde_json::to_value(&record)
                .expect("health serializes")
                .get("version")
                .and_then(|v| v.as_u64())
                .expect("version field is written") as u32
        );
        let legacy = serde_json::json!({
            "path": "/srv/repo",
            "project_id": 9,
            "last_sync": null,
            "baseline_files": 3,
            "started_at": "2025-01-01T00:00:00Z"
        });
        let parsed: GatewayHealth =
            serde_json::from_value(legacy).expect("legacy heartbeat parses leniently");
        assert_eq!(parsed.version, 0);
        assert!(parsed.last_full_scan.is_none());
    }
}
