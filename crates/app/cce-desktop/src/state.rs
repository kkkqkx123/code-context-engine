//! Global UI state shared across views.
//!
//! The [`Client`] itself lives in [`crate::connection::ClientStore`]; this
//! entity only mirrors what the UI renders so views can observe it without
//! touching the client stack. CCE has no login session, so the state tracks
//! only the base URL, the connection lifecycle and the latest health probe.

use gpui_kit::{App, AppContext as _, Entity, Global, SharedString};

use crate::settings::{Density, Locale};

/// Connection lifecycle as displayed in the status bar and by view switching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    /// Server became unreachable while a connection was believed to be live.
    Unhealthy,
}

/// Health snapshot rendered by the status bar and the health placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSummary {
    pub healthy: bool,
    pub detail: SharedString,
}

pub struct AppState {
    pub status: ConnectionStatus,
    pub base_url: Option<SharedString>,
    pub health: Option<HealthSummary>,
    pub locale: Locale,
    pub density: Density,
}

pub struct AppStateGlobal(Entity<AppState>);

impl Global for AppStateGlobal {}

impl AppState {
    pub fn init(cx: &mut App) {
        let settings = crate::settings::global(cx);
        let locale = settings.locale;
        let density = settings.density;
        let entity = cx.new(|_| Self {
            status: ConnectionStatus::Disconnected,
            base_url: None,
            health: None,
            locale,
            density,
        });
        cx.set_global(AppStateGlobal(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<AppStateGlobal>().0.clone()
    }

    /// Switch the interface language and notify observers so views re-render
    /// with the new catalogue.
    pub fn set_locale(&mut self, locale: Locale, cx: &mut gpui_kit::Context<Self>) {
        self.locale = locale;
        cx.notify();
    }

    /// Switch the row density and notify observers.
    pub fn set_density(&mut self, density: Density, cx: &mut gpui_kit::Context<Self>) {
        self.density = density;
        cx.notify();
    }

    pub fn set_connecting(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self.status = ConnectionStatus::Connecting;
        cx.notify();
    }

    pub fn set_connected(
        &mut self,
        base_url: SharedString,
        health: HealthSummary,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.status = ConnectionStatus::Connected;
        self.base_url = Some(base_url);
        self.health = Some(health);
        cx.notify();
    }

    pub fn set_disconnected(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self.status = ConnectionStatus::Disconnected;
        self.base_url = None;
        self.health = None;
        cx.notify();
    }

    pub fn set_unhealthy(&mut self, unhealthy: bool, cx: &mut gpui_kit::Context<Self>) {
        if self.status == ConnectionStatus::Connected && unhealthy {
            self.status = ConnectionStatus::Unhealthy;
            cx.notify();
        } else if self.status == ConnectionStatus::Unhealthy && !unhealthy {
            self.status = ConnectionStatus::Connected;
            cx.notify();
        }
    }
}
