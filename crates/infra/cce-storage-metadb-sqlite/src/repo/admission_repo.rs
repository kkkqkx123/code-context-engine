//! Per-token admission audit persistence.
//!
//! The admission middleware stays stateless; this repository durably records
//! per-token usage, last-use time, and rejection distributions alongside the
//! existing metadata store so quota and audit survive restarts.

use rusqlite::{Connection, OptionalExtension, params};

use cce_types::StorageError;

fn current_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

pub use cce_storage_common::metadb::AdmissionAuditRecord;

/// Admission audit persistence.
pub struct AdmissionAuditRepository;

impl AdmissionAuditRepository {
    fn projects_text(projects: &[i64]) -> String {
        projects
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Fetch one audit row by fingerprint.
    pub fn get(
        conn: &Connection,
        fingerprint: &str,
    ) -> Result<Option<AdmissionAuditRecord>, StorageError> {
        conn.query_row(
            "SELECT token_fingerprint, projects, quota_bytes, bytes_used, admitted,
                    auth_rejections, scope_rejections, rate_rejections, body_rejections,
                    quota_rejections, last_used, last_reject_reason
             FROM admission_audit WHERE token_fingerprint = ?1",
            params![fingerprint],
            |row| {
                Ok(AdmissionAuditRecord {
                    token_fingerprint: row.get(0)?,
                    projects: row.get(1)?,
                    quota_bytes: row.get(2)?,
                    bytes_used: row.get(3)?,
                    admitted: row.get(4)?,
                    auth_rejections: row.get(5)?,
                    scope_rejections: row.get(6)?,
                    rate_rejections: row.get(7)?,
                    body_rejections: row.get(8)?,
                    quota_rejections: row.get(9)?,
                    last_used: row.get(10)?,
                    last_reject_reason: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(|e| StorageError::Query(format!("Failed to read admission audit: {e}")))
    }

    /// List every audit row ordered by fingerprint.
    pub fn list_all(conn: &Connection) -> Result<Vec<AdmissionAuditRecord>, StorageError> {
        let mut stmt = conn
            .prepare(
                "SELECT token_fingerprint, projects, quota_bytes, bytes_used, admitted,
                        auth_rejections, scope_rejections, rate_rejections, body_rejections,
                        quota_rejections, last_used, last_reject_reason
                 FROM admission_audit ORDER BY token_fingerprint",
            )
            .map_err(|e| StorageError::Query(format!("Failed to list admission audit: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(AdmissionAuditRecord {
                    token_fingerprint: row.get(0)?,
                    projects: row.get(1)?,
                    quota_bytes: row.get(2)?,
                    bytes_used: row.get(3)?,
                    admitted: row.get(4)?,
                    auth_rejections: row.get(5)?,
                    scope_rejections: row.get(6)?,
                    rate_rejections: row.get(7)?,
                    body_rejections: row.get(8)?,
                    quota_rejections: row.get(9)?,
                    last_used: row.get(10)?,
                    last_reject_reason: row.get(11)?,
                })
            })
            .map_err(|e| StorageError::Query(format!("Failed to list admission audit: {e}")))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| {
                StorageError::Query(format!("Failed to read admission audit row: {e}"))
            })?);
        }
        Ok(out)
    }

    /// Record one admitted batch with its stored byte count.
    pub fn record_admitted(
        conn: &Connection,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        bytes: u64,
    ) -> Result<(), StorageError> {
        let now = current_timestamp();
        let projects_text = Self::projects_text(projects);
        let quota = quota_bytes.map(|q| q.min(i64::MAX as u64) as i64);
        let bytes = bytes.min(i64::MAX as u64) as i64;
        conn.execute(
            "INSERT INTO admission_audit
                (token_fingerprint, projects, quota_bytes, bytes_used, admitted,
                 auth_rejections, scope_rejections, rate_rejections, body_rejections,
                 quota_rejections, last_used, last_reject_reason, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, 0, 0, 0, 0, 0, ?5, NULL, ?5)
             ON CONFLICT(token_fingerprint) DO UPDATE SET
                projects = excluded.projects,
                quota_bytes = excluded.quota_bytes,
                bytes_used = bytes_used + excluded.bytes_used,
                admitted = admitted + 1,
                last_used = excluded.last_used,
                updated_at = excluded.updated_at",
            params![fingerprint, projects_text, quota, bytes, now],
        )
        .map_err(|e| StorageError::Query(format!("Failed to record admission use: {e}")))?;
        Ok(())
    }

    /// Record one rejection with its cause.
    pub fn record_rejection(
        conn: &Connection,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        reason: &str,
    ) -> Result<(), StorageError> {
        let now = current_timestamp();
        let projects_text = Self::projects_text(projects);
        let quota = quota_bytes.map(|q| q.min(i64::MAX as u64) as i64);
        let (auth, scope, rate, body, quota_count) = match reason {
            "auth" => (1, 0, 0, 0, 0),
            "scope" => (0, 1, 0, 0, 0),
            "rate" => (0, 0, 1, 0, 0),
            "body" => (0, 0, 0, 1, 0),
            "quota" => (0, 0, 0, 0, 1),
            _ => (0, 0, 0, 0, 0),
        };
        conn.execute(
            "INSERT INTO admission_audit
                (token_fingerprint, projects, quota_bytes, bytes_used, admitted,
                 auth_rejections, scope_rejections, rate_rejections, body_rejections,
                 quota_rejections, last_used, last_reject_reason, updated_at)
             VALUES (?1, ?2, ?3, 0, 0, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?9)
             ON CONFLICT(token_fingerprint) DO UPDATE SET
                projects = excluded.projects,
                quota_bytes = excluded.quota_bytes,
                auth_rejections = auth_rejections + excluded.auth_rejections,
                scope_rejections = scope_rejections + excluded.scope_rejections,
                rate_rejections = rate_rejections + excluded.rate_rejections,
                body_rejections = body_rejections + excluded.body_rejections,
                quota_rejections = quota_rejections + excluded.quota_rejections,
                last_used = excluded.last_used,
                last_reject_reason = excluded.last_reject_reason,
                updated_at = excluded.updated_at",
            params![
                fingerprint,
                projects_text,
                quota,
                auth,
                scope,
                rate,
                body,
                quota_count,
                now,
                reason
            ],
        )
        .map_err(|e| StorageError::Query(format!("Failed to record admission rejection: {e}")))?;
        Ok(())
    }
}
