//! Tools API handlers
//!
//! This module provides HTTP handlers for tool APIs that support programming tasks.
//! These tools offer functionality similar to LSP features but operate on-demand
//! without side effects.
//!
//! # Available Endpoints
//!
//! - **Compression**: `/api/tools/compress`, `/api/tools/compress/batch`
//! - **AST Diagnosis**: `/api/tools/diagnose`
//! - **File Fold**: `/api/tools/fold`
//! - **Keyword Search**: `/api/tools/keyword-search`
//! - **Symbol Lookup**: `/api/tools/symbols`, `/api/tools/references`, `/api/tools/definition`

pub mod compression;
pub mod diagnosis;
pub mod fold;
pub mod keyword;
pub mod symbol;

pub use compression::{handle_compress, handle_compress_batch};
pub use diagnosis::handle_diagnose;
pub use fold::handle_fold;
pub use keyword::handle_keyword_search;
pub use symbol::{handle_find_references, handle_get_symbols, handle_goto_definition};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Convert an orchestrator response into the shared cce-api wire model.
///
/// The orchestrator and cce-api shapes mirror each other; the only
/// differences are enum-as-string fields and ID newtypes, which serde
/// handles transparently.
pub(crate) fn to_api_model<T: Serialize, R: DeserializeOwned>(value: T) -> Result<R, String> {
    serde_json::from_value(serde_json::to_value(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
