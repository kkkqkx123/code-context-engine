# cce-desktop Developer Guide

Native desktop client for CCE, built on GPUI. This crate is intentionally
**not** a member of the root workspace: it is its own workspace consuming
`../../gpui-kit` through a local path (nested-workspace inheritance makes a
single-workspace setup unresolvable without mirroring private entries into
the root workspace, which is rejected by design).
Always build and test with the manifest flag or from inside this directory:

```sh
cargo build --manifest-path crates/app/cce-desktop/Cargo.toml
cargo test --manifest-path crates/app/cce-desktop/Cargo.toml
# or: cd crates/app/cce-desktop && cargo build
```

Plain `cargo build` from the repository root does **not** cover this crate.

## Prerequisites

- Rust 1.97+ (see `rust-version` in `Cargo.toml`). The floor is set by
  `gpui-pre`, which calls `std::hint::cold_path`; older toolchains fail with
  `error[E0658]` inside that dependency, not inside this crate.
- The `crates/gpui-kit` checkout (already vendored) and its `tray` submodule.
  The tray never blocks startup: without a usable desktop tray host, closing
  the window still quits the app.
- A C toolchain plus `pkg-config`.
- System libraries for font rendering and Linux windowing:
  `fontconfig`, `xcb`, `xkbcommon`, `xkbcommon-x11`, `wayland`, plus the
  platform tray/desktop integration used by `ksni`. `xkbcommon-dev` alone is
  not enough: the renderer links `xkbcommon-x11`, which ships separately.
  KDE Plasma provides a StatusNotifier host; GNOME needs an AppIndicator
  extension for tray icons.

Per-distro install commands (require root):

```sh
# Debian / Ubuntu
sudo apt install build-essential pkg-config libfontconfig-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libwayland-dev

# Fedora
sudo dnf install gcc pkgconf fontconfig-devel libxkbcommon-devel \
  libxkbcommon-x11-devel libxcb-devel wayland-devel

# Arch Linux
sudo pacman -S --needed base-devel pkgconf fontconfig libxkbcommon \
  libxkbcommon-x11 libxcb wayland
```

## Build and verify

Run all three from inside this directory; each one is cheap after the first
build and together they gate a change:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Tests cover the pure layers only — parameter assembly in `services/`, the
scope and delivery rules in `state.rs`, settings round-trips, catalogue parity
in `i18n.rs`, design tokens, close decisions, layout validation, CSV rendering
and filename rules, and the small helpers behind the shared widgets. Nothing
asserts rendered pixels; dock and tray behaviour is verified by running the
binary.

## Panels

The main window has a sidebar split into two groups. Every panel is created
lazily on first navigation and dropped whenever the active project changes, so
no panel can keep showing another project's results.

| Group | Panel | Source | Capabilities |
|---|---|---|---|
| Workspace | Search | `views/search_view.rs` | Single and aggregated queries, hybrid/vector/keyword types, directory prefix, limit, skip-tests; each hit can be sent to the graph panel |
| Workspace | Graph | `views/graph_view.rs` | Neighbours, path, subgraph, components, cycles, impact, module, structural, export |
| Workspace | Entities | `views/entity_view.rs` | Detail, calls, callers, call chain, call path, inheritance, implementations, classification |
| Workspace | Index | `views/index_view.rs` | Full, incremental, single-file parse, summary, statistics |
| Workspace | Projects | `views/projects_view.rs` | List, create, delete, index, read config, reload config, switch the active scope |
| Operations | Tools | `views/tools_view.rs` | Fold, compress, diagnose, keyword, symbols, references, definition |
| Operations | Storage | `views/storage_view.rs` | Storage status, index statistics, scoped clear, delete by file, delete by entity, batch delete |
| Operations | Watch | `views/watch_view.rs` | Watch status, start with path and extension filters, stop |
| Operations | Health | `views/health_view.rs` | Aggregate probe plus qdrant, embedding, BM25 and admission diagnostics |
| Operations | Metrics | `views/metrics_view.rs` | Prometheus text, JSON snapshot, metric history, history cleanup |
| Operations | Config | `views/config_view.rs` | Server config snapshot, validation, reload |
| Operations | Tasks | `views/tasks_view.rs` | Dead letters, retry and acknowledge, retry-queue status, process, clear, clear dead |

Panels live in a dock area. The default layout keeps the four workspace
panels as centre tabs; operations panels open as tabs on demand. Layout,
active tab and dock sizes are saved to `layout.json` next to `settings.toml`.
A layout version change, an unknown panel name or a duplicate panel name
discards the saved layout and restores the default. Changing the project scope
rebuilds every panel from the current layout so no panel can keep another
project's results.

Shared behaviour, so a new panel does not reinvent it:

- `services::request::Load<T>` holds the four states every panel renders
  (idle, loading, ready, failed); `Failure` carries a message key plus the
  transport detail so errors read the same everywhere.
- `components::notice` renders the loading row, failure block, hint and empty
  state; `components::data_grid` renders read-only result tables.
- Panels that need a project read the active scope from `AppState`; with no
  scope they render an empty state telling the user to pick one first.
- Recent inputs are persisted per panel kind through `services::history`
  (capped), so a restart keeps the last queries.
- A panel's rendered grid is captured by `services::export_service`. The
  export button in the dock title bar writes the current result as CSV (UTF-8
  BOM, CRLF, every field quoted) to the system download directory, falling
  back to the config directory's `exports` folder. File names combine the
  panel identity, project scope or `global`, and a UTC stamp; existing files
  are never overwritten.

## Running

The client needs a reachable CCE server; enter the base URL (default
`http://127.0.0.1:9000`) and optionally a token in the connection view.
An empty token falls back to the `CCE_API_TOKEN` environment variable.
A running Wayland/X11 session is required for the window; on headless
machines only `cargo build` and `cargo test` are meaningful.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `Package fontconfig was not found` | Missing system fontconfig | Install system packages (see above) |
| `rust-lld: error: unable to find library -lxkbcommon` | Missing windowing dev libraries | Same as above |
| `rust-lld: error: unable to find library -lxkbcommon-x11` | `libxkbcommon-dev` installed without the X11 variant | Install `libxkbcommon-x11-dev` |
| `error[E0658]: use of unstable library feature 'cold_path'` in `gpui-pre` | Toolchain below the floor | `rustup update stable` |
| Binary starts but windows stay blank | No Wayland/X11 session | Run inside a desktop session or nested compositor |
| Panels show an empty state after connecting | No project selected | Open the Projects panel and switch to one |
| Closing the window quits even with "Close to tray" enabled | No StatusNotifier host in the session | Install the desktop's AppIndicator support; otherwise closing falls back to quit |
| Layout resets after an upgrade | Layout version changed or panel identity set changed | Expected: the default layout is restored and overwritten |
