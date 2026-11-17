//! Shared feedback blocks: in-flight, failure and hint rows.
//!
//! Panels render exactly one of these three states around their result area,
//! so a failed call never looks like an empty result and a running call never
//! looks idle.

use gpui_kit::component::{ActiveTheme as _, h_flex, spinner::Spinner, v_flex};
use gpui_kit::{App, Div, ParentElement as _, SharedString, Styled as _, div, px};

use crate::i18n::t;
use crate::metrics::SECTION_GAP;
use crate::services::request::Failure;
use crate::settings::Locale;

/// In-flight row: spinner plus the action being performed.
pub fn loading_row(label: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .gap_2()
        .items_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(Spinner::new())
        .child(label.into())
}

/// Failure block: the localised failure class above the transport detail.
///
/// The detail stays untranslated on purpose: it names the method, path and
/// status the server answered with, which is what makes a report actionable.
pub fn failure_block(failure: &Failure, locale: Locale, cx: &App) -> Div {
    let theme = cx.theme();
    let mut block = v_flex()
        .gap_1()
        .p_3()
        .rounded(px(crate::metrics::CARD_RADIUS))
        .border_1()
        .border_color(theme.danger)
        .child(
            div()
                .text_sm()
                .text_color(theme.danger)
                .child(SharedString::from(t(failure.key, locale))),
        );
    if !failure.detail.is_empty() {
        block = block.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(failure.detail.clone()),
        );
    }
    block
}

/// Muted hint line used under inputs and above result areas.
pub fn hint(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// Success or neutral notice shown after an action completed.
pub fn notice(text: impl Into<SharedString>, positive: bool, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .text_sm()
        .text_color(if positive {
            theme.success
        } else {
            theme.warning
        })
        .child(text.into())
}

/// Vertical stack used by panels to keep result spacing uniform.
pub fn stack() -> Div {
    v_flex().gap(px(SECTION_GAP)).w_full()
}
