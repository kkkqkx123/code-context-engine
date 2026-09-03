//! Configuration merge trait and implementations.
//!
//! Provides the `Mergeable` trait for declarative config merging,
//! replacing the manual field-by-field merge in `AppConfig::merge_with_project`.

/// Trait for types that can merge an override into themselves.
///
/// Implementors define how an `Override` value is applied to `self`.
/// The merge mutates `self` in place.
pub trait Mergeable {
    /// The override type that provides partial values.
    type Override;

    /// Apply `override_config` onto `self`.
    ///
    /// Fields present in `override_config` replace the corresponding
    /// fields in `self`. Fields absent (None) leave `self` unchanged.
    fn merge(&mut self, override_config: &Self::Override);
}

/// Merge for Option<T>: if override is Some, replace self.
impl<T: Clone> Mergeable for Option<T> {
    type Override = Option<T>;

    fn merge(&mut self, override_config: &Option<T>) {
        if let Some(value) = override_config {
            *self = Some(value.clone());
        }
    }
}

use crate::modules::OrchestratorConfig;
use crate::project::ProjectOrchestratorConfig;

impl Mergeable for OrchestratorConfig {
    type Override = ProjectOrchestratorConfig;

    fn merge(&mut self, override_config: &ProjectOrchestratorConfig) {
        if let Some(ref batch) = override_config.batch {
            self.batch = batch.clone();
        }
        if let Some(ref hot_update) = override_config.hot_update {
            self.hot_update = hot_update.clone();
        }
        if let Some(ref indexer) = override_config.indexer {
            self.indexer = indexer.clone();
        }
        if let Some(ref cache) = override_config.cache {
            self.cache = cache.clone();
        }
        if let Some(ttl) = override_config.checkpoint_ttl_seconds {
            self.checkpoint_ttl_seconds = ttl;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_option_merge_some_replaces() {
        let mut base: Option<String> = Some("old".into());
        base.merge(&Some("new".into()));
        assert_eq!(base.as_deref(), Some("new"));
    }

    #[test]
    fn test_option_merge_none_preserves() {
        let mut base: Option<String> = Some("old".into());
        base.merge(&None);
        assert_eq!(base.as_deref(), Some("old"));
    }

    #[test]
    fn test_option_merge_none_to_some() {
        let mut base: Option<String> = None;
        base.merge(&Some("new".into()));
        assert_eq!(base.as_deref(), Some("new"));
    }

    #[test]
    fn test_option_merge_none_to_none() {
        let mut base: Option<String> = None;
        base.merge(&None);
        assert_eq!(base, None);
    }

    #[test]
    fn test_orchestrator_merge_partial() {
        use crate::modules::OrchestratorConfig;
        use crate::project::ProjectOrchestratorConfig;

        let mut base = OrchestratorConfig {
            checkpoint_ttl_seconds: 100,
            ..Default::default()
        };

        let override_config = ProjectOrchestratorConfig {
            checkpoint_ttl_seconds: Some(200),
            ..Default::default()
        };
        base.merge(&override_config);
        assert_eq!(base.checkpoint_ttl_seconds, 200);
    }

    #[test]
    fn test_orchestrator_merge_preserves_unoverridden() {
        use crate::modules::OrchestratorConfig;
        use crate::project::ProjectOrchestratorConfig;

        let mut base = OrchestratorConfig::default();
        let original_batch = base.batch.clone();
        let override_config = ProjectOrchestratorConfig {
            checkpoint_ttl_seconds: Some(999),
            ..Default::default()
        };
        base.merge(&override_config);
        assert_eq!(base.batch.scan_batch_size, original_batch.scan_batch_size);
        assert_eq!(base.checkpoint_ttl_seconds, 999);
    }

    #[test]
    fn test_merge_with_project_uses_trait() {
        use crate::global::AppConfig;
        use crate::modules::ScannerConfig;
        use crate::project::ProjectAppConfig;

        let global = AppConfig::default();
        let project = ProjectAppConfig {
            scanner: Some(ScannerConfig {
                follow_symlinks: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = global.merge_with_project(&project);
        assert!(merged.scanner.follow_symlinks);
    }
}
