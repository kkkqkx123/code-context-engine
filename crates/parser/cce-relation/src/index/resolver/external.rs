//! Relation resolver for converting raw relations to resolved relations
//!
//! Provides functionality to resolve raw relations by looking up symbols in
//! a global symbol table and classifying external calls (standard library,
//! external packages, or unknown).

use super::RelationResolver;
use crate::index::EntityIndexOps;
use crate::index::core::RelationIndex;
use crate::stdlib_classifier::with_stdlib_classifier;
use crate::symbol::SymbolRef;
use crate::symbol_table::ProjectSymbolTable;

use cce_types::entity::EntityId;
use cce_types::relation::{ExternalCallType, RelationSymbolLocation, RelationSymbolRecord};
use cce_types::{ParsedFile, RawRelationData};

impl RelationResolver {
    /// Check if a call is an external call
    ///
    /// # Arguments
    ///
    /// * `raw_data` - The raw relation data
    /// * `parsed` - The parsed file containing the relation
    /// * `symbol_table` - The global symbol table for cross-file resolution
    ///
    /// # Returns
    ///
    pub fn is_external_call(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
        symbol_table: &ProjectSymbolTable,
    ) -> bool {
        let qualified_name = format!("{}::{}", parsed.path, raw_data.dst_name);
        let found: Option<EntityId> = symbol_table
            .get_by_qualified_name(&qualified_name)
            .or_else(|| symbol_table.get_by_simple_name(&raw_data.dst_name));

        if found.is_some() {
            return false;
        }

        // Qualified names retry with the last segment so a local
        // definition still counts as internal.
        let last = self.last_segment_for_resolution(&raw_data.dst_name, false);
        if let Some(last) = last {
            if parsed.local_symbols.contains_key(last) {
                return false;
            }
        }

        // Check local symbols
        parsed.local_symbols.contains_key(&raw_data.dst_name)
    }

    /// Resolve a symbol reference to a real entity ID if the entity index already contains it.
    pub(crate) fn resolve_symbol_to_entity_id(
        &self,
        symbol_ref: &SymbolRef,
        entity_index: &RelationIndex,
    ) -> Option<EntityId> {
        let target_name = symbol_ref.metadata.name_str();
        let target_file = symbol_ref.metadata.location.file_path.as_ref();
        self.resolve_entity_by_name(target_name, Some(target_file), entity_index)
    }

    /// Resolve a bare target name to a real entity ID via the entity index.
    ///
    /// this is the only legitimate source of entity IDs during
    /// resolution. Symbol-table IDs live in a separate ID space
    /// and must never be cast into `EntityId` — doing so produced dangling
    /// `callee_id`s that only surfaced at snapshot validation.
    ///
    /// When `prefer_file` is supplied, entities in that file win; otherwise
    /// the first entity registered under the name is used.
    pub(crate) fn resolve_entity_by_name(
        &self,
        name: &str,
        prefer_file: Option<&str>,
        entity_index: &RelationIndex,
    ) -> Option<EntityId> {
        let ids = entity_index.get_function_ids_by_name(name);
        match prefer_file {
            Some(file_path) => ids
                .iter()
                .copied()
                .find(|id| {
                    entity_index
                        .get_file_path_by_entity(*id)
                        .as_deref()
                        .is_some_and(|p| Self::paths_equivalent(p, file_path))
                })
                .or_else(|| ids.first().copied()),
            None => ids.first().copied(),
        }
    }

    /// Snapshot a symbol reference for storage inside resolved relations.
    pub(crate) fn snapshot_symbol(
        &self,
        symbol_ref: &SymbolRef,
        entity_id: Option<EntityId>,
    ) -> RelationSymbolRecord {
        let location = &symbol_ref.metadata.location;
        RelationSymbolRecord {
            symbol_id: symbol_ref.symbol_id().0,
            entity_id,
            name: symbol_ref.metadata.name_str().to_string(),
            kind: symbol_ref.metadata.kind,
            location: RelationSymbolLocation {
                file_path: location.file_path.to_string(),
                package_path: location.package_path.as_ref().map(|p| p.to_string()),
                module_path: location.module_path.as_ref().map(|m| m.to_string()),
                span: location.span,
            },
            source_module: symbol_ref.source_module_str().map(|m| m.to_string()),
        }
    }

    /// Compare file paths using the canonical normalized equality
    ///
    /// Separator-agnostic (`/` vs `\`) and tolerant of a leading `/` (absolute
    /// vs relative), but deliberately not suffix-aware: `a/b/c.rs` and
    /// `b/c.rs` are different files. Shares the single implementation in
    /// `cce_utils::path`.
    pub(crate) fn paths_equivalent(left: &str, right: &str) -> bool {
        cce_types::path::normalized_equals(left, right)
    }

    /// Internal method to classify external packages (non-stdlib)
    ///
    /// This is the shared logic for both resolve() and classify_external_call()
    /// to determine the type of external non-stdlib calls.
    ///
    /// Implements the degraded fallback chain:
    /// 1. manifest-based lookup via dependency_index / external_packages
    /// 2. import-table lookup
    /// 3. naming-convention heuristic (java.*, javax.*, std:: etc.)
    /// 4. Unknown
    pub(crate) fn classify_external_package(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
    ) -> Option<ExternalCallType> {
        if let Some(ext) = self.try_classify_from_manifest(raw_data, parsed) {
            return Some(ext);
        }
        if let Some(ext) = self.try_classify_from_imports(raw_data, parsed) {
            return Some(ext);
        }
        if let Some(ext) = self.try_classify_from_naming_convention(raw_data, parsed) {
            return Some(ext);
        }
        Some(ExternalCallType::Unknown {
            raw_target: raw_data.dst_name.clone(),
        })
    }

    fn try_classify_from_manifest(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
    ) -> Option<ExternalCallType> {
        let matched_dep = self
            .dependency_index
            .as_ref()
            .and_then(|idx| idx.find_dependency(parsed.language, &raw_data.dst_name));

        if let Some(dep) = matched_dep {
            let external_type = match dep.package_type_str() {
                "dev" => ExternalCallType::dev_dependency(dep.name.clone()),
                "local" => ExternalCallType::local_dependency(dep.name.clone()),
                _ => ExternalCallType::external_library(dep.name.clone()),
            };
            return Some(external_type);
        }

        let packages_to_check = self
            .external_packages
            .as_ref()
            .and_then(|pkgs| pkgs.get(&parsed.language));

        let first_segment = raw_data
            .dst_name
            .split([':', '.'])
            .next()
            .unwrap_or(&raw_data.dst_name);

        let external_package = packages_to_check.and_then(|packages| {
            packages
                .iter()
                .find(|pkg| pkg.as_str() == first_segment)
                .cloned()
        });

        external_package.map(ExternalCallType::external_library)
    }

    fn try_classify_from_imports(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
    ) -> Option<ExternalCallType> {
        let Some(import_table) = &parsed.import_table else {
            return None;
        };
        let first_segment = raw_data
            .dst_name
            .split([':', '.', '/'])
            .next()
            .unwrap_or(&raw_data.dst_name);
        for import in &import_table.standardized_imports {
            let import_first = import
                .source
                .split([':', '.', '/', '@'])
                .next()
                .unwrap_or(&import.source);
            if import_first == first_segment {
                // If import source matches callee prefix, treat as external_library
                // Use import source's first segment as package name
                return Some(ExternalCallType::external_library(
                    first_segment.to_string(),
                ));
            }
            if import.source == raw_data.dst_name {
                return Some(ExternalCallType::external_library(
                    first_segment.to_string(),
                ));
            }
        }
        None
    }

    fn try_classify_from_naming_convention(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
    ) -> Option<ExternalCallType> {
        let name = raw_data.dst_name.as_str();
        let lower = name.to_ascii_lowercase();
        // Java standard library prefixes
        if lower.starts_with("java.") || lower.starts_with("javax.") || lower.starts_with("java::")
        {
            return Some(ExternalCallType::standard_library(
                self.extract_stdlib_name(&raw_data.dst_name, &parsed.language),
            ));
        }
        // Rust stdlib common crates
        if lower.starts_with("std::") || lower.starts_with("core::") || lower.starts_with("alloc::")
        {
            return Some(ExternalCallType::standard_library(
                self.extract_stdlib_name(&raw_data.dst_name, &parsed.language),
            ));
        }
        // Python stdlib heuristics
        if matches!(
            lower.split('.').next().unwrap_or(""),
            "os" | "sys" | "json" | "re" | "typing" | "collections" | "pathlib"
        ) {
            // Check via stdlib classifier for more accurate result
            let is_stdlib = with_stdlib_classifier(|c| {
                c.is_stdlib_by_type(
                    &raw_data.dst_name,
                    &raw_data.relation_type,
                    &parsed.language,
                )
            })
            .unwrap_or(false);
            if is_stdlib {
                return Some(ExternalCallType::standard_library(
                    self.extract_stdlib_name(&raw_data.dst_name, &parsed.language),
                ));
            }
        }
        // Go stdlib single-segment imports are often stdlib (fmt, net/http)
        if parsed.language == cce_types::language::Language::Go {
            let first = name
                .split('.')
                .next()
                .unwrap_or(name)
                .split('/')
                .next()
                .unwrap_or(name);
            if first.len() <= 6 && first.chars().all(|c| c.is_ascii_lowercase()) {
                // heuristic: short lowercase package likely stdlib, but let fallback to unknown
                return None;
            }
        }
        None
    }

    /// Classify an external call
    ///
    /// Determines the type of external call (standard library, external package, or unknown).
    ///
    /// # Arguments
    ///
    /// * `raw_data` - The raw relation data
    /// * `parsed` - The parsed file containing the relation
    ///
    /// # Returns
    ///
    /// The external call type if the call is external, `None` otherwise
    pub fn classify_external_call(
        &self,
        raw_data: &RawRelationData,
        parsed: &ParsedFile,
    ) -> Option<ExternalCallType> {
        let is_stdlib = with_stdlib_classifier(|c| {
            c.is_stdlib_by_type(
                &raw_data.dst_name,
                &raw_data.relation_type,
                &parsed.language,
            )
        })
        .unwrap_or(false);

        if is_stdlib {
            Some(ExternalCallType::standard_library(
                self.extract_stdlib_name(&raw_data.dst_name, &parsed.language),
            ))
        } else {
            self.classify_external_package(raw_data, parsed)
        }
    }
}
