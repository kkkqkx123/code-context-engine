//! Tree-sitter query type enumeration (cross-layer contract)
//!
//! Moved from `cce_parser::tree_sitter_query::loader` so the plugin trait
//! (`cce_core::plugin::CodePlugin`) can reference it for the `AstLanguage`
//! `query_scheme` capability without depending on the parser crate.

/// Query type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum QueryType {
    /// Entity query (function, class, struct, etc.)
    Entity,
    /// Call query (function calls)
    Call,
    /// Control-flow query (if/match/loop/return)
    ControlFlow,
    /// Behavior query (Rust function-body raw behavior snippets)
    Behavior,
    /// Dependency query (imports, includes, etc.)
    Dependency,
    /// Comment query (comments)
    Comment,
    /// Embedded block query (for Vue/Svelte SFC files)
    Embedded,
    /// Structural query (component hierarchy, element contains, etc.)
    Structural,
}

impl QueryType {
    /// All query types in a stable order.
    pub const ALL: [QueryType; 8] = [
        QueryType::Entity,
        QueryType::Call,
        QueryType::ControlFlow,
        QueryType::Behavior,
        QueryType::Dependency,
        QueryType::Comment,
        QueryType::Embedded,
        QueryType::Structural,
    ];

    /// The zero-based index used by the native ABI `query_scheme` symbol.
    pub fn as_u32(self) -> u32 {
        match self {
            QueryType::Entity => 0,
            QueryType::Call => 1,
            QueryType::ControlFlow => 2,
            QueryType::Behavior => 3,
            QueryType::Dependency => 4,
            QueryType::Comment => 5,
            QueryType::Embedded => 6,
            QueryType::Structural => 7,
        }
    }
}

impl std::fmt::Display for QueryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryType::Entity => write!(f, "entity"),
            QueryType::Call => write!(f, "call"),
            QueryType::ControlFlow => write!(f, "control_flow"),
            QueryType::Behavior => write!(f, "behavior"),
            QueryType::Dependency => write!(f, "dependency"),
            QueryType::Comment => write!(f, "comment"),
            QueryType::Embedded => write!(f, "embedded"),
            QueryType::Structural => write!(f, "structural"),
        }
    }
}
