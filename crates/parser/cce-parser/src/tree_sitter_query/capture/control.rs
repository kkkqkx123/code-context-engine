//! Control-flow capture types

use serde::{Deserialize, Serialize};

use crate::tree_sitter_query::capture::Domain;
use crate::tree_sitter_query::parser_types::CaptureName;

crate::capture_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum ControlKind {
        #[serde(rename = "flow.if")]
        If,
        #[serde(rename = "flow.match")]
        Match,
        #[serde(rename = "flow.loop")]
        Loop,
        #[serde(rename = "flow.return")]
        Return,
        #[serde(rename = "flow.break")]
        Break,
        #[serde(rename = "flow.continue")]
        Continue,
        #[serde(rename = "flow.yield")]
        Yield,
        #[serde(rename = "flow.try")]
        Try,
    }
}

impl ControlKind {
    /// Get the capture label without the `@control.` prefix.
    pub const fn capture_label(&self) -> &'static str {
        match self {
            ControlKind::If => "flow.if",
            ControlKind::Match => "flow.match",
            ControlKind::Loop => "flow.loop",
            ControlKind::Return => "flow.return",
            ControlKind::Break => "flow.break",
            ControlKind::Continue => "flow.continue",
            ControlKind::Yield => "flow.yield",
            ControlKind::Try => "flow.try",
        }
    }
}

/// Extract the control-flow kind from a capture name.
pub fn extract_control_kind(capture: &str) -> Option<ControlKind> {
    let parsed = CaptureName::parse(capture).ok()?;
    if parsed.domain != Domain::Control {
        return None;
    }
    if parsed.role.is_some() || parsed.attribute.is_some() {
        return None;
    }

    let category = parsed.category?;
    let subtype = parsed.subtype?;
    let label = format!("{}.{}", category, subtype);
    ControlKind::from_capture_name(&label)
}

/// Check if the capture name is a main control-flow capture.
pub fn is_main_control_capture(capture: &str) -> bool {
    extract_control_kind(capture).is_some()
}

#[cfg(test)]
mod tests {
    use super::{ControlKind, extract_control_kind, is_main_control_capture};

    #[test]
    fn test_control_kind_from_capture_name() {
        assert_eq!(
            ControlKind::from_capture_name("flow.if"),
            Some(ControlKind::If)
        );
        assert_eq!(
            ControlKind::from_capture_name("flow.return"),
            Some(ControlKind::Return)
        );
        assert_eq!(
            ControlKind::from_capture_name("flow.try"),
            Some(ControlKind::Try)
        );
    }

    #[test]
    fn test_control_capture_helpers() {
        assert!(is_main_control_capture("@control.flow.if"));
        assert!(!is_main_control_capture("@control.flow.if.condition"));
        assert_eq!(
            extract_control_kind("@control.flow.if"),
            Some(ControlKind::If)
        );
    }
}
