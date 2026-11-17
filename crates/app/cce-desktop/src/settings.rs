//! Persisted desktop settings.
//!
//! Window geometry, theme, density, interface language and the most recent
//! base URL survive restarts. Credentials never touch disk: an explicit token
//! lives only in the running process, and the environment fallback is read at
//! client construction time.

use std::{collections::BTreeMap, fs, path::PathBuf};

use directories::ProjectDirs;
use gpui_kit::{App, Global};
use serde::{Deserialize, Serialize};

/// Theme preference persisted across runs. The web studio ships a light
/// appearance only, so the desktop client does the same: `System` currently
/// resolves to light, and a dark appearance arrives together with the web
/// dark theme instead of inventing its own brand colours early.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeModeSetting {
    #[default]
    System,
    Light,
}

/// Row density for future tables and cards, mirroring the web spacing scale.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Dense,
}

impl Density {
    pub fn is_dense(self) -> bool {
        matches!(self, Self::Dense)
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Comfortable => Self::Dense,
            Self::Dense => Self::Comfortable,
        }
    }
}

/// Interface language persisted across runs. English is the default;
/// Chinese strings live in `messages/*.json` so no non-English text
/// appears in Rust sources.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    #[default]
    En,
    Zh,
}

/// Window position and size in logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Everything the desktop client persists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub theme_mode: ThemeModeSetting,
    #[serde(default)]
    pub density: Density,
    #[serde(default)]
    pub locale: Locale,
    #[serde(default)]
    pub window: Option<WindowGeometry>,
    #[serde(default)]
    pub last_base_url: Option<String>,
    /// Recent inputs per panel, newest first and capped by the panel service.
    /// Only query text and paths live here; no token, response or snippet is
    /// ever written, so the file stays small and free of secrets.
    #[serde(default)]
    pub history: BTreeMap<String, Vec<String>>,
}

struct SettingsGlobal(Settings);

impl Global for SettingsGlobal {}

impl Settings {
    fn path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "cce-desktop").map(|dirs| dirs.config_dir().join("settings.toml"))
    }

    /// Load persisted settings. A missing file means a first run and yields
    /// defaults; an unreadable or corrupt file is reported and also yields
    /// defaults so a stale config can never block startup.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return Self::default(),
        };
        match toml::from_str(&raw) {
            Ok(settings) => settings,
            Err(err) => {
                log::warn!("ignoring corrupt settings {}: {err}", path.display());
                Self::default()
            }
        }
    }

    /// Atomically write settings to disk. Failures are logged and ignored so
    /// a read-only config dir never blocks startup or shutdown.
    pub fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        let raw = match toml::to_string_pretty(self) {
            Ok(raw) => raw,
            Err(err) => {
                log::warn!("settings are not serializable: {err}");
                return;
            }
        };
        if let Some(parent) = path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            log::warn!("failed to create config dir {}: {err}", parent.display());
            return;
        }
        let tmp = path.with_extension("toml.tmp");
        if let Err(err) = fs::write(&tmp, raw) {
            log::warn!("failed to write settings {}: {err}", tmp.display());
            return;
        }
        if let Err(err) = fs::rename(&tmp, &path) {
            log::warn!("failed to replace settings {}: {err}", path.display());
        }
    }
}

/// Install the loaded settings as a process-wide global.
pub fn init_global(settings: Settings, cx: &mut App) {
    cx.set_global(SettingsGlobal(settings));
}

/// Read the process-wide settings.
pub fn global(cx: &App) -> &Settings {
    &cx.global::<SettingsGlobal>().0
}

/// Mutate the process-wide settings and persist the result.
pub fn update(cx: &mut App, edit: impl FnOnce(&mut Settings)) {
    cx.global_mut::<SettingsGlobal>().0.save_after(edit);
}

impl Settings {
    fn save_after(&mut self, edit: impl FnOnce(&mut Self)) {
        edit(self);
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_select_light_comfortable_english() {
        let settings = Settings::default();
        assert_eq!(settings.theme_mode, ThemeModeSetting::System);
        assert_eq!(settings.density, Density::Comfortable);
        assert_eq!(settings.locale, Locale::En);
        assert_eq!(settings.window, None);
        assert_eq!(settings.last_base_url, None);
        assert!(settings.history.is_empty());
    }

    #[test]
    fn density_toggles() {
        assert_eq!(Density::Comfortable.toggled(), Density::Dense);
        assert_eq!(Density::Dense.toggled(), Density::Comfortable);
        assert!(Density::Dense.is_dense());
        assert!(!Density::Comfortable.is_dense());
    }

    #[test]
    fn settings_round_trip_through_toml() {
        let settings = Settings {
            theme_mode: ThemeModeSetting::Light,
            density: Density::Dense,
            locale: Locale::Zh,
            window: Some(WindowGeometry {
                x: 10.0,
                y: 20.0,
                width: 1280.0,
                height: 800.0,
            }),
            last_base_url: Some("http://127.0.0.1:9000".to_string()),
            history: BTreeMap::from([
                ("search".to_string(), vec!["retry budget".to_string()]),
                ("graph".to_string(), vec!["sym_abc".to_string()]),
            ]),
        };
        let raw = toml::to_string_pretty(&settings).expect("settings serialize");
        let back: Settings = toml::from_str(&raw).expect("settings parse");
        assert_eq!(settings, back);
    }
}
