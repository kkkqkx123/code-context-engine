//! Behavior capture types

use serde::{Deserialize, Serialize};

use crate::tree_sitter_query::capture::Domain;
use crate::tree_sitter_query::parser_types::CaptureName;

crate::capture_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum BehaviorKind {
        #[serde(rename = "data.bind")]
        DataBind,
        #[serde(rename = "data.reference")]
        DataReference,
        #[serde(rename = "data.object")]
        DataObject,
        #[serde(rename = "data.array")]
        DataArray,
        #[serde(rename = "data.query")]
        DataQuery,
        #[serde(rename = "data.statement")]
        DataStatement,
        #[serde(rename = "effect.error")]
        EffectError,
        #[serde(rename = "op.shift_left")]
        OpShiftLeft,
        #[serde(rename = "op.shift_right")]
        OpShiftRight,
    }
}

impl BehaviorKind {
    /// Get the capture label without the `@behavior.` prefix.
    pub const fn capture_label(&self) -> &'static str {
        match self {
            BehaviorKind::DataBind => "data.bind",
            BehaviorKind::DataReference => "data.reference",
            BehaviorKind::DataObject => "data.object",
            BehaviorKind::DataArray => "data.array",
            BehaviorKind::DataQuery => "data.query",
            BehaviorKind::DataStatement => "data.statement",
            BehaviorKind::EffectError => "effect.error",
            BehaviorKind::OpShiftLeft => "op.shift_left",
            BehaviorKind::OpShiftRight => "op.shift_right",
        }
    }
}

/// Extract the behavior kind from a capture name.
///
/// Only main behavior captures are recognized here. Detail captures such as
/// `@behavior.data.bind.pattern` are intentionally excluded.
pub fn extract_behavior_kind(capture: &str) -> Option<BehaviorKind> {
    let parsed = CaptureName::parse(capture).ok()?;
    if parsed.domain != Domain::Behavior {
        return None;
    }
    if parsed.role.is_some() || parsed.attribute.is_some() {
        return None;
    }

    let category = parsed.category?;
    let subtype = parsed.subtype?;
    let label = format!("{}.{}", category, subtype);
    BehaviorKind::from_capture_name(&label)
}

/// Check if the capture name is a main behavior capture.
pub fn is_main_behavior_capture(capture: &str) -> bool {
    extract_behavior_kind(capture).is_some()
}

#[cfg(test)]
mod tests {
    use super::{BehaviorKind, extract_behavior_kind, is_main_behavior_capture};

    #[test]
    fn test_behavior_kind_from_capture_name() {
        assert_eq!(
            BehaviorKind::from_capture_name("data.bind"),
            Some(BehaviorKind::DataBind)
        );
        assert_eq!(
            BehaviorKind::from_capture_name("data.statement"),
            Some(BehaviorKind::DataStatement)
        );
        assert_eq!(
            BehaviorKind::from_capture_name("effect.error"),
            Some(BehaviorKind::EffectError)
        );
        assert_eq!(
            BehaviorKind::from_capture_name("op.shift_left"),
            Some(BehaviorKind::OpShiftLeft)
        );
        assert_eq!(
            BehaviorKind::from_capture_name("op.shift_right"),
            Some(BehaviorKind::OpShiftRight)
        );
    }

    #[test]
    fn test_behavior_capture_helpers() {
        assert!(is_main_behavior_capture("@behavior.data.bind"));
        assert!(!is_main_behavior_capture("@behavior.data.bind.pattern"));
        assert_eq!(
            extract_behavior_kind("@behavior.data.bind"),
            Some(BehaviorKind::DataBind)
        );
        assert_eq!(
            extract_behavior_kind("@behavior.data.statement"),
            Some(BehaviorKind::DataStatement)
        );
        assert_eq!(
            extract_behavior_kind("@behavior.op.shift_right"),
            Some(BehaviorKind::OpShiftRight)
        );
    }
}
