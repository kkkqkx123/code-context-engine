//! Local CSV export of the grid a panel currently displays.
//!
//! The service owns capture, formatting and writing. Views hand over the
//! columns and rows they already rendered; no backend call is repeated and no
//! result list is accumulated across pages.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use directories::UserDirs;
use gpui_kit::{App, AppContext as _, Entity, Global, SharedString};

use crate::components::{Cell, Column};
use crate::services::request::Failure;
use crate::services::time;
use crate::settings;
use crate::views::PanelKind;

/// Full text of the table a panel rendered, independent of styling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSnapshot {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl TableSnapshot {
    fn from_grid(columns: &[Column], rows: &[Vec<Cell>]) -> Self {
        Self {
            headers: columns
                .iter()
                .map(|column| column.title.to_string())
                .collect(),
            rows: rows
                .iter()
                .map(|row| {
                    (0..columns.len())
                        .map(|index| {
                            row.get(index)
                                .map(|cell| cell.text.to_string())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .collect(),
        }
    }
}

/// Why an export could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFailure {
    pub key: &'static str,
    pub detail: String,
}

impl ExportFailure {
    pub fn to_failure(&self) -> Failure {
        Failure {
            key: self.key,
            detail: SharedString::from(self.detail.clone()),
            candidates: Vec::new(),
        }
    }
}

/// Last outcome, shown by the panel the export came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportStatus {
    pub kind: Option<PanelKind>,
    pub saved: Option<SharedString>,
    pub failure: Option<ExportFailure>,
    pub generation: u64,
}

impl ExportStatus {
    pub fn init(cx: &mut App) {
        let entity = cx.new(|_| Self {
            kind: None,
            saved: None,
            failure: None,
            generation: 0,
        });
        cx.set_global(ExportStatusGlobal(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<ExportStatusGlobal>().0.clone()
    }
}

struct ExportStatusGlobal(Entity<ExportStatus>);

impl Global for ExportStatusGlobal {}

struct CapturesGlobal(Mutex<BTreeMap<PanelKind, TableSnapshot>>);

impl Global for CapturesGlobal {}

/// Record a rendered grid for its owning panel.
pub fn capture(kind: PanelKind, columns: &[Column], rows: &[Vec<Cell>], cx: &mut App) {
    let snapshot = TableSnapshot::from_grid(columns, rows);
    let mut captures = cx
        .global::<CapturesGlobal>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let changed = captures.get(&kind) != Some(&snapshot);
    captures.insert(kind, snapshot);
    drop(captures);
    if changed {
        let status = ExportStatus::global(cx);
        status.update(cx, |this, cx| {
            this.kind = Some(kind);
            this.generation += 1;
            cx.notify();
        });
    }
}

/// Read the captured grid of a panel.
pub fn snapshot(kind: PanelKind, cx: &App) -> Option<TableSnapshot> {
    cx.global::<CapturesGlobal>()
        .0
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .get(&kind)
        .cloned()
}

/// CSV with a UTF-8 BOM and CRLF lines. Every field is quoted so commas,
/// newlines and quotes need no dialect negotiation.
pub fn render_csv(table: &TableSnapshot) -> String {
    let mut lines = Vec::with_capacity(table.rows.len() + 1);
    lines.push(quote_row(&table.headers));
    lines.extend(table.rows.iter().map(|row| quote_row(row)));
    format!("\u{feff}{}\r\n", lines.join("\r\n"))
}

fn quote_row(fields: &[String]) -> String {
    fields
        .iter()
        .map(|field| format!("\"{}\"", field.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",")
}

/// System download directory, with the config directory as a safe fallback.
pub fn export_dir() -> PathBuf {
    UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(Path::to_path_buf))
        .or_else(|| settings::Settings::config_dir().map(|dir| dir.join("exports")))
        .unwrap_or_else(|| PathBuf::from("exports"))
}

/// Stable file name: panel identity, project scope and a UTC stamp.
pub fn file_name(kind: PanelKind, project_id: Option<i64>, secs: u64) -> String {
    let scope = project_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "global".to_string());
    format!(
        "cce-{}-{scope}-{}.csv",
        kind.file_slug(),
        time::file_stamp(secs)
    )
}

/// Candidate path for a collision counter. Counters start at one.
pub fn candidate(dir: &Path, name: &str, sequence: u32) -> PathBuf {
    if sequence == 1 {
        dir.join(name)
    } else {
        let stem = name.strip_suffix(".csv").unwrap_or(name);
        dir.join(format!("{stem}-{sequence}.csv"))
    }
}

/// Write the captured grid of `kind` and return the final path.
pub fn export(
    kind: PanelKind,
    project_id: Option<i64>,
    cx: &mut App,
) -> Result<PathBuf, ExportFailure> {
    let outcome = export_inner(kind, project_id, cx);
    match &outcome {
        Ok(path) => publish(
            kind,
            Some(SharedString::from(path.display().to_string())),
            None,
            cx,
        ),
        Err(failure) => publish(kind, None, Some(failure.clone()), cx),
    }
    outcome
}

fn export_inner(
    kind: PanelKind,
    project_id: Option<i64>,
    cx: &App,
) -> Result<PathBuf, ExportFailure> {
    let Some(table) = snapshot(kind, cx) else {
        return Err(ExportFailure {
            key: "export.idle",
            detail: String::new(),
        });
    };
    if table.rows.is_empty() {
        return Err(ExportFailure {
            key: "export.empty",
            detail: String::new(),
        });
    }

    let dir = export_dir();
    if let Err(err) = fs::create_dir_all(&dir) {
        return Err(ExportFailure {
            key: "export.failed",
            detail: err.to_string(),
        });
    }

    let name = file_name(kind, project_id, time::unix_seconds());
    let content = render_csv(&table);
    let mut last_error = String::new();
    for sequence in 1..=999 {
        let path = candidate(&dir, &name, sequence);
        if path.exists() {
            continue;
        }
        let temporary = path.with_extension(format!("csv.tmp-{sequence}"));
        match fs::write(&temporary, content.as_bytes()) {
            Ok(()) => match fs::rename(&temporary, &path) {
                Ok(()) => return Ok(path),
                Err(err) => {
                    last_error = err.to_string();
                    _ = fs::remove_file(&temporary);
                }
            },
            Err(err) => {
                last_error = err.to_string();
                _ = fs::remove_file(&temporary);
            }
        }
    }
    Err(ExportFailure {
        key: "export.failed",
        detail: if last_error.is_empty() {
            "no free file name".to_string()
        } else {
            last_error
        },
    })
}

fn publish(
    kind: PanelKind,
    saved: Option<SharedString>,
    failure: Option<ExportFailure>,
    cx: &mut App,
) {
    let status = ExportStatus::global(cx);
    status.update(cx, |this, cx| {
        this.kind = Some(kind);
        this.saved = saved;
        this.failure = failure;
        this.generation += 1;
        cx.notify();
    });
}

/// Initialize the global capture and status stores.
pub fn init(cx: &mut App) {
    if cx.try_global::<CapturesGlobal>().is_none() {
        cx.set_global(CapturesGlobal(Mutex::new(BTreeMap::new())));
    }
    ExportStatus::init(cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> TableSnapshot {
        TableSnapshot {
            headers: vec!["name".to_string(), "note".to_string()],
            rows: vec![
                vec!["a,b".to_string(), "say \"hi\"".to_string()],
                vec!["line\nbreak".to_string(), String::new()],
            ],
        }
    }

    #[test]
    fn csv_quotes_every_field() {
        let csv = render_csv(&table());
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("\"name\",\"note\"\r\n"));
        assert!(csv.contains("\"a,b\",\"say \"\"hi\"\"\"\r\n"));
        assert!(csv.contains("\"line\nbreak\",\"\"\r\n"));
    }

    #[test]
    fn grid_snapshot_pads_and_ignores_extra_cells() {
        let columns = vec![Column::fixed("a", 10.0), Column::grow("b")];
        let rows = vec![
            vec![Cell::new("x")],
            vec![Cell::new("1"), Cell::new("2"), Cell::new("ignored")],
        ];
        let snapshot = TableSnapshot::from_grid(&columns, &rows);
        assert_eq!(snapshot.headers, vec!["a", "b"]);
        assert_eq!(snapshot.rows[0], vec!["x", ""]);
        assert_eq!(snapshot.rows[1], vec!["1", "2"]);
    }

    #[test]
    fn file_names_are_scope_specific() {
        assert_eq!(
            file_name(PanelKind::Search, Some(7), 0),
            "cce-search-7-19700101T000000Z.csv"
        );
        assert_eq!(
            file_name(PanelKind::Health, None, 0),
            "cce-health-global-19700101T000000Z.csv"
        );
    }

    #[test]
    fn collision_suffix_keeps_the_csv_extension() {
        assert_eq!(
            candidate(Path::new("/tmp"), "a.csv", 1).to_string_lossy(),
            "/tmp/a.csv"
        );
        assert_eq!(
            candidate(Path::new("/tmp"), "a.csv", 2).to_string_lossy(),
            "/tmp/a-2.csv"
        );
    }
}
