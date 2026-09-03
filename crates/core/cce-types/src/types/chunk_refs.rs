//! Chunk-to-entity reference codec
//!
//! A single chunk may cover multiple entities, and the same association is
//! persisted in three backends with different physical encodings:
//!
//! - BM25/tantivy stored field: comma-separated ID list (`entity_id` field,
//!   one tantivy value per ID) plus a `segment_id` string field;
//! - SQLite `chunks` table: JSON array columns (`entity_ids` / `entity_names`);
//! - Qdrant payload: native repeated integer list.
//!
//! [`ChunkEntityRefs`] is the single in-memory representation and the only
//! place that knows about the wire formats, so the three backends cannot
//! drift apart. Entity IDs are numeric, which keeps every format
//! comma-safe by construction; parsing still goes through this module so any
//! future format change stays local.

use serde::{Deserialize, Serialize};

use super::entity::EntityId;

/// The set of entities covered by one chunk plus its cross-path alignment key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkEntityRefs {
    /// All entities whose content is covered by the chunk (may be empty for
    /// document/plain-text chunks).
    pub entity_ids: Vec<EntityId>,
    /// Logical segment identity shared by the embedding-family and BM25-family
    /// chunks of the same source group; used as the fusion alignment key when
    /// no entity is available.
    pub segment_id: String,
}

impl ChunkEntityRefs {
    /// Build refs from chunk metadata.
    pub fn new(entity_ids: Vec<EntityId>, segment_id: impl Into<String>) -> Self {
        Self {
            entity_ids,
            segment_id: segment_id.into(),
        }
    }

    /// Encode the entity list for the BM25 stored field.
    ///
    /// The value round-trips through [`Self::parse_bm25_csv`].
    pub fn to_bm25_csv(&self) -> String {
        self.entity_ids
            .iter()
            .map(|id| id.0.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Decode the entity list from a BM25 stored-field value.
    ///
    /// Accepts both the multi-value joined form and legacy single-ID values;
    /// non-numeric fragments are skipped instead of failing the whole list.
    pub fn parse_bm25_csv(value: &str) -> Vec<EntityId> {
        value
            .split(',')
            .filter_map(|id| id.trim().parse::<u64>().ok())
            .map(EntityId)
            .collect()
    }

    /// Encode the entity list for the SQLite `chunks.entity_ids` column.
    pub fn to_sql_json(&self) -> String {
        let ids: Vec<u64> = self.entity_ids.iter().map(|id| id.0).collect();
        serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string())
    }

    /// Decode the entity list from the SQLite `chunks.entity_ids` column.
    pub fn parse_sql_json(value: &str) -> Vec<EntityId> {
        serde_json::from_str::<Vec<i64>>(value)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| u64::try_from(id).ok())
            .map(EntityId)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_csv_round_trips_multiple_ids() {
        let refs = ChunkEntityRefs::new(vec![EntityId(10), EntityId(20), EntityId(30)], "g1");
        assert_eq!(refs.to_bm25_csv(), "10,20,30");
        assert_eq!(
            ChunkEntityRefs::parse_bm25_csv("10,20,30"),
            vec![EntityId(10), EntityId(20), EntityId(30)]
        );
    }

    #[test]
    fn bm25_csv_handles_legacy_single_value_and_whitespace() {
        assert_eq!(ChunkEntityRefs::parse_bm25_csv("42"), vec![EntityId(42)]);
        assert_eq!(
            ChunkEntityRefs::parse_bm25_csv(" 10 , 20 , 30 "),
            vec![EntityId(10), EntityId(20), EntityId(30)]
        );
        assert!(ChunkEntityRefs::parse_bm25_csv("").is_empty());
    }

    #[test]
    fn sql_json_round_trips_and_rejects_negative_ids() {
        let refs = ChunkEntityRefs::new(vec![EntityId(7), EntityId(9)], "g2");
        assert_eq!(refs.to_sql_json(), "[7,9]");
        assert_eq!(ChunkEntityRefs::parse_sql_json("[7,9]"), refs.entity_ids);
        assert!(ChunkEntityRefs::parse_sql_json("[-1]").is_empty());
        assert!(ChunkEntityRefs::parse_sql_json("not json").is_empty());
    }
}
