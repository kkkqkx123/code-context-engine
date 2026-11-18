//! Two-step confirmation for destructive actions.
//!
//! Every panel that can destroy state used to bind the destructive call
//! straight to a button. A single misclick dropped an index, a project or a
//! Qdrant volume. The confirmation here is deliberately local to the panel: the
//! first click arms the row and the button turns into a confirm/cancel pair in
//! place, so the user commits while still looking at the scope they are about
//! to destroy. A window-level modal would also work, but it hides that scope.
//!
//! [`Confirm`] is plain state so the arming rule is testable without a window.

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
};
use gpui_kit::{App, Div, ParentElement as _, SharedString, Styled as _, Window, div, px};

use crate::i18n::t;
use crate::metrics::CARD_RADIUS;
use crate::settings::Locale;

/// Which action is currently armed, if any.
///
/// The key is a small integer the panel chooses; it only has to be unique
/// within one panel, so a row index or a fixed action number both work.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Confirm {
    armed: Option<usize>,
}

impl Confirm {
    pub fn none() -> Self {
        Self::default()
    }

    /// Ask whether `key` may run now.
    ///
    /// The first call arms the action and returns `false`; the second call for
    /// the same key clears the arming and returns `true`. Asking for a
    /// different key re-arms instead of running, so two different destructive
    /// buttons can never chain into one click each.
    pub fn request(&mut self, key: usize) -> bool {
        if self.armed == Some(key) {
            self.armed = None;
            true
        } else {
            self.armed = Some(key);
            false
        }
    }

    pub fn is_armed(&self, key: usize) -> bool {
        self.armed == Some(key)
    }

    pub fn armed(&self) -> Option<usize> {
        self.armed
    }

    /// Disarm; called on cancel and after any completed action.
    pub fn cancel(&mut self) {
        self.armed = None;
    }
}

/// Inline confirmation strip: impact sentence, confirm and cancel.
///
/// `on_confirm` and `on_cancel` are handed the click event so the caller can
/// keep its existing listener style.
pub fn confirm_bar<F, G>(
    key: &'static str,
    detail: SharedString,
    locale: Locale,
    cx: &App,
    on_confirm: F,
    on_cancel: G,
) -> Div
where
    F: Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
    G: Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
{
    let theme = cx.theme();
    h_flex()
        .gap_2()
        .items_center()
        .p_2()
        .rounded(px(CARD_RADIUS))
        .border_1()
        .border_color(theme.danger)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_color(theme.danger)
                .child(detail),
        )
        .child(
            Button::new(format!("{key}-confirm"))
                .small()
                .label(t("confirm.yes", locale))
                .on_click(on_confirm),
        )
        .child(
            Button::new(format!("{key}-cancel"))
                .small()
                .ghost()
                .label(t("confirm.no", locale))
                .on_click(on_cancel),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_request_arms_and_second_runs() {
        let mut confirm = Confirm::none();
        assert!(!confirm.request(0));
        assert!(confirm.is_armed(0));
        assert!(confirm.request(0));
        assert!(!confirm.is_armed(0));
    }

    #[test]
    fn a_different_key_rearms_instead_of_running() {
        let mut confirm = Confirm::none();
        assert!(!confirm.request(0));
        assert!(!confirm.request(1));
        assert_eq!(confirm.armed(), Some(1));
    }

    #[test]
    fn cancel_disarms() {
        let mut confirm = Confirm::none();
        assert!(!confirm.request(3));
        confirm.cancel();
        assert_eq!(confirm.armed(), None);
        assert!(!confirm.request(3), "a cancelled action needs a fresh arm");
    }
}
