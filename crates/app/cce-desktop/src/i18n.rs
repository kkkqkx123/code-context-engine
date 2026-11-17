//! Interface message catalogue for the desktop client.
//!
//! The JSON files in `messages/` are the only place holding non-English
//! text, so Rust sources stay English-only. Lookups fall back to English
//! and then to the key itself, so a missing entry never blanks the UI.
//!
//! Catalogue rules: every key is namespaced (`nav.search`, never a bare
//! word), so backend field names, paths, numbers and protocol fragments stay
//! out of the JSON files.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::settings::Locale;
use crate::state::AppState;

static EN: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| load(include_str!("../messages/en.json")));
static ZH: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| load(include_str!("../messages/zh.json")));

fn load(raw: &str) -> HashMap<String, String> {
    serde_json::from_str(raw).expect("bundled message catalogue parses")
}

/// Interface text for `key` in `locale`. Unknown keys surface as-is so gaps
/// stay visible instead of rendering blank controls.
pub fn t(key: &str, locale: Locale) -> &str {
    match locale {
        Locale::Zh => ZH.get(key).or_else(|| EN.get(key)),
        Locale::En => EN.get(key),
    }
    .map(String::as_str)
    .unwrap_or(key)
}

/// Current interface locale, mirrored in the global state.
pub fn locale(cx: &gpui_kit::App) -> Locale {
    AppState::global(cx).read(cx).locale
}

/// Persist the language and mirror it into the observable global state so
/// observing views re-render.
pub fn set_locale(locale: Locale, cx: &mut gpui_kit::App) {
    crate::settings::update(cx, |settings| settings.locale = locale);
    AppState::global(cx).update(cx, |state, cx| {
        state.set_locale(locale, cx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogues_parse() {
        assert!(!EN.is_empty());
        assert!(!ZH.is_empty());
    }

    #[test]
    fn namespaced_keys_resolve_in_both_locales() {
        for key in [
            "connection.title",
            "connection.connect",
            "nav.search",
            "nav.health",
            "status.connected",
            "health.title",
            "theme.light",
            "density.dense",
        ] {
            assert_ne!(t(key, Locale::En), key, "{key} resolves in English");
            assert_ne!(t(key, Locale::Zh), key, "{key} resolves in Chinese");
        }
    }

    #[test]
    fn unknown_keys_surface_as_is() {
        assert_eq!(t("no.such.key", Locale::En), "no.such.key");
        assert_eq!(t("no.such.key", Locale::Zh), "no.such.key");
    }
}
