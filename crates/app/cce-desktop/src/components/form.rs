//! Form rows shared by the panels.
//!
//! Every panel collects a handful of text fields before it runs a command.
//! Consistency matters more than flexibility here: one label style, one gap,
//! one row shape, so a panel reads like its neighbours.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, px};

/// Label above a control.
pub fn field(label: impl Into<SharedString>, control: impl IntoElement, cx: &App) -> Div {
    v_flex()
        .gap_1()
        .min_w_0()
        .child(
            gpui_kit::div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(control)
}

/// Bordered group that holds a panel's input fields.
pub fn field_group(cx: &App) -> Div {
    v_flex()
        .gap_3()
        .w_full()
        .p(px(16.0))
        .border_1()
        .border_color(cx.theme().border)
        .rounded(px(crate::metrics::CARD_RADIUS))
}

/// Action row under the inputs.
pub fn actions() -> Div {
    h_flex().gap_2().items_center().flex_wrap()
}
