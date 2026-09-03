//! Type definitions for grouper module
//!
//! This module contains all shared type definitions used across
//! the grouper pipeline stages. Types are now defined in cce_core
//! and re-exported here for backward compatibility.

pub use cce_types::StdlibCategory;
pub use cce_types::grouper::pattern;
pub use cce_types::grouper::{
    EntityGroup, EntityMeta, GetterSetterSummary, GroupRole, GroupType, MemberRole,
    MemberRolesBuilder, PatternInfo, ProcessingResult, ProcessingStats, SpanError, ValidationError,
    get_member_role,
};
