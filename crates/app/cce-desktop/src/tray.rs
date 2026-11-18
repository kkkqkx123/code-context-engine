//! System tray and window lifecycle.
//!
//! The tray handle is main-thread only, so it lives in a thread local rather
//! than a process global. Everything the window callback needs is mirrored
//! into an observable entity and a small window-handle store.

use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};

use gpui_kit::{AnyWindowHandle, App, AppContext as _, Entity, Global, Subscription, Window};
use gpui_tray::{LeftClick, Tray, TrayMenu, TrayOptions, tray_menu};

use crate::app::{Quit, ShowWindow, ToggleCloseToTray};
use crate::i18n::t;
use crate::settings::{self, CloseBehavior};
use crate::state::AppState;

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
    static MENU: RefCell<Option<Subscription>> = const { RefCell::new(None) };
}

/// Render-facing snapshot of the tray settings.
pub struct TrayState {
    behavior: CloseBehavior,
    locale: settings::Locale,
    available: bool,
}

/// Kept in a global because the window callback only has `&mut App`.
pub struct TrayGlobals {
    pub state: Entity<TrayState>,
    window: Mutex<Option<AnyWindowHandle>>,
}

impl Global for TrayGlobals {}

/// Why a close request ends one way or the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDecision {
    Quit,
    Hide,
    QuitBecauseUnavailable,
}

/// Pure close rule, kept separate from platform calls.
pub fn close_decision(behavior: CloseBehavior, available: bool) -> CloseDecision {
    match (behavior, available) {
        (CloseBehavior::Quit, _) => CloseDecision::Quit,
        (CloseBehavior::MinimizeToTray, true) => CloseDecision::Hide,
        (CloseBehavior::MinimizeToTray, false) => CloseDecision::QuitBecauseUnavailable,
    }
}

/// Create the tray and bind its menu to the shared state.
pub fn init(cx: &mut App) {
    let behavior = settings::global(cx).close_behavior;
    let locale = settings::global(cx).locale;
    let state = cx.new(|_| TrayState {
        behavior,
        locale,
        available: false,
    });
    cx.set_global(TrayGlobals {
        state: state.clone(),
        window: Mutex::new(None),
    });

    cx.on_action(|_: &ShowWindow, cx| restore(cx));
    cx.on_action(|_: &ToggleCloseToTray, cx| toggle_close_behavior(cx));
    cx.on_action(|_: &Quit, cx| quit(cx));

    let app_state = AppState::global(cx);
    let locale_state = state.clone();
    cx.observe(&app_state, move |observed, cx| {
        let locale = observed.read(cx).locale;
        locale_state.update(cx, |this, cx| {
            if this.locale != locale {
                this.locale = locale;
                cx.notify();
            }
        });
    })
    .detach();

    let tray = Tray::new(
        cx,
        TrayOptions::builder("cce.desktop.tray")
            .with_left_click(LeftClick::Action(Arc::new(ShowWindow)))
            .with_tooltip(t("tray.tooltip", settings::global(cx).locale))
            .build(),
    );
    let tray = match tray {
        Ok(tray) => tray,
        Err(err) => {
            log::warn!("tray is unavailable: {err}");
            return;
        }
    };

    let available = tray.is_available();
    state.update(cx, |this, cx| {
        this.available = available;
        cx.notify();
    });
    let snapshot = state.read(cx);
    if let Err(err) = tray.set_menu(cx, menu(snapshot)) {
        log::warn!("failed to set tray menu: {err}");
    }
    let subscription = tray.bind_menu(cx, state.clone(), |state, _| menu(state));
    MENU.with(|slot| *slot.borrow_mut() = Some(subscription));
    TRAY.with(|slot| *slot.borrow_mut() = Some(tray.clone()));

    if !available {
        log::warn!("no system tray in this session; closing the window quits the app");
    }
}

fn menu(state: &TrayState) -> TrayMenu {
    let locale = state.locale;
    tray_menu(|builder| {
        builder.action(ShowWindow, t("tray.show", locale));
        builder.separator();
        builder.check(
            state.behavior == CloseBehavior::MinimizeToTray,
            ToggleCloseToTray,
            t("tray.closeToTray", locale),
        );
        builder.separator();
        builder.action(Quit, t("tray.quit", locale));
    })
}

/// Remember the main window so the tray can restore it later.
pub fn set_window(handle: AnyWindowHandle, cx: &mut App) {
    let mut window = cx
        .global_mut::<TrayGlobals>()
        .window
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    *window = Some(handle);
}

fn toggle_close_behavior(cx: &mut App) {
    let next = match settings::global(cx).close_behavior {
        CloseBehavior::MinimizeToTray => CloseBehavior::Quit,
        CloseBehavior::Quit => CloseBehavior::MinimizeToTray,
    };
    settings::update(cx, |settings| settings.close_behavior = next);
    sync_state(cx);
}

fn sync_state(cx: &mut App) {
    let behavior = settings::global(cx).close_behavior;
    let state = TrayGlobals::global(cx);
    state.update(cx, |this, cx| {
        if this.behavior != behavior {
            this.behavior = behavior;
            cx.notify();
        }
    });
}

impl TrayGlobals {
    fn global(cx: &App) -> Entity<TrayState> {
        cx.global::<Self>().state.clone()
    }
}

/// Restore or reopen the main window from a tray activation.
pub fn restore(cx: &mut App) {
    let handle = *cx
        .global_mut::<TrayGlobals>()
        .window
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let Some(handle) = handle else {
        crate::app::open_main_window(cx);
        return;
    };
    let alive = handle
        .update(cx, |_, window, cx| {
            #[cfg(target_os = "windows")]
            show_native(window, true);
            window.activate_window();
            cx.activate(true);
        })
        .is_ok();
    if !alive {
        crate::app::open_main_window(cx);
    }
}

/// Handle `on_window_should_close`: persist first, then hide or quit.
pub fn on_window_should_close(window: &mut Window, cx: &mut App) -> bool {
    let state = TrayGlobals::global(cx);
    let (behavior, available) = {
        let snapshot = state.read(cx);
        (snapshot.behavior, snapshot.available)
    };
    crate::app::persist_window_geometry(window, cx);
    crate::dock_layout::flush(cx);
    match close_decision(behavior, available) {
        CloseDecision::Hide => {
            hide(window, cx);
            false
        }
        CloseDecision::Quit | CloseDecision::QuitBecauseUnavailable => {
            if behavior == CloseBehavior::MinimizeToTray {
                log::warn!("closing quits because no system tray is available");
            }
            true
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn hide(_: &mut Window, cx: &mut App) {
    cx.hide();
}

#[cfg(target_os = "windows")]
fn hide(window: &mut Window, _: &mut App) {
    show_native(window, false);
}

#[cfg(target_os = "windows")]
fn show_native(window: &Window, visible: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SW_HIDE, SW_SHOW, ShowWindow as ShowWindowNative,
    };

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        log::warn!("cannot hide the window without a native handle");
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        log::warn!("window handle is not a Win32 handle");
        return;
    };
    let hwnd = HWND(win32.hwnd.get() as *mut core::ffi::c_void);
    // SAFETY: the HWND comes from the live window's raw-window-handle, so it
    // is valid for the duration of this callback. ShowWindow only changes
    // visibility and reports the previous state, which is ignored here.
    unsafe {
        ShowWindowNative(hwnd, if visible { SW_SHOW } else { SW_HIDE });
    }
}

/// Deterministic shutdown: flush local state, remove the icon, then quit.
pub fn quit(cx: &mut App) {
    crate::dock_layout::flush(cx);
    let tray = TRAY.with(|slot| slot.borrow_mut().take());
    if let Some(tray) = tray {
        tray.remove(cx);
    }
    cx.quit();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_preference_always_quits() {
        assert_eq!(
            close_decision(CloseBehavior::Quit, true),
            CloseDecision::Quit
        );
        assert_eq!(
            close_decision(CloseBehavior::Quit, false),
            CloseDecision::Quit
        );
    }

    #[test]
    fn tray_preference_hides_only_when_a_tray_exists() {
        assert_eq!(
            close_decision(CloseBehavior::MinimizeToTray, true),
            CloseDecision::Hide
        );
        assert_eq!(
            close_decision(CloseBehavior::MinimizeToTray, false),
            CloseDecision::QuitBecauseUnavailable
        );
    }
}
