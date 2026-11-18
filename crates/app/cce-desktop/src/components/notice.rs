//! Shared feedback blocks: in-flight, failure and hint rows.
//!
//! Panels render exactly one of these three states around their result area,
//! so a failed call never looks like an empty result and a running call never
//! looks idle.

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    spinner::Spinner,
    v_flex,
};
use gpui_kit::{App, Div, ParentElement as _, SharedString, Styled as _, div, px};

use crate::i18n::t;
use crate::metrics::SECTION_GAP;
use crate::navigator::Target;
use crate::services::request::Failure;
use crate::settings::Locale;
use crate::state::AppState;

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
    if !failure.candidates.is_empty() {
        block = block.child(candidate_row(failure, locale, cx));
    }
    block
}

/// Candidate entities for a failure the server could not resolve alone.
///
/// Picking one opens it in the entity panel, which is where the caller wanted
/// to go in the first place: the seed was ambiguous, not wrong.
fn candidate_row(failure: &Failure, locale: Locale, cx: &App) -> Div {
    let theme = cx.theme();
    let mut list = v_flex().gap_1().w_full().child(
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(SharedString::from(t("common.candidates", locale))),
    );
    for candidate in &failure.candidates {
        let id = candidate.stable_id.clone();
        list = list.child(
            Button::new(SharedString::from(format!("candidate-{id}")))
                .small()
                .ghost()
                .label(SharedString::from(format!(
                    "{} / {} / {}",
                    candidate.scoped_name, candidate.kind, candidate.file_path
                )))
                .on_click(move |_, _, cx| {
                    let id = id.clone();
                    AppState::global(cx).update(cx, |state, cx| {
                        state.navigate(Target::Entity { id: id.into() }, cx);
                    });
                }),
        );
    }
    list
}

/// Muted hint line used under inputs and above result areas.
pub fn hint(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// Warning shown when a payload was served from an older relation epoch than
/// the one the server is on now.
///
/// The result is kept, not reloaded: an automatic reload would collapse a
/// neighbourhood the user just expanded. The caller pairs this note with a
/// replay button, so refreshing stays an explicit act.
pub fn stale_epoch(
    served: Option<i64>,
    current: Option<i64>,
    locale: Locale,
    cx: &App,
) -> Option<Div> {
    let served = served?;
    let current = current?;
    if current <= served {
        return None;
    }
    let theme = cx.theme();
    Some(
        v_flex()
            .gap_1()
            .p_2()
            .rounded(px(crate::metrics::CARD_RADIUS))
            .border_1()
            .border_color(theme.warning)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.warning)
                    .child(SharedString::from(format!(
                        "{} (epoch {served} \u{2192} {current})",
                        t("common.staleEpoch", locale)
                    ))),
            ),
    )
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
