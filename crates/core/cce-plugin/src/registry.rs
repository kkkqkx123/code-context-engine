use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    CodePlugin, OverridePluginSplit, PRIORITY_MAX, PRIORITY_MIN, PluginBundle, PluginCapability,
    PluginError, PluginSource,
};

struct RegistryEntry {
    plugin: Arc<dyn CodePlugin>,
    file_patterns: Option<Vec<String>>,
    compiled_globs: Option<Vec<Option<globset::GlobMatcher>>>,
    languages: Option<Vec<String>>,
    capabilities: Vec<String>,
    priority: i32,
    capability_priorities: HashMap<PluginCapability, i32>,
    content_digest: Option<String>,
    seq: u64,
}

impl RegistryEntry {
    fn effective_priority(&self, capability: PluginCapability) -> i32 {
        self.capability_priorities
            .get(&capability)
            .copied()
            .unwrap_or(self.priority)
    }
}

/// In-memory plugin registry with no I/O dependencies.
///
/// The registry stores plugins and answers queries like
/// "which detectors apply to this file?" It does **not** load plugins
/// from disk — use a [`PluginSource`] implementation for that.
///
/// Internally uses a `HashMap` keyed by plugin ID for O(1) lookup,
/// insertion, and removal. Query results are ordered by `priority`
/// (descending); plugins with equal priority are ordered by registration
/// order (`seq`), which is deterministic across processes.
pub struct PluginRegistry {
    entries: HashMap<String, RegistryEntry>,
    next_seq: u64,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            next_seq: 0,
        }
    }

    /// Register a plugin directly with no file/language filter.
    ///
    /// If a plugin with the same ID already exists, it is replaced
    /// and a warning is logged.
    pub fn register(&mut self, plugin: Arc<dyn CodePlugin>) {
        self.register_bundle(PluginBundle::new(plugin));
    }

    /// Register a bundle produced by a [`PluginSource`].
    pub fn register_bundle(&mut self, bundle: PluginBundle) {
        let id = bundle.plugin.metadata().id.clone();
        if self.entries.contains_key(&id) {
            tracing::warn!("Replacing duplicate plugin: {id}");
        }
        let compiled_globs = bundle.file_patterns.as_ref().map(|patterns| {
            patterns
                .iter()
                .map(|p| {
                    if p.contains('*') || p.contains('?') || p.contains('[') {
                        globset::Glob::new(p).ok().map(|g| g.compile_matcher())
                    } else {
                        None
                    }
                })
                .collect()
        });
        let capabilities = bundle
            .capabilities
            .clone()
            .unwrap_or_else(|| bundle.plugin.metadata().capabilities.clone());
        let priority = bundle
            .priority
            .unwrap_or_else(|| bundle.plugin.metadata().priority);
        if !(PRIORITY_MIN..=PRIORITY_MAX).contains(&priority) {
            tracing::warn!(
                id = %id,
                priority,
                min = PRIORITY_MIN,
                max = PRIORITY_MAX,
                "Plugin priority outside the recommended interval ({PRIORITY_MIN}-{PRIORITY_MAX})"
            );
        }
        let capability_priorities = Self::resolve_capability_priorities(
            id.as_str(),
            bundle
                .capability_priorities
                .as_ref()
                .unwrap_or(&bundle.plugin.metadata().capability_priorities),
        );
        let seq = self.entries.get(&id).map(|e| e.seq).unwrap_or_else(|| {
            let s = self.next_seq;
            self.next_seq += 1;
            s
        });
        self.entries.insert(
            id,
            RegistryEntry {
                plugin: bundle.plugin,
                file_patterns: bundle.file_patterns,
                compiled_globs,
                languages: bundle.languages,
                capabilities,
                priority,
                capability_priorities,
                content_digest: bundle.content_digest,
                seq,
            },
        );
    }

    fn resolve_capability_priorities(
        id: &str,
        source: &HashMap<String, i32>,
    ) -> HashMap<PluginCapability, i32> {
        let mut resolved = HashMap::new();
        for (name, priority) in source {
            let Ok(capability) = PluginCapability::parse(name.as_str()) else {
                tracing::warn!(id, capability = %name, "Unknown capability in priority overrides, ignored");
                continue;
            };
            if !(PRIORITY_MIN..=PRIORITY_MAX).contains(priority) {
                tracing::warn!(
                    id,
                    capability = %name,
                    priority,
                    min = PRIORITY_MIN,
                    max = PRIORITY_MAX,
                    "Capability priority outside the recommended interval ({PRIORITY_MIN}-{PRIORITY_MAX})"
                );
            }
            resolved.insert(capability, *priority);
        }
        resolved
    }

    /// Load plugins from a source and register them all.
    pub fn load_source(&mut self, source: &dyn PluginSource) -> Result<usize, PluginError> {
        let bundles = source.collect()?;
        let count = bundles.len();
        for bundle in bundles {
            self.register_bundle(bundle);
        }
        Ok(count)
    }

    fn filter_plugins(
        &self,
        file_path: Option<&str>,
        language: Option<&str>,
        capability: PluginCapability,
        admit: impl Fn(&RegistryEntry) -> bool,
    ) -> Vec<&Arc<dyn CodePlugin>> {
        self.filter_plugin_entries(file_path, language, capability, admit)
            .into_iter()
            .map(|e| &e.plugin)
            .collect()
    }

    fn filter_plugin_entries(
        &self,
        file_path: Option<&str>,
        language: Option<&str>,
        capability: PluginCapability,
        admit: impl Fn(&RegistryEntry) -> bool,
    ) -> Vec<&RegistryEntry> {
        let mut matched: Vec<&RegistryEntry> = self
            .entries
            .values()
            .filter(|e| admit(e))
            .filter(|e| {
                if let Some(path) = file_path {
                    if let Some(patterns) = &e.file_patterns {
                        if !Self::matches_any_pattern(path, patterns, e.compiled_globs.as_deref()) {
                            return false;
                        }
                    }
                }
                if let Some(langs) = &e.languages {
                    if !Self::matches_language(language, langs) {
                        return false;
                    }
                }
                true
            })
            .collect();

        matched.sort_by_key(|e| (std::cmp::Reverse(e.effective_priority(capability)), e.seq));
        matched
    }

    fn matches_any_pattern(
        path: &str,
        patterns: &[String],
        compiled_globs: Option<&[Option<globset::GlobMatcher>]>,
    ) -> bool {
        if patterns.is_empty() {
            return true;
        }
        let iter = patterns.iter().enumerate();
        if let Some(globs) = compiled_globs {
            for (i, pattern) in iter {
                let ok = if let Some(Some(matcher)) = globs.get(i) {
                    matcher.is_match(path)
                } else {
                    path.ends_with(pattern)
                };
                if ok {
                    return true;
                }
            }
        } else {
            for pattern in patterns {
                let ok = if pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
                {
                    match globset::Glob::new(pattern) {
                        Ok(g) => g.compile_matcher().is_match(path),
                        Err(_) => false,
                    }
                } else {
                    path.ends_with(pattern)
                };
                if ok {
                    return true;
                }
            }
        }
        false
    }

    fn matches_language(language: Option<&str>, allowed: &[String]) -> bool {
        if allowed.is_empty() {
            return true;
        }
        match language {
            Some(lang) => {
                let lang_lower = lang.to_lowercase();
                allowed.iter().any(|l| l.to_lowercase() == lang_lower)
            }
            None => false,
        }
    }

    pub fn get_plugins(
        &self,
        capability: PluginCapability,
        file_path: Option<&str>,
        language: Option<&str>,
    ) -> Vec<&Arc<dyn CodePlugin>> {
        self.filter_plugins(file_path, language, capability, |e| {
            PluginCapability::declared(&e.capabilities, capability)
                && PluginCapability::supported(e.plugin.as_ref(), capability)
        })
    }

    pub fn get_override_plugins(
        &self,
        capability: PluginCapability,
        file_path: Option<&str>,
        language: Option<&str>,
    ) -> OverridePluginSplit<'_> {
        self.get_override_with(file_path, language, capability, |e| {
            PluginCapability::declared(&e.capabilities, capability)
                && PluginCapability::supported(e.plugin.as_ref(), capability)
        })
    }

    fn get_override_with(
        &self,
        file_path: Option<&str>,
        language: Option<&str>,
        capability: PluginCapability,
        admit: impl Fn(&RegistryEntry) -> bool,
    ) -> OverridePluginSplit<'_> {
        let entries = self.filter_plugin_entries(file_path, language, capability, admit);
        let split = entries
            .iter()
            .position(|e| e.effective_priority(capability) < 0)
            .unwrap_or(entries.len());
        let (above, below) = entries.split_at(split);
        (
            above.iter().map(|e| &e.plugin).collect(),
            below.iter().map(|e| &e.plugin).collect(),
        )
    }

    pub fn get_bm25_generators(
        &self,
        file_path: Option<&str>,
        language: Option<&str>,
    ) -> Vec<&Arc<dyn CodePlugin>> {
        self.filter_plugins(file_path, language, PluginCapability::TextGen, |e| {
            PluginCapability::declared(&e.capabilities, PluginCapability::TextGen)
                && e.plugin.supports_bm25()
        })
    }

    pub fn get_override_bm25_generators(
        &self,
        file_path: Option<&str>,
        language: Option<&str>,
    ) -> OverridePluginSplit<'_> {
        self.get_override_with(file_path, language, PluginCapability::TextGen, |e| {
            PluginCapability::declared(&e.capabilities, PluginCapability::TextGen)
                && e.plugin.supports_bm25()
        })
    }

    pub fn get_embedding_generators(
        &self,
        file_path: Option<&str>,
        language: Option<&str>,
    ) -> Vec<&Arc<dyn CodePlugin>> {
        self.filter_plugins(file_path, language, PluginCapability::TextGen, |e| {
            PluginCapability::declared(&e.capabilities, PluginCapability::TextGen)
                && e.plugin.supports_embedding()
        })
    }

    pub fn get_override_embedding_generators(
        &self,
        file_path: Option<&str>,
        language: Option<&str>,
    ) -> OverridePluginSplit<'_> {
        self.get_override_with(file_path, language, PluginCapability::TextGen, |e| {
            PluginCapability::declared(&e.capabilities, PluginCapability::TextGen)
                && e.plugin.supports_embedding()
        })
    }

    pub fn remove(&mut self, id: &str) -> bool {
        self.entries.remove(id).is_some()
    }

    pub fn ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn textgen_fingerprint(&self) -> String {
        use cce_utils::hash::calculate_hash;

        let mut rows: Vec<String> = self
            .entries
            .values()
            .filter(|entry| {
                PluginCapability::declared(&entry.capabilities, PluginCapability::TextGen)
                    && PluginCapability::supported(entry.plugin.as_ref(), PluginCapability::TextGen)
            })
            .map(|entry| {
                let meta = entry.plugin.metadata();
                format!(
                    "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                    meta.id,
                    meta.version,
                    entry.priority,
                    entry.content_digest.as_deref().unwrap_or("-")
                )
            })
            .collect();
        rows.sort();
        calculate_hash(rows.join("\u{1e}").as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PluginMetadata;

    struct TestPlugin {
        meta: PluginMetadata,
        bm25: bool,
        embedding: bool,
        fusion: bool,
    }

    impl TestPlugin {
        fn new(id: &str) -> Self {
            Self {
                meta: PluginMetadata {
                    id: id.to_string(),
                    name: id.to_string(),
                    version: "0.1.0".to_string(),
                    priority: 0,
                    capability_priorities: HashMap::new(),
                    description: None,
                    capabilities: Vec::new(),
                },
                bm25: true,
                embedding: true,
                fusion: true,
            }
        }
    }

    impl CodePlugin for TestPlugin {
        fn metadata(&self) -> &PluginMetadata {
            &self.meta
        }
        fn supports_bm25(&self) -> bool {
            self.bm25
        }
        fn supports_embedding(&self) -> bool {
            self.embedding
        }
        fn supports_fusion(&self) -> bool {
            self.fusion
        }
    }

    fn id_of(plugin: &Arc<dyn CodePlugin>) -> &str {
        &plugin.metadata().id
    }

    #[test]
    fn test_plugin_without_filters_matches_all_files() {
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(TestPlugin::new("unfiltered")));

        let generators = registry.get_bm25_generators(Some("src/main.rs"), Some("rust"));
        assert_eq!(generators.len(), 1);
        assert_eq!(id_of(generators[0]), "unfiltered");
    }

    #[test]
    fn test_empty_pattern_list_matches_all_files() {
        let mut registry = PluginRegistry::new();
        registry.register_bundle(
            PluginBundle::new(Arc::new(TestPlugin::new("empty_patterns")))
                .with_file_patterns(vec![])
                .with_languages(vec![]),
        );

        let generators = registry.get_bm25_generators(Some("any/path/file.py"), Some("python"));
        assert_eq!(generators.len(), 1);
    }

    #[test]
    fn test_file_patterns_filter() {
        let mut registry = PluginRegistry::new();
        registry.register_bundle(
            PluginBundle::new(Arc::new(TestPlugin::new("py_only")))
                .with_file_patterns(vec!["*.py".to_string()]),
        );

        assert_eq!(registry.get_bm25_generators(Some("app.py"), None).len(), 1);
        assert_eq!(registry.get_bm25_generators(Some("app.rs"), None).len(), 0);
    }

    #[test]
    fn test_language_filter_applies_without_file_path() {
        let mut registry = PluginRegistry::new();
        registry.register_bundle(
            PluginBundle::new(Arc::new(TestPlugin::new("python_only")))
                .with_languages(vec!["python".to_string()]),
        );

        assert_eq!(registry.get_bm25_generators(None, Some("python")).len(), 1);
        assert_eq!(registry.get_bm25_generators(None, Some("rust")).len(), 0);
    }

    #[test]
    fn test_language_match_is_case_insensitive() {
        let mut registry = PluginRegistry::new();
        registry.register_bundle(
            PluginBundle::new(Arc::new(TestPlugin::new("py_case")))
                .with_languages(vec!["Python".to_string()]),
        );

        assert_eq!(
            registry
                .get_bm25_generators(Some("app.py"), Some("python"))
                .len(),
            1
        );
    }

    #[test]
    fn test_language_constrained_plugin_skips_unknown_language() {
        let mut registry = PluginRegistry::new();
        registry.register_bundle(
            PluginBundle::new(Arc::new(TestPlugin::new("python_only")))
                .with_languages(vec!["python".to_string()]),
        );

        assert_eq!(
            registry.get_bm25_generators(Some("README.md"), None).len(),
            0
        );
        assert_eq!(registry.get_bm25_generators(None, None).len(), 0);
        assert_eq!(registry.get_bm25_generators(None, Some("python")).len(), 1);
    }

    #[test]
    fn test_priority_sorting_descending() {
        let mut registry = PluginRegistry::new();
        let mut low = TestPlugin::new("low");
        low.meta.priority = 1;
        let mut high = TestPlugin::new("high");
        high.meta.priority = 100;
        registry.register(Arc::new(low));
        registry.register(Arc::new(high));

        let generators = registry.get_bm25_generators(None, None);
        assert_eq!(generators.len(), 2);
        assert_eq!(id_of(generators[0]), "high");
        assert_eq!(id_of(generators[1]), "low");
    }

    #[test]
    fn test_priority_ties_follow_registration_order() {
        let mut registry = PluginRegistry::new();
        let mut top = TestPlugin::new("top");
        top.meta.priority = 10;
        registry.register(Arc::new(top));
        registry.register(Arc::new(TestPlugin::new("a")));
        registry.register(Arc::new(TestPlugin::new("b")));
        registry.register(Arc::new(TestPlugin::new("c")));

        let generators = registry.get_bm25_generators(None, None);
        let ids: Vec<&str> = generators.iter().map(|p| id_of(p)).collect();
        assert_eq!(ids, vec!["top", "a", "b", "c"]);

        let again: Vec<&str> = registry
            .get_bm25_generators(None, None)
            .iter()
            .map(|p| id_of(p))
            .collect();
        assert_eq!(again, ids);
    }

    #[test]
    fn test_replaced_plugin_keeps_registration_order() {
        let mut registry = PluginRegistry::new();
        registry.register(Arc::new(TestPlugin::new("a")));
        registry.register(Arc::new(TestPlugin::new("b")));
        registry.register(Arc::new(TestPlugin::new("a")));

        let generators = registry.get_bm25_generators(None, None);
        let ids: Vec<&str> = generators.iter().map(|p| id_of(p)).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn test_bundle_priority_override_wins_over_metadata() {
        let mut registry = PluginRegistry::new();
        let mut declared_high = TestPlugin::new("decl_100");
        declared_high.meta.priority = 100;
        registry.register(Arc::new(declared_high));
        registry.register_bundle(
            PluginBundle::new(Arc::new(TestPlugin::new("overridden_0"))).with_priority(200),
        );

        let generators = registry.get_bm25_generators(None, None);
        assert_eq!(id_of(generators[0]), "overridden_0");
        assert_eq!(id_of(generators[1]), "decl_100");
    }

    #[test]
    fn test_bundle_priority_falls_back_to_metadata() {
        let mut registry = PluginRegistry::new();
        let mut from_metadata = TestPlugin::new("from_metadata");
        from_metadata.meta.priority = 50;
        registry.register_bundle(PluginBundle::new(Arc::new(from_metadata)));

        let generators = registry.get_bm25_generators(None, None);
        assert_eq!(generators.len(), 1);
        assert_eq!(id_of(generators[0]), "from_metadata");
    }

    #[test]
    fn test_capability_filtering() {
        let mut registry = PluginRegistry::new();
        let mut bm25_only = TestPlugin::new("bm25_only");
        bm25_only.embedding = false;
        registry.register(Arc::new(bm25_only));

        assert_eq!(registry.get_bm25_generators(None, None).len(), 1);
        assert_eq!(registry.get_embedding_generators(None, None).len(), 0);
    }

    #[test]
    fn test_capability_priority_overrides_plugin_priority() {
        let mut registry = PluginRegistry::new();
        let mut high_global = TestPlugin::new("high_global");
        high_global.meta.priority = 500;
        registry.register(Arc::new(high_global));
        let mut fusion_fast = TestPlugin::new("fusion_fast");
        fusion_fast.meta.priority = 10;
        fusion_fast
            .meta
            .capability_priorities
            .insert("fusion".to_string(), 1000);
        registry.register(Arc::new(fusion_fast));

        let generators = registry.get_bm25_generators(None, None);
        let ids: Vec<&str> = generators.iter().map(|p| id_of(p)).collect();
        assert_eq!(ids, vec!["high_global", "fusion_fast"]);

        let fusion = registry.get_plugins(PluginCapability::Fusion, None, None);
        let ids: Vec<&str> = fusion.iter().map(|p| id_of(p)).collect();
        assert_eq!(ids, vec!["fusion_fast", "high_global"]);
    }

    #[test]
    fn test_bundle_capability_priority_override_wins_over_metadata() {
        let mut registry = PluginRegistry::new();
        let mut declared = TestPlugin::new("declared_cap");
        declared.meta.priority = 300;
        declared
            .meta
            .capability_priorities
            .insert("fusion".to_string(), 200);
        registry.register(Arc::new(declared));

        let mut overridden = TestPlugin::new("overridden_cap");
        overridden.meta.priority = 10;
        registry.register_bundle(
            PluginBundle::new(Arc::new(overridden))
                .with_priority(100)
                .with_capability_priorities(HashMap::from([("fusion".to_string(), 9999)])),
        );

        let fusion = registry.get_plugins(PluginCapability::Fusion, None, None);
        let ids: Vec<&str> = fusion.iter().map(|p| id_of(p)).collect();
        assert_eq!(ids, vec!["overridden_cap", "declared_cap"]);

        let generators = registry.get_bm25_generators(None, None);
        let ids: Vec<&str> = generators.iter().map(|p| id_of(p)).collect();
        assert_eq!(ids, vec!["declared_cap", "overridden_cap"]);
    }

    #[test]
    fn test_unknown_capability_priority_is_ignored() {
        let mut registry = PluginRegistry::new();
        registry.register_bundle(
            PluginBundle::new(Arc::new(TestPlugin::new("bad_cap")))
                .with_capability_priorities(HashMap::from([("not_a_cap".to_string(), 42)])),
        );

        let generators = registry.get_bm25_generators(None, None);
        assert_eq!(generators.len(), 1);
    }

    #[test]
    fn test_override_split_at_builtin_boundary() {
        let mut registry = PluginRegistry::new();

        let mut high = TestPlugin::new("high");
        high.meta.priority = 100;
        registry.register(Arc::new(high));

        let mut neutral = TestPlugin::new("neutral");
        neutral.meta.priority = 0;
        registry.register(Arc::new(neutral));

        let mut fallback = TestPlugin::new("fallback");
        fallback.meta.priority = -1;
        registry.register(Arc::new(fallback));

        let (above, below) = registry.get_override_plugins(PluginCapability::TextGen, None, None);
        let above_ids: Vec<&str> = above.iter().map(|p| id_of(p)).collect();
        let below_ids: Vec<&str> = below.iter().map(|p| id_of(p)).collect();
        assert_eq!(above_ids, vec!["high", "neutral"]);
        assert_eq!(below_ids, vec!["fallback"]);

        let all = registry.get_plugins(PluginCapability::TextGen, None, None);
        let ids: Vec<&str> = all.iter().map(|p| id_of(p)).collect();
        assert_eq!(ids, vec!["high", "neutral", "fallback"]);
    }

    #[test]
    fn test_override_split_honors_negative_capability_priority() {
        let mut registry = PluginRegistry::new();
        let mut plugin = TestPlugin::new("fusion_fallback");
        plugin.meta.priority = 50;
        plugin
            .meta
            .capability_priorities
            .insert("fusion".to_string(), -1);
        registry.register(Arc::new(plugin));

        let (fusion_above, fusion_below) =
            registry.get_override_plugins(PluginCapability::Fusion, None, None);
        assert!(fusion_above.is_empty());
        assert_eq!(fusion_below.len(), 1);

        let (bm25_above, bm25_below) = registry.get_override_bm25_generators(None, None);
        assert_eq!(bm25_above.len(), 1);
        assert!(bm25_below.is_empty());
    }

    #[test]
    fn test_priority_below_min_warns_and_keeps_fallback_ordering() {
        let mut registry = PluginRegistry::new();
        let mut deep = TestPlugin::new("deep_fallback");
        deep.meta.priority = -100_000;
        registry.register(Arc::new(deep));

        let (_, below) = registry.get_override_plugins(PluginCapability::TextGen, None, None);
        assert_eq!(below.len(), 1);
    }

    #[test]
    fn test_textgen_fingerprint_reflects_registry_changes() {
        let mut registry = PluginRegistry::new();
        let empty_fp = registry.textgen_fingerprint();

        registry.register(Arc::new(TestPlugin::new("gen")));
        let with_plugin_fp = registry.textgen_fingerprint();
        assert_ne!(empty_fp, with_plugin_fp);

        let mut bundle = PluginBundle::new(Arc::new(TestPlugin::new("gen2")));
        bundle = bundle.with_content_digest("deadbeef".to_string());
        registry.register_bundle(bundle);
        let with_digest_fp = registry.textgen_fingerprint();
        assert_ne!(with_plugin_fp, with_digest_fp);

        let mut bundle = PluginBundle::new(Arc::new(TestPlugin::new("gen2")));
        bundle = bundle.with_content_digest("feedface".to_string());
        registry.register_bundle(bundle);
        assert_ne!(with_digest_fp, registry.textgen_fingerprint());

        let mut non_textgen = TestPlugin::new("non_textgen");
        non_textgen.bm25 = false;
        non_textgen.embedding = false;
        registry.register(Arc::new(non_textgen));
        let before = registry.textgen_fingerprint();
        registry.remove("non_textgen");
        assert_eq!(before, registry.textgen_fingerprint());

        registry.remove("gen2");
        registry.register(Arc::new(TestPlugin::new("gen2")));
        assert_ne!(with_digest_fp, registry.textgen_fingerprint());
    }
}
