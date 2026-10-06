//! Symbol seed resolution shared by every relation endpoint.
//!
//! All project-scoped relation endpoints address entities by *symbol seed*, so
//! one resolver defines the addressing contract for the whole API surface:
//!
//! | Form | Meaning |
//! |------|---------|
//! | `sym_…` | exact stable symbol id |
//! | `path/to/file#name` | name within one file |
//! | `#name` | name within any file |
//! | `name` | bare name anywhere in the project |
//!
//! Resolution goes through the snapshot's name index, so every form costs
//! O(matches of the name) rather than a project-wide scan. Runtime entity ids
//! are never accepted: they are allocated per process and are not reproducible
//! across reindex runs, so an id from an earlier run silently addresses the
//! wrong entity.

use cce_relation::index::snapshot_query::{SnapshotEntityQueryOps, SnapshotSymbolQueryOps};
use cce_types::EntityId;

use cce_api::models::{ErrorResponse, error_codes};

/// Upper bound on candidates reported for an ambiguous seed.
const MAX_CANDIDATES: usize = 50;

/// Which form of seed the caller supplied.
#[derive(Debug, PartialEq, Eq)]
enum SeedRef<'a> {
    /// A stable symbol id (`sym_…`).
    StableId,
    /// A name optionally scoped to one file (`file#name`, `#name`).
    Named {
        file: Option<&'a str>,
        name: &'a str,
    },
}

/// Parse a raw seed string into its form.
fn parse_seed(raw: &str) -> SeedRef<'_> {
    if let Some((file, name)) = raw.split_once('#') {
        let file = file.trim();
        return SeedRef::Named {
            file: (!file.is_empty()).then_some(file),
            name: name.trim(),
        };
    }
    // Stable ids always start with `sym_`; anything else is a bare symbol name.
    if raw.starts_with("sym_") {
        return SeedRef::StableId;
    }
    SeedRef::Named {
        file: None,
        name: raw.trim(),
    }
}

/// A candidate shown to the caller when a seed is ambiguous. Carries the stable
/// id the client must retry with.
#[derive(Debug, serde::Serialize)]
pub struct SymbolCandidate {
    pub stable_id: String,
    pub file_path: String,
    pub scoped_name: String,
    pub kind: String,
}

fn not_found(seed: &str) -> ErrorResponse {
    ErrorResponse::with_details(
        error_codes::ENTITY_NOT_FOUND,
        format!("Unknown symbol seed: {seed}"),
        "Seed must be a stable symbol ID (sym_…), 'path/to/file#name', '#name', or a bare symbol name.",
    )
}

fn ambiguous(seed: &str, candidates: Vec<SymbolCandidate>) -> ErrorResponse {
    ErrorResponse::with_details(
        error_codes::AMBIGUOUS_SYMBOL,
        format!(
            "Symbol seed '{seed}' matches {} entities; pass one of the candidate stable IDs",
            candidates.len()
        ),
        serde_json::to_string(&candidates).unwrap_or_else(|_| "[]".to_string()),
    )
}

/// Resolve a symbol seed against a published relation snapshot.
///
/// A single match resolves directly; several matches yield `AMBIGUOUS_SYMBOL`
/// carrying the candidate stable ids so the client can disambiguate.
pub fn resolve_symbol_seed<I>(index: &I, seed: &str) -> Result<EntityId, ErrorResponse>
where
    I: SnapshotEntityQueryOps + SnapshotSymbolQueryOps,
{
    let (ids, candidates) = collect_candidates(index, seed);
    match ids.len() {
        1 => Ok(ids[0]),
        0 => Err(not_found(seed)),
        _ => Err(ambiguous(seed, candidates)),
    }
}

/// Resolve several seeds in order, failing on the first unresolvable or
/// ambiguous one.
pub fn resolve_symbol_seeds<I>(index: &I, seeds: &[String]) -> Result<Vec<EntityId>, ErrorResponse>
where
    I: SnapshotEntityQueryOps + SnapshotSymbolQueryOps,
{
    seeds
        .iter()
        .map(|seed| resolve_symbol_seed(index, seed))
        .collect()
}

/// Candidate entity ids for a seed, plus their wire descriptions.
///
/// The name index covers every registered entity regardless of kind, so both
/// the bare-name and the file-scoped form cost a single O(1) index lookup plus
/// O(matches of the name) filtering.
fn collect_candidates<I>(index: &I, seed: &str) -> (Vec<EntityId>, Vec<SymbolCandidate>)
where
    I: SnapshotEntityQueryOps + SnapshotSymbolQueryOps,
{
    let SeedRef::Named { file, name } = parse_seed(seed) else {
        return match index.get_entity_id_by_stable_symbol_id(seed) {
            Some(id) => (vec![id], Vec::new()),
            None => (Vec::new(), Vec::new()),
        };
    };
    if name.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut ids = index.get_function_ids_by_name(name);
    if let Some(file) = file {
        let normalized = cce_types::normalize_project_path(file);
        ids.retain(|id| {
            index
                .get_file_path_by_entity(*id)
                .is_some_and(|path| cce_types::normalize_project_path(&path) == normalized)
        });
    }
    ids.sort();
    ids.dedup();
    let candidates = ids
        .iter()
        .take(MAX_CANDIDATES)
        .filter_map(|id| candidate_for(index, *id))
        .collect();
    (ids, candidates)
}

/// Wire description of one candidate entity.
pub fn candidate_for<I>(index: &I, id: EntityId) -> Option<SymbolCandidate>
where
    I: SnapshotEntityQueryOps + SnapshotSymbolQueryOps,
{
    let key = index.get_symbol_key_by_entity_id(id)?;
    Some(SymbolCandidate {
        stable_id: key.stable_id().0,
        file_path: key.file_path.clone(),
        scoped_name: key.scoped_name.clone(),
        kind: key.kind.to_string(),
    })
}

/// Stable id of an entity, or an empty string when the entity has no symbol key.
pub fn stable_id<I: SnapshotSymbolQueryOps>(index: &I, entity_id: EntityId) -> String {
    index
        .get_symbol_key_by_entity_id(entity_id)
        .map(|key| key.stable_id().0)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_seed_form() {
        assert_eq!(parse_seed("sym_abc"), SeedRef::StableId);
        assert_eq!(
            parse_seed("src/a.rs#foo"),
            SeedRef::Named {
                file: Some("src/a.rs"),
                name: "foo"
            }
        );
        assert_eq!(
            parse_seed("#foo"),
            SeedRef::Named {
                file: None,
                name: "foo"
            }
        );
        assert_eq!(
            parse_seed("foo"),
            SeedRef::Named {
                file: None,
                name: "foo"
            }
        );
        assert_eq!(
            parse_seed("src/a.rs#"),
            SeedRef::Named {
                file: Some("src/a.rs"),
                name: ""
            }
        );
    }
}
