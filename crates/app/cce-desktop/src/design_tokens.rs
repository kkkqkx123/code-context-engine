//! Machine-readable design tokens shared with the web studio.
//!
//! `design/tokens.json` is the single source of truth. The web stylesheet is
//! checked against it by `frontend/scripts/check-design-tokens.mjs`, and the
//! desktop client reads the same file at compile time so the two clients
//! cannot drift apart through hand-copied colour constants.
//!
//! Sections mirror the `:root` groups of `frontend/src/app.css`. Every token
//! key is a globally unique CSS custom property, so lookups take the property
//! name only. `desktop` holds values with no web counterpart (system font
//! stack, pressed colour).

use std::collections::HashMap;
use std::sync::LazyLock;

use gpui_kit::{Hsla, rgb, rgba};
use serde::Deserialize;

const TOKEN_JSON: &str = include_str!("../../../../design/tokens.json");

type Vars = HashMap<String, String>;

/// Token catalogue sections the desktop client reads.
///
/// Sections mirror the `:root` groups of `frontend/src/app.css`. Groups the
/// desktop does not render yet (`spacing`, `borders`, `breakpoints`,
/// `statusDark`) stay in `tokens.json` under frontend ownership and gain a
/// variant here together with their first consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Chrome,
    Ops,
    Accent,
    Gray,
    Status,
    StatusBg,
    Shape,
    Nodes,
    Typography,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopVars {
    font_family: String,
    accent_active: String,
}

#[derive(Debug, Deserialize)]
struct TokenFile {
    #[serde(default)]
    chrome: Vars,
    #[serde(default)]
    ops: Vars,
    #[serde(default)]
    accent: Vars,
    #[serde(default)]
    gray: Vars,
    #[serde(default)]
    status: Vars,
    #[serde(default, rename = "statusBg")]
    status_bg: Vars,
    #[serde(default)]
    shape: Vars,
    #[serde(default)]
    nodes: Vars,
    #[serde(default)]
    typography: Vars,
    desktop: DesktopVars,
}

static TOKENS: LazyLock<Result<TokenFile, String>> =
    LazyLock::new(|| serde_json::from_str(TOKEN_JSON).map_err(|err| err.to_string()));

fn file() -> Option<&'static TokenFile> {
    TOKENS.as_ref().ok()
}

fn vars(section: Section) -> Option<&'static Vars> {
    let file = file()?;
    Some(match section {
        Section::Chrome => &file.chrome,
        Section::Ops => &file.ops,
        Section::Accent => &file.accent,
        Section::Gray => &file.gray,
        Section::Status => &file.status,
        Section::StatusBg => &file.status_bg,
        Section::Shape => &file.shape,
        Section::Nodes => &file.nodes,
        Section::Typography => &file.typography,
    })
}

/// Raw token text for a CSS custom property. Missing sections, missing keys
/// and an unparsable catalogue all surface as `None`.
pub fn raw(section: Section, name: &str) -> Option<&'static str> {
    vars(section)?.get(name).map(String::as_str)
}

/// Parse a CSS colour into GPUI's colour type. Hex triplets, full hex and
/// hex with alpha are supported; `var()` references and composite values
/// (borders, shadows) are not colours and return `None`.
fn parse_color(text: &str) -> Option<Hsla> {
    let text = text.trim();
    let hex = text.strip_prefix('#')?;
    let expanded = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 | 8 => hex.to_string(),
        _ => return None,
    };
    let packed = u32::from_str_radix(&expanded, 16).ok()?;
    if expanded.len() == 8 {
        Some(rgba(packed).into())
    } else {
        Some(rgb(packed).into())
    }
}

/// Semantic colour token. Returns `None` when the token is absent or is not
/// a plain hex colour.
pub fn color(section: Section, name: &str) -> Option<Hsla> {
    raw(section, name).and_then(parse_color)
}

/// Parse a CSS length into logical pixels. `px` values pass through, `rem`
/// values scale by the 16px root, and bare `0` is zero. Relative units
/// (`em`, `%`) and composite values return `None`.
pub fn length_px(section: Section, name: &str) -> Option<f32> {
    let text = raw(section, name)?.trim();
    if text == "0" {
        return Some(0.0);
    }
    if let Some(digits) = text.strip_suffix("px") {
        return digits.trim().parse::<f32>().ok();
    }
    if let Some(digits) = text.strip_suffix("rem") {
        return digits.trim().parse::<f32>().ok().map(|rem| rem * 16.0);
    }
    None
}

/// Desktop-only system font stack. Falls back to a plain sans-serif family
/// when the catalogue cannot be read, so text always renders.
pub fn desktop_font_family() -> &'static str {
    file()
        .map(|file| file.desktop.font_family.as_str())
        .unwrap_or("sans-serif")
}

/// Desktop-only pressed colour for the brand accent.
pub fn desktop_accent_active() -> Option<Hsla> {
    file().and_then(|file| parse_color(&file.desktop.accent_active))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_parses() {
        TOKENS.as_ref().expect("bundled design tokens parse");
    }

    #[test]
    fn brand_tokens_match_web_studio() {
        assert_eq!(raw(Section::Accent, "--accent"), Some("#e63600"));
        assert_eq!(raw(Section::Accent, "--accent-bg"), Some("#fef2f0"));
        assert_eq!(raw(Section::Ops, "--ops-surface"), Some("#ffffff"));
        assert_eq!(raw(Section::Ops, "--ops-text"), Some("#262626"));
        assert_eq!(raw(Section::Ops, "--ops-link"), Some("#2563eb"));
        assert_eq!(raw(Section::Ops, "--black"), Some("#0a0a0a"));
        assert_eq!(raw(Section::Chrome, "--chrome-bg"), Some("#0a0a0a"));
        assert_eq!(raw(Section::Chrome, "--chrome-fg"), Some("#fafafa"));
    }

    #[test]
    fn status_tokens_match_web_studio() {
        assert_eq!(raw(Section::Status, "--info"), Some("#2563eb"));
        assert_eq!(raw(Section::Status, "--success"), Some("#1b7a3d"));
        assert_eq!(raw(Section::Status, "--warning"), Some("#8a6d00"));
        assert_eq!(raw(Section::Status, "--danger"), Some("#c62828"));
        assert_eq!(raw(Section::Gray, "--gray-500"), Some("#737373"));
        assert_eq!(raw(Section::Nodes, "--node-type-border"), Some("#5b21b6"));
        assert_eq!(raw(Section::Nodes, "--node-callable-bg"), Some("#eff6ff"));
    }

    #[test]
    fn metric_tokens_match_web_studio() {
        assert_eq!(length_px(Section::Shape, "--radius-sm"), Some(4.0));
        assert_eq!(length_px(Section::Typography, "--text-body"), Some(16.0));
        assert_eq!(length_px(Section::Typography, "--tracking-tight"), None);
        assert_eq!(length_px(Section::Shape, "--radius-round"), None);
    }

    #[test]
    fn desktop_block_is_present() {
        assert!(desktop_font_family().contains("sans-serif"));
        assert!(desktop_accent_active().is_some());
    }

    #[test]
    fn missing_tokens_surface_as_none() {
        assert_eq!(raw(Section::Ops, "--does-not-exist"), None);
        assert_eq!(color(Section::Ops, "--does-not-exist"), None);
        assert_eq!(length_px(Section::Ops, "--does-not-exist"), None);
    }
}
