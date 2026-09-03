//! Trait abstracting standard-library detection so that `cce-relation`
//! does not need to depend on the full `StdlibDetector` from `cce-parser`.

use cce_types::language::Language;
use cce_types::relation::RelationType;
use std::sync::OnceLock;

/// Abstraction for standard-library call detection.
///
/// The only method mirrors `StdlibDetector::is_stdlib_by_type`.  A
/// concrete implementation is provided by `cce-parser` and registered
/// via [`set_stdlib_classifier`].
pub trait StdlibClassifier: Send + Sync {
    /// Return `true` when `call_name` belongs to the language's standard
    /// library, using the faster relation-type-based path when available.
    fn is_stdlib_by_type(
        &self,
        call_name: &str,
        relation_type: &RelationType,
        language: &Language,
    ) -> bool;
}

static CLASSIFIER: OnceLock<Box<dyn StdlibClassifier>> = OnceLock::new();

/// Register the global [`StdlibClassifier`] implementation.
///
/// Must be called exactly once, before any concurrent use.
pub fn set_stdlib_classifier(classifier: Box<dyn StdlibClassifier>) {
    let _ = CLASSIFIER.set(classifier);
}

/// Access the global classifier.
///
/// Returns `None` if [`set_stdlib_classifier`] has not been called yet.
pub fn with_stdlib_classifier<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&dyn StdlibClassifier) -> R,
{
    CLASSIFIER.get().map(|c| f(c.as_ref()))
}
