//! Export trigger and shared result notice for dockable panels.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
};
use gpui_kit::{App, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div};

use crate::components::{failure_block, notice};
use crate::i18n::{locale, t};
use crate::services::export_service::{self, ExportStatus};
use crate::views::PanelKind;

/// Button shown in a panel's dock title bar when it has a grid to export.
pub fn export_button<T: 'static>(
    kind: PanelKind,
    project_id: Option<i64>,
    cx: &mut Context<T>,
) -> impl IntoElement {
    let label = SharedString::from(t("export.current", locale(cx)));
    Button::new(kind.panel_name())
        .small()
        .ghost()
        .icon(IconName::Download)
        .label(label)
        .on_click(cx.listener(move |_, _, _, cx| {
            _ = export_service::export(kind, project_id, cx);
        }))
}

/// Result of the last export belonging to `kind`, when present.
pub fn export_notice(kind: PanelKind, cx: &App) -> Option<impl IntoElement> {
    let status = ExportStatus::global(cx);
    let status = status.read(cx);
    if status.kind != Some(kind) {
        return None;
    }
    let locale = locale(cx);
    if let Some(failure) = &status.failure {
        return Some(failure_block(&failure.to_failure(), locale, cx).into_any_element());
    }
    status.saved.as_ref().map(|path| {
        h_flex()
            .gap_2()
            .child(notice(t("export.saved", locale), true, cx))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(path.clone()),
            )
            .into_any_element()
    })
}
