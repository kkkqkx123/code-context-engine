//! Stable, disk-serializable representation of a simple-name candidate.
//!
//! Full builds keep the unbounded candidate sets in the operation-local
//! SQLite database. The in-memory project table and the disk-backed view use
//! the same ordering and identity rules through this type.

use crate::type_inference::overload::ParamArity;
use cce_types::entity::EntityId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolCandidateKind {
    FileSymbol,
    PackageExport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolCandidate {
    pub name: String,
    pub arity: ParamArity,
    /// Deterministic ordering key: lower values are preferred.
    pub sort_key: String,
    pub kind: SymbolCandidateKind,
    pub entity_id: EntityId,
    pub file_path: Option<String>,
    pub module_path: String,
    pub package_id: Option<String>,
}

impl SymbolCandidate {
    pub fn for_file(
        name: &str,
        arity: ParamArity,
        file_path: &str,
        module_path: &str,
        entity_id: EntityId,
        call_frequency: Option<u64>,
    ) -> Self {
        let frequency = Self::frequency_key(call_frequency);
        Self {
            name: name.to_string(),
            arity,
            sort_key: format!("{frequency}::{file_path}::{module_path}"),
            kind: SymbolCandidateKind::FileSymbol,
            entity_id,
            file_path: Some(file_path.to_string()),
            module_path: module_path.to_string(),
            package_id: None,
        }
    }

    pub fn for_package(
        name: &str,
        package_id: &str,
        entity_id: EntityId,
        call_frequency: Option<u64>,
    ) -> Self {
        let frequency = Self::frequency_key(call_frequency);
        Self {
            name: name.to_string(),
            arity: ParamArity::unknown(),
            sort_key: format!("{frequency}::{package_id}::"),
            kind: SymbolCandidateKind::PackageExport,
            entity_id,
            file_path: None,
            module_path: String::new(),
            package_id: Some(package_id.to_string()),
        }
    }

    fn frequency_key(call_frequency: Option<u64>) -> u64 {
        call_frequency
            .map(|value| u64::MAX - value)
            .unwrap_or(u64::MAX)
    }

    /// Key used for `(name, arity)` candidate buckets.
    pub fn bucket_id(name: &str, arity: &ParamArity) -> String {
        format!(
            "{name}\0{}\0{}\0{}\0{}",
            arity.total, arity.required, arity.is_varargs, arity.known
        )
    }

    /// Whether this candidate can accept `count` call arguments.
    pub fn accepts(&self, count: usize) -> bool {
        self.arity.accepts(count)
    }
}
