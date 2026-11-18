//! Row grid shared by the desktop panels.
//!
//! Panels render query results as read-only rows: no sorting, no inline
//! editing, no column reordering. That is why this is a plain layout instead
//! of the component library table, which brings selection and virtualisation
//! state a read-only answer does not need. Long answers paginate in the service
//! layer, so the grid never holds an unbounded list.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{App, Div, Hsla, ParentElement as _, SharedString, Styled as _, div, px};

use crate::metrics::{CARD_RADIUS, SECTION_GAP, card_pad};
use crate::services::export_service;
use crate::views::PanelKind;

/// Semantic tone of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Muted,
    Info,
    Success,
    Warning,
    Danger,
}

impl Tone {
    fn color(self, normal: Hsla, theme: &gpui_kit::component::ThemeColor) -> Hsla {
        match self {
            Self::Normal => normal,
            Self::Muted => theme.muted_foreground,
            Self::Info => theme.info,
            Self::Success => theme.success,
            Self::Warning => theme.warning,
            Self::Danger => theme.danger,
        }
    }
}

/// One cell of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub text: SharedString,
    pub tone: Tone,
}

impl Cell {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Normal,
        }
    }

    pub fn muted(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Muted,
        }
    }

    pub fn toned(text: impl Into<SharedString>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

/// Column definition: a title plus either a fixed width or flexible growth.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub title: SharedString,
    pub width: Option<f32>,
}

impl Column {
    /// Column that takes a fixed share of the row.
    pub fn fixed(title: impl Into<SharedString>, width: f32) -> Self {
        Self {
            title: title.into(),
            width: Some(width),
        }
    }

    /// Column that absorbs the remaining space.
    pub fn grow(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            width: None,
        }
    }
}

const CELL_TEXT_SIZE: f32 = 12.0;

fn render_cell(cell: &Cell, column: &Column, theme: &gpui_kit::component::ThemeColor) -> Div {
    let text = div()
        .truncate()
        .text_size(px(CELL_TEXT_SIZE))
        .text_color(cell.tone.color(theme.foreground, theme))
        .child(cell.text.clone());
    let mut wrapper = div().min_w_0().child(text);
    wrapper = match column.width {
        Some(width) => wrapper.w(px(width)).flex_shrink_0(),
        None => wrapper.flex_1(),
    };
    wrapper
}

/// Header plus body rows.
///
/// `rows` shorter than the column count is padded with empty cells and longer
/// rows are truncated, so a malformed row cannot shift the grid.
pub fn data_grid(
    kind: PanelKind,
    columns: &[Column],
    rows: &[Vec<Cell>],
    dense: bool,
    cx: &mut App,
) -> Div {
    export_service::capture(kind, columns, rows, cx);
    let theme = cx.theme();
    let row_pad = if dense { 4.0 } else { 8.0 };

    let header = h_flex()
        .gap_3()
        .w_full()
        .py(px(row_pad))
        .border_b_1()
        .border_color(theme.border)
        .children(columns.iter().map(|column| {
            let mut cell = div()
                .min_w_0()
                .text_size(px(CELL_TEXT_SIZE))
                .text_color(theme.muted_foreground)
                .child(column.title.clone());
            cell = match column.width {
                Some(width) => cell.w(px(width)).flex_shrink_0(),
                None => cell.flex_1(),
            };
            cell
        }));

    let body = v_flex().w_full().children(rows.iter().map(|row| {
        h_flex()
            .gap_3()
            .w_full()
            .py(px(row_pad))
            .border_b_1()
            .border_color(theme.border)
            .children(columns.iter().enumerate().map(|(ix, column)| {
                let cell = row
                    .get(ix)
                    .cloned()
                    .unwrap_or_else(|| Cell::new(SharedString::default()));
                render_cell(&cell, column, theme)
            }))
    }));

    v_flex().w_full().child(header).child(body)
}

/// Bordered surface a panel result sits on.
pub fn surface(dense: bool, cx: &App) -> Div {
    v_flex()
        .w_full()
        .gap(px(SECTION_GAP))
        .p(px(card_pad(dense) / 2.0))
        .border_1()
        .border_color(cx.theme().border)
        .rounded(px(CARD_RADIUS))
}

/// Single label and value line, used for counters above a grid.
pub fn stat_line(label: impl Into<SharedString>, value: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    h_flex()
        .gap_2()
        .items_baseline()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label.into()),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child(value.into()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tones_are_distinct() {
        assert_ne!(Tone::Normal, Tone::Muted);
        assert_ne!(Tone::Info, Tone::Danger);
    }

    #[test]
    fn fixed_and_growing_columns_differ() {
        assert_eq!(Column::fixed("score", 80.0).width, Some(80.0));
        assert_eq!(Column::grow("path").width, None);
    }

    #[test]
    fn cell_helpers_carry_the_tone() {
        assert_eq!(Cell::new("a").tone, Tone::Normal);
        assert_eq!(Cell::muted("a").tone, Tone::Muted);
        assert_eq!(Cell::toned("a", Tone::Danger).tone, Tone::Danger);
    }
}
