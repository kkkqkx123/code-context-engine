//! Behavior sidecar data for function-like entities
//!
//! This module stores raw behavior facts extracted from tree-sitter queries.
//! The data is kept separate from ordinary entity metadata so downstream
//! consumers can decide when and how to render it.

use std::collections::BTreeMap;
use std::fmt;

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

use super::EntityId;

/// Stable behavior fact kind captured from source code.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
)]
pub enum BehaviorFactKind {
    DataBind,
    DataReference,
    DataObject,
    DataArray,
    DataQuery,
    DataStatement,
    EffectError,
    OpShiftLeft,
    OpShiftLeftAssign,
    OpShiftRight,
    OpShiftRightAssign,
    /// Plain (non-documentation) comment fragment attached to its context entity.
    ///
    /// Injected by the comment processor dispatch, not by behavior captures.
    Comment,
    /// Raw `macro_rules!` definition body stored as cleaned source.
    ///
    /// Injected by the macro body extractor, not by behavior captures.
    MacroBody,
}

impl BehaviorFactKind {
    /// Stable capture label without the `@behavior.` prefix.
    pub const fn capture_label(&self) -> &'static str {
        match self {
            BehaviorFactKind::DataBind => "data.bind",
            BehaviorFactKind::DataReference => "data.reference",
            BehaviorFactKind::DataObject => "data.object",
            BehaviorFactKind::DataArray => "data.array",
            BehaviorFactKind::DataQuery => "data.query",
            BehaviorFactKind::DataStatement => "data.statement",
            BehaviorFactKind::EffectError => "effect.error",
            BehaviorFactKind::OpShiftLeft => "op.shift_left",
            BehaviorFactKind::OpShiftLeftAssign => "op.shift_left_assign",
            BehaviorFactKind::OpShiftRight => "op.shift_right",
            BehaviorFactKind::OpShiftRightAssign => "op.shift_right_assign",
            BehaviorFactKind::Comment => "comment",
            BehaviorFactKind::MacroBody => "macro.body",
        }
    }

    /// Create a fact kind from a stable capture label.
    pub fn from_capture_label(label: &str) -> Option<Self> {
        match label {
            "data.bind" => Some(Self::DataBind),
            "data.reference" => Some(Self::DataReference),
            "data.object" => Some(Self::DataObject),
            "data.array" => Some(Self::DataArray),
            "data.query" => Some(Self::DataQuery),
            "data.statement" => Some(Self::DataStatement),
            "effect.error" => Some(Self::EffectError),
            "op.shift_left" => Some(Self::OpShiftLeft),
            "op.shift_left_assign" => Some(Self::OpShiftLeftAssign),
            "op.shift_right" => Some(Self::OpShiftRight),
            "op.shift_right_assign" => Some(Self::OpShiftRightAssign),
            "comment" => Some(Self::Comment),
            "macro.body" => Some(Self::MacroBody),
            _ => None,
        }
    }
}

impl fmt::Display for BehaviorFactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.capture_label())
    }
}

/// Raw behavior fact captured from source code.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct BehaviorFact {
    /// Stable behavior kind.
    pub kind: BehaviorFactKind,
    /// Cleaned source text for the matched node.
    ///
    /// Inline comments are stripped before storage so the fact remains a
    /// stable code fragment rather than a code-plus-comment mixture.
    pub text: String,
    /// Number of non-empty code lines in the cleaned fragment.
    pub content_line_count: usize,
    /// Start byte offset in the source file.
    pub start_byte: usize,
    /// End byte offset in the source file.
    pub end_byte: usize,
}

impl BehaviorFact {
    /// Create a new behavior fact.
    pub fn new(
        kind: BehaviorFactKind,
        text: impl Into<String>,
        start_byte: usize,
        end_byte: usize,
    ) -> Self {
        let text = text.into();
        Self {
            content_line_count: text.lines().filter(|line| !line.trim().is_empty()).count(),
            kind,
            text,
            start_byte,
            end_byte,
        }
    }
}

/// Behavior facts collected for a single entity.
#[derive(
    Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct EntityBehavior {
    /// Flat behavior facts extracted for this entity.
    #[serde(default)]
    pub facts: Vec<BehaviorFact>,
}

impl EntityBehavior {
    /// Returns `true` when no facts are stored.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Add a fact to the entity.
    pub fn push_fact(&mut self, fact: BehaviorFact) {
        self.facts.push(fact);
    }
}

/// Behavior sidecar for the whole file.
#[derive(
    Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct BehaviorStore {
    /// Behavior facts indexed by entity ID.
    #[serde(default)]
    entities: BTreeMap<EntityId, EntityBehavior>,
}

impl BehaviorStore {
    /// Returns `true` if no behavior facts are present.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Get the behavior entry for an entity.
    pub fn get(&self, entity_id: EntityId) -> Option<&EntityBehavior> {
        self.entities.get(&entity_id)
    }

    /// Iterate over all behavior entries keyed by entity ID.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &EntityBehavior)> {
        self.entities.iter().map(|(id, behavior)| (*id, behavior))
    }

    /// Iterate over mutable behavior entries.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut EntityBehavior> {
        self.entities.values_mut()
    }

    /// Get the behavior entry for an entity, creating it if needed.
    pub fn entry_mut(&mut self, entity_id: EntityId) -> &mut EntityBehavior {
        self.entities.entry(entity_id).or_default()
    }

    /// Add a fact for an entity.
    pub fn push_fact(&mut self, entity_id: EntityId, fact: BehaviorFact) {
        self.entry_mut(entity_id).push_fact(fact);
    }

    /// Remove entities that do not contain any facts.
    pub fn retain_non_empty(&mut self) {
        self.entities.retain(|_, entry| !entry.is_empty());
    }
}
