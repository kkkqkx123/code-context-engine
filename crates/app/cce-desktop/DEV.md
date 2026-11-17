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

- Rust 1.86+ (see `rust-version` in `Cargo.toml`).
- The `crates/gpui-kit` checkout (already vendored; the `tray`,
  `autostart`, `compograph`, `global-hotkey` and `gpui-image` submodules are
  not initialized and therefore unavailable — the tray integration lands with
  them in a later stage, and closing the window quits the process until then).
- A C toolchain plus `pkg-config`.
- System libraries for font rendering and Linux windowing:
  `fontconfig`, `xcb`, `xkbcommon`.

Per-distro install commands (require root):

```sh
# Debian / Ubuntu
sudo apt install build-essential pkg-config libfontconfig-dev libxkbcommon-dev libxcb1-dev

# Fedora
sudo dnf install gcc pkgconf fontconfig-devel libxkbcommon-devel libxcb-devel

# Arch Linux
sudo pacman -S --needed base-devel pkgconf fontconfig libxkbcommon libxcb
```

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
| Binary starts but windows stay blank | No Wayland/X11 session | Run inside a desktop session or nested compositor |
