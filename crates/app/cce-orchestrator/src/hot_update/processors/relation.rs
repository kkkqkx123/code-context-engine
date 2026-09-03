//! Relation index update processor
//!
//! This module handles updates to the relation index during hot updates.
//! It also persists relation data to SQLite for fast cold start recovery.
//!
//! The implementation is split across submodules:
//! - `relation_processor` – core processor and `UpdateProcessor` impl
//! - `external_packages` – lightweight package data structures
//! - `config_parser` – build-config reload and affected-file identification
//! - `file_identifier` – fingerprint and file-identity helpers

pub mod config_parser;
pub mod external_packages;
pub mod file_identifier;
pub mod relation_processor;

#[cfg(test)]
mod tests;

pub use external_packages::ExternalPackageData;
pub use relation_processor::RelationUpdateProcessor;
