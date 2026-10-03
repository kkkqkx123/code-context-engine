//! Cross-path alignment key derivation for hybrid fusion.
//!
//! Single source of truth for the alignment key format shared by the
//! orchestrator's fusion pipeline and the offline benchmark/e2e mirror: code
//! chunks align on `e:{entity_id}`, document/plain-text chunks on
//! `s:{segment_id}`, and the raw chunk id is the final `c:{chunk_id}`
//! fallback so unkeyed results survive as individual single-path entries.

use super::entity::EntityId;

/// Alignment key for a single entity: `e:{entity_id}`.
pub fn entity_alignment_key(entity_id: &EntityId) -> String {
    format!("e:{}", entity_id.0)
}

/// Derive the cross-path alignment key for a search result or chunk.
///
/// Priority: the first entity in `entity_ids` for code chunks, `segment_id`
/// for document/plain-text chunks, chunk id as the final fallback. Returns
/// `None` only when every key source is empty.
pub fn alignment_key(
    entity_ids: &[EntityId],
    segment_id: Option<&str>,
    chunk_id: &str,
) -> Option<String> {
    match entity_ids.first() {
        Some(eid) => Some(entity_alignment_key(eid)),
        None => segment_id
            .filter(|s| !s.is_empty())
            .map(|s| format!("s:{}", s))
            .or_else(|| {
                if chunk_id.is_empty() {
                    None
                } else {
                    Some(format!("c:{}", chunk_id))
                }
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_key_takes_priority() {
        let key = alignment_key(&[EntityId(1)], Some("seg"), "chunk");
        assert_eq!(key.as_deref(), Some("e:1"));
    }

    #[test]
    fn segment_key_when_no_entity() {
        let key = alignment_key(&[], Some("seg"), "chunk");
        assert_eq!(key.as_deref(), Some("s:seg"));
    }

    #[test]
    fn empty_segment_falls_back_to_chunk_id() {
        let key = alignment_key(&[], Some(""), "chunk");
        assert_eq!(key.as_deref(), Some("c:chunk"));
    }

    #[test]
    fn none_when_all_sources_empty() {
        assert!(alignment_key(&[], Some(""), "").is_none());
        assert!(alignment_key(&[], None, "").is_none());
    }

    #[test]
    fn multi_entity_list_uses_first() {
        let key = alignment_key(&[EntityId(7), EntityId(8)], Some("seg"), "chunk");
        assert_eq!(key.as_deref(), Some("e:7"));
    }
}
