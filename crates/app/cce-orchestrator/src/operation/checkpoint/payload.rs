use crate::hot_update::FileChangeType;
use cce_types::{ParsedFile, StorageError};
use serde::{Deserialize, Serialize};

use super::CheckpointManager;

/// Schema version for parsed checkpoint payloads.
///
/// The project is in the development stage and treats checkpoint schema as an
/// internal iteration concern. Version 3 introduced the explicit
/// [`cce_types::INDEX_FORMAT_VERSION`] record on the envelope; payloads
/// written before the change fail to decode (missing required field) or the
/// compatibility check, and recovery falls back to re-parsing the source file.
pub const PARSED_CHECKPOINT_SCHEMA_VERSION: u32 = 3;

/// Schema version for summary checkpoint payloads.
pub const SUMMARY_CHECKPOINT_SCHEMA_VERSION: u32 = 2;

/// Tagged payload stored in the `checkpoint_file.parsed_data` blob.
///
/// The two record kinds were previously multiplexed onto one struct via an
/// optional parse result; the tombstone is now explicit so a deleted entry
/// can never carry parse data, and every consumer of
/// [`ParsedCheckpointEnvelope`] can rely on its parse result being present.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParsedCheckpointPayload {
    /// A successfully parsed file. Boxed because the tombstone variant makes
    /// the payload size otherwise dominated by the envelope.
    Parsed(Box<ParsedCheckpointEnvelope>),
    /// Deletion marker for a removed file. The path identity lives on the
    /// checkpoint row (`file_path`), so the variant carries no data.
    Deleted,
}

impl ParsedCheckpointPayload {
    /// Whether this payload can be adopted by recovery. Tombstones carry no
    /// versioned data; envelopes delegate to their own compatibility check.
    pub fn is_compatible(&self) -> bool {
        match self {
            Self::Parsed(envelope) => envelope.is_compatible(),
            Self::Deleted => true,
        }
    }
}

/// Envelope for parsed file data stored in checkpoint
///
/// Wraps `ParsedFile` with schema/parser version metadata to enable
/// compatibility checks during recovery. Used by both full-index and
/// hot-update paths. The hot-update event kind (`change_type`) drives
/// resume-side Added/Modified handling; it is never `Deleted` — deletions use
/// [`ParsedCheckpointPayload::Deleted`]. Generated summaries are stored next
/// to this envelope in their own record column (see
/// [`SummaryCheckpointPayload`]), not inside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedCheckpointEnvelope {
    pub schema_version: u32,
    pub parser_version: u32,
    pub path_normalization_version: u32,
    /// Index format version the payload was written with. Entries whose
    /// recorded version differs from [`cce_types::INDEX_FORMAT_VERSION`]
    /// (classification encoding changed) are invalid and re-parsed.
    pub index_format_version: u32,
    /// Fingerprint of the plugin-language registry — recorded only when the
    /// parsed file references a [`cce_types::Language::Custom`] index.
    /// `None` entries stay valid regardless of plugin registration changes;
    /// `Some(fp)` entries are invalid as soon as the current fingerprint
    /// differs (plugin added/removed/reordered), because the persisted custom
    /// index may dangle.
    pub plugin_language_fingerprint: Option<String>,
    pub change_type: FileChangeType,
    pub parsed_file: ParsedFile,
}

impl ParsedCheckpointEnvelope {
    pub fn new(change_type: FileChangeType, parsed_file: ParsedFile) -> Self {
        let plugin_language_fingerprint = match parsed_file.language {
            cce_types::Language::Custom(_) => {
                Some(cce_types::language::plugin_language_fingerprint())
            }
            _ => None,
        };
        Self {
            schema_version: PARSED_CHECKPOINT_SCHEMA_VERSION,
            parser_version: cce_types::RELATION_PARSER_VERSION,
            path_normalization_version: cce_types::RELATION_PATH_NORMALIZATION_VERSION,
            index_format_version: cce_types::INDEX_FORMAT_VERSION,
            plugin_language_fingerprint,
            change_type,
            parsed_file,
        }
    }

    pub fn is_compatible(&self) -> bool {
        self.schema_version == PARSED_CHECKPOINT_SCHEMA_VERSION
            && self.parser_version == cce_types::RELATION_PARSER_VERSION
            && self.path_normalization_version == cce_types::RELATION_PATH_NORMALIZATION_VERSION
            && self.index_format_version == cce_types::INDEX_FORMAT_VERSION
            && self
                .plugin_language_fingerprint
                .as_ref()
                .map(|fp| fp == &cce_types::language::plugin_language_fingerprint())
                .unwrap_or(true)
    }
}

/// Pre-generated summary persisted next to the parse checkpoint so recovery
/// can reuse it without invoking the LLM-based summary generator again.
///
/// Stored in its own record column (`summary_data`) instead of inside the
/// parsed envelope: summaries are produced after parsing, and a dedicated
/// small payload lets the summary pass avoid re-encoding (and re-compressing)
/// the source-bearing parse blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryCheckpointPayload {
    pub schema_version: u32,
    /// Index format version recorded at write time; see
    /// [`ParsedCheckpointEnvelope::index_format_version`].
    pub index_format_version: u32,
    /// Plugin-language fingerprint for files whose content references a
    /// custom language (see [`ParsedCheckpointEnvelope`]); `None` entries
    /// stay valid regardless of plugin registration changes.
    pub plugin_language_fingerprint: Option<String>,
    pub file_summary: cce_parser::summary::FileSummary,
    /// Fingerprint of the summary configuration that produced `file_summary`.
    /// Recovery regenerates the summary when this no longer matches the
    /// current configuration.
    pub summary_config_fingerprint: Option<String>,
}

impl SummaryCheckpointPayload {
    pub fn new(
        file_summary: cce_parser::summary::FileSummary,
        plugin_language_fingerprint: Option<String>,
        summary_config_fingerprint: Option<String>,
    ) -> Self {
        Self {
            schema_version: SUMMARY_CHECKPOINT_SCHEMA_VERSION,
            index_format_version: cce_types::INDEX_FORMAT_VERSION,
            plugin_language_fingerprint,
            file_summary,
            summary_config_fingerprint,
        }
    }

    pub fn is_compatible(&self) -> bool {
        self.schema_version == SUMMARY_CHECKPOINT_SCHEMA_VERSION
            && self.index_format_version == cce_types::INDEX_FORMAT_VERSION
            && self
                .plugin_language_fingerprint
                .as_ref()
                .map(|fp| fp == &cce_types::language::plugin_language_fingerprint())
                .unwrap_or(true)
    }
}

/// Plugin-language fingerprint to record for a parsed file.
///
/// `Some(current)` only when the file references a
/// [`cce_types::Language::Custom`] index — files without custom
/// languages are immune to plugin-registration changes and record `None`.
pub(crate) fn plugin_fingerprint_for(language: cce_types::Language) -> Option<String> {
    match language {
        cce_types::Language::Custom(_) => Some(cce_types::language::plugin_language_fingerprint()),
        _ => None,
    }
}

/// Single-byte encoding for deletion tombstones.
///
/// Tombstones carry no data, so routing them through JSON serialization and
/// zstd compression only burns cycles on the hottest checkpoint path.
/// Compressed payloads always start with the zstd magic bytes, so this
/// marker can never collide with a real parse blob. Decoding recognizes the
/// marker first and falls back to the legacy compressed format, keeping rows
/// written before this change readable.
const TOMBSTONE_MARKER: u8 = 0x00;

/// Encode a parsed checkpoint payload for durable storage.
///
/// The JSON payload (which embeds the full file source) is zstd-compressed;
/// uncompressed checkpoints would dominate the `checkpoint_file` volume.
/// Deletion tombstones use the single-byte marker instead.
pub fn encode_parsed_checkpoint(
    payload: &ParsedCheckpointPayload,
) -> Result<Vec<u8>, StorageError> {
    if matches!(payload, ParsedCheckpointPayload::Deleted) {
        return Ok(vec![TOMBSTONE_MARKER]);
    }
    let json = serde_json::to_vec(payload).map_err(|error| {
        StorageError::query(format!("Failed to serialize parsed checkpoint: {error}"))
    })?;
    zstd::encode_all(&*json, 3).map_err(|error| {
        StorageError::query(format!("Failed to compress parsed checkpoint: {error}"))
    })
}

/// Decode a stored parsed checkpoint payload.
///
/// Returns `None` when the payload cannot be decoded; callers treat this as
/// a missing checkpoint and fall back to re-parsing the source file.
pub fn decode_parsed_checkpoint(bytes: &[u8]) -> Option<ParsedCheckpointPayload> {
    if bytes == [TOMBSTONE_MARKER] {
        return Some(ParsedCheckpointPayload::Deleted);
    }
    let json = zstd::decode_all(bytes).ok()?;
    serde_json::from_slice(&json).ok()
}

/// Encode a summary checkpoint payload (same compression contract as
/// [`encode_parsed_checkpoint`]).
pub fn encode_summary_checkpoint(
    payload: &SummaryCheckpointPayload,
) -> Result<Vec<u8>, StorageError> {
    let json = serde_json::to_vec(payload).map_err(|error| {
        StorageError::query(format!("Failed to serialize summary checkpoint: {error}"))
    })?;
    zstd::encode_all(&*json, 3).map_err(|error| {
        StorageError::query(format!("Failed to compress summary checkpoint: {error}"))
    })
}

/// Decode a stored summary checkpoint payload.
///
/// Returns `None` when the payload cannot be decoded or was written by an
/// incompatible schema version; callers regenerate the summary in that case.
pub fn decode_summary_checkpoint(bytes: &[u8]) -> Option<SummaryCheckpointPayload> {
    let json = zstd::decode_all(bytes).ok()?;
    serde_json::from_slice(&json)
        .ok()
        .filter(SummaryCheckpointPayload::is_compatible)
}

/// Persist generated file summaries into the batch's file checkpoints.
///
/// Summaries are generated after parsing, so they are written to the
/// dedicated `summary_data` column instead of rewriting the parse blob. The
/// upsert preserves the existing parse checkpoint (COALESCE semantics), so
/// this pass only ever attaches summary payloads. Shared by the full-index
/// and hot-update paths.
pub async fn persist_summaries_to_checkpoints(
    cm: &CheckpointManager,
    operation_id: &str,
    batch_index: u32,
    entries: &[(String, cce_parser::summary::FileSummary, Option<String>)],
    summary_config_fingerprint: Option<String>,
) -> Result<(), StorageError> {
    for (path, summary, plugin_fingerprint) in entries {
        let mut checkpoint = cm
            .create_file_checkpoint(operation_id, batch_index, path)
            .await?;
        let payload = SummaryCheckpointPayload::new(
            summary.clone(),
            plugin_fingerprint.clone(),
            summary_config_fingerprint.clone(),
        );
        checkpoint.summary_data = Some(encode_summary_checkpoint(&payload)?);
        checkpoint.updated_at = chrono::Utc::now().to_rfc3339();
        cm.save_file_checkpoint(&checkpoint).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsed_payload_roundtrip_preserves_envelope() {
        let parsed = cce_types::ParsedFile::default();
        let payload = ParsedCheckpointPayload::Parsed(Box::new(ParsedCheckpointEnvelope::new(
            FileChangeType::Modified,
            parsed,
        )));

        let encoded = encode_parsed_checkpoint(&payload).expect("payload encodes");
        let back = decode_parsed_checkpoint(&encoded).expect("payload decodes");

        assert!(back.is_compatible());
        match back {
            ParsedCheckpointPayload::Parsed(envelope) => {
                assert_eq!(envelope.change_type, FileChangeType::Modified);
                assert_eq!(envelope.schema_version, PARSED_CHECKPOINT_SCHEMA_VERSION);
            }
            ParsedCheckpointPayload::Deleted => panic!("variant must survive roundtrip"),
        }
    }

    #[test]
    fn deleted_payload_roundtrip() {
        let encoded =
            encode_parsed_checkpoint(&ParsedCheckpointPayload::Deleted).expect("payload encodes");
        let back = decode_parsed_checkpoint(&encoded).expect("payload decodes");

        assert!(matches!(back, ParsedCheckpointPayload::Deleted));
        assert!(back.is_compatible(), "tombstones carry no versioned data");
    }

    /// Files without custom languages record no plugin fingerprint and stay
    /// valid regardless of registry changes; custom-language files pin the
    /// current fingerprint and invalidate once it drifts.
    #[test]
    fn plugin_fingerprint_gates_custom_language_envelopes() {
        let plain = ParsedCheckpointEnvelope::new(
            FileChangeType::Modified,
            cce_types::ParsedFile {
                language: cce_types::Language::Rust,
                ..Default::default()
            },
        );
        assert!(plain.plugin_language_fingerprint.is_none());
        assert!(plain.is_compatible());

        let custom = ParsedCheckpointEnvelope::new(
            FileChangeType::Modified,
            cce_types::ParsedFile {
                language: cce_types::Language::Custom(0),
                ..Default::default()
            },
        );
        assert!(custom.plugin_language_fingerprint.is_some());
        assert!(custom.is_compatible());

        // Simulate a registry change between write and read.
        let mut stale = custom;
        stale.plugin_language_fingerprint = Some("drifted-fingerprint".to_string());
        assert!(!stale.is_compatible());
    }

    #[test]
    fn summary_payload_roundtrip_preserves_fingerprint() {
        let summary = cce_parser::summary::FileSummary::new("src/lib.rs")
            .with_summary("Main entry point")
            .with_entities(vec!["run".to_string()]);
        let payload = SummaryCheckpointPayload::new(summary, None, Some("fp-1".to_string()));

        let encoded = encode_summary_checkpoint(&payload).expect("summary encodes");
        let mut back = decode_summary_checkpoint(&encoded).expect("summary decodes");
        assert_eq!(back.file_summary.summary_text, "Main entry point");
        assert_eq!(back.file_summary.main_entities, vec!["run".to_string()]);
        assert_eq!(back.summary_config_fingerprint.as_deref(), Some("fp-1"));

        // An incompatible schema version must make recovery regenerate.
        back.schema_version = 999;
        let tampered = encode_summary_checkpoint(&back).expect("tampered payload encodes");
        assert!(
            decode_summary_checkpoint(&tampered).is_none(),
            "incompatible summary payloads must be rejected"
        );
    }

    #[test]
    fn parsed_payload_compresses_source_bearing_data() {
        let parsed = cce_types::ParsedFile {
            source: "fn example() {}".repeat(512).into(),
            ..Default::default()
        };
        let payload = ParsedCheckpointPayload::Parsed(Box::new(ParsedCheckpointEnvelope::new(
            FileChangeType::Modified,
            parsed,
        )));

        let json_len = serde_json::to_vec(&payload)
            .expect("payload serializes")
            .len();
        let encoded = encode_parsed_checkpoint(&payload).expect("payload encodes");
        assert!(
            encoded.len() < json_len / 2,
            "compressed payload ({}) must be far smaller than raw JSON ({})",
            encoded.len(),
            json_len
        );
    }

    #[test]
    fn decode_rejects_undecodable_payloads() {
        assert!(decode_parsed_checkpoint(b"not a zstd stream").is_none());
        assert!(decode_parsed_checkpoint(&[0xff, 0xff, 0xff]).is_none());
    }
}
