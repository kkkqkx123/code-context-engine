//! Relation verification status tracking
//!
//! Tracks the verification status of virtual relations during Phase 2.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Verification status of a virtual relation
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub enum RelationVerificationStatus {
    /// Relation verified with current symbol versions
    Verified,
    /// Caller symbol became stale
    CallerStale,
    /// Target symbol became stale
    TargetStale,
    /// Needs re-parsing due to changes
    NeedsReparse,
    /// Target could not be resolved (not found in project symbols, not identified as external)
    Unresolved,
}

impl std::fmt::Display for RelationVerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verified => write!(f, "verified"),
            Self::CallerStale => write!(f, "caller_stale"),
            Self::TargetStale => write!(f, "target_stale"),
            Self::NeedsReparse => write!(f, "needs_reparse"),
            Self::Unresolved => write!(f, "unresolved"),
        }
    }
}

impl std::str::FromStr for RelationVerificationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "verified" => Ok(Self::Verified),
            "caller_stale" => Ok(Self::CallerStale),
            "target_stale" => Ok(Self::TargetStale),
            "needs_reparse" => Ok(Self::NeedsReparse),
            "unresolved" => Ok(Self::Unresolved),
            _ => Err(format!("Unknown verification status: {}", s)),
        }
    }
}
