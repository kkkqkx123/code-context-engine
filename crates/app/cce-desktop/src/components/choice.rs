//! Segmented choice row.
//!
//! Panels pick between two and eight fixed options (query type, traversal
//! direction, cycle level, operation tab). A row of toggle buttons keeps the
//! current choice visible without opening a popup, which matters because the
//! choice changes what the run button will send.

use gpui_kit::component::{
    Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
};
use gpui_kit::{Context, Div, ParentElement as _, SharedString, Styled as _};

use crate::i18n::t;

/// Row of toggle buttons; exactly one option is active.
///
/// `set` receives the newly picked index. It is `Copy` so every button can own
/// its own copy without reaching for shared state.
pub fn choice_row<V: 'static>(
    id: &'static str,
    options: &[&'static str],
    selected: usize,
    set: impl Fn(&mut V, usize) + Copy + 'static,
    cx: &mut Context<V>,
) -> Div {
    let locale = crate::i18n::locale(cx);
    let mut row = h_flex().gap_1().items_center().flex_wrap();
    for (index, option) in options.iter().enumerate() {
        let active = index == selected;
        let mut button = Button::new(element_id(id, index))
            .small()
            .label(SharedString::from(t(option, locale)))
            .selected(active)
            .on_click(cx.listener(move |this, _, _, cx| {
                set(this, index);
                cx.notify();
            }));
        button = if active {
            button.primary()
        } else {
            button.ghost()
        };
        row = row.child(button);
    }
    row
}

/// Element id for one option of a choice row.
fn element_id(id: &'static str, index: usize) -> String {
    format!("{id}-{index}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_ids_are_unique_per_option() {
        assert_ne!(element_id("query-type", 0), element_id("query-type", 1));
        assert_eq!(element_id("query-type", 0), "query-type-0");
    }
}
