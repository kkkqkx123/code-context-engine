//! Panel header and page body shared by every desktop panel.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, FontWeight, InteractiveElement as _, ParentElement as _, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, div, px,
};

use crate::design_tokens::{self, Section};
use crate::metrics::{CARD_HEAD_H, PAGE_PAD_X, PAGE_PAD_Y};

fn title_size() -> f32 {
    design_tokens::length_px(Section::Typography, "--text-title").unwrap_or(20.0)
}

/// Panel header: title, optional subtitle and trailing actions.
pub fn panel_header(
    title: impl Into<SharedString>,
    subtitle: Option<SharedString>,
    actions: Option<AnyElement>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let mut titles = v_flex().gap_0p5().min_w_0().child(
        div()
            .text_size(px(title_size()))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.foreground)
            .child(title.into()),
    );
    if let Some(subtitle) = subtitle {
        titles = titles.child(
            div()
                .text_size(px(12.))
                .text_color(theme.muted_foreground)
                .child(subtitle),
        );
    }

    h_flex()
        .h(px(CARD_HEAD_H))
        .w_full()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap_3()
        .px(px(PAGE_PAD_X))
        .child(titles)
        .children(actions)
}

/// Scrollable panel body with the page padding.
pub fn page_body(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_y_scroll()
        .px(px(PAGE_PAD_X))
        .py(px(PAGE_PAD_Y))
}
