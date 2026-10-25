//! Shared index-option construction for local and HTTP transports.
//!
//! Keeps the request-to-`IndexOptions` mapping in one place so the two
//! transports cannot drift.

use cce_api::models::IndexRequest;
use cce_orchestrator::IndexOptions;
use std::path::PathBuf;

/// Build `IndexOptions` from an index request, applying the same defaults
/// used by both the local service and the HTTP handler.
pub fn build_index_options(request: &IndexRequest) -> IndexOptions {
    let root_dir = PathBuf::from(&request.path);
    let mut options = IndexOptions::new(&root_dir)
        .with_extensions(if request.extensions.is_empty() {
            vec![
                "rs".to_string(),
                "py".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "c".to_string(),
                "cpp".to_string(),
                "java".to_string(),
            ]
        } else {
            request.extensions.clone()
        })
        .with_exclude_dirs(if request.exclude_dirs.is_empty() {
            vec![
                "node_modules".to_string(),
                "target".to_string(),
                ".git".to_string(),
                "vendor".to_string(),
            ]
        } else {
            request.exclude_dirs.clone()
        })
        .with_gitignore(request.respect_gitignore)
        .with_ignore_patterns(request.ignore_patterns.clone());
    if let Some(custom) = request.custom_gitignore.clone() {
        options = options.with_custom_gitignore(custom);
    }
    options
}
