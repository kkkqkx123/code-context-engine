use std::collections::HashSet;

use smallvec::SmallVec;

use cce_types::{
    CanonicalExport, DependencyDiff, EntityKind, ExportDiff, ImportDiff, StableSymbolKey,
};

use crate::index::core::RelationIndex;
use crate::index::view::RelationIndexView;
use crate::types::{ExportInfo, ExportType};

pub(super) fn compute_import_diff<V: RelationIndexView>(
    new_index: &RelationIndex,
    old: &V,
    affected_files: Option<&HashSet<String>>,
) -> Vec<ImportDiff> {
    let mut import_diffs = Vec::new();
    let mut all_import_files: HashSet<String> = HashSet::new();
    old.for_each_import(|path, _| {
        all_import_files.insert(path.to_string());
    });
    for entry in new_index.file_records.read().iter() {
        all_import_files.insert(entry.0.clone());
    }
    let all_import_files: Vec<String> = all_import_files
        .into_iter()
        .filter(|path| affected_files.is_none_or(|files| files.contains(path)))
        .collect();

    for file_path in &all_import_files {
        let old_imports = old
            .imports_of(file_path)
            .map(|table| table.standardized_imports)
            .unwrap_or_default();
        let new_imports = new_index
            .file_records
            .read()
            .get(file_path)
            .map(|r| r.imports.standardized_imports.clone())
            .unwrap_or_default();

        let removed: Vec<_> = old_imports
            .iter()
            .filter(|i| !new_imports.contains(i))
            .cloned()
            .collect();
        let added: Vec<_> = new_imports
            .iter()
            .filter(|i| !old_imports.contains(i))
            .cloned()
            .collect();

        if !removed.is_empty() || !added.is_empty() {
            import_diffs.push(ImportDiff {
                file_path: file_path.clone(),
                removed_imports: removed,
                added_imports: added,
            });
        }
    }

    import_diffs
}

pub(super) fn compute_export_diff<V: RelationIndexView>(
    new_index: &RelationIndex,
    old: &V,
    affected_files: Option<&HashSet<String>>,
) -> Vec<ExportDiff> {
    let mut export_diffs = Vec::new();
    let mut all_export_files: HashSet<String> = HashSet::new();
    old.for_each_export(|path, _| {
        all_export_files.insert(path.to_string());
    });
    for entry in new_index.file_records.read().iter() {
        all_export_files.insert(entry.0.clone());
    }
    let all_export_files: Vec<String> = all_export_files
        .into_iter()
        .filter(|path| affected_files.is_none_or(|files| files.contains(path)))
        .collect();

    for file_path in &all_export_files {
        let old_exports = old.exports_of(file_path).unwrap_or_default();
        let new_exports: SmallVec<[ExportInfo; 2]> = new_index
            .file_records
            .read()
            .get(file_path)
            .map(|r| r.exports.clone())
            .unwrap_or_default();

        let old_symbols: HashSet<String> = old_exports
            .iter()
            .map(|e| e.function_name.clone())
            .collect();
        let new_symbols: HashSet<String> = new_exports
            .iter()
            .map(|e| e.function_name.clone())
            .collect();

        let removed: Vec<_> = old_exports
            .iter()
            .filter(|e| !new_symbols.contains(&e.function_name))
            .map(|e| CanonicalExport {
                symbol: old.symbol_key_of(e.function_id).unwrap_or_else(|| {
                    StableSymbolKey::new(
                        file_path,
                        &e.function_name,
                        EntityKind::Function,
                        &e.function_name,
                    )
                }),
                export_type: format!("{:?}", e.export_type).to_lowercase(),
            })
            .collect();

        let added: Vec<_> = new_exports
            .iter()
            .filter(|e| !old_symbols.contains(&e.function_name))
            .map(|e| CanonicalExport {
                symbol: new_index
                    .entity_to_symbol_key
                    .read()
                    .get(&e.function_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        StableSymbolKey::new(
                            file_path,
                            &e.function_name,
                            EntityKind::Function,
                            &e.function_name,
                        )
                    }),
                export_type: format!("{:?}", e.export_type).to_lowercase(),
            })
            .collect();

        if !removed.is_empty() || !added.is_empty() {
            export_diffs.push(ExportDiff {
                file_path: file_path.clone(),
                removed_exports: removed,
                added_exports: added,
            });
        }
    }

    export_diffs
}

pub(super) fn compute_dependency_diff<V: RelationIndexView>(
    new_index: &RelationIndex,
    old: &V,
    affected_files: Option<&HashSet<String>>,
) -> Vec<DependencyDiff> {
    let mut dependency_diffs = Vec::new();
    let mut all_dep_files: HashSet<String> = HashSet::new();
    all_dep_files.extend(old.dependency_files());
    all_dep_files.extend(new_index.dependency_graph.get_all_files());
    let all_dep_files: Vec<String> = all_dep_files
        .into_iter()
        .filter(|path| affected_files.is_none_or(|files| files.contains(path)))
        .collect();

    for source_file in &all_dep_files {
        let old_deps = old.dependencies_of(source_file);
        let new_deps = new_index.dependency_graph.get_dependencies(source_file);

        let removed: Vec<_> = old_deps
            .iter()
            .filter(|d| !new_deps.contains(d))
            .cloned()
            .collect();
        let added: Vec<_> = new_deps
            .iter()
            .filter(|d| !old_deps.contains(d))
            .cloned()
            .collect();

        if !removed.is_empty() || !added.is_empty() {
            dependency_diffs.push(DependencyDiff {
                source_file: source_file.clone(),
                removed_dependencies: removed,
                added_dependencies: added,
            });
        }
    }

    dependency_diffs
}

pub(super) fn apply_import_diffs(index: &RelationIndex, diffs: &[ImportDiff]) {
    for diff in diffs {
        if let Some(record) = index.file_records.write().get_mut(&diff.file_path) {
            record
                .imports
                .standardized_imports
                .retain(|i| !diff.removed_imports.contains(i));
            for import in &diff.added_imports {
                if !record.imports.standardized_imports.contains(import) {
                    record.imports.standardized_imports.push(import.clone());
                }
            }
        }
    }
}

pub(super) fn apply_export_diffs(index: &RelationIndex, diffs: &[ExportDiff]) {
    for diff in diffs {
        if let Some(record) = index.file_records.write().get_mut(&diff.file_path) {
            let removed: HashSet<&str> = diff
                .removed_exports
                .iter()
                .map(|e| e.symbol.scoped_name.as_str())
                .collect();
            record
                .exports
                .retain(|e| !removed.contains(e.function_name.as_str()));

            let existing: HashSet<String> = record
                .exports
                .iter()
                .map(|e| e.function_name.clone())
                .collect();
            for export in &diff.added_exports {
                if existing.contains(export.symbol.scoped_name.as_str()) {
                    continue;
                }
                let Some(entity_id) = index.get_entity_id_by_symbol_key(&export.symbol) else {
                    tracing::warn!(
                        file = %diff.file_path,
                        symbol = %export.symbol.scoped_name,
                        "delta export references an unresolvable symbol key; skipping export"
                    );
                    index
                        .diagnostics
                        .delta_export_unresolved_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    continue;
                };
                let export_type = match export.export_type.as_str() {
                    "default" => ExportType::Default,
                    "wildcard" => ExportType::Wildcard,
                    _ => ExportType::Named,
                };
                record.exports.push(ExportInfo {
                    function_id: entity_id,
                    function_name: export.symbol.scoped_name.clone(),
                    export_type,
                });
            }
        }
    }
}

pub(super) fn apply_dependency_diffs(index: &RelationIndex, diffs: &[DependencyDiff]) {
    for diff in diffs {
        for dep in &diff.removed_dependencies {
            index
                .dependency_graph
                .remove_dependency(&diff.source_file, dep);
        }
        for dep in &diff.added_dependencies {
            index
                .dependency_graph
                .add_dependency(&diff.source_file, dep);
        }
    }
}
