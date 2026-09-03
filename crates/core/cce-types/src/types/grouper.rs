//! Grouper type definitions for EntityGroup
//!
//! This module provides all shared type definitions used across
//! the grouper pipeline stages. Types are organized into:
//!
//! - `group`: Core entity group types (EntityGroup, GroupType, etc.)
//! - `pattern`: Pattern detection information (PatternInfo, MemberRole, etc.)
//! - `design_pattern`: Getter/Setter summary type

mod design_pattern;
mod group;
pub mod pattern;

pub use group::{
    EntityGroup, EntityMeta, GroupRole, GroupType, MemberRolesBuilder, ProcessingResult,
    ProcessingStats, SpanError, ValidationError,
};
pub use pattern::{GetterSetterSummary, MemberRole, PatternInfo, get_member_role};
