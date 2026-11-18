//! Desktop application entry: initialization order and main window.

use gpui_kit::component::ThemeMode;
use gpui_kit::{
    App, AppContext as _, Bounds, KeyBinding, Size, WindowBounds, WindowOptions, point, px, size,
};

use crate::connection::ClientStore;
use crate::runtime;
use crate::services::connection_service;
use crate::settings::{self, Settings, WindowGeometry};
use crate::state;
use crate::views::Shell;

pub mod actions {
    gpui_kit::actions!(
        cce,
        [
            Quit,
            ShowWindow,
            ToggleCloseToTray,
            ThemeLight,
            ThemeSystem,
            ToggleDensity,
            ToggleLocale
        ]
    );
}

pub use actions::*;

/// Default window size when no geometry has been persisted yet.
const DEFAULT_WINDOW_SIZE: (f32, f32) = (1280., 800.);
const MIN_WINDOW_SIZE: (f32, f32) = (960., 600.);

/// Entry point: initialize layers and globals, then open the shell window.
pub fn run(settings: Settings) {
    let app = gpui_kit::application().with_assets(gpui_kit::assets::AllAssets);
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);

        settings::init_global(settings, cx);
        state::AppState::init(cx);
        crate::components::toast::init(cx);
        crate::services::refresh::init(cx);
        init_runtime(cx);
        ClientStore::init(cx);
        crate::dock_layout::init(cx);
        crate::dock_layout::register_panels(cx);

        bind_keys(cx);
        apply_saved_theme(cx);
        start_heartbeat(cx);
        // One refresh loop for every polling panel; the task is detached
        // because it lives as long as the app does.
        let _refresh = crate::services::refresh::start(cx);
        crate::tray::init(cx);
        open_main_window(cx);
    });
}

/// Start the tokio reactor the client stack polls on. A failure only disables
/// networking; the UI still comes up and reports the client-side error.
fn init_runtime(cx: &mut App) {
    match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime::init(runtime, cx),
        Err(err) => log::error!("failed to start the async runtime: {err}"),
    }
}

/// Keep the background health probe alive for the whole app run.
fn start_heartbeat(cx: &mut App) {
    let heartbeat = connection_service::start_heartbeat(cx);
    cx.global_mut::<ClientStore>().set_heartbeat(heartbeat);
}

fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-shift-l", ThemeLight, None),
        KeyBinding::new("secondary-shift-s", ThemeSystem, None),
        KeyBinding::new("secondary-d", ToggleDensity, None),
        KeyBinding::new("secondary-l", ToggleLocale, None),
    ]);
    cx.on_action(|_: &ThemeLight, cx| {
        crate::settings::update(cx, |settings| {
            settings.theme_mode = crate::settings::ThemeModeSetting::Light;
        });
        crate::theme::set_mode(crate::settings::ThemeModeSetting::Light, None, cx);
    });
    cx.on_action(|_: &ThemeSystem, cx| {
        crate::settings::update(cx, |settings| {
            settings.theme_mode = crate::settings::ThemeModeSetting::System;
        });
        crate::theme::set_mode(crate::settings::ThemeModeSetting::System, None, cx);
    });
    cx.on_action(|_: &ToggleDensity, cx| {
        let next = crate::settings::global(cx).density.toggled();
        crate::settings::update(cx, |settings| {
            settings.density = next;
        });
        state::AppState::global(cx).update(cx, |state, cx| state.set_density(next, cx));
    });
    cx.on_action(|_: &ToggleLocale, cx| {
        let next = match crate::i18n::locale(cx) {
            crate::settings::Locale::En => crate::settings::Locale::Zh,
            crate::settings::Locale::Zh => crate::settings::Locale::En,
        };
        crate::i18n::set_locale(next, cx);
    });
}

/// Apply the persisted theme preference.
fn apply_saved_theme(cx: &mut App) {
    let mode = settings::global(cx).theme_mode;
    crate::theme::set_mode(mode, None, cx);
}

/// Map the persisted preference onto a concrete theme mode. The web studio
/// ships a light appearance only, so `System` resolves to light; the dark
/// appearance arrives together with the web dark theme.
pub fn to_theme_mode(mode: crate::settings::ThemeModeSetting, _cx: &App) -> ThemeMode {
    match mode {
        crate::settings::ThemeModeSetting::System | crate::settings::ThemeModeSetting::Light => {
            ThemeMode::Light
        }
    }
}

pub(crate) fn open_main_window(cx: &mut App) {
    let saved = settings::global(cx).window;
    let window_bounds = match saved {
        Some(geometry) => Some(WindowBounds::Windowed(Bounds {
            origin: point(px(geometry.x as f32), px(geometry.y as f32)),
            size: size(px(geometry.width as f32), px(geometry.height as f32)),
        })),
        None => Some(WindowBounds::centered(
            size(px(DEFAULT_WINDOW_SIZE.0), px(DEFAULT_WINDOW_SIZE.1)),
            cx,
        )),
    };

    let options = WindowOptions {
        window_bounds,
        window_min_size: Some(Size {
            width: px(MIN_WINDOW_SIZE.0),
            height: px(MIN_WINDOW_SIZE.1),
        }),
        #[cfg(target_os = "linux")]
        window_background: gpui_kit::WindowBackgroundAppearance::Transparent,
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui_kit::WindowDecorations::Client),
        ..gpui_kit::component::TitleBar::window_options()
    };

    let (handle, _) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Shell::new(window, cx))
    })
    .expect("failed to open main window");

    crate::tray::set_window(handle, cx);
    _ = handle.update(cx, |_, window, cx| {
        window.on_window_should_close(cx, |window, cx| {
            crate::tray::on_window_should_close(window, cx)
        });
        window.activate_window();
        window.set_window_title("Code Context Engine");
    });
}

pub(crate) fn persist_window_geometry(window: &mut gpui_kit::Window, cx: &mut App) {
    let bounds = match window.window_bounds() {
        WindowBounds::Windowed(bounds)
        | WindowBounds::Maximized(bounds)
        | WindowBounds::Fullscreen(bounds) => bounds,
    };
    let geometry = WindowGeometry {
        x: f32::from(bounds.origin.x) as f64,
        y: f32::from(bounds.origin.y) as f64,
        width: f32::from(bounds.size.width) as f64,
        height: f32::from(bounds.size.height) as f64,
    };
    settings::update(cx, |settings| {
        settings.window = Some(geometry);
    });
}
