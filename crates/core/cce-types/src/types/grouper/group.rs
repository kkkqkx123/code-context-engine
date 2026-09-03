use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::OnceLock;
use thiserror::Error;

use super::pattern::{MemberRole, PatternInfo};
use crate::types::Span;
use crate::types::entity::{
    BehaviorStore, ControlFlowStore, Entity, EntityId, EntityKind, GroupedEntity,
};

pub use super::pattern::MemberRolesBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum GroupType {
    #[default]
    ClassWithMethods,
    InterfaceWithImpls,
    RelatedFunctions,
    Standalone,
    TraitWithImpls,
    ModuleWithContents,
    TestSuiteWithCases,
    CompositePattern,
    ClassWithNestedClasses,
    StructWithNestedStructs,
    FunctionWithLogicalBlocks,
    /// Function with its internal members (macros, closures, statements)
    FunctionWithMembers,
    /// Merged small fragments
    MergedFragments,
    /// File-level documentation comment (module/package documentation)
    FileDocumentation,
}

impl std::fmt::Display for GroupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupType::ClassWithMethods => write!(f, "class_with_methods"),
            GroupType::InterfaceWithImpls => write!(f, "interface_with_impls"),
            GroupType::RelatedFunctions => write!(f, "related_functions"),
            GroupType::Standalone => write!(f, "standalone"),
            GroupType::TraitWithImpls => write!(f, "trait_with_impls"),
            GroupType::ModuleWithContents => write!(f, "module_with_contents"),
            GroupType::TestSuiteWithCases => write!(f, "test_suite_with_cases"),
            GroupType::CompositePattern => write!(f, "composite_pattern"),
            GroupType::ClassWithNestedClasses => write!(f, "class_with_nested_classes"),
            GroupType::StructWithNestedStructs => write!(f, "struct_with_nested_structs"),
            GroupType::FunctionWithLogicalBlocks => write!(f, "function_with_logical_blocks"),
            GroupType::FunctionWithMembers => write!(f, "function_with_members"),
            GroupType::MergedFragments => write!(f, "merged_fragments"),
            GroupType::FileDocumentation => write!(f, "file_documentation"),
        }
    }
}

impl GroupType {
    pub fn has_members(&self) -> bool {
        matches!(
            self,
            GroupType::ClassWithMethods
                | GroupType::InterfaceWithImpls
                | GroupType::RelatedFunctions
                | GroupType::TraitWithImpls
                | GroupType::ModuleWithContents
                | GroupType::TestSuiteWithCases
                | GroupType::CompositePattern
                | GroupType::ClassWithNestedClasses
                | GroupType::StructWithNestedStructs
                | GroupType::FunctionWithMembers
                | GroupType::MergedFragments
        )
    }

    pub fn is_standalone(&self) -> bool {
        matches!(self, GroupType::Standalone)
    }

    pub fn is_test_related(&self) -> bool {
        matches!(self, GroupType::TestSuiteWithCases)
    }

    pub fn is_composite_pattern(&self) -> bool {
        matches!(self, GroupType::CompositePattern)
    }

    pub fn has_nested_groups(&self) -> bool {
        matches!(
            self,
            GroupType::ClassWithNestedClasses | GroupType::StructWithNestedStructs
        )
    }

    /// Group types where members belong to a parent container (not just related).
    /// Members should inherit parent context.
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            GroupType::ClassWithMethods
                | GroupType::TraitWithImpls
                | GroupType::InterfaceWithImpls
                | GroupType::TestSuiteWithCases
                | GroupType::FunctionWithMembers
                | GroupType::CompositePattern
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GroupRole {
    Header,
    Member,
}

impl std::fmt::Display for GroupRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupRole::Header => write!(f, "header"),
            GroupRole::Member => write!(f, "member"),
        }
    }
}

impl std::str::FromStr for GroupRole {
    type Err = crate::types::error::ParseGroupRoleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "header" => Ok(GroupRole::Header),
            "member" => Ok(GroupRole::Member),
            _ => Err(crate::types::error::ParseGroupRoleError::unknown(s)),
        }
    }
}

impl GroupRole {
    pub fn is_header(&self) -> bool {
        matches!(self, GroupRole::Header)
    }

    pub fn is_member(&self) -> bool {
        matches!(self, GroupRole::Member)
    }
}

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum SpanError {
    #[error("Invalid span for entity {entity_id:?}: {reason}")]
    InvalidSpan {
        entity_id: Option<EntityId>,
        reason: String,
    },

    #[error("Overlapping spans in group {group_id}: {span1:?} and {span2:?}")]
    OverlappingSpans {
        group_id: String,
        span1: Span,
        span2: Span,
    },
}

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum ValidationError {
    #[error("Entity count mismatch: expected {expected}, got {actual}")]
    EntityCountMismatch { expected: usize, actual: usize },

    #[error("Duplicate entity ID: {0:?}")]
    DuplicateEntityId(EntityId),

    #[error("Empty combined source for group: {group_id}")]
    EmptyCombinedSource { group_id: String },

    #[error("Invalid span in group {group_id}: start={start}, end={end}")]
    InvalidSpan {
        group_id: String,
        start: usize,
        end: usize,
    },

    #[error("Missing entity ID in result: {0:?}")]
    MissingEntityId(EntityId),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityMeta {
    pub entity_id: EntityId,
    pub name: String,
    pub is_utility: bool,

    #[serde(default)]
    pub call_count: usize,
    #[serde(default)]
    pub callers: Vec<EntityId>,
    #[serde(default)]
    pub callees: Vec<EntityId>,
    #[serde(default)]
    pub is_merged: bool,
    #[serde(default)]
    pub original_ids: Vec<EntityId>,
    #[serde(default)]
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityGroup {
    pub group_id: CompactString,
    pub group_type: GroupType,
    pub header: Option<GroupedEntity>,
    pub header_id: Option<EntityId>,
    pub members: SmallVec<[GroupedEntity; 4]>,
    pub member_ids: SmallVec<[EntityId; 8]>,
    /// Per-entity source spans captured at grouping time. Serialized so
    /// cached conversion outputs keep exact chunk source coverage.
    #[serde(default)]
    pub entity_spans: HashMap<EntityId, Span>,
    #[serde(skip)]
    pub combined_source: Option<Arc<str>>,
    #[serde(skip)]
    pub combined_source_lazy: OnceLock<Arc<str>>,
    pub span: Span,
    pub kind: EntityKind,
    pub name: CompactString,
    pub language: crate::types::language::Language,
    #[serde(default)]
    pub pattern_info: PatternInfo,
    #[serde(default)]
    pub member_roles: SmallVec<[(EntityId, MemberRole); 8]>,
    #[serde(default)]
    pub nested_groups: Box<[EntityGroup]>,
    #[serde(default)]
    pub nesting_level: usize,
    #[serde(default)]
    pub parent_group_id: Option<CompactString>,
    #[serde(default)]
    pub has_significant_nested: bool,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Test-code marker (AST detection merged with file-path rules).
    /// Orthogonal to `group_type`; propagates to every chunk of this group.
    #[serde(default)]
    pub test_info: crate::types::test_info::TestInfo,
}

impl Default for EntityGroup {
    fn default() -> Self {
        Self {
            group_id: CompactString::new(""),
            group_type: GroupType::Standalone,
            header: None,
            header_id: None,
            members: SmallVec::new(),
            member_ids: SmallVec::new(),
            entity_spans: HashMap::new(),
            combined_source: None,
            combined_source_lazy: OnceLock::new(),
            span: Span::default(),
            kind: EntityKind::Function,
            name: CompactString::new(""),
            language: crate::types::language::Language::Unknown,
            pattern_info: PatternInfo::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: HashMap::new(),
            test_info: crate::types::test_info::TestInfo::unknown(),
        }
    }
}

impl EntityGroup {
    pub fn new(group_id: impl Into<CompactString>, group_type: GroupType) -> Self {
        Self {
            group_id: group_id.into(),
            group_type,
            ..Default::default()
        }
    }

    pub fn from_entity(entity: Entity, language: crate::types::language::Language) -> Self {
        let name = CompactString::from(entity.name.as_str());
        let kind = entity.kind;
        let span = entity.span;
        let entity_id = entity.id;

        let mut entity_spans = HashMap::new();
        entity_spans.insert(entity_id, span);

        Self {
            group_id: CompactString::from(format!("group_{}", entity_id.0)),
            group_type: GroupType::Standalone,
            header: Some(GroupedEntity::from_entity(&entity)),
            header_id: Some(entity_id),
            members: SmallVec::new(),
            member_ids: SmallVec::new(),
            entity_spans,
            combined_source: None,
            combined_source_lazy: OnceLock::new(),
            span,
            kind,
            name,
            language,
            pattern_info: PatternInfo::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: entity.metadata.clone(),
            test_info: crate::types::test_info::TestInfo::unknown(),
        }
    }

    pub fn class_with_methods_with_fields(
        class: Entity,
        fields: Vec<Entity>,
        methods: Vec<Entity>,
        language: crate::types::language::Language,
    ) -> Self {
        let class_id = class.id;
        let name = CompactString::from(class.name.as_str());
        let kind = class.kind;
        let class_span = class.span;

        // Merge fields and methods sorted by source position
        let mut all_members: Vec<Entity> = fields;
        all_members.extend(methods);
        all_members.sort_by_key(|e| e.span.start_byte);

        let member_ids: SmallVec<[EntityId; 8]> = all_members.iter().map(|m| m.id).collect();

        let mut entity_spans = HashMap::new();
        entity_spans.insert(class_id, class_span);

        let semantic_members: SmallVec<[GroupedEntity; 4]> = all_members
            .iter()
            .map(|m| {
                entity_spans.insert(m.id, m.span);
                GroupedEntity::from_entity(m)
            })
            .collect();

        let combined_span = Self::calculate_span_from_map(&entity_spans);

        Self {
            group_id: CompactString::from(format!("group_{}", class_id.0)),
            group_type: GroupType::ClassWithMethods,
            header: Some(GroupedEntity::from_entity(&class)),
            header_id: Some(class_id),
            members: semantic_members,
            member_ids,
            entity_spans,
            combined_source: None,
            combined_source_lazy: OnceLock::new(),
            span: combined_span,
            kind,
            name,
            language,
            pattern_info: PatternInfo::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: HashMap::new(),
            test_info: crate::types::test_info::TestInfo::unknown(),
        }
    }

    pub fn class_with_methods(
        class: Entity,
        methods: Vec<Entity>,
        language: crate::types::language::Language,
    ) -> Self {
        let class_id = class.id;
        let name = CompactString::from(class.name.as_str());
        let kind = class.kind;
        let class_span = class.span;

        let member_ids: SmallVec<[EntityId; 8]> = methods.iter().map(|m| m.id).collect();

        let mut entity_spans = HashMap::new();
        entity_spans.insert(class_id, class_span);

        let semantic_methods: SmallVec<[GroupedEntity; 4]> = methods
            .iter()
            .map(|m| {
                entity_spans.insert(m.id, m.span);
                GroupedEntity::from_entity(m)
            })
            .collect();

        let combined_span = Self::calculate_span_from_map(&entity_spans);

        Self {
            group_id: CompactString::from(format!("group_{}", class_id.0)),
            group_type: GroupType::ClassWithMethods,
            header: Some(GroupedEntity::from_entity(&class)),
            header_id: Some(class_id),
            members: semantic_methods,
            member_ids,
            entity_spans,
            combined_source: None,
            combined_source_lazy: OnceLock::new(),
            span: combined_span,
            kind,
            name,
            language,
            pattern_info: PatternInfo::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: HashMap::new(),
            test_info: crate::types::test_info::TestInfo::unknown(),
        }
    }

    /// Create an impl-block-with-methods group
    ///
    /// Similar to `class_with_methods` but for Rust `impl` blocks
    /// (inherent and trait impls).
    /// The header is the impl entity, members are the methods.
    pub fn impl_block_with_methods(
        impl_entity: Entity,
        methods: Vec<Entity>,
        language: crate::types::language::Language,
    ) -> Self {
        let impl_id = impl_entity.id;
        let name = CompactString::from(impl_entity.name.as_str());
        let kind = impl_entity.kind;
        let impl_span = impl_entity.span;

        let member_ids: SmallVec<[EntityId; 8]> = methods.iter().map(|m| m.id).collect();

        let mut entity_spans = HashMap::new();
        entity_spans.insert(impl_id, impl_span);

        let semantic_methods: SmallVec<[GroupedEntity; 4]> = methods
            .iter()
            .map(|m| {
                entity_spans.insert(m.id, m.span);
                GroupedEntity::from_entity(m)
            })
            .collect();

        let combined_span = Self::calculate_span_from_map(&entity_spans);

        Self {
            group_id: CompactString::from(format!("group_{}", impl_id.0)),
            group_type: GroupType::ClassWithMethods,
            header: Some(GroupedEntity::from_entity(&impl_entity)),
            header_id: Some(impl_id),
            members: semantic_methods,
            member_ids,
            entity_spans,
            combined_source: None,
            combined_source_lazy: OnceLock::new(),
            span: combined_span,
            kind,
            name,
            language,
            pattern_info: PatternInfo::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: HashMap::new(),
            test_info: crate::types::test_info::TestInfo::unknown(),
        }
    }

    pub fn test_suite_with_cases(
        suite: Entity,
        test_cases: Vec<Entity>,
        language: crate::types::language::Language,
    ) -> Self {
        let suite_id = suite.id;
        let name = CompactString::from(suite.name.as_str());
        let kind = suite.kind;
        let suite_span = suite.span;

        let member_ids: SmallVec<[EntityId; 8]> = test_cases.iter().map(|tc| tc.id).collect();

        let mut entity_spans = HashMap::new();
        entity_spans.insert(suite_id, suite_span);

        let semantic_cases: SmallVec<[GroupedEntity; 4]> = test_cases
            .iter()
            .map(|tc| {
                entity_spans.insert(tc.id, tc.span);
                GroupedEntity::from_entity(tc)
            })
            .collect();

        let combined_span = Self::calculate_span_from_map(&entity_spans);

        Self {
            group_id: CompactString::from(format!("test_suite_{}", suite_id.0)),
            group_type: GroupType::TestSuiteWithCases,
            header: Some(GroupedEntity::from_entity(&suite)),
            header_id: Some(suite_id),
            members: semantic_cases,
            member_ids,
            entity_spans,
            combined_source: None,
            combined_source_lazy: OnceLock::new(),
            span: combined_span,
            kind,
            name,
            language,
            pattern_info: PatternInfo::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: HashMap::new(),
            test_info: crate::types::test_info::TestInfo::unknown(),
        }
    }

    pub fn composite_pattern(
        component_interface: Entity,
        leaf_classes: Vec<Entity>,
        composite_classes: Vec<Entity>,
        language: crate::types::language::Language,
    ) -> Self {
        let interface_id = component_interface.id;
        let name = CompactString::from(component_interface.name.as_str());
        let kind = component_interface.kind;
        let interface_span = component_interface.span;

        let mut member_ids = SmallVec::<[EntityId; 8]>::new();
        member_ids.extend(leaf_classes.iter().map(|lc| lc.id));
        member_ids.extend(composite_classes.iter().map(|cc| cc.id));

        let mut entity_spans = HashMap::new();
        entity_spans.insert(interface_id, interface_span);

        let semantic_leafs: SmallVec<[GroupedEntity; 4]> = leaf_classes
            .iter()
            .map(|lc| {
                entity_spans.insert(lc.id, lc.span);
                GroupedEntity::from_entity(lc)
            })
            .collect();

        let semantic_composites: SmallVec<[GroupedEntity; 4]> = composite_classes
            .iter()
            .map(|cc| {
                entity_spans.insert(cc.id, cc.span);
                GroupedEntity::from_entity(cc)
            })
            .collect();

        let mut members = SmallVec::<[GroupedEntity; 4]>::new();
        members.extend(semantic_leafs);
        members.extend(semantic_composites);

        let combined_span = Self::calculate_span_from_map(&entity_spans);

        Self {
            group_id: CompactString::from(format!("composite_{}", interface_id.0)),
            group_type: GroupType::CompositePattern,
            header: Some(GroupedEntity::from_entity(&component_interface)),
            header_id: Some(interface_id),
            members,
            member_ids,
            entity_spans,
            combined_source: None,
            combined_source_lazy: OnceLock::new(),
            span: combined_span,
            kind,
            name,
            language,
            pattern_info: PatternInfo::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: HashMap::new(),
            test_info: crate::types::test_info::TestInfo::unknown(),
        }
    }

    fn calculate_span_from_map(entity_spans: &HashMap<EntityId, Span>) -> Span {
        let mut min_span: Option<Span> = None;
        let mut max_span: Option<Span> = None;

        for span in entity_spans.values() {
            match min_span {
                None => min_span = Some(*span),
                Some(ref cur) => {
                    if span.start_byte < cur.start_byte
                        || (span.start_byte == cur.start_byte
                            && span.start_position.row < cur.start_position.row)
                    {
                        min_span = Some(*span);
                    }
                }
            }
            match max_span {
                None => max_span = Some(*span),
                Some(ref cur) => {
                    if span.end_byte > cur.end_byte
                        || (span.end_byte == cur.end_byte
                            && span.end_position.row > cur.end_position.row)
                    {
                        max_span = Some(*span);
                    }
                }
            }
        }

        match (min_span, max_span) {
            (Some(min), Some(max)) => Span {
                start_byte: min.start_byte,
                end_byte: max.end_byte,
                start_position: min.start_position,
                end_position: max.end_position,
            },
            _ => Span::default(),
        }
    }

    pub fn with_combined_source(mut self, source: impl Into<Arc<str>>) -> Self {
        self.combined_source = Some(source.into());
        self
    }

    pub fn entity_count(&self) -> usize {
        let header_count = if self.header.is_some() { 1 } else { 0 };
        header_count + self.members.len()
    }

    pub fn has_header(&self) -> bool {
        self.header.is_some()
    }

    pub fn has_members(&self) -> bool {
        !self.members.is_empty()
    }

    pub fn all_entity_ids(&self) -> Vec<EntityId> {
        let mut ids = Vec::new();
        if let Some(header_id) = self.header_id {
            ids.push(header_id);
        }
        ids.extend(&self.member_ids);
        ids
    }

    /// Resolve a display name for each of the given entity IDs.
    ///
    /// Names come from the group header and members; unknown IDs (e.g. the
    /// group header on paths where it is absent from `members`) fall back to
    /// the entity's own record when available and finally to the group name,
    /// so the returned list always has one entry per input ID and stays
    /// positionally aligned with it.
    pub fn entity_display_names(&self, entity_ids: &[EntityId]) -> Vec<String> {
        let name_by_id: std::collections::HashMap<EntityId, &str> = self
            .header
            .iter()
            .map(|h| (h.id, h.name.as_str()))
            .chain(self.members.iter().map(|m| (m.id, m.name.as_str())))
            .collect();
        entity_ids
            .iter()
            .map(|id| {
                name_by_id
                    .get(id)
                    .map(|name| (*name).to_string())
                    .unwrap_or_else(|| self.name.to_string())
            })
            .collect()
    }

    pub fn get_combined_source(&self, file_source: &str) -> &str {
        if let Some(ref source) = self.combined_source {
            return source;
        }

        let span = &self.span;
        if span.start_byte >= span.end_byte || span.end_byte > file_source.len() {
            return "";
        }

        self.combined_source_lazy
            .get_or_init(|| Arc::from(&file_source[span.start_byte..span.end_byte]))
    }

    pub fn preload_combined_source(&mut self, file_source: &str) {
        if self.combined_source.is_none() {
            let span = &self.span;
            if span.start_byte < span.end_byte && span.end_byte <= file_source.len() {
                let source_segment = &file_source[span.start_byte..span.end_byte];
                self.combined_source = Some(Arc::from(source_segment));
            }
        }
    }

    pub fn generate_combined_source(&mut self, file_source: &str) -> bool {
        let span = &self.span;

        if span.start_byte >= span.end_byte {
            tracing::warn!(
                "Invalid span for group {}: start={} >= end={}",
                self.group_id,
                span.start_byte,
                span.end_byte
            );
            return false;
        }

        if span.end_byte > file_source.len() {
            tracing::warn!(
                "Span exceeds file length for group {}: end={} > file_len={}, falling back to signature",
                self.group_id,
                span.end_byte,
                file_source.len()
            );
            // Fallback: generate combined source from entity signatures
            let mut source = String::new();
            if let Some(ref header) = self.header {
                source.push_str(&header.name);
                source.push(' ');
            }
            for member in &self.members {
                source.push_str(&member.name);
                source.push(' ');
            }
            self.combined_source = Some(Arc::from(source));
            return true;
        }

        let source_segment = &file_source[span.start_byte..span.end_byte];
        self.combined_source = Some(Arc::from(source_segment));

        tracing::debug!(
            "Generated combined source for group {} ({} bytes)",
            self.group_id,
            source_segment.len()
        );

        true
    }

    pub fn calculate_combined_span_from_map(entity_spans: &HashMap<EntityId, Span>) -> Span {
        Self::calculate_span_from_map(entity_spans)
    }

    pub fn calculate_combined_span(&mut self) {
        if self.entity_spans.is_empty() {
            tracing::warn!("No entity spans available for group {}", self.group_id);
            return;
        }
        self.span = Self::calculate_span_from_map(&self.entity_spans);
    }

    pub fn calculate_combined_span_validated(&mut self) -> Result<(), SpanError> {
        if self.entity_spans.is_empty() {
            return Err(SpanError::InvalidSpan {
                entity_id: None,
                reason: "No entity spans available".to_string(),
            });
        }

        let mut spans: Vec<(Option<EntityId>, Span)> = Vec::new();

        if let Some(header_id) = self.header_id {
            if let Some(&span) = self.entity_spans.get(&header_id) {
                spans.push((Some(header_id), span));
            }
        }

        for &member_id in &self.member_ids {
            if let Some(&span) = self.entity_spans.get(&member_id) {
                spans.push((Some(member_id), span));
            }
        }

        for (entity_id, span) in &spans {
            if span.start_byte >= span.end_byte {
                return Err(SpanError::InvalidSpan {
                    entity_id: *entity_id,
                    reason: "start_byte >= end_byte".to_string(),
                });
            }
        }

        for i in 0..spans.len() {
            for j in (i + 1)..spans.len() {
                let (_, span1) = spans[i];
                let (_, span2) = spans[j];
                if span1.overlaps(&span2) {
                    tracing::warn!(
                        "Overlapping spans in group {}: {:?} and {:?}",
                        self.group_id,
                        span1,
                        span2
                    );
                }
            }
        }

        self.span = Self::calculate_span_from_map(&self.entity_spans);
        Ok(())
    }

    pub fn is_nested(&self) -> bool {
        self.nesting_level > 0
    }

    pub fn total_nesting_depth(&self) -> usize {
        if self.nested_groups.is_empty() {
            self.nesting_level
        } else {
            self.nested_groups
                .iter()
                .map(|g| g.total_nesting_depth())
                .max()
                .unwrap_or(self.nesting_level)
        }
    }

    pub fn count_all_nested(&self) -> usize {
        let direct_count = self.nested_groups.len();
        let nested_count: usize = self
            .nested_groups
            .iter()
            .map(|g| g.count_all_nested())
            .sum();
        direct_count + nested_count
    }

    pub fn build_role_map(&self) -> HashMap<EntityId, MemberRole> {
        self.member_roles
            .iter()
            .map(|(id, role)| (*id, *role))
            .collect()
    }

    pub fn find_nested_by_name(&self, name: &str) -> Option<&EntityGroup> {
        for nested in &self.nested_groups {
            if nested.name == name {
                return Some(nested);
            }
            if let Some(deeper) = nested.find_nested_by_name(name) {
                return Some(deeper);
            }
        }
        None
    }

    pub fn find_nested_by_id(&self, group_id: &str) -> Option<&EntityGroup> {
        for nested in &self.nested_groups {
            if nested.group_id == group_id {
                return Some(nested);
            }
            if let Some(deeper) = nested.find_nested_by_id(group_id) {
                return Some(deeper);
            }
        }
        None
    }

    pub fn collect_nested_spans(&self) -> Vec<(String, Span)> {
        let mut spans = Vec::new();

        fn collect_recursive(group: &EntityGroup, spans: &mut Vec<(String, Span)>) {
            for nested in &group.nested_groups {
                spans.push((nested.group_id.to_string(), nested.span));
                collect_recursive(nested, spans);
            }
        }

        collect_recursive(self, &mut spans);
        spans.sort_by_key(|(_, span)| span.start_byte);
        spans
    }

    /// Absorb members from another group into this group.
    ///
    /// Merges the other group's members, member_ids, and entity_spans into this group,
    /// then recalculates the combined span.
    pub fn absorb_members(&mut self, other: &EntityGroup) {
        // Merge members
        self.members.extend(other.members.iter().cloned());

        // Merge member_ids
        self.member_ids.extend(other.member_ids.iter().copied());

        // Merge entity_spans
        for (entity_id, span) in &other.entity_spans {
            self.entity_spans.insert(*entity_id, *span);
        }

        // Recalculate combined span
        self.calculate_combined_span();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessingStats {
    pub input_entities: usize,
    pub output_groups: usize,
    pub class_method_associations: usize,
    pub utility_functions: usize,
    pub merged_calls: usize,
    pub standalone_entities: usize,
    pub impl_associations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    pub groups: Vec<EntityGroup>,
    pub entity_meta: std::collections::HashMap<String, EntityMeta>,
    #[serde(default)]
    pub behavior: BehaviorStore,
    #[serde(default)]
    pub control_flow: ControlFlowStore,
    pub stats: ProcessingStats,
}

impl ProcessingResult {
    pub fn validate(&self, expected_entity_count: usize) -> Result<(), ValidationError> {
        let total_entities: usize = self.groups.iter().map(|g| g.entity_count()).sum();

        if total_entities != expected_entity_count {
            return Err(ValidationError::EntityCountMismatch {
                expected: expected_entity_count,
                actual: total_entities,
            });
        }

        let mut seen_ids = HashSet::new();
        for group in &self.groups {
            for entity_id in group.all_entity_ids() {
                if !seen_ids.insert(entity_id) {
                    return Err(ValidationError::DuplicateEntityId(entity_id));
                }
            }
        }

        for group in &self.groups {
            if group.combined_source.is_none()
                || group
                    .combined_source
                    .as_ref()
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
            {
                return Err(ValidationError::EmptyCombinedSource {
                    group_id: group.group_id.to_string(),
                });
            }
        }

        for group in &self.groups {
            if group.span.start_byte >= group.span.end_byte {
                return Err(ValidationError::InvalidSpan {
                    group_id: group.group_id.to_string(),
                    start: group.span.start_byte,
                    end: group.span.end_byte,
                });
            }
        }

        Ok(())
    }

    pub fn validate_relaxed(&self, expected_entity_count: usize) -> Result<(), ValidationError> {
        let total_entities: usize = self.groups.iter().map(|g| g.entity_count()).sum();

        if total_entities != expected_entity_count {
            return Err(ValidationError::EntityCountMismatch {
                expected: expected_entity_count,
                actual: total_entities,
            });
        }

        let mut seen_ids = HashSet::new();
        for group in &self.groups {
            for entity_id in group.all_entity_ids() {
                if !seen_ids.insert(entity_id) {
                    return Err(ValidationError::DuplicateEntityId(entity_id));
                }
            }
        }

        for group in &self.groups {
            if group.span.start_byte >= group.span.end_byte {
                return Err(ValidationError::InvalidSpan {
                    group_id: group.group_id.to_string(),
                    start: group.span.start_byte,
                    end: group.span.end_byte,
                });
            }
        }

        Ok(())
    }

    pub fn all_entity_ids(&self) -> Vec<EntityId> {
        let mut ids = Vec::new();
        for group in &self.groups {
            ids.extend(group.all_entity_ids());
        }
        ids
    }

    pub fn find_group_by_entity_id(&self, entity_id: EntityId) -> Option<&EntityGroup> {
        self.groups
            .iter()
            .find(|g| g.header_id == Some(entity_id) || g.member_ids.contains(&entity_id))
    }
}
