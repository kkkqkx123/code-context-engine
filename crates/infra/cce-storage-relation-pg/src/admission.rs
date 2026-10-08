use cce_types::StorageError;

use cce_storage_common::relation::AdmissionAuditRecord;

use crate::error::classify_pg;
use crate::rows::admission_from_row;

use super::PostgresClient;

impl PostgresClient {
    pub(crate) async fn record_admitted(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        bytes: u64,
    ) -> Result<(), StorageError> {
        let fingerprint = fingerprint.to_string();
        let projects = projects.to_vec();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().timestamp();
            let projects_text = projects
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let quota = quota_bytes.map(|q| q.min(i64::MAX as u64) as i64);
            let bytes = bytes.min(i64::MAX as u64) as i64;
            client
                .execute(
                    "INSERT INTO admission_audit (token_fingerprint, projects, quota_bytes, \
                     bytes_used, admitted, auth_rejections, scope_rejections, rate_rejections, \
                     body_rejections, quota_rejections, last_used, last_reject_reason, updated_at) \
                     VALUES ($1, $2, $3, $4, 1, 0, 0, 0, 0, 0, $5, NULL, $5) \
                     ON CONFLICT (token_fingerprint) DO UPDATE SET \
                        projects = EXCLUDED.projects, quota_bytes = EXCLUDED.quota_bytes, \
                        bytes_used = admission_audit.bytes_used + EXCLUDED.bytes_used, \
                        admitted = admission_audit.admitted + 1, \
                        last_used = EXCLUDED.last_used, updated_at = EXCLUDED.updated_at",
                    &[&fingerprint, &projects_text, &quota, &bytes, &now],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn record_rejection(
        &self,
        fingerprint: &str,
        projects: &[i64],
        quota_bytes: Option<u64>,
        reason: &str,
    ) -> Result<(), StorageError> {
        let fingerprint = fingerprint.to_string();
        let projects = projects.to_vec();
        let reason = reason.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let now = chrono::Utc::now().timestamp();
            let projects_text = projects
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let quota = quota_bytes.map(|q| q.min(i64::MAX as u64) as i64);
            let (auth, scope, rate, body, quota_count) = match reason.as_str() {
                "auth" => (1, 0, 0, 0, 0),
                "scope" => (0, 1, 0, 0, 0),
                "rate" => (0, 0, 1, 0, 0),
                "body" => (0, 0, 0, 1, 0),
                "quota" => (0, 0, 0, 0, 1),
                _ => (0, 0, 0, 0, 0),
            };
            client
                .execute(
                    "INSERT INTO admission_audit (token_fingerprint, projects, quota_bytes, \
                     bytes_used, admitted, auth_rejections, scope_rejections, rate_rejections, \
                     body_rejections, quota_rejections, last_used, last_reject_reason, \
                     updated_at) \
                     VALUES ($1, $2, $3, 0, 0, $4, $5, $6, $7, $8, $9, $10, $9) \
                     ON CONFLICT (token_fingerprint) DO UPDATE SET \
                        projects = EXCLUDED.projects, quota_bytes = EXCLUDED.quota_bytes, \
                        auth_rejections = admission_audit.auth_rejections \
                            + EXCLUDED.auth_rejections, \
                        scope_rejections = admission_audit.scope_rejections \
                            + EXCLUDED.scope_rejections, \
                        rate_rejections = admission_audit.rate_rejections \
                            + EXCLUDED.rate_rejections, \
                        body_rejections = admission_audit.body_rejections \
                            + EXCLUDED.body_rejections, \
                        quota_rejections = admission_audit.quota_rejections \
                            + EXCLUDED.quota_rejections, \
                        last_used = EXCLUDED.last_used, \
                        last_reject_reason = EXCLUDED.last_reject_reason, \
                        updated_at = EXCLUDED.updated_at",
                    &[
                        &fingerprint,
                        &projects_text,
                        &quota,
                        &auth,
                        &scope,
                        &rate,
                        &body,
                        &quota_count,
                        &now,
                        &reason,
                    ],
                )
                .await
                .map_err(classify_pg)?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn get(
        &self,
        fingerprint: &str,
    ) -> Result<Option<AdmissionAuditRecord>, StorageError> {
        let fingerprint = fingerprint.to_string();
        self.run(async move {
            let client = self.pooled().await?;
            let row = client
                .query_opt(
                    "SELECT token_fingerprint, projects, quota_bytes, bytes_used, admitted, \
                     auth_rejections, scope_rejections, rate_rejections, body_rejections, \
                     quota_rejections, last_used, last_reject_reason FROM admission_audit \
                     WHERE token_fingerprint = $1",
                    &[&fingerprint],
                )
                .await
                .map_err(classify_pg)?;
            row.map(|row| admission_from_row(&row)).transpose()
        })
        .await
    }

    pub(crate) async fn list(&self) -> Result<Vec<AdmissionAuditRecord>, StorageError> {
        self.run(async move {
            let client = self.pooled().await?;
            let rows = client
                .query(
                    "SELECT token_fingerprint, projects, quota_bytes, bytes_used, admitted, \
                     auth_rejections, scope_rejections, rate_rejections, body_rejections, \
                     quota_rejections, last_used, last_reject_reason FROM admission_audit \
                     ORDER BY token_fingerprint",
                    &[],
                )
                .await
                .map_err(classify_pg)?;
            rows.iter().map(admission_from_row).collect()
        })
        .await
    }
}
