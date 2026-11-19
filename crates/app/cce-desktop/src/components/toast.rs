//! Process-wide feedback queue.
//!
//! Panels used to report the outcome of an action into their own `outcome`
//! field, which was invisible as soon as the panel lost focus. Results that
//! concern a whole operation (a save, a delete, a failed write) go through
//! this queue instead: one overlay renders them above every panel, and the
//! entry survives a panel rebuild.
//!
//! The queue is bounded and manually dismissed. There is no timer: a message
//! that disappears on its own is exactly the message a user misses while
//! reading the diff of what just changed.

use std::sync::Mutex;

use gpui_kit::{App, AppContext as _, Entity, Global, SharedString};

use crate::i18n::{locale, t};

/// Maximum entries kept; the oldest is dropped first.
const CAPACITY: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub positive: bool,
    pub text: SharedString,
}

#[derive(Default)]
pub struct ToastState {
    items: Vec<Toast>,
    next_id: u64,
}

impl ToastState {
    pub fn push(&mut self, text: SharedString, positive: bool) {
        self.next_id += 1;
        self.items.push(Toast {
            id: self.next_id,
            positive,
            text,
        });
        while self.items.len() > CAPACITY {
            self.items.remove(0);
        }
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|toast| toast.id != id);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn items(&self) -> &[Toast] {
        &self.items
    }
}

/// Entity holding the queue; observers re-render the overlay.
pub struct Toasts(pub Mutex<Option<Entity<ToastState>>>);

impl Global for Toasts {}

/// Install the queue. Safe to call once per process.
pub fn init(cx: &mut App) {
    if cx.try_global::<Toasts>().is_some() {
        return;
    }
    let state = cx.new(|_| ToastState::default());
    cx.set_global(Toasts(Mutex::new(Some(state))));
}

/// Queue behind the global, creating it on first use.
///
/// The entity is created outside the lock: `cx.new` may run arbitrary code and
/// must never be called while the global is held.
pub fn state(cx: &mut App) -> Entity<ToastState> {
    if cx.try_global::<Toasts>().is_none() {
        init(cx);
    }
    let existing = cx
        .global::<Toasts>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clone();
    if let Some(state) = existing {
        return state;
    }
    let state = cx.new(|_| ToastState::default());
    let mut slot = cx
        .global::<Toasts>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    *slot = Some(state.clone());
    state
}

/// Report a completed action above every panel.
pub fn push(text: impl Into<SharedString>, positive: bool, cx: &mut App) {
    let state = state(cx);
    let text = text.into();
    state.update(cx, |state, cx| {
        state.push(text, positive);
        cx.notify();
    });
}

/// Report a catalogue key above every panel.
///
/// Action results are keyed the same way as the panel's inline notice, so a
/// locale switch changes both without freezing translated text in a toast.
pub fn push_key(key: &'static str, positive: bool, cx: &mut App) {
    push(t(key, locale(cx)), positive, cx);
}

/// Drop every entry; called when the connection drops, because the results
/// they describe belong to a server this window no longer talks to.
pub fn clear(cx: &mut App) {
    let state = state(cx);
    state.update(cx, |state, cx| {
        state.clear();
        cx.notify();
    });
}

/// Drop one entry.
pub fn dismiss(id: u64, cx: &mut App) {
    let state = state(cx);
    state.update(cx, |state, cx| {
        state.dismiss(id);
        cx.notify();
    });
}

/// Overlay rendered by the window root, or `None` when the queue is empty.
///
/// Rendered above every panel so a result stays visible after the panel that
/// produced it was replaced by a project-scope rebuild.
pub fn overlay(cx: &mut App) -> Option<gpui_kit::AnyElement> {
    use gpui_kit::component::{
        ActiveTheme as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    };
    use gpui_kit::{IntoElement as _, ParentElement as _, Styled as _, div, px};

    let state = state(cx);
    let items = state.read(cx).items().to_vec();
    if items.is_empty() {
        return None;
    }
    let theme = cx.theme();
    let mut column = v_flex().gap_2().w(px(340.)).items_end();
    for toast in items {
        let id = toast.id;
        column = column.child(
            h_flex()
                .gap_2()
                .items_center()
                .p_2()
                .rounded(px(crate::metrics::CARD_RADIUS))
                .border_1()
                .border_color(if toast.positive {
                    theme.success
                } else {
                    theme.danger
                })
                .bg(theme.popover)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(if toast.positive {
                            theme.success
                        } else {
                            theme.danger
                        })
                        .child(toast.text.clone()),
                )
                .child(
                    Button::new(format!("toast-{id}"))
                        .ghost()
                        .small()
                        .label("\u{00d7}")
                        .on_click(move |_, _, cx| dismiss(id, cx)),
                ),
        );
    }
    Some(
        div()
            .absolute()
            .top(px(12.))
            .right(px(12.))
            .child(column)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_carry_distinct_ids() {
        let mut state = ToastState::default();
        state.push(SharedString::from("saved"), true);
        state.push(SharedString::from("deleted"), true);
        let ids: Vec<u64> = state.items().iter().map(|toast| toast.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert!(state.items()[0].positive);
    }

    #[test]
    fn the_queue_is_bounded() {
        let mut state = ToastState::default();
        for index in 0..CAPACITY + 3 {
            state.push(SharedString::from(format!("entry {index}")), index % 2 == 0);
        }
        assert_eq!(state.items().len(), CAPACITY);
        assert_eq!(state.items()[0].text, SharedString::from("entry 3"));
    }

    #[test]
    fn dismissing_removes_only_the_named_entry() {
        let mut state = ToastState::default();
        state.push(SharedString::from("a"), true);
        state.push(SharedString::from("b"), false);
        state.dismiss(1);
        assert_eq!(state.items().len(), 1);
        assert_eq!(state.items()[0].text, SharedString::from("b"));
        state.clear();
        assert!(state.items().is_empty());
    }
}
