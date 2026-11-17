//! Brand palette applied on top of the bundled GPUI Kit theme.
//!
//! The web studio is the visual reference: black call-to-action buttons, an
//! orange-red brand accent reserved for emphasis, selection and focus, grey
//! raised surfaces on white cards, and a dark chrome rail. Colours come from
//! `design/tokens.json`, the same catalogue the web stylesheet is checked
//! against; `apply` rewrites the semantic colour tokens once per theme switch
//! so every gpui-kit component inherits the same palette.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, SharedString, Window};

use crate::design_tokens::{self, Section};
use crate::settings::ThemeModeSetting;

/// Semantic colours in web token order. Every field resolves from the shared
/// catalogue; `load` returns `None` when any required token is absent so a
/// half-applied palette can never render.
struct Palette {
    background: Hsla,
    surface: Hsla,
    raised: Hsla,
    text: Hsla,
    text_muted: Hsla,
    border: Hsla,
    cta: Hsla,
    cta_hover: Hsla,
    cta_active: Hsla,
    cta_text: Hsla,
    accent_soft: Hsla,
    accent: Hsla,
    link: Hsla,
    link_hover: Hsla,
    info: Hsla,
    success: Hsla,
    warning: Hsla,
    danger: Hsla,
    chrome_bg: Hsla,
    chrome_fg: Hsla,
    chrome_line: Hsla,
    chrome_raised: Hsla,
    grid: Hsla,
    info_bg: Hsla,
    success_bg: Hsla,
    warning_bg: Hsla,
    danger_bg: Hsla,
    node_type: Hsla,
    node_type_bg: Hsla,
    node_callable: Hsla,
    node_callable_bg: Hsla,
}

impl Palette {
    fn load() -> Option<Self> {
        let color = |section: Section, name: &str| design_tokens::color(section, name);
        Some(Self {
            background: color(Section::Ops, "--ops-surface")?,
            surface: color(Section::Ops, "--white")?,
            raised: color(Section::Ops, "--ops-surface-raised")?,
            text: color(Section::Ops, "--ops-text")?,
            text_muted: color(Section::Ops, "--ops-text-muted")?,
            border: color(Section::Ops, "--ops-border")?,
            cta: color(Section::Ops, "--black")?,
            cta_hover: color(Section::Accent, "--accent")?,
            cta_active: design_tokens::desktop_accent_active()?,
            cta_text: color(Section::Ops, "--white")?,
            accent_soft: color(Section::Accent, "--accent-bg")?,
            accent: color(Section::Accent, "--accent")?,
            link: color(Section::Ops, "--ops-link")?,
            link_hover: color(Section::Ops, "--black")?,
            info: color(Section::Status, "--info")?,
            success: color(Section::Status, "--success")?,
            warning: color(Section::Status, "--warning")?,
            danger: color(Section::Status, "--danger")?,
            chrome_bg: color(Section::Chrome, "--chrome-bg")?,
            chrome_fg: color(Section::Chrome, "--chrome-fg")?,
            chrome_line: color(Section::Chrome, "--chrome-input-border")?,
            chrome_raised: color(Section::Chrome, "--chrome-input-bg")?,
            grid: color(Section::Gray, "--gray-200")?,
            info_bg: color(Section::StatusBg, "--info-bg")?,
            success_bg: color(Section::StatusBg, "--success-bg")?,
            warning_bg: color(Section::StatusBg, "--warning-bg")?,
            danger_bg: color(Section::StatusBg, "--danger-bg")?,
            node_type: color(Section::Nodes, "--node-type-border")?,
            node_type_bg: color(Section::Nodes, "--node-type-bg")?,
            node_callable: color(Section::Nodes, "--node-callable-border")?,
            node_callable_bg: color(Section::Nodes, "--node-callable-bg")?,
        })
    }
}

/// Change the theme and reapply the brand overrides.
///
/// `Theme::change` reloads the registered theme and drops every override, so
/// callers must go through this single entry point instead of pairing the two
/// calls themselves.
pub fn set_mode(mode: ThemeModeSetting, window: Option<&mut Window>, cx: &mut App) {
    Theme::change(to_theme_mode(mode, cx), window, cx);
    apply(cx);
}

fn to_theme_mode(mode: ThemeModeSetting, cx: &App) -> ThemeMode {
    crate::app::to_theme_mode(mode, cx)
}

/// Apply the brand palette to the active theme. Tokens that fail to resolve
/// keep the base theme value; the unit tests fail first because the bundled
/// catalogue is required to be complete.
pub fn apply(cx: &mut App) {
    let Some(palette) = Palette::load() else {
        log::error!("design token catalogue is incomplete; keeping the base theme");
        return;
    };
    let white = palette.cta_text;

    Theme::update(cx, |theme| {
        theme.colors.background = palette.background;
        theme.colors.foreground = palette.text;

        theme.colors.popover = palette.surface;
        theme.colors.popover_foreground = palette.text;
        theme.colors.secondary = palette.raised;
        theme.colors.secondary_hover = palette.raised;
        theme.colors.secondary_active = palette.raised;
        theme.colors.secondary_foreground = palette.text;
        theme.colors.muted = palette.raised;
        theme.colors.muted_foreground = palette.text_muted;
        theme.colors.group_box = palette.surface;
        theme.colors.group_box_foreground = palette.text;
        theme.colors.accordion = palette.surface;
        theme.colors.caret = palette.text_muted;

        theme.colors.border = palette.border;
        theme.colors.input = palette.border;
        theme.colors.scrollbar_thumb = palette.border;
        theme.colors.window_border = palette.border;

        theme.colors.primary = palette.cta;
        theme.colors.primary_hover = palette.cta_hover;
        theme.colors.primary_active = palette.cta_active;
        theme.colors.primary_foreground = white;
        theme.colors.accent = palette.accent_soft;
        theme.colors.accent_foreground = palette.accent;
        theme.colors.ring = palette.accent;

        theme.colors.button = palette.surface;
        theme.colors.button_hover = palette.raised;
        theme.colors.button_active = palette.raised;
        theme.colors.button_foreground = palette.text;
        theme.colors.button_primary = palette.cta;
        theme.colors.button_primary_hover = palette.cta_hover;
        theme.colors.button_primary_active = palette.cta_active;
        theme.colors.button_primary_foreground = white;
        theme.colors.button_secondary = palette.surface;
        theme.colors.button_secondary_hover = palette.raised;
        theme.colors.button_secondary_active = palette.raised;
        theme.colors.button_secondary_foreground = palette.text;
        theme.colors.button_danger = palette.danger;
        theme.colors.button_danger_hover = palette.danger;
        theme.colors.button_danger_active = palette.danger;
        theme.colors.button_danger_foreground = white;
        theme.colors.button_info = palette.info;
        theme.colors.button_info_hover = palette.info;
        theme.colors.button_info_active = palette.info;
        theme.colors.button_info_foreground = white;
        theme.colors.button_success = palette.success;
        theme.colors.button_success_hover = palette.success;
        theme.colors.button_success_active = palette.success;
        theme.colors.button_success_foreground = white;
        theme.colors.button_warning = palette.warning;
        theme.colors.button_warning_hover = palette.warning;
        theme.colors.button_warning_active = palette.warning;
        theme.colors.button_warning_foreground = white;

        theme.colors.info = palette.info;
        theme.colors.info_hover = palette.info;
        theme.colors.info_active = palette.info;
        theme.colors.info_foreground = white;
        theme.colors.success = palette.success;
        theme.colors.success_hover = palette.success;
        theme.colors.success_active = palette.success;
        theme.colors.success_foreground = white;
        theme.colors.warning = palette.warning;
        theme.colors.warning_foreground = white;
        theme.colors.danger = palette.danger;
        theme.colors.danger_hover = palette.danger;
        theme.colors.danger_active = palette.danger;
        theme.colors.danger_foreground = white;

        theme.colors.link = palette.link;
        theme.colors.link_hover = palette.link_hover;
        theme.colors.link_active = palette.accent;

        theme.colors.table = palette.surface;
        theme.colors.table_even = palette.surface;
        theme.colors.table_head = palette.raised;
        theme.colors.table_head_foreground = palette.text_muted;
        theme.colors.table_hover = palette.raised;
        theme.colors.table_row_border = palette.border;
        theme.colors.table_active = palette.accent_soft;
        theme.colors.table_active_border = palette.accent;

        theme.colors.list = palette.surface;
        theme.colors.list_even = palette.surface;
        theme.colors.list_head = palette.raised;
        theme.colors.list_hover = palette.raised;
        theme.colors.list_active = palette.accent_soft;
        theme.colors.list_active_border = palette.accent;

        theme.colors.title_bar = palette.surface;
        theme.colors.title_bar_border = palette.border;
        theme.colors.status_bar = palette.surface;
        theme.colors.status_bar_border = palette.border;

        theme.colors.tab = palette.raised;
        theme.colors.tab_foreground = palette.text_muted;
        theme.colors.tab_bar = palette.raised;
        theme.colors.tab_bar_segmented = palette.surface;
        theme.colors.tab_active = palette.surface;
        theme.colors.tab_active_foreground = palette.text;

        theme.colors.sidebar = palette.chrome_bg;
        theme.colors.sidebar_foreground = palette.chrome_fg;
        theme.colors.sidebar_accent = palette.chrome_raised;
        theme.colors.sidebar_accent_foreground = palette.chrome_fg;
        theme.colors.sidebar_border = palette.chrome_line;
        theme.colors.sidebar_primary = palette.accent;
        theme.colors.sidebar_primary_foreground = white;

        theme.colors.selection = palette.accent_soft;
        theme.colors.skeleton = palette.border;
        theme.colors.progress_bar = palette.accent;

        theme.colors.chart_1 = palette.info;
        theme.colors.chart_2 = palette.accent;
        theme.colors.chart_3 = palette.danger;
        theme.colors.chart_4 = palette.node_type;
        theme.colors.chart_5 = palette.warning;
        theme.colors.chart_bullish = palette.success;
        theme.colors.chart_bearish = palette.danger;
        theme.colors.chart_grid = palette.grid;
        theme.colors.drag_border = palette.accent;
        theme.colors.drop_target = palette.accent_soft;

        theme.colors.red = palette.danger;
        theme.colors.red_light = palette.danger_bg;
        theme.colors.green = palette.success;
        theme.colors.green_light = palette.success_bg;
        theme.colors.blue = palette.info;
        theme.colors.blue_light = palette.info_bg;
        theme.colors.yellow = palette.warning;
        theme.colors.yellow_light = palette.warning_bg;
        theme.colors.magenta = palette.node_type;
        theme.colors.magenta_light = palette.node_type_bg;
        theme.colors.cyan = palette.node_callable;
        theme.colors.cyan_light = palette.node_callable_bg;

        theme.font_size = gpui_kit::px(
            design_tokens::length_px(Section::Typography, "--text-body").unwrap_or(16.0),
        );
        theme.font_family = SharedString::from(design_tokens::desktop_font_family());
        theme.mono_font_size = gpui_kit::px(
            design_tokens::length_px(Section::Typography, "--text-code").unwrap_or(13.6),
        );
        theme.radius =
            gpui_kit::px(design_tokens::length_px(Section::Shape, "--radius-sm").unwrap_or(4.0));
        theme.radius_lg =
            gpui_kit::px(design_tokens::length_px(Section::Shape, "--radius-xl").unwrap_or(12.0));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_loads_from_bundled_catalogue() {
        assert!(Palette::load().is_some());
    }
}
