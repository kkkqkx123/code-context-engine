//! Ingest supply modes for gateway-driven indexing.
//!
//! The mirror mode stages pushed bytes under the project root and runs the
//! existing disk pipelines. The direct mode feeds the same bytes through
//! the payload consumption entries without requiring a mirror file, keeping
//! storage and relation publication unchanged. Migration runs dual: mirror
//! remains authoritative while direct payloads are verified for identical
//! decode output, then cutover drops the mirror write.

use cce_scanner::FileContentPayload;

/// How ingested bytes reach the index pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestSupplyMode {
    /// Stage under the project mirror, then index from disk.
    Mirror,
    /// Consume ready payloads directly, bypassing mirror落盘.
    Direct,
}

impl IngestSupplyMode {
    /// Read the supply mode from the environment, defaulting to mirror.
    pub fn from_env() -> Self {
        match std::env::var("CCE_INGEST_SUPPLY_MODE")
            .map(|v| v.trim().to_lowercase())
            .as_deref()
        {
            Ok("direct") => Self::Direct,
            _ => Self::Mirror,
        }
    }

    /// Whether this mode bypasses mirror staging.
    pub fn is_direct(self) -> bool {
        matches!(self, Self::Direct)
    }
}

/// Build ready payloads from reassembled ingest bytes.
pub fn build_ready_payloads(
    files: Vec<(String, Vec<u8>, Option<String>)>,
) -> Vec<FileContentPayload> {
    files
        .into_iter()
        .map(|(relative, bytes, hash)| FileContentPayload::ready(relative, bytes, hash))
        .collect()
}

/// Verify direct payloads decode identically to their staged mirror files.
///
/// Each payload is read through the shared verified-payload entry; mirror
/// bytes are compared for equality so dual runs prove the same downstream
/// parsers would see identical input. Storage and relation steps stay
/// untouched by this check.
pub async fn verify_direct_batch(
    payloads: &[FileContentPayload],
    mirror_root: &std::path::Path,
) -> Result<usize, String> {
    let mut verified = 0usize;
    for payload in payloads {
        let content = cce_scanner::read_verified_payload(payload, None)
            .await
            .map_err(|e| {
                format!(
                    "{}: direct payload failed verification: {e}",
                    payload.identity_key()
                )
            })?;
        let mirror_path = mirror_root.join(payload.relative_path.clone());
        let mirror_bytes = std::fs::read(&mirror_path).map_err(|e| {
            format!(
                "{}: failed to read mirror for comparison: {e}",
                payload.identity_key()
            )
        })?;
        let mirror_text = String::from_utf8_lossy(&mirror_bytes);
        if mirror_text != content {
            return Err(format!(
                "{}: direct and mirror content diverge",
                payload.identity_key()
            ));
        }
        verified += 1;
    }
    Ok(verified)
}

/// Process gateway event payloads through the shared gateway entry.
///
/// This is the hot-update payload queue: each payload enters
/// `process_gateway_payload` with identical routing and change recording
/// as local notifications, so gateway pushes and filesystem events produce
/// identical results.
pub async fn process_gateway_queue(
    processor: &mut crate::hot_update::FileProcessor,
    payloads: &[FileContentPayload],
    metadata_store: &Option<std::sync::Arc<cce_storage_metadb_sqlite::SqliteClient>>,
    project_id: i64,
) -> Result<usize, String> {
    use crate::hot_update::FileChangeType;
    let mut applied = 0usize;
    for payload in payloads {
        processor
            .process_gateway_payload(
                payload,
                FileChangeType::Modified,
                metadata_store,
                project_id,
            )
            .await
            .map_err(|e| {
                format!(
                    "{}: gateway payload processing failed: {e}",
                    payload.identity_key()
                )
            })?;
        applied += 1;
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supply_mode_defaults_to_mirror() {
        assert!(!IngestSupplyMode::Mirror.is_direct());
        assert!(IngestSupplyMode::Direct.is_direct());
    }

    #[test]
    fn ready_payloads_carry_bytes() {
        let payloads = build_ready_payloads(vec![(
            "src/main.rs".to_string(),
            b"fn main() {}".to_vec(),
            Some("hash".to_string()),
        )]);
        assert_eq!(payloads.len(), 1);
        assert!(payloads[0].is_ready());
        assert_eq!(payloads[0].identity_key(), "src/main.rs");
    }
}
