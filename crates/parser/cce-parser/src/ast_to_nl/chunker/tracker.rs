//! Group relation tracking
//!
//! Tracks relationships between entity groups for cross-group navigation.

use compact_str::CompactString;
use std::collections::HashMap;

use crate::grouper::EntityGroup;

use super::result::{GroupRelation, GroupRelationType};

/// Group relation tracker
pub struct GroupTracker {
    /// Group processing sequence
    group_sequence: Vec<CompactString>,
    /// Relation graph between groups
    relation_graph: HashMap<CompactString, Vec<GroupRelation>>,
    /// Current group index
    current_index: usize,
}

impl GroupTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self {
            group_sequence: Vec::new(),
            relation_graph: HashMap::new(),
            current_index: 0,
        }
    }

    /// Record group processing
    pub fn record_group(&mut self, group: &EntityGroup) {
        let group_id = group.group_id.clone();

        // Establish relation with previous group
        if let Some(prev_id) = self.group_sequence.last().cloned() {
            self.add_relation(
                prev_id.clone(),
                group_id.clone(),
                GroupRelationType::Successor,
            );
            self.add_relation(group_id.clone(), prev_id, GroupRelationType::Predecessor);
        }

        self.group_sequence.push(group_id);
        self.current_index += 1;
    }

    /// Add relation between groups
    pub fn add_relation(
        &mut self,
        from: CompactString,
        to: CompactString,
        relation_type: GroupRelationType,
    ) {
        let relation = GroupRelation {
            group_id: to.to_string(),
            relation_type,
            strength: 1.0,
        };

        self.relation_graph.entry(from).or_default().push(relation);
    }

    /// Get related groups for a group ID
    pub fn get_related_groups(&self, group_id: &str) -> Vec<GroupRelation> {
        let mut relations = Vec::new();

        // Get from relation graph
        if let Some(graph_relations) = self.relation_graph.get(group_id) {
            relations.extend(graph_relations.iter().cloned());
        }

        // Get from sequence (predecessor/successor)
        if let Some(pos) = self.group_sequence.iter().position(|id| id == group_id) {
            // Predecessor
            if pos > 0 {
                if let Some(prev_id) = self.group_sequence.get(pos - 1) {
                    if !relations
                        .iter()
                        .any(|r| r.group_id.as_str() == prev_id.as_str())
                    {
                        relations.push(GroupRelation {
                            group_id: prev_id.to_string(),
                            relation_type: GroupRelationType::Predecessor,
                            strength: 0.8,
                        });
                    }
                }
            }

            // Successor
            if pos + 1 < self.group_sequence.len() {
                if let Some(next_id) = self.group_sequence.get(pos + 1) {
                    if !relations
                        .iter()
                        .any(|r| r.group_id.as_str() == next_id.as_str())
                    {
                        relations.push(GroupRelation {
                            group_id: next_id.to_string(),
                            relation_type: GroupRelationType::Successor,
                            strength: 0.8,
                        });
                    }
                }
            }
        }

        relations
    }

    /// Get current group ID
    pub fn current_group_id(&self) -> Option<&CompactString> {
        if self.current_index > 0 {
            self.group_sequence.get(self.current_index - 1)
        } else {
            None
        }
    }

    /// Get previous group ID
    pub fn prev_group_id(&self) -> Option<&CompactString> {
        if self.current_index > 1 {
            self.group_sequence.get(self.current_index - 2)
        } else {
            None
        }
    }

    /// Reset tracker
    pub fn reset(&mut self) {
        self.group_sequence.clear();
        self.relation_graph.clear();
        self.current_index = 0;
    }

    /// Get total tracked groups
    pub fn tracked_count(&self) -> usize {
        self.group_sequence.len()
    }
}

impl Default for GroupTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::{EntityGroup, GroupType};
    use cce_types::entity::{EntityId, EntityKind, GroupedEntity};
    use cce_types::language::Language;
    use std::collections::HashMap;

    fn create_test_group(id: &str) -> EntityGroup {
        use compact_str::CompactString;
        use smallvec::SmallVec;

        EntityGroup {
            group_id: CompactString::from(id),
            group_type: GroupType::Standalone,
            header: Some(GroupedEntity::new(
                EntityId(0),
                EntityKind::Function,
                "test".to_string(),
                "fn test()".to_string(),
            )),
            header_id: Some(EntityId(0)),
            members: SmallVec::new(),
            member_ids: SmallVec::new(),
            entity_spans: HashMap::new(),
            combined_source: Some(std::sync::Arc::from("test")),
            combined_source_lazy: std::sync::OnceLock::new(),

            span: cce_types::Span::default(),
            kind: EntityKind::Function,
            name: CompactString::from("test"),
            language: Language::Rust,
            pattern_info: crate::grouper::types::PatternInfo::None,
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        }
    }

    #[test]
    fn test_record_group() {
        let mut tracker = GroupTracker::new();
        let group1 = create_test_group("group_1");
        let group2 = create_test_group("group_2");

        tracker.record_group(&group1);
        assert_eq!(tracker.tracked_count(), 1);

        tracker.record_group(&group2);
        assert_eq!(tracker.tracked_count(), 2);

        let relations = tracker.get_related_groups("group_1");
        assert!(relations.iter().any(|r| r.group_id == "group_2"));
    }

    #[test]
    fn test_predecessor_successor() {
        let mut tracker = GroupTracker::new();
        let group1 = create_test_group("group_1");
        let group2 = create_test_group("group_2");
        let group3 = create_test_group("group_3");

        tracker.record_group(&group1);
        tracker.record_group(&group2);
        tracker.record_group(&group3);

        let relations = tracker.get_related_groups("group_2");
        assert!(relations.iter().any(|r| {
            r.group_id == "group_1" && r.relation_type == GroupRelationType::Predecessor
        }));
        assert!(relations.iter().any(|r| {
            r.group_id == "group_3" && r.relation_type == GroupRelationType::Successor
        }));
    }

    #[test]
    fn test_reset() {
        let mut tracker = GroupTracker::new();
        let group = create_test_group("group_1");

        tracker.record_group(&group);
        assert_eq!(tracker.tracked_count(), 1);

        tracker.reset();
        assert_eq!(tracker.tracked_count(), 0);
    }
}
