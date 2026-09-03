use std::collections::{HashMap, HashSet};

use crate::index::core::SymbolKey;
use crate::index::delta::relation_identity;
use crate::index::view::RelationIndexView;
use crate::types::{ExportInfo, ExportType};
use cce_types::{Entity, EntityId, FileInfo, ImportTable, ResolvedRelation};

impl crate::index::snapshot_index::LayeredSnapshotIndex {
    fn is_removed_entity(&self, id: EntityId) -> bool {
        self.deltas.iter().any(|d| d.removed_entities.contains(&id))
    }

    fn is_removed_file(&self, path: &str) -> bool {
        self.deltas
            .iter()
            .any(|d| d.removed_files.iter().any(|f| f == path))
    }

    /// Resolve an entity ID for a stable symbol key against the merged state:
    /// added entities first (their keys were registered last), then the base.
    fn merged_entity_id_by_symbol_key(&self, key: &SymbolKey) -> Option<EntityId> {
        for d in &self.deltas {
            for added in &d.added_entities {
                if &added.symbol_key == key {
                    return Some(added.entity.id);
                }
            }
        }
        self.base.symbol_key_to_entity.read().get(key).copied()
    }
}

impl RelationIndexView for crate::index::snapshot_index::LayeredSnapshotIndex {
    // ---- File layer ----
    fn file_contains(&self, path: &str) -> bool {
        let mut contains = self.base.file_contains(path);
        for d in &self.deltas {
            if d.removed_files.iter().any(|f| f == path) {
                contains = false;
            }
            if d.added_files.iter().any(|f| f.path == path) {
                contains = true;
            }
        }
        contains
    }

    fn for_each_file<F: FnMut(&str, &FileInfo)>(&self, mut f: F) {
        let mut merged: HashMap<String, FileInfo> = HashMap::new();
        self.base.for_each_file(|path, info| {
            merged.insert(path.to_string(), info.clone());
        });
        for d in &self.deltas {
            for removed in &d.removed_files {
                merged.remove(removed);
            }
            for added in &d.added_files {
                merged.insert(added.path.clone(), added.clone());
            }
        }
        for (path, info) in &merged {
            f(path, info);
        }
    }

    fn file_relations_of(&self, path: &str) -> Vec<ResolvedRelation> {
        let mut rels = if self.is_removed_file(path) {
            Vec::new()
        } else {
            self.base.file_relations_of(path)
        };
        for d in &self.deltas {
            if d.removed_files.iter().any(|f| f == path) {
                rels.clear();
            }
            for diff in &d.file_relation_diffs {
                if diff.file_path == path {
                    let removed: HashSet<_> = diff
                        .removed_relations
                        .iter()
                        .map(relation_identity)
                        .collect();
                    rels.retain(|r| !removed.contains(&relation_identity(r)));
                }
            }
            for diff in &d.file_relation_diffs {
                if diff.file_path == path {
                    for added in &diff.added_relations {
                        let identity = relation_identity(added);
                        if !rels.iter().any(|r| relation_identity(r) == identity) {
                            rels.push(added.clone());
                        }
                    }
                }
            }
        }
        rels
    }

    fn for_each_file_relation<F: FnMut(&str, &[ResolvedRelation])>(&self, mut f: F) {
        let mut merged: HashMap<String, Vec<ResolvedRelation>> = HashMap::new();
        self.base.for_each_file_relation(|path, rels| {
            merged.insert(path.to_string(), rels.to_vec());
        });
        for d in &self.deltas {
            for removed in &d.removed_files {
                merged.remove(removed);
            }
            for diff in &d.file_relation_diffs {
                let rels = merged.entry(diff.file_path.clone()).or_default();
                let removed: HashSet<_> = diff
                    .removed_relations
                    .iter()
                    .map(relation_identity)
                    .collect();
                rels.retain(|r| !removed.contains(&relation_identity(r)));
                for added in &diff.added_relations {
                    let identity = relation_identity(added);
                    if !rels.iter().any(|r| relation_identity(r) == identity) {
                        rels.push(added.clone());
                    }
                }
            }
        }
        for (path, rels) in &merged {
            if !rels.is_empty() {
                f(path, rels);
            }
        }
    }

    // ---- Entity layer ----
    fn function_contains(&self, id: EntityId) -> bool {
        let mut contains = self.base.function_contains(id);
        for d in &self.deltas {
            if d.removed_entities.contains(&id) {
                contains = false;
            }
            if d.added_entities.iter().any(|a| a.entity.id == id) {
                contains = true;
            }
        }
        contains
    }

    fn for_each_function<F: FnMut(EntityId, &Entity)>(&self, mut f: F) {
        let mut merged: HashMap<EntityId, Entity> = HashMap::new();
        self.base.for_each_function(|id, entity| {
            merged.insert(id, entity.clone());
        });
        for d in &self.deltas {
            for removed in &d.removed_entities {
                merged.remove(removed);
            }
            for added in &d.added_entities {
                merged.insert(added.entity.id, added.entity.clone());
            }
        }
        for (id, entity) in &merged {
            f(*id, entity);
        }
    }

    fn entity_file_of(&self, id: EntityId) -> Option<String> {
        let mut file = self.base.entity_file_of(id);
        for d in &self.deltas {
            if d.removed_entities.contains(&id) {
                file = None;
            }
            if let Some(added) = d.added_entities.iter().find(|a| a.entity.id == id) {
                file = Some(added.file_path.clone());
            }
        }
        file
    }

    // ---- Relation layer ----
    fn relations_of(&self, caller: EntityId) -> Option<Vec<ResolvedRelation>> {
        let mut rels = if self.is_removed_entity(caller) {
            Vec::new()
        } else {
            self.base.relations_of(caller).unwrap_or_default()
        };
        for d in &self.deltas {
            for removed in &d.removed_relations {
                if removed.caller == caller {
                    let identity = relation_identity(removed);
                    rels.retain(|c| relation_identity(c) != identity);
                }
            }
            for added in &d.added_relations {
                if added.caller == caller {
                    rels.push(added.clone());
                }
            }
        }
        if rels.is_empty() { None } else { Some(rels) }
    }

    fn for_each_resolved_relation<F: FnMut(EntityId, &[ResolvedRelation])>(&self, mut f: F) {
        let mut merged: HashMap<EntityId, Vec<ResolvedRelation>> = HashMap::new();
        self.base.for_each_resolved_relation(|caller, rels| {
            merged.insert(caller, rels.to_vec());
        });
        for d in &self.deltas {
            for removed in &d.removed_entities {
                merged.remove(removed);
            }
            for removed in &d.removed_relations {
                if let Some(rels) = merged.get_mut(&removed.caller) {
                    let identity = relation_identity(removed);
                    rels.retain(|c| relation_identity(c) != identity);
                }
            }
            for added in &d.added_relations {
                merged.entry(added.caller).or_default().push(added.clone());
            }
        }
        for (caller, rels) in &merged {
            if !rels.is_empty() {
                f(*caller, rels);
            }
        }
    }

    fn callers_of(&self, callee: EntityId) -> Vec<EntityId> {
        if self.is_removed_entity(callee) {
            return Vec::new();
        }
        let removed_entities: HashSet<EntityId> = self
            .deltas
            .iter()
            .flat_map(|d| d.removed_entities.iter().copied())
            .collect();
        let mut callers = self.base.callers_of(callee);
        callers.retain(|c| !removed_entities.contains(c));
        for d in &self.deltas {
            for removed in &d.removed_relations {
                if removed.callee_id == Some(callee) {
                    callers.retain(|c| *c != removed.caller);
                }
            }
            for added in &d.added_relations {
                if added.callee_id == Some(callee) && !callers.contains(&added.caller) {
                    callers.push(added.caller);
                }
            }
        }
        callers
    }

    // ---- Import / export layer ----
    fn imports_of(&self, path: &str) -> Option<ImportTable> {
        if self.is_removed_file(path) {
            return None;
        }
        let mut table = self.base.imports_of(path)?;
        for d in &self.deltas {
            for diff in &d.import_diffs {
                if diff.file_path == path {
                    table
                        .standardized_imports
                        .retain(|i| !diff.removed_imports.contains(i));
                    for import in &diff.added_imports {
                        if !table.standardized_imports.contains(import) {
                            table.standardized_imports.push(import.clone());
                        }
                    }
                }
            }
        }
        Some(table)
    }

    fn for_each_import<F: FnMut(&str, &ImportTable)>(&self, mut f: F) {
        let mut merged: HashMap<String, ImportTable> = HashMap::new();
        self.base.for_each_import(|path, table| {
            merged.insert(path.to_string(), table.clone());
        });
        for d in &self.deltas {
            for removed in &d.removed_files {
                merged.remove(removed);
            }
            for diff in &d.import_diffs {
                if let Some(table) = merged.get_mut(&diff.file_path) {
                    table
                        .standardized_imports
                        .retain(|i| !diff.removed_imports.contains(i));
                    for import in &diff.added_imports {
                        if !table.standardized_imports.contains(import) {
                            table.standardized_imports.push(import.clone());
                        }
                    }
                }
            }
        }
        for (path, table) in &merged {
            f(path, table);
        }
    }

    fn exports_of(&self, path: &str) -> Option<Vec<ExportInfo>> {
        if self.is_removed_file(path) {
            return None;
        }
        let mut exports = self.base.exports_of(path)?;
        for d in &self.deltas {
            for diff in &d.export_diffs {
                if diff.file_path == path {
                    let removed: HashSet<&str> = diff
                        .removed_exports
                        .iter()
                        .map(|e| e.symbol.scoped_name.as_str())
                        .collect();
                    exports.retain(|e| !removed.contains(e.function_name.as_str()));
                    let existing: HashSet<String> =
                        exports.iter().map(|e| e.function_name.clone()).collect();
                    for export in &diff.added_exports {
                        if existing.contains(export.symbol.scoped_name.as_str()) {
                            continue;
                        }
                        // Skip unresolvable exports rather than falling back to EntityId(0),
                        // which could corrupt the call graph by aliasing a real entity.
                        let Some(entity_id) = self.merged_entity_id_by_symbol_key(&export.symbol)
                        else {
                            tracing::debug!(
                                scoped_name = %export.symbol.scoped_name,
                                "export symbol key unresolvable in layered snapshot, skipping"
                            );
                            continue;
                        };
                        let export_type = match export.export_type.as_str() {
                            "default" => ExportType::Default,
                            "wildcard" => ExportType::Wildcard,
                            _ => ExportType::Named,
                        };
                        exports.push(ExportInfo {
                            function_id: entity_id,
                            function_name: export.symbol.scoped_name.clone(),
                            export_type,
                        });
                    }
                }
            }
        }
        Some(exports)
    }

    fn for_each_export<F: FnMut(&str, &[ExportInfo])>(&self, mut f: F) {
        let mut merged: HashMap<String, Vec<ExportInfo>> = HashMap::new();
        self.base.for_each_export(|path, exports| {
            merged.insert(path.to_string(), exports.to_vec());
        });
        for d in &self.deltas {
            for removed in &d.removed_files {
                merged.remove(removed);
            }
            for diff in &d.export_diffs {
                if let Some(exports) = merged.get_mut(&diff.file_path) {
                    let removed: HashSet<&str> = diff
                        .removed_exports
                        .iter()
                        .map(|e| e.symbol.scoped_name.as_str())
                        .collect();
                    exports.retain(|e| !removed.contains(e.function_name.as_str()));
                    let existing: HashSet<String> =
                        exports.iter().map(|e| e.function_name.clone()).collect();
                    for export in &diff.added_exports {
                        if existing.contains(export.symbol.scoped_name.as_str()) {
                            continue;
                        }
                        // Skip unresolvable exports rather than falling back to EntityId(0),
                        // which could corrupt the call graph by aliasing a real entity.
                        let Some(entity_id) = self.merged_entity_id_by_symbol_key(&export.symbol)
                        else {
                            tracing::debug!(
                                scoped_name = %export.symbol.scoped_name,
                                "export symbol key unresolvable in layered snapshot, skipping"
                            );
                            continue;
                        };
                        let export_type = match export.export_type.as_str() {
                            "default" => ExportType::Default,
                            "wildcard" => ExportType::Wildcard,
                            _ => ExportType::Named,
                        };
                        exports.push(ExportInfo {
                            function_id: entity_id,
                            function_name: export.symbol.scoped_name.clone(),
                            export_type,
                        });
                    }
                }
            }
        }
        for (path, exports) in &merged {
            f(path, exports);
        }
    }

    fn symbol_key_of(&self, id: EntityId) -> Option<SymbolKey> {
        let mut key = self.base.symbol_key_of(id);
        for d in &self.deltas {
            if d.removed_entities.contains(&id) {
                key = None;
            }
            if let Some(added) = d.added_entities.iter().find(|a| a.entity.id == id) {
                key = Some(added.symbol_key.clone());
            }
        }
        key
    }

    // ---- Dependency graph layer ----
    fn dependency_files(&self) -> Vec<String> {
        let mut files: HashSet<String> = self.base.dependency_files().into_iter().collect();
        for d in &self.deltas {
            for removed in &d.removed_files {
                files.remove(removed);
            }
            for diff in &d.dependency_diffs {
                if !diff.added_dependencies.is_empty() {
                    files.insert(diff.source_file.clone());
                    files.extend(diff.added_dependencies.iter().cloned());
                }
            }
        }
        files.into_iter().collect()
    }

    fn dependencies_of(&self, source: &str) -> Vec<String> {
        let mut deps = self.base.dependencies_of(source);
        for d in &self.deltas {
            // `remove_file` drops this source's forward entry entirely
            // (apply_delta step 1, which runs before step 9 diffs).
            if d.removed_files.iter().any(|f| f == source) {
                deps.clear();
            }
            // `remove_file` also drops edges from this source to any removed
            // file (a removed target can no longer be depended on).
            if !d.removed_files.is_empty() {
                deps.retain(|x| !d.removed_files.iter().any(|f| f == x));
            }
            for diff in &d.dependency_diffs {
                if diff.source_file == source {
                    deps.retain(|x| !diff.removed_dependencies.contains(x));
                    for added in &diff.added_dependencies {
                        if !deps.contains(added) {
                            deps.push(added.clone());
                        }
                    }
                }
            }
        }
        deps
    }

    fn dependents_of(&self, file: &str) -> Vec<String> {
        if self.is_removed_file(file) {
            return Vec::new();
        }
        let mut dependents = self.base.dependents_of(file);
        for d in &self.deltas {
            // A removed file can no longer be a dependent of `file`.
            if !d.removed_files.is_empty() {
                dependents.retain(|x| !d.removed_files.iter().any(|f| f == x));
            }
            for diff in &d.dependency_diffs {
                if diff.removed_dependencies.iter().any(|x| x == file) {
                    dependents.retain(|x| x != &diff.source_file);
                }
                if diff.added_dependencies.iter().any(|x| x == file)
                    && !dependents.contains(&diff.source_file)
                {
                    dependents.push(diff.source_file.clone());
                }
            }
        }
        dependents
    }

    fn collect_transitive_dependents(&self, file: &str, max_depth: usize) -> Vec<String> {
        let mut dependents = HashSet::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: std::collections::VecDeque<(String, usize)> =
            std::collections::VecDeque::new();
        for dep in self.dependents_of(file) {
            if !visited.contains(&dep) {
                visited.insert(dep.clone());
                queue.push_back((dep, 1));
            }
        }
        while let Some((current, depth)) = queue.pop_front() {
            if max_depth > 0 && depth > max_depth {
                continue;
            }
            dependents.insert(current.clone());
            for dep in self.dependents_of(&current) {
                if !visited.contains(&dep) {
                    visited.insert(dep.clone());
                    queue.push_back((dep, depth + 1));
                }
            }
        }
        dependents.into_iter().collect()
    }

    fn collect_transitive_dependencies(&self, file: &str, max_depth: usize) -> Vec<String> {
        let mut dependencies = HashSet::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: std::collections::VecDeque<(String, usize)> =
            std::collections::VecDeque::new();
        for dep in self.dependencies_of(file) {
            if !visited.contains(&dep) {
                visited.insert(dep.clone());
                queue.push_back((dep, 1));
            }
        }
        while let Some((current, depth)) = queue.pop_front() {
            if max_depth > 0 && depth > max_depth {
                continue;
            }
            dependencies.insert(current.clone());
            for dep in self.dependencies_of(&current) {
                if !visited.contains(&dep) {
                    visited.insert(dep.clone());
                    queue.push_back((dep, depth + 1));
                }
            }
        }
        dependencies.into_iter().collect()
    }

    // ---- Symbol context ----
    fn entities_by_file(&self) -> HashMap<String, Vec<Entity>> {
        let mut merged = self.base.entities_by_file();
        for d in &self.deltas {
            if !d.removed_entities.is_empty() {
                let removed: HashSet<EntityId> = d.removed_entities.iter().copied().collect();
                for entities in merged.values_mut() {
                    entities.retain(|e| !removed.contains(&e.id));
                }
            }
            for added in &d.added_entities {
                merged
                    .entry(added.file_path.clone())
                    .or_default()
                    .push(added.entity.clone());
            }
            // A file whose entities were all removed disappears from the
            // grouping, mirroring the materialized `entity_file_index`.
            merged.retain(|_, entities| !entities.is_empty());
        }
        merged
    }

    fn entities_of_file(&self, path: &str) -> Vec<Entity> {
        // Same merge semantics as `entities_by_file`, scoped to one file:
        // the base contribution disappears entirely once any delta removed
        // the file, then each delta filters removed entities and appends its
        // additions for this path.
        let mut entities = if self.is_removed_file(path) {
            Vec::new()
        } else {
            self.base.entities_of_file(path)
        };
        for d in &self.deltas {
            if d.removed_files.iter().any(|f| f == path) {
                entities.clear();
            }
            if !d.removed_entities.is_empty() {
                let removed: HashSet<EntityId> = d.removed_entities.iter().copied().collect();
                entities.retain(|e| !removed.contains(&e.id));
            }
            for added in &d.added_entities {
                if added.file_path == path && !entities.iter().any(|e| e.id == added.entity.id) {
                    entities.push(added.entity.clone());
                }
            }
        }
        entities
    }

    fn file_callers_of(&self, callee: EntityId) -> Vec<String> {
        if self.is_removed_entity(callee) {
            return Vec::new();
        }
        let mut callers = self.base.file_callers_of(callee);
        for d in &self.deltas {
            for removed_file in &d.removed_files {
                callers.retain(|path| path != removed_file);
            }
            // Only files touched by a diff need re-derivation; their merged
            // edge set decides whether they still reference `callee`.
            for diff in &d.file_relation_diffs {
                let touches_callee = diff
                    .removed_relations
                    .iter()
                    .chain(diff.added_relations.iter())
                    .any(|rel| rel.callee_id == Some(callee));
                if !touches_callee {
                    continue;
                }
                let still_calls = self
                    .file_relations_of(&diff.file_path)
                    .iter()
                    .any(|rel| rel.callee_id == Some(callee));
                callers.retain(|path| path != &diff.file_path);
                if still_calls {
                    callers.push(diff.file_path.clone());
                }
            }
        }
        callers
    }

    fn stable_symbol_keys(&self) -> Vec<SymbolKey> {
        let mut keys: HashMap<EntityId, SymbolKey> = HashMap::new();
        self.base.for_each_function(|id, _| {
            if let Some(key) = self.base.symbol_key_of(id) {
                keys.insert(id, key);
            }
        });
        for d in &self.deltas {
            for removed in &d.removed_entities {
                keys.remove(removed);
            }
            for added in &d.added_entities {
                keys.insert(added.entity.id, added.symbol_key.clone());
            }
        }
        keys.values().cloned().collect()
    }

    fn stable_symbol_keys_in_files(&self, files: &HashSet<String>) -> Vec<SymbolKey> {
        let mut keys: HashMap<EntityId, SymbolKey> = HashMap::new();
        let fsk_guard = self.base.file_symbol_keys.read();
        let ske_guard = self.base.symbol_key_to_entity.read();
        for file in files {
            if let Some(vec) = fsk_guard.get(file.as_str()) {
                for key in vec.iter() {
                    if let Some(id) = ske_guard.get(key) {
                        keys.insert(*id, key.clone());
                    }
                }
            }
        }
        drop(fsk_guard);
        drop(ske_guard);
        for d in &self.deltas {
            for removed in &d.removed_entities {
                keys.remove(removed);
            }
            for added in &d.added_entities {
                if files.contains(&added.file_path) {
                    keys.insert(added.entity.id, added.symbol_key.clone());
                }
            }
        }
        keys.values().cloned().collect()
    }

    fn max_entity_id(&self) -> u64 {
        let mut max = self.base.max_entity_id();
        for d in &self.deltas {
            for added in &d.added_entities {
                max = max.max(added.entity.id.0);
            }
        }
        max
    }

    fn fingerprint_in_files(&self, files: &HashSet<String>) -> String {
        // Verification-only: materialize base + deltas into a concrete index
        // (identical to `apply_delta` semantics, which registers the added
        // symbol keys and file memberships) and delegate to the shared
        // map-based implementation so the layered fingerprint is
        // byte-identical to the materialized index.
        self.materialize_merged_index().fingerprint_in_files(files)
    }
}
