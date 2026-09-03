//! Group conversion association (cross-layer contract)
//!
//! Moved from `cce_parser::ast_to_nl::converter::group_converter` so the
//! plugin chunk contract (`cce_core::plugin::CodePlugin::chunk`) can reference
//! it without depending on the parser crate.

use serde::{Deserialize, Serialize};

use crate::types::ConversionResult;
use crate::types::grouper::EntityGroup;

/// Represents a group with its associated conversion results
/// Maintains the hierarchical relationship between groups and their conversions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupConversions {
    /// The original entity group
    pub group: EntityGroup,

    /// Header conversion (group-level description, e.g., class overview)
    /// May be None if the group has no header or header conversion failed
    pub header_conversion: Option<ConversionResult>,

    /// Member conversions (individual entity descriptions, e.g., method descriptions)
    /// Empty vector if the group has no members or all members were filtered out
    pub member_conversions: Vec<ConversionResult>,
}
