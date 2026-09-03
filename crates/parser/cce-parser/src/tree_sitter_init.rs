//! Tree-sitter language resolution
//!
//! This module provides static dispatch of tree-sitter language instances
//! using match expressions instead of runtime HashMap lookups, plus a
//! plugin-language table for `AstLanguage` plugins.
//!
//! # Design
//!
//! - No runtime initialization or state management (built-in languages)
//! - Zero-cost abstraction via compiler inlining
//! - Thread-safe by design (no shared mutable state)
//! - Each call returns a fresh `TsLanguage` instance (lightweight copy)
//!
//! # Plugin languages (`Language::Custom`)
//!
//! `AstLanguage` plugins register a raw `*const TSLanguage` pointer plus
//! query schemes via [`register_plugin_ts_language`]. The pointer is owned
//! by the plugin library (which the host keeps loaded for the process
//! lifetime); see `docs/archive/unsafe.md`.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use tree_sitter::Language as TsLanguage;

use cce_types::ast_to_nl::QueryType;
use cce_types::language::Language;

/// Plugin-provided tree-sitter language data.
#[derive(Clone)]
pub struct PluginTsLanguage {
    /// Raw `*const TSLanguage` pointer (valid for the process lifetime).
    ///
    /// Stored as an opaque `c_void` pointer; only ever re-imported into
    /// tree-sitter via [`language_from_raw_ptr`], never dereferenced by Rust.
    /// Null for `LanguageRemap` plugins (which use [`Self::remap_target`]).
    pub language_ptr: *const std::ffi::c_void,
    /// Query schemes keyed by [`QueryType`].
    pub query_schemes: HashMap<QueryType, String>,
    /// Host built-in grammar backing this custom language (`LanguageRemap`),
    /// `None` for native `AstLanguage` plugins.
    pub remap_target: Option<Language>,
}

// SAFETY: The pointer is an opaque token re-imported into tree-sitter only.
// It is never dereferenced from Rust and the plugin guarantees the grammar
// is immutable and safe to read concurrently.
unsafe impl Send for PluginTsLanguage {}
unsafe impl Sync for PluginTsLanguage {}

/// Process-global plugin tree-sitter languages keyed by `Language::Custom`
/// index (aligned with `cce_types::language::register_plugin_language`).
static PLUGIN_TS_LANGUAGES: OnceLock<DashMap<u32, PluginTsLanguage>> = OnceLock::new();

/// Register a plugin language's tree-sitter grammar and query schemes
/// (native `AstLanguage` plugins).
pub fn register_plugin_ts_language(
    index: u32,
    language_ptr: *const std::ffi::c_void,
    query_schemes: HashMap<QueryType, String>,
) {
    PLUGIN_TS_LANGUAGES.get_or_init(DashMap::new).insert(
        index,
        PluginTsLanguage {
            language_ptr,
            query_schemes,
            remap_target: None,
        },
    );
}

/// Register a plugin custom language backed by a host built-in grammar
/// (`LanguageRemap` plugins). Query schemes are the plugin-provided ones;
/// query types without a scheme fall back to the target language's scheme
/// at load time.
pub fn register_remap_ts_language(
    index: u32,
    target: Language,
    query_schemes: HashMap<QueryType, String>,
) {
    PLUGIN_TS_LANGUAGES.get_or_init(DashMap::new).insert(
        index,
        PluginTsLanguage {
            language_ptr: std::ptr::null(),
            query_schemes,
            remap_target: Some(target),
        },
    );
}

/// Clear all registered plugin tree-sitter languages (tests / teardown).
pub fn clear_plugin_ts_languages() {
    if let Some(map) = PLUGIN_TS_LANGUAGES.get() {
        map.clear();
    }
}

/// Look up a registered plugin tree-sitter language by index.
pub fn get_plugin_ts_language(index: u32) -> Option<PluginTsLanguage> {
    PLUGIN_TS_LANGUAGES
        .get_or_init(DashMap::new)
        .get(&index)
        .map(|entry| entry.clone())
}

/// Wrap a raw `*const TSLanguage` pointer into a [`TsLanguage`].
///
/// # Safety
///
/// `ptr` must point to a valid, immutable tree-sitter grammar for the
/// process lifetime. `Language` is `#[repr(transparent)]` over the same
/// pointer type, so the conversion is layout-compatible.
pub unsafe fn language_from_raw_ptr(ptr: *const std::ffi::c_void) -> TsLanguage {
    // SAFETY: `Language` is `#[repr(transparent)] struct Language(*const
    // ffi::TSLanguage)`; a raw `c_void` pointer is the same size and
    // alignment. The caller guarantees the pointer's validity.
    unsafe { std::mem::transmute::<*const std::ffi::c_void, TsLanguage>(ptr) }
}

/// Read the tree-sitter ABI version of a plugin grammar pointer.
///
/// Returns `None` for a null pointer.
///
/// # Safety
///
/// `ptr` must point to a valid, immutable grammar for the process lifetime
/// (the plugin library is kept loaded by the host). Only the immutable ABI
/// metadata is read (`ts_language_abi_version`), never the AST.
pub unsafe fn plugin_grammar_abi_version(ptr: *const std::ffi::c_void) -> Option<usize> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees pointer validity; only ABI metadata is
    // read via the re-imported `TsLanguage`.
    Some(unsafe { language_from_raw_ptr(ptr) }.abi_version())
}

/// Get a tree-sitter language instance for the given language
///
/// Uses static match-based dispatch for zero runtime overhead.
/// Tree-sitter language initialization is lightweight (returns a grammar pointer),
/// so calling this function multiple times has negligible cost.
///
/// # Arguments
///
/// * `lang` - The programming language
///
/// # Returns
///
/// * `Some(TsLanguage)` - If the language supports AST parsing
/// * `None` - If the language is not supported for AST parsing
///
/// # Example
///
/// ```
/// use cce_parser::tree_sitter_init::get_tree_sitter_language;
/// use cce_types::language::Language;
///
/// let ts_lang = get_tree_sitter_language(&Language::Rust);
/// assert!(ts_lang.is_some());
/// ```
pub fn get_tree_sitter_language(lang: &Language) -> Option<TsLanguage> {
    let language = match lang {
        // Core languages
        Language::C => tree_sitter_c::LANGUAGE.into(),
        Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        Language::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
        Language::Java => tree_sitter_java::LANGUAGE.into(),
        Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        Language::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        Language::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
        Language::Scala => tree_sitter_scala::LANGUAGE.into(),
        Language::Dart => tree_sitter_dart::LANGUAGE.into(),

        // Shell/Scripting languages
        Language::Bash => tree_sitter_bash::LANGUAGE.into(),
        Language::Lua => tree_sitter_lua::LANGUAGE.into(),

        // Frontend languages
        Language::Html => tree_sitter_html::LANGUAGE.into(),
        Language::Css => tree_sitter_css::LANGUAGE.into(),
        Language::Vue => tree_sitter_vue::language(),
        Language::Svelte => tree_sitter_svelte::language(),
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::Jsx => tree_sitter_javascript::LANGUAGE.into(),

        // Plugin-registered custom languages
        Language::Custom(index) => {
            let table = PLUGIN_TS_LANGUAGES.get_or_init(DashMap::new);
            let plugin = table.get(index)?;
            // `LanguageRemap` plugins reuse a host built-in grammar.
            if let Some(target) = plugin.remap_target {
                return get_tree_sitter_language(&target);
            }
            // SAFETY: the plugin registered its grammar pointer via
            // `register_plugin_ts_language`; the pointer is valid for the
            // process lifetime (plugin library kept loaded).
            return Some(unsafe { language_from_raw_ptr(plugin.language_ptr) });
        }

        // Unsupported languages (no tree-sitter grammar)
        Language::Scss
        | Language::Less
        | Language::Unknown
        | Language::Json
        | Language::Yaml
        | Language::Toml
        | Language::Xml => return None,
    };
    Some(language)
}

/// Get a plugin's tree-sitter query scheme for `query_type`, if registered.
pub fn plugin_query_scheme(index: u32, query_type: QueryType) -> Option<String> {
    get_plugin_ts_language(index).and_then(|plugin| plugin.query_schemes.get(&query_type).cloned())
}

/// The host built-in grammar backing a `LanguageRemap` custom language.
pub fn remap_target(index: u32) -> Option<Language> {
    get_plugin_ts_language(index).and_then(|plugin| plugin.remap_target)
}

/// Number of registered plugin tree-sitter languages.
pub fn plugin_ts_language_count() -> usize {
    PLUGIN_TS_LANGUAGES.get_or_init(DashMap::new).len()
}

/// Alias so callers can keep a stable handle to the plugin data.
pub type PluginTsLanguageRef = Arc<PluginTsLanguage>;

/// Register all `AstLanguage` and `LanguageRemap` plugins from a registry.
///
/// Populates both the cce_core extension→language mapping (so
/// `LanguageInfo::detect_from_extension` routes files to `Language::Custom`)
/// and the parser's grammar/query table. Returns the number of languages
/// registered.
///
/// Plugins are iterated in priority order (descending) across both
/// capabilities: `AstLanguage` (native grammar pointer) and `LanguageRemap`
/// (host built-in grammar). When a plugin provides both, the native branch
/// wins. When two plugins declare the same language name, the higher-priority
/// plugin wins: the lower-priority one is skipped with a warning, so its
/// grammar never silently overwrites the higher-priority registration.
///
/// `extension_conflict_policy` governs plugins claiming extensions already
/// owned by built-in languages (or by a higher-priority plugin):
/// - `Warn` (default): register, log a warning.
/// - `Deny`: skip the conflicting plugin entirely.
/// - `Allow`: register silently.
///
/// `grammar_abi_policy` governs native plugin grammars whose tree-sitter ABI
/// version is outside the host's compatible range (remap plugins reuse host
/// grammars and are inherently compatible):
/// - `Deny` (default): skip the conflicting plugin entirely.
/// - `Warn`: register, log a warning (parse-time validation still applies).
///
/// Plugins whose grammar is missing, null, or referencing an unknown built-in
/// language are always skipped with a warning (they cannot parse files).
pub fn register_ast_language_plugins(
    registry: &crate::plugin::PluginRegistry,
    extension_conflict_policy: cce_config::project::LanguageExtensionConflictPolicy,
    grammar_abi_policy: cce_config::project::GrammarAbiPolicy,
) -> usize {
    use cce_config::project::GrammarAbiPolicy as AbiPolicy;
    use cce_plugin::{CodePlugin, PluginCapability};

    let plugins = registry.get_plugins(PluginCapability::AstLanguage, None, None);
    let remap_plugins = registry.get_plugins(PluginCapability::LanguageRemap, None, None);

    // Merge both lists and re-sort by priority (stable, so ties keep the
    // native list ahead of the remap list).
    let mut ordered: Vec<&Arc<dyn CodePlugin>> =
        Vec::with_capacity(plugins.len() + remap_plugins.len());
    for plugin in plugins {
        if plugin.tree_sitter_language().is_some() {
            ordered.push(plugin);
        }
    }
    for plugin in remap_plugins {
        if plugin.supports_language_remap() {
            ordered.push(plugin);
        }
    }
    ordered.sort_by_key(|plugin| std::cmp::Reverse(plugin.metadata().priority));

    let mut count = 0usize;
    let mut registered_names: HashMap<String, String> = HashMap::new();
    let mut registered_extensions: HashMap<String, String> = HashMap::new();
    for plugin in ordered {
        let plugin_id = plugin.metadata().id.clone();
        let Some(name) = plugin.language_name() else {
            continue;
        };
        let extensions = plugin.language_extensions();
        if extensions.is_empty() {
            continue;
        }
        let lower = name.to_lowercase();
        if let Some(existing) = registered_names.get(&lower) {
            tracing::warn!(
                language = %name,
                plugin = %plugin_id,
                existing_plugin = %existing,
                "plugin language name conflict: higher-priority plugin wins, skipping this registration"
            );
            continue;
        }

        let mut skip = false;
        for ext in &extensions {
            let ext = ext.to_lowercase();
            let builtin = cce_types::language::builtin_language_for_extension(&ext)
                .map(|(lang, _)| lang.to_string());
            let owner = builtin.as_ref().or_else(|| registered_extensions.get(&ext));
            if let Some(owner) = owner {
                match extension_conflict_policy {
                    cce_config::project::LanguageExtensionConflictPolicy::Warn => {
                        tracing::warn!(
                            language = %name,
                            plugin = %plugin_id,
                            extension = %ext,
                            owned_by = %owner,
                            "plugin language extension conflict with existing language: plugin wins (set plugins.language_extension_conflict = \"deny\" to refuse)"
                        );
                    }
                    cce_config::project::LanguageExtensionConflictPolicy::Deny => {
                        tracing::warn!(
                            language = %name,
                            plugin = %plugin_id,
                            extension = %ext,
                            owned_by = %owner,
                            "plugin language extension conflict with existing language: registration refused by deny policy"
                        );
                        skip = true;
                        break;
                    }
                    cce_config::project::LanguageExtensionConflictPolicy::Allow => {}
                }
            }
        }
        if skip {
            continue;
        }

        let mut schemes = HashMap::new();
        for qt in QueryType::ALL {
            if let Some(scheme) = plugin.query_scheme(qt) {
                schemes.insert(qt, scheme);
            }
        }

        // Native `AstLanguage` branch: raw grammar pointer with ABI check.
        if let Some(ptr) = plugin.tree_sitter_language() {
            // SAFETY: the pointer comes from the plugin library the host
            // keeps loaded; only immutable ABI metadata is read.
            let Some(abi_version) = (unsafe { plugin_grammar_abi_version(ptr) }) else {
                tracing::warn!(
                    language = %name,
                    plugin = %plugin_id,
                    "AstLanguage plugin returned a null tree-sitter grammar pointer: registration skipped"
                );
                continue;
            };
            let abi_ok = (tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION
                ..=tree_sitter::LANGUAGE_VERSION)
                .contains(&abi_version);
            if !abi_ok {
                match grammar_abi_policy {
                    AbiPolicy::Deny => {
                        tracing::warn!(
                            language = %name,
                            plugin = %plugin_id,
                            plugin_abi_version = abi_version,
                            host_range = ?(tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION
                                ..=tree_sitter::LANGUAGE_VERSION),
                            "AstLanguage grammar ABI version out of host range: registration refused by deny policy (set plugins.grammar_abi_policy = \"warn\" to allow)"
                        );
                        continue;
                    }
                    AbiPolicy::Warn => {
                        tracing::warn!(
                            language = %name,
                            plugin = %plugin_id,
                            plugin_abi_version = abi_version,
                            host_range = ?(tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION
                                ..=tree_sitter::LANGUAGE_VERSION),
                            "AstLanguage grammar ABI version out of host range: registered anyway by warn policy (parse-time validation still applies)"
                        );
                    }
                }
            }
            let index = cce_types::language::register_plugin_language(&name, &extensions);
            register_plugin_ts_language(index, ptr, schemes);
            count += 1;
        } else if plugin.supports_language_remap() {
            // `LanguageRemap` branch: host built-in grammar by name.
            let Some(target_name) = plugin.remap_grammar_language() else {
                tracing::warn!(
                    language = %name,
                    plugin = %plugin_id,
                    "LanguageRemap plugin declares no remap_grammar_language: registration skipped"
                );
                continue;
            };
            let Some(target) = Language::from_name(&target_name) else {
                tracing::warn!(
                    language = %name,
                    plugin = %plugin_id,
                    remap_grammar = %target_name,
                    "LanguageRemap plugin references an unknown host grammar: registration skipped"
                );
                continue;
            };
            let index = cce_types::language::register_remap_plugin_language(
                &name,
                &extensions,
                &target_name,
            );
            register_remap_ts_language(index, target, schemes);
            count += 1;
        } else {
            continue;
        }

        registered_names.insert(lower, plugin_id.clone());
        for ext in &extensions {
            registered_extensions
                .entry(ext.to_lowercase())
                .or_insert_with(|| plugin_id.clone());
        }
    }
    count
}

/// Register a plugin custom language backed by one of the built-in grammars
/// (test / stand-in helper).
///
/// The custom language reuses a built-in tree-sitter grammar and registers an
/// empty query scheme for every [`QueryType`], so files of this language parse
/// (producing zero entities) without a real `AstLanguage` plugin. Useful for
/// E2E tests that exercise the `SymbolExtract` capability (which operates on
/// raw source text and never consults the AST) and for plugins that reuse a
/// built-in grammar for a dialect.
///
/// Returns the stable index used by [`Language::Custom`].
pub fn register_plugin_language_with_builtin_grammar(
    name: &str,
    extensions: &[String],
    grammar: Language,
) -> u32 {
    let index = cce_types::language::register_plugin_language(name, extensions);
    let Some(ts_lang) = get_tree_sitter_language(&grammar) else {
        return index;
    };
    // SAFETY: `tree_sitter::Language` is `#[repr(transparent)]` over
    // `*const TSLanguage`; the built-in grammar pointer is valid for the
    // process lifetime. The pointer is only re-imported into tree-sitter via
    // `language_from_raw_ptr`, never dereferenced from Rust.
    let ptr: *const std::ffi::c_void =
        unsafe { std::mem::transmute::<tree_sitter::Language, *const std::ffi::c_void>(ts_lang) };
    let schemes: HashMap<QueryType, String> = QueryType::ALL
        .iter()
        .map(|qt| (*qt, String::new()))
        .collect();
    register_plugin_ts_language(index, ptr, schemes);
    index
}

/// Clear all plugin-language registrations (both tables).
pub fn clear_plugin_languages_all() {
    clear_plugin_ts_languages();
    cce_types::language::clear_plugin_languages();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that mutate the process-global plugin language tables
    /// (they run in parallel by default and would race on shared state).
    static GLOBAL_TABLE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_get_supported_languages() {
        assert!(get_tree_sitter_language(&Language::C).is_some());
        assert!(get_tree_sitter_language(&Language::Cpp).is_some());
        assert!(get_tree_sitter_language(&Language::CSharp).is_some());
        assert!(get_tree_sitter_language(&Language::Python).is_some());
        assert!(get_tree_sitter_language(&Language::Rust).is_some());
        assert!(get_tree_sitter_language(&Language::Go).is_some());
        assert!(get_tree_sitter_language(&Language::Java).is_some());
        assert!(get_tree_sitter_language(&Language::JavaScript).is_some());
        assert!(get_tree_sitter_language(&Language::TypeScript).is_some());
        assert!(get_tree_sitter_language(&Language::Ruby).is_some());
        assert!(get_tree_sitter_language(&Language::Php).is_some());
        assert!(get_tree_sitter_language(&Language::Kotlin).is_some());
        assert!(get_tree_sitter_language(&Language::Scala).is_some());
        assert!(get_tree_sitter_language(&Language::Dart).is_some());

        assert!(get_tree_sitter_language(&Language::Bash).is_some());
        assert!(get_tree_sitter_language(&Language::Lua).is_some());

        assert!(get_tree_sitter_language(&Language::Html).is_some());
        assert!(get_tree_sitter_language(&Language::Css).is_some());
        assert!(get_tree_sitter_language(&Language::Vue).is_some());
        assert!(get_tree_sitter_language(&Language::Svelte).is_some());
        assert!(get_tree_sitter_language(&Language::Tsx).is_some());
        assert!(get_tree_sitter_language(&Language::Jsx).is_some());
    }

    #[test]
    fn test_get_unsupported_languages() {
        assert!(get_tree_sitter_language(&Language::Unknown).is_none());
        assert!(get_tree_sitter_language(&Language::Json).is_none());
        assert!(get_tree_sitter_language(&Language::Yaml).is_none());
        assert!(get_tree_sitter_language(&Language::Toml).is_none());
        assert!(get_tree_sitter_language(&Language::Xml).is_none());
        assert!(get_tree_sitter_language(&Language::Scss).is_none());
        assert!(get_tree_sitter_language(&Language::Less).is_none());
    }

    #[test]
    fn test_plugin_language_registration_roundtrip() {
        let _guard = GLOBAL_TABLE_LOCK.lock().expect("test lock poisoned");
        clear_plugin_ts_languages();
        let ptr: *const std::ffi::c_void = std::ptr::null();
        let mut schemes = HashMap::new();
        schemes.insert(QueryType::Entity, "test".to_string());
        register_plugin_ts_language(0, ptr, schemes.clone());

        assert_eq!(plugin_ts_language_count(), 1);
        let plugin = get_plugin_ts_language(0).expect("plugin registered");
        assert_eq!(
            plugin.query_schemes.get(&QueryType::Entity),
            Some(&"test".to_string())
        );
        assert_eq!(
            plugin_query_scheme(0, QueryType::Entity).as_deref(),
            Some("test")
        );
        // A null pointer is not a valid grammar; only the registration is
        // exercised here (get_tree_sitter_language would be UB on it).
        clear_plugin_ts_languages();
    }

    #[test]
    fn test_plugin_grammar_abi_version_null_pointer() {
        // SAFETY: no dereference happens for a null pointer.
        let version = unsafe { plugin_grammar_abi_version(std::ptr::null()) };
        assert!(version.is_none(), "null pointer must report no ABI version");
    }

    #[test]
    fn test_plugin_grammar_abi_version_builtin() {
        let ts_lang = get_tree_sitter_language(&Language::Rust).expect("rust grammar");
        // SAFETY: `TsLanguage` is `#[repr(transparent)]` over the pointer;
        // the built-in grammar is valid for the process lifetime.
        let ptr: *const std::ffi::c_void =
            unsafe { std::mem::transmute::<TsLanguage, *const std::ffi::c_void>(ts_lang) };
        // SAFETY: built-in grammar pointer is valid; only ABI metadata is read.
        let version = unsafe { plugin_grammar_abi_version(ptr) }.expect("abi version");
        assert!(
            (tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION..=tree_sitter::LANGUAGE_VERSION)
                .contains(&version),
            "built-in grammar must be ABI-compatible with the host"
        );
    }

    /// Fake `AstLanguage` plugin for registration-flow tests.
    struct FakeAstLanguagePlugin {
        meta: cce_plugin::PluginMetadata,
        name: String,
        extensions: Vec<String>,
        ptr: Option<*const std::ffi::c_void>,
    }

    // SAFETY: the pointer is set at construction time and only read; it is
    // never dereferenced by this struct (same contract as `PluginTsLanguage`).
    unsafe impl Send for FakeAstLanguagePlugin {}
    unsafe impl Sync for FakeAstLanguagePlugin {}

    impl FakeAstLanguagePlugin {
        fn new(
            id: &str,
            name: &str,
            extensions: &[&str],
            ptr: Option<*const std::ffi::c_void>,
        ) -> Self {
            Self {
                meta: cce_plugin::PluginMetadata {
                    id: id.to_string(),
                    name: id.to_string(),
                    version: "0.1.0".to_string(),
                    priority: 0,
                    capability_priorities: HashMap::new(),
                    description: None,
                    capabilities: Vec::new(),
                },
                name: name.to_string(),
                extensions: extensions.iter().map(|s| s.to_string()).collect(),
                ptr,
            }
        }
    }

    impl cce_plugin::CodePlugin for FakeAstLanguagePlugin {
        fn metadata(&self) -> &cce_plugin::PluginMetadata {
            &self.meta
        }
        fn supports_ast_language(&self) -> bool {
            true
        }
        fn tree_sitter_language(&self) -> Option<*const std::ffi::c_void> {
            self.ptr
        }
        fn language_name(&self) -> Option<String> {
            Some(self.name.clone())
        }
        fn language_extensions(&self) -> Vec<String> {
            self.extensions.clone()
        }
    }

    #[test]
    fn test_register_ast_language_plugins_skips_null_pointer() {
        use cce_config::project::{GrammarAbiPolicy, LanguageExtensionConflictPolicy};

        let _guard = GLOBAL_TABLE_LOCK.lock().expect("test lock poisoned");
        clear_plugin_languages_all();
        let mut registry = crate::plugin::PluginRegistry::new();
        registry.register(std::sync::Arc::new(FakeAstLanguagePlugin::new(
            "null_grammar",
            "nulang",
            &["nux"],
            None,
        )));
        let count = register_ast_language_plugins(
            &registry,
            LanguageExtensionConflictPolicy::Allow,
            GrammarAbiPolicy::Deny,
        );
        assert_eq!(count, 0, "missing grammar symbol must not register");
        assert!(
            cce_types::language::plugin_language_name(0).is_none(),
            "no plugin language may be registered for a null grammar"
        );
        assert_eq!(plugin_ts_language_count(), 0);
        clear_plugin_languages_all();
    }

    #[test]
    fn test_register_ast_language_plugins_accepts_compatible_grammar() {
        use cce_config::project::{GrammarAbiPolicy, LanguageExtensionConflictPolicy};

        let _guard = GLOBAL_TABLE_LOCK.lock().expect("test lock poisoned");
        clear_plugin_languages_all();
        let ts_lang = get_tree_sitter_language(&Language::Rust).expect("rust grammar");
        // SAFETY: built-in grammar pointer is valid for the process lifetime.
        let ptr: *const std::ffi::c_void =
            unsafe { std::mem::transmute::<TsLanguage, *const std::ffi::c_void>(ts_lang) };
        let mut registry = crate::plugin::PluginRegistry::new();
        registry.register(std::sync::Arc::new(FakeAstLanguagePlugin::new(
            "ok_grammar",
            "oklang",
            &["okx"],
            Some(ptr),
        )));
        let count = register_ast_language_plugins(
            &registry,
            LanguageExtensionConflictPolicy::Allow,
            GrammarAbiPolicy::Deny,
        );
        assert_eq!(count, 1, "ABI-compatible grammar must be registered");
        assert_eq!(
            cce_types::language::plugin_language_name(0).as_deref(),
            Some("oklang")
        );
        assert!(get_tree_sitter_language(&Language::Custom(0)).is_some());
        clear_plugin_languages_all();
    }

    /// Fake `LanguageRemap` plugin for registration-flow tests.
    struct FakeRemapPlugin {
        meta: cce_plugin::PluginMetadata,
        name: String,
        extensions: Vec<String>,
        target: String,
    }

    impl FakeRemapPlugin {
        fn new(id: &str, name: &str, extensions: &[&str], target: &str) -> Self {
            Self {
                meta: cce_plugin::PluginMetadata {
                    id: id.to_string(),
                    name: id.to_string(),
                    version: "0.1.0".to_string(),
                    priority: 0,
                    capability_priorities: HashMap::new(),
                    description: None,
                    capabilities: Vec::new(),
                },
                name: name.to_string(),
                extensions: extensions.iter().map(|s| s.to_string()).collect(),
                target: target.to_string(),
            }
        }
    }

    impl cce_plugin::CodePlugin for FakeRemapPlugin {
        fn metadata(&self) -> &cce_plugin::PluginMetadata {
            &self.meta
        }
        fn supports_language_remap(&self) -> bool {
            true
        }
        fn remap_grammar_language(&self) -> Option<String> {
            Some(self.target.clone())
        }
        fn language_name(&self) -> Option<String> {
            Some(self.name.clone())
        }
        fn language_extensions(&self) -> Vec<String> {
            self.extensions.clone()
        }
    }

    #[test]
    fn test_register_language_remap_plugin() {
        use cce_config::project::{GrammarAbiPolicy, LanguageExtensionConflictPolicy};

        let _guard = GLOBAL_TABLE_LOCK.lock().expect("test lock poisoned");
        let _cache_guard = crate::tree_sitter_query::loader::QUERY_CACHE_TEST_LOCK
            .lock()
            .expect("test lock poisoned");
        clear_plugin_languages_all();
        let mut registry = crate::plugin::PluginRegistry::new();
        registry.register(std::sync::Arc::new(FakeRemapPlugin::new(
            "remap_js",
            "tplx",
            &["tplx"],
            "JavaScript",
        )));
        let count = register_ast_language_plugins(
            &registry,
            LanguageExtensionConflictPolicy::Allow,
            GrammarAbiPolicy::Deny,
        );
        assert_eq!(count, 1, "remap plugin must be registered");
        let index = cce_types::language::plugin_language_for_extension("tplx").expect("extension");
        assert_eq!(
            cce_types::language::plugin_language_name(index).as_deref(),
            Some("tplx")
        );
        assert_eq!(remap_target(index), Some(Language::JavaScript));
        assert!(
            get_tree_sitter_language(&Language::Custom(index)).is_some(),
            "remap grammar must resolve to the host built-in grammar"
        );
        // Query scheme falls back to the referenced language's scheme.
        let loader = crate::tree_sitter_query::QueryLoader::new();
        assert!(
            loader.get_entity_query(&Language::Custom(index)).is_ok(),
            "entity query must fall back to the JavaScript scheme"
        );
        loader.clear_cache();
        clear_plugin_languages_all();
    }

    #[test]
    fn test_register_language_remap_unknown_target_skipped() {
        use cce_config::project::{GrammarAbiPolicy, LanguageExtensionConflictPolicy};

        let _guard = GLOBAL_TABLE_LOCK.lock().expect("test lock poisoned");
        clear_plugin_languages_all();
        let mut registry = crate::plugin::PluginRegistry::new();
        registry.register(std::sync::Arc::new(FakeRemapPlugin::new(
            "remap_unknown",
            "weird",
            &["wex"],
            "NoSuchLanguage",
        )));
        let count = register_ast_language_plugins(
            &registry,
            LanguageExtensionConflictPolicy::Allow,
            GrammarAbiPolicy::Deny,
        );
        assert_eq!(count, 0, "unknown remap target must not register");
        assert!(cce_types::language::plugin_language_for_extension("wex").is_none());
        clear_plugin_languages_all();
    }

    #[test]
    fn test_language_from_name() {
        assert_eq!(Language::from_name("rust"), Some(Language::Rust));
        assert_eq!(Language::from_name("C++"), Some(Language::Cpp));
        assert_eq!(Language::from_name("c#"), Some(Language::CSharp));
        assert_eq!(Language::from_name("js"), Some(Language::JavaScript));
        assert_eq!(Language::from_name("  Go "), Some(Language::Go));
        assert_eq!(Language::from_name("nope"), None);
    }
}
