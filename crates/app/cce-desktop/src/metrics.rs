//! Shared layout metrics for the desktop client.
//!
//! Values with a web counterpart resolve from `design/tokens.json` so a
//! token change on either client fails the build instead of silently
//! drifting. The remaining constants describe desktop-only surfaces and are
//! documented here.

/// Horizontal page padding inside a panel body.
pub const PAGE_PAD_X: f32 = 24.0;
/// Vertical page padding inside a panel body.
pub const PAGE_PAD_Y: f32 = 32.0;
/// Card inner padding at comfortable density, matching `--space-xl`.
pub const CARD_PAD: f32 = 32.0;
/// Card inner padding at dense density, matching `--space-md`.
pub const CARD_PAD_DENSE: f32 = 16.0;
/// Card corner radius, matching `--radius-sm`.
pub const CARD_RADIUS: f32 = 4.0;
/// Card header height.
pub const CARD_HEAD_H: f32 = 52.0;
/// Default expanded sidebar width.
pub const SIDEBAR_W: f32 = 200.0;
/// Maximum content width for document-style pages, matching the web
/// `.container` cap.
pub const CONTENT_MAX_W: f32 = 1400.0;
/// Gap between stacked cards, matching `--space-md`.
pub const SECTION_GAP: f32 = 16.0;

/// Card inner padding for the active density.
pub fn card_pad(dense: bool) -> f32 {
    if dense { CARD_PAD_DENSE } else { CARD_PAD }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_tokens::{self, Section};

    #[test]
    fn radius_matches_web_studio() {
        assert_eq!(
            design_tokens::length_px(Section::Shape, "--radius-sm"),
            Some(CARD_RADIUS)
        );
    }

    #[test]
    fn card_padding_follows_density() {
        assert_eq!(card_pad(false), CARD_PAD);
        assert_eq!(card_pad(true), CARD_PAD_DENSE);
    }
}
