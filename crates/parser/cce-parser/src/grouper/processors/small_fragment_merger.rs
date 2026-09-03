use std::collections::HashMap;

use compact_str::CompactString;
use smallvec::SmallVec;

use crate::grouper::types::EntityGroup;
use cce_types::entity::{EntityId, EntityKind, GroupedEntity};

/// Small fragment merger
///
/// Merges adjacent standalone groups that are below configured size thresholds
/// into larger groups to reduce fragmentation. Operates on existing groups
/// after all other processors have run.
///
/// # Merge Strategy
///
/// 1. Collect all standalone groups sorted by source position
/// 2. Greedily merge fragments within proximity threshold until size threshold is met
/// 3. Create MergedFragments groups from merged fragments
/// 4. Replace original standalone groups with merged groups
pub struct SmallFragmentMerger {
    min_tokens: usize,
    min_words: usize,
    max_span_lines: usize,
    proximity_bytes: usize,
}

impl Default for SmallFragmentMerger {
    fn default() -> Self {
        Self::new(128, 80, 50, 0)
    }
}

impl SmallFragmentMerger {
    /// Create a new SmallFragmentMerger
    pub fn new(
        min_tokens: usize,
        min_words: usize,
        max_span_lines: usize,
        proximity_bytes: usize,
    ) -> Self {
        Self {
            min_tokens,
            min_words,
            max_span_lines,
            proximity_bytes,
        }
    }

    /// Create with config from NestProcessorConfig
    pub fn with_config(config: &cce_config::NestProcessorConfig) -> Self {
        Self::new(
            config.small_fragment_min_tokens,
            config.small_fragment_min_words,
            config.small_fragment_max_span_lines,
            config.near_merge_proximity_bytes,
        )
    }

    /// Process groups to merge small standalone fragments
    ///
    /// # Arguments
    /// * `groups` - Mutable reference to the list of entity groups
    /// * `file_source` - The full source code of the file (for span calculation)
    ///
    /// # Returns
    /// Number of groups created by merging
    pub fn process(&self, groups: &mut Vec<EntityGroup>, file_source: &str) -> usize {
        // Collect indices of standalone groups with their metadata
        let standalone_indices: Vec<usize> = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.group_type.is_standalone())
            .map(|(i, _)| i)
            .collect();

        if standalone_indices.is_empty() {
            return 0;
        }

        // Determine which standalone groups are "small" and should be candidates for merging
        let mut small_standalone: Vec<(usize, &EntityGroup)> = standalone_indices
            .iter()
            .filter_map(|&idx| {
                let group = &groups[idx];
                if self.is_small_fragment(group, file_source) {
                    Some((idx, group))
                } else {
                    None
                }
            })
            .collect();

        if small_standalone.is_empty() {
            return 0;
        }

        // Sort by source position
        small_standalone.sort_by_key(|(_, g)| g.span.start_byte);

        // Greedy merge adjacent small fragments
        let merged_groups = self.merge_adjacent_fragments(&small_standalone, groups);

        if merged_groups.is_empty() {
            return 0;
        }

        // Build set of indices to remove
        let indices_to_remove: std::collections::HashSet<usize> =
            small_standalone.iter().map(|(idx, _)| *idx).collect();

        // Replace the first standalone group with the first merged group, add rest
        let merge_count = merged_groups.len();

        // Collect indices in sorted order (descending to avoid shift issues)
        let mut sorted_indices: Vec<usize> = indices_to_remove.into_iter().collect();
        sorted_indices.sort_unstable_by(|a, b| b.cmp(a));

        // Remove old standalone groups
        for idx in &sorted_indices {
            groups.remove(*idx);
        }

        // Insert merged groups at the position of the first standalone
        let insert_pos = sorted_indices.last().copied().unwrap_or(0);
        for merged in merged_groups {
            groups.insert(insert_pos, merged);
        }

        merge_count
    }

    /// Check if a group is a small fragment eligible for merging
    fn is_small_fragment(&self, group: &EntityGroup, file_source: &str) -> bool {
        let span = &group.span;

        // Import-like entities are always considered small and should be merged
        // into adjacent code blocks — standalone imports/exports have no semantic
        // retrieval value (symbol search should use BM25/symbol index).
        if let Some(ref header) = group.header {
            if matches!(
                header.kind,
                EntityKind::Import | EntityKind::Require | EntityKind::Include | EntityKind::Export
            ) {
                return true;
            }
        }

        // Zero-variant void enums (e.g., `enum Void {}`) have empty spans
        // and no doc comment — treat them as always small.
        if let Some(ref header) = group.header {
            if matches!(header.kind, EntityKind::Enum)
                && header.doc_comment.is_none()
                && group.members.is_empty()
            {
                return true;
            }
        }

        // Check line count first (uses span, no need for combined_source)
        let line_count = if span.end_position.row > span.start_position.row {
            span.end_position.row - span.start_position.row
        } else {
            1
        };
        if line_count > self.max_span_lines {
            return false;
        }

        // Extract source text from file_source using byte offsets
        let source = if span.end_byte >= span.start_byte && span.end_byte <= file_source.len() {
            &file_source[span.start_byte..span.end_byte]
        } else if let Some(ref s) = group.combined_source {
            s.as_ref()
        } else {
            return false;
        };

        if source.is_empty() {
            return false;
        }

        // Estimate tokens (rough: ~4 chars per token)
        let estimated_tokens = source.len() / 4;
        if estimated_tokens > self.min_tokens {
            return false;
        }

        // Estimate words
        let word_count = source.split_whitespace().filter(|w| !w.is_empty()).count();
        if word_count > self.min_words {
            return false;
        }

        true
    }

    /// Greedily merge small fragments within proximity threshold
    ///
    /// Groups that are adjacent or within `self.proximity_bytes` of each other will be merged,
    /// unless a non-standalone group exists in the gap between them.
    fn merge_adjacent_fragments(
        &self,
        candidates: &[(usize, &EntityGroup)],
        all_groups: &[EntityGroup],
    ) -> Vec<EntityGroup> {
        let mut merged_groups = Vec::new();
        let mut current_batch: Vec<&EntityGroup> = Vec::new();
        let mut prev_end_byte: Option<usize> = None;

        for (_, group) in candidates {
            // Import-like fragments must never merge with non-import fragments.
            // Import-only groups are dropped before chunking (they have no
            // retrieval value), so absorbing a non-import entity into an
            // import batch would silently remove its content from retrieval.
            if let Some(prev) = current_batch.last() {
                if Self::is_import_like_group(prev) != Self::is_import_like_group(group)
                    && !current_batch.is_empty()
                {
                    if let Some(merged) =
                        self.create_merged_group(std::mem::take(&mut current_batch))
                    {
                        merged_groups.push(merged);
                    }
                }
            }

            // Check gap from previous fragment
            if let Some(prev_end) = prev_end_byte {
                let gap = group.span.start_byte.saturating_sub(prev_end);

                if gap > self.proximity_bytes {
                    // Gap exceeds proximity threshold: flush current batch
                    if !current_batch.is_empty() {
                        if let Some(merged) =
                            self.create_merged_group(std::mem::take(&mut current_batch))
                        {
                            merged_groups.push(merged);
                        }
                    }
                } else if gap > 0 {
                    // Small gap within proximity: check for non-standalone barrier
                    let has_non_standalone_between = all_groups.iter().any(|g| {
                        !g.group_type.is_standalone()
                            && g.span.start_byte > prev_end
                            && g.span.end_byte <= group.span.start_byte
                    });
                    if has_non_standalone_between {
                        // Non-standalone group blocks the merge: flush
                        if !current_batch.is_empty() {
                            if let Some(merged) =
                                self.create_merged_group(std::mem::take(&mut current_batch))
                            {
                                merged_groups.push(merged);
                            }
                        }
                    }
                }

                // Check parent scope: fragments in different parent scopes should not
                // be merged even when adjacent. E.g., a top-level function and imports
                // nested inside a test module may be adjacent in source but belong to
                // different logical containers.
                if let Some(prev) = current_batch.last() {
                    if prev.parent_group_id != group.parent_group_id && !current_batch.is_empty() {
                        if let Some(merged) =
                            self.create_merged_group(std::mem::take(&mut current_batch))
                        {
                            merged_groups.push(merged);
                        }
                    }
                }

                // Check test boundary: a test fragment must never be merged
                // into a production fragment group (and vice versa). Merging
                // would mark the production chunk as `Test` and cause the
                // no-test evaluation variant to drop production content.
                if let Some(prev) = current_batch.last() {
                    if prev.test_info.is_test() != group.test_info.is_test()
                        && !current_batch.is_empty()
                    {
                        if let Some(merged) =
                            self.create_merged_group(std::mem::take(&mut current_batch))
                        {
                            merged_groups.push(merged);
                        }
                    }
                }
            }

            current_batch.push(group);
            prev_end_byte = Some(group.span.end_byte);
        }

        // Flush last batch
        if !current_batch.is_empty() {
            if let Some(merged) = self.create_merged_group(current_batch) {
                merged_groups.push(merged);
            }
        }

        merged_groups
    }

    /// Check if a group is import-like (header kind is import/require/include/export).
    fn is_import_like_group(group: &EntityGroup) -> bool {
        group
            .header
            .as_ref()
            .is_some_and(|h| h.kind.is_import_like())
    }

    /// Create a merged group from a batch of fragments
    fn create_merged_group(&self, fragments: Vec<&EntityGroup>) -> Option<EntityGroup> {
        if fragments.is_empty() {
            return None;
        }

        // Single fragment: return a clone to preserve it (it will replace itself)
        if fragments.len() == 1 {
            return Some(fragments[0].clone());
        }

        let first = fragments[0];

        // Use the first fragment's header as the merged group's header,
        // so it gets a group-level conversion instead of being buried as a member.
        let header = first.header.clone();
        let header_id = header.as_ref().map(|h| h.id);

        // Collect all entity IDs and spans
        let mut entity_spans = HashMap::new();
        let mut member_ids = SmallVec::<[EntityId; 8]>::new();
        let mut members = SmallVec::<[GroupedEntity; 4]>::new();

        for (frag_idx, fragment) in fragments.iter().enumerate() {
            // Add header entities to members (skip first fragment's header —
            // it becomes the group header) and collect spans
            if let Some(ref hdr) = fragment.header {
                if frag_idx > 0 {
                    member_ids.push(hdr.id);
                    members.push(hdr.clone());
                }
                if let Some(span) = fragment.entity_spans.get(&hdr.id) {
                    entity_spans.insert(hdr.id, *span);
                }
            }
            // Add existing members
            for member in &fragment.members {
                member_ids.push(member.id);
                members.push(member.clone());
                if let Some(span) = fragment.entity_spans.get(&member.id) {
                    entity_spans.insert(member.id, *span);
                }
            }
        }

        // Sort members by source position
        let mut sorted_members: Vec<(EntityId, GroupedEntity)> =
            members.into_iter().map(|m| (m.id, m)).collect();
        sorted_members
            .sort_by_key(|(id, _)| entity_spans.get(id).map(|s| s.start_byte).unwrap_or(0));

        let sorted_member_ids: SmallVec<[EntityId; 8]> =
            sorted_members.iter().map(|(id, _)| *id).collect();
        let sorted_members_semantic: SmallVec<[GroupedEntity; 4]> =
            sorted_members.into_iter().map(|(_, m)| m).collect();

        // Calculate combined span
        let combined_span = EntityGroup::calculate_combined_span_from_map(&entity_spans);

        // Use the first fragment's name as the group name.
        // Concatenating all fragment names (e.g. "process_user_process_user_name_etc")
        // breaks BM25 lookup by original entity name. The first fragment is the
        // primary entity; additional fragments are merged into it as members.
        let name = first.name.as_str();

        Some(EntityGroup {
            group_id: CompactString::from(format!("merged_{}", first.group_id)),
            group_type: crate::grouper::types::GroupType::MergedFragments,
            header,
            header_id,
            members: sorted_members_semantic,
            member_ids: sorted_member_ids,
            entity_spans,
            combined_source: None,
            combined_source_lazy: std::sync::OnceLock::new(),
            span: combined_span,
            kind: first.kind,
            name: CompactString::from(name),
            language: first.language,
            pattern_info: crate::grouper::types::PatternInfo::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: first.nesting_level,
            parent_group_id: first.parent_group_id.clone(),
            has_significant_nested: false,
            metadata: std::collections::HashMap::new(),
            test_info: cce_types::TestInfo::unknown(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;
    use cce_types::{Position, Span};

    /// Source text long enough to satisfy every test group's byte offsets
    /// (spans use `start * 10` byte positions; rows must be adjacent for the
    /// proximity=0 merger to merge them).
    fn padded_source(base: &str) -> String {
        format!("{}{}", base, " ".repeat(120))
    }

    fn group(kind: EntityKind, name: &str, start: usize, end: usize) -> EntityGroup {
        EntityGroup::from_entity(
            cce_types::entity::Entity::new(
                EntityId(start as u64 + 1),
                kind,
                name.to_string(),
                Span {
                    start_position: Position {
                        row: start,
                        column: 0,
                    },
                    end_position: Position {
                        row: end,
                        column: 0,
                    },
                    start_byte: start * 10,
                    end_byte: end * 10,
                },
            ),
            Language::Rust,
        )
    }

    #[test]
    fn test_import_like_fragments_never_merge_with_non_import() {
        // Imports are separated at the entity level. The merger must
        // never absorb a real entity into an import batch — import-only groups
        // are dropped before chunking, so an absorbed entity would silently
        // disappear from retrieval.
        let merger = SmallFragmentMerger::new(128, 80, 50, 0);
        let mut groups = vec![
            group(EntityKind::Import, "use std::fmt;", 0, 1),
            group(EntityKind::Function, "helper", 1, 3),
        ];

        let merged = merger.process(
            &mut groups,
            &padded_source("use std::fmt;\n\nfn helper() {}"),
        );

        assert_eq!(
            merged, 2,
            "import-like and non-import fragments must be kept in separate batches \
             (got {merged} merged groups, expected 2 single-fragment groups)"
        );
        assert_eq!(
            groups.len(),
            2,
            "both fragments must survive as separate groups"
        );
        assert!(
            groups
                .iter()
                .any(|g| g.kind == EntityKind::Import && g.members.is_empty()),
            "the import fragment must stay an import-only group (dropped later by the pipeline)"
        );
        assert!(
            groups
                .iter()
                .any(|g| g.kind == EntityKind::Function && g.members.is_empty()),
            "the function fragment must stay untouched by the import fragment"
        );
    }

    #[test]
    fn test_import_like_fragments_still_merge_with_each_other() {
        // Adjacent imports may merge into one import-only group.
        // The pipeline drops that group as a whole — no retrieval content is
        // lost because imports carry no retrieval value.
        let merger = SmallFragmentMerger::new(128, 80, 50, 0);
        let mut groups = vec![
            group(EntityKind::Import, "use std::fmt;", 0, 1),
            group(EntityKind::Export, "pub use crate::x;", 1, 2),
        ];

        let merged = merger.process(
            &mut groups,
            &padded_source("use std::fmt;\npub use crate::x;"),
        );

        assert_eq!(
            merged, 1,
            "adjacent import-like fragments should merge into one import-only group"
        );
        assert_eq!(
            groups.len(),
            1,
            "the two import-like fragments must be one group"
        );
        assert!(
            groups[0].members.iter().all(|m| m.kind.is_import_like()),
            "the merged group must contain only import-like entities"
        );
    }

    #[test]
    fn test_non_import_fragments_still_merge_with_each_other() {
        // Regression guard: the import boundary rule must not disable the
        // regular small-fragment merging between real entities.
        let merger = SmallFragmentMerger::new(128, 80, 50, 0);
        let mut groups = vec![
            group(EntityKind::Function, "helper_a", 0, 1),
            group(EntityKind::Function, "helper_b", 1, 2),
        ];

        let merged = merger.process(
            &mut groups,
            &padded_source("fn helper_a() {}\nfn helper_b() {}"),
        );

        assert_eq!(
            merged, 1,
            "regular small fragments must still merge into one group"
        );
        assert_eq!(groups.len(), 1);
    }
}
