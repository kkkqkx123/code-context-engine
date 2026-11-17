//! Centered empty-state block shared by every desktop panel.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, App, Div, FontWeight, ParentElement as _, SharedString, Styled as _, div, px,
};

use crate::metrics::SECTION_GAP;

/// Empty state: title, optional hint and an optional action row.
pub fn empty_state(
    title: impl Into<SharedString>,
    hint: Option<SharedString>,
    action: Option<AnyElement>,
    cx: &App,
) -> Div {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .items_center()
        .justify_center()
        .gap(px(SECTION_GAP))
        .p_8()
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .children(hint.map(|hint| div().text_sm().text_color(muted).child(hint)))
        .children(action)
}
