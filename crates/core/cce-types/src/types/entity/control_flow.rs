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
    /// Byte range of the `else` continuation when the fact carries one.
    ///
    /// The range lies inside `[start_byte, end_byte]` and marks where the
    /// negated branch starts. `None` means the fact has no recorded `else`
    /// side, in which case consumers fall back to the fact text.
    #[serde(default)]
    pub else_start_byte: Option<usize>,
    /// End of the `else` continuation byte range.
    #[serde(default)]
    pub else_end_byte: Option<usize>,
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
            else_start_byte: None,
            else_end_byte: None,
        }
    }

    /// Attach the byte range of the `else` continuation.
    pub fn with_else_range(mut self, else_start_byte: usize, else_end_byte: usize) -> Self {
        self.else_start_byte = Some(else_start_byte);
        self.else_end_byte = Some(else_end_byte);
        self
    }

    /// Whether the fact records an `else` continuation range.
    pub fn has_else_range(&self) -> bool {
        self.else_start_byte.is_some_and(|start| {
            self.else_end_byte
                .is_some_and(|end| self.start_byte <= start && start < end && end <= self.end_byte)
        })
    }

    /// Whether a source byte offset falls inside the `else` continuation.
    pub fn contains_byte_in_else(&self, byte: usize) -> bool {
        match (self.else_start_byte, self.else_end_byte) {
            (Some(start), Some(end)) => start <= byte && byte < end,
            _ => false,
        }
    }
}

/// Byte offset of the outer `else` keyword within an `if` fact text.
///
/// Only the `else` belonging to the outer conditional counts: occurrences
/// nested inside the then-block or inside string literals are ignored. The
/// scan skips quoted regions and only accepts `else` at the top brace depth
/// once the then-branch has produced a block close or a statement
/// terminator. Returns `None` when no outer `else` continuation exists.
pub fn find_outer_else_offset(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    // Locate the condition end: balanced parens after the leading `if`.
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'(' {
        let mut depth = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                b'"' | b'\'' | b'`' => {
                    i = skip_quoted_region(bytes, i);
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
    }
    let mut depth = 0usize;
    let mut then_closed = false;
    let mut j = i;
    while j < bytes.len() {
        match bytes[j] {
            b'"' | b'\'' | b'`' => {
                j = skip_quoted_region(bytes, j);
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    then_closed = true;
                }
            }
            b';' => {
                if depth == 0 {
                    then_closed = true;
                }
            }
            _ => {
                if depth == 0 && then_closed && is_branch_keyword_at(bytes, j, "else") {
                    return Some(j);
                }
            }
        }
        j += 1;
    }
    None
}

/// Whether an `if` fact text carries an outer `else` continuation.
pub fn has_outer_else_branch(text: &str) -> bool {
    find_outer_else_offset(text).is_some()
}

fn skip_quoted_region(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut j = start + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == quote {
            return j + 1;
        }
        j += 1;
    }
    j
}

fn is_branch_keyword_at(bytes: &[u8], pos: usize, keyword: &str) -> bool {
    let word = keyword.as_bytes();
    if pos + word.len() > bytes.len() {
        return false;
    }
    if &bytes[pos..pos + word.len()] != word {
        return false;
    }
    let before_ok = pos == 0 || {
        let c = bytes[pos - 1];
        !(c.is_ascii_alphanumeric() || c == b'_')
    };
    let after = pos + word.len();
    let after_ok = after >= bytes.len() || {
        let c = bytes[after];
        !(c.is_ascii_alphanumeric() || c == b'_')
    };
    before_ok && after_ok
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
