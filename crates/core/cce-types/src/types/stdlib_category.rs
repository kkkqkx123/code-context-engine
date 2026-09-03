use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

/// Standard library entity category
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Default,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum StdlibCategory {
    Collection,
    Io,
    Concurrency,
    Utility,
    String,
    Numeric,
    Error,
    Macro,
    Trait,
    #[default]
    Other,
}

impl std::fmt::Display for StdlibCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StdlibCategory::Collection => write!(f, "collections"),
            StdlibCategory::Io => write!(f, "io"),
            StdlibCategory::Concurrency => write!(f, "concurrency"),
            StdlibCategory::Utility => write!(f, "utilities"),
            StdlibCategory::String => write!(f, "strings"),
            StdlibCategory::Numeric => write!(f, "numerics"),
            StdlibCategory::Error => write!(f, "errors"),
            StdlibCategory::Macro => write!(f, "macros"),
            StdlibCategory::Trait => write!(f, "traits"),
            StdlibCategory::Other => write!(f, "other_stdlib"),
        }
    }
}

impl StdlibCategory {
    /// Human-readable label used in descriptions.
    pub fn description_label(self) -> &'static str {
        match self {
            StdlibCategory::Collection => "collection type",
            StdlibCategory::Io => "I/O utility",
            StdlibCategory::Concurrency => "concurrency primitive",
            StdlibCategory::Utility => "utility type",
            StdlibCategory::String => "string type",
            StdlibCategory::Numeric => "numeric type",
            StdlibCategory::Error => "error handling type",
            StdlibCategory::Macro => "macro",
            StdlibCategory::Trait => "trait",
            StdlibCategory::Other => "standard library entity",
        }
    }
}
