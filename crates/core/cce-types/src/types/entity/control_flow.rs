//! Control-flow sidecar data for function-like entities
//!
//! This module stores raw control-flow facts extracted from tree-sitter
//! queries. The data is kept separate from ordinary entity metadata so
//! downstream consumers can decide when and how to render it.

use std::collections::BTreeMap;
use std::fmt;

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

use super::EntityId;

/// Stable control-flow fact kind captured from source code.
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
pub enum ControlFlowFactKind {
    If,
    Match,
    Loop,
    Return,
    Break,
    Continue,
    Yield,
    Try,
}

impl ControlFlowFactKind {
    /// Stable capture label without the `@control.` prefix.
    pub const fn capture_label(&self) -> &'static str {
        match self {
            ControlFlowFactKind::If => "flow.if",
            ControlFlowFactKind::Match => "flow.match",
            ControlFlowFactKind::Loop => "flow.loop",
            ControlFlowFactKind::Return => "flow.return",
            ControlFlowFactKind::Break => "flow.break",
            ControlFlowFactKind::Continue => "flow.continue",
            ControlFlowFactKind::Yield => "flow.yield",
            ControlFlowFactKind::Try => "flow.try",
        }
    }

    /// Returns `true` for kinds that establish a nesting boundary.
    ///
    /// Only structural constructs (if, match, loop) define new nesting scopes.
    /// Terminal constructs (return, break, continue, yield, try) do not enclose
    /// other control-flow facts and should not inflate computed depth.
    pub fn is_structural(self) -> bool {
        matches!(self, Self::If | Self::Match | Self::Loop)
    }

    /// Create a fact kind from a stable capture label.
    pub fn from_capture_label(label: &str) -> Option<Self> {
        match label {
            "flow.if" => Some(Self::If),
            "flow.match" => Some(Self::Match),
            "flow.loop" => Some(Self::Loop),
            "flow.return" => Some(Self::Return),
            "flow.break" => Some(Self::Break),
            "flow.continue" => Some(Self::Continue),
            "flow.yield" => Some(Self::Yield),
            "flow.try" => Some(Self::Try),
            _ => None,
        }
    }
}

impl fmt::Display for ControlFlowFactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.capture_label())
    }
}

/// Raw control-flow fact captured from source code.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct ControlFlowFact {
    /// Stable control-flow kind.
    pub kind: ControlFlowFactKind,
    /// Cleaned source text for the matched node.
    ///
    /// Inline comments are stripped before storage so the fact stays focused
    /// on control structure instead of source commentary.
    pub text: String,
    /// Number of non-empty code lines in the cleaned fragment.
    pub content_line_count: usize,
    /// Start byte offset in the source file.
    pub start_byte: usize,
    /// End byte offset in the source file.
    pub end_byte: usize,
}

impl ControlFlowFact {
    /// Create a new control-flow fact.
    pub fn new(
        kind: ControlFlowFactKind,
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

/// Control-flow facts collected for a single entity.
#[derive(
    Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct EntityControlFlow {
    /// Flat control-flow facts extracted for this entity.
    #[serde(default)]
    pub facts: Vec<ControlFlowFact>,
}

impl EntityControlFlow {
    /// Returns `true` when no facts are stored.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Add a fact to the entity.
    pub fn push_fact(&mut self, fact: ControlFlowFact) {
        self.facts.push(fact);
    }
}

/// Control-flow sidecar for the whole file.
#[derive(
    Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize,
)]
pub struct ControlFlowStore {
    /// Control-flow facts indexed by entity ID.
    #[serde(default)]
    entities: BTreeMap<EntityId, EntityControlFlow>,
}

impl ControlFlowStore {
    /// Returns `true` if no control-flow facts are present.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Get the control-flow entry for an entity.
    pub fn get(&self, entity_id: EntityId) -> Option<&EntityControlFlow> {
        self.entities.get(&entity_id)
    }

    /// Iterate over mutable control-flow entries.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut EntityControlFlow> {
        self.entities.values_mut()
    }

    /// Get the control-flow entry for an entity, creating it if needed.
    pub fn entry_mut(&mut self, entity_id: EntityId) -> &mut EntityControlFlow {
        self.entities.entry(entity_id).or_default()
    }

    /// Add a fact for an entity.
    pub fn push_fact(&mut self, entity_id: EntityId, fact: ControlFlowFact) {
        self.entry_mut(entity_id).push_fact(fact);
    }

    /// Remove entities that do not contain any facts.
    pub fn retain_non_empty(&mut self) {
        self.entities.retain(|_, entry| !entry.is_empty());
    }
}
