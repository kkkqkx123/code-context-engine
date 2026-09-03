//! Change detection module for hot update
//!
//! This module provides types and computation logic for detecting
//! and tracking file and entity changes.

pub(crate) mod compute;
mod types;

pub use types::{
    BatchChangeResult, EntityChange, EntityChangeType, FileChange, FileChangeType,
    ParseResultWithChanges,
};
