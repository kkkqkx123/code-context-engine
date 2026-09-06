use super::IndexBuilder;
use crate::index::{EntityIndexOps, FileLevelOps};
use crate::symbol_table::ProjectSymbolTable;
use cce_plugin::PluginCapability;
use cce_types::relation::CallContext;
use cce_types::{
    EntityId, ParsedFile, PluginRelation, PluginSymbol, RelationType, normalize_project_path,
};
use std::collections::HashMap;

impl IndexBuilder {
    /// Register plugin symbols (`RelationExtract`) into the project symbol
    /// table for a file.
    ///
    /// Runs after the built-in symbols are registered. Plugin symbols are
    /// registered into the project's global + simple-name indexes (via
    /// [`ProjectSymbolTable::insert_symbol`]), which the `resolve_enhanced`
    /// fallback and the relation resolver consume. This keeps the injection
    /// independent of the module-table mutation APIs while making plugin
    /// symbols resolvable as relation targets.
    pub fn register_file_plugin_symbols(&self, file: &ParsedFile, symbols: &ProjectSymbolTable) {
        let Some(registry) = self.plugin_registry.as_ref() else {
            return;
        };
        let language = file.language.to_string();
        let extractors = registry.get_plugins(
            PluginCapability::RelationExtract,
            Some(&file.path),
            Some(&language),
        );
        if extractors.is_empty() {
            return;
        }
        let mut any_injected = false;
        for plugin in extractors {
            match plugin.extract_symbols(&file.source, &file.path, &language) {
                Ok(Some(plugin_symbols)) if !plugin_symbols.is_empty() => {
                    let count = self.inject_plugin_symbols(file, symbols, &plugin_symbols);
                    if count > 0 {
                        any_injected = true;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.metadata().id,
                        file_path = %file.path,
                        error = %e,
                        "extract_symbols failed, skipping plugin symbols"
                    );
                }
            }
        }
        if any_injected {
            symbols.rebuild_resolution_cache();
        }
    }

    /// Inject plugin symbols into the project's global + simple-name indexes.
    ///
    /// Returns the number of symbols registered (including nested children).
    fn inject_plugin_symbols(
        &self,
        file: &ParsedFile,
        symbols: &ProjectSymbolTable,
        plugin_symbols: &[PluginSymbol],
    ) -> usize {
        let mut count = 0;
        let mut queue: Vec<&PluginSymbol> = plugin_symbols.iter().collect();
        while let Some(sym) = queue.pop() {
            // Generate a synthetic entity ID for plugin symbols.
            // Use a counter in the high bits to avoid collision with real entity IDs.
            let entity_id = EntityId((1u64 << 63) | (count as u64));
            let qualified_name = format!("{}::{}", file.path, sym.name);
            symbols.insert_symbol(
                qualified_name,
                entity_id,
                file.path.clone(),
                sym.module_path.clone().unwrap_or_default(),
            );
            count += 1;
            queue.extend(sym.children.iter());
        }
        count
    }

    /// Inject plugin relations (`RelationExtract`) into the index for a file.
    ///
    /// Runs alongside `resolve_file_relations`. Relations whose from/to cannot
    /// be resolved are dropped with a warning (they never abort the build).
    pub fn inject_plugin_relations(&self, file: &ParsedFile, symbols: &ProjectSymbolTable) {
        let Some(registry) = self.plugin_registry.as_ref() else {
            return;
        };
        let language = file.language.to_string();
        let extractors = registry.get_plugins(
            PluginCapability::RelationExtract,
            Some(&file.path),
            Some(&language),
        );
        if extractors.is_empty() {
            return;
        }
        for plugin in extractors {
            match plugin.extract_relations(&file.source, &file.path, &language) {
                Ok(Some(relations)) if !relations.is_empty() => {
                    self.resolve_plugin_relations(file, symbols, &relations);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.metadata().id,
                        file_path = %file.path,
                        error = %e,
                        "extract_relations failed, skipping plugin relations"
                    );
                }
            }
        }
    }

    /// Resolve plugin relations to entity ids and register them.
    ///
    /// Returns the number of relations injected.
    fn resolve_plugin_relations(
        &self,
        file: &ParsedFile,
        symbols: &ProjectSymbolTable,
        relations: &[PluginRelation],
    ) -> usize {
        // `file.entities` carry ParsedFile-local IDs; the index stores
        // globally remapped IDs. Plugin relations are injected in a separate
        // phase after `index_file_core`, so local IDs must be translated
        // through the per-file remap or the injected edges would dangle.
        let normalized_path = normalize_project_path(&file.path);
        let file_remap = self.index.entity_id_remap_for(&normalized_path);
        let remap_id = |id: EntityId| -> EntityId {
            file_remap
                .as_ref()
                .and_then(|m| m.get(&id))
                .copied()
                .unwrap_or(id)
        };

        let entity_ids_by_name: HashMap<&str, EntityId> = file
            .entities
            .iter()
            .map(|e| (e.name.as_str(), remap_id(e.id)))
            .collect();
        // Seed the dedup set with the file's already-resolved (built-in)
        // relations so plugin relations duplicating built-in edges (e.g. a
        // plugin that also parses imports) are skipped instead of injected
        // twice.
        let mut seen: std::collections::HashSet<(EntityId, Option<EntityId>, RelationType)> = self
            .index
            .get_resolved_relations_by_file(&file.path)
            .into_iter()
            .flat_map(|(caller, rels)| {
                rels.into_iter()
                    .map(move |r| (caller, r.callee_id, r.relation_type))
            })
            .collect();
        let mut injected = 0;
        for rel in relations {
            let Some(from_id) = entity_ids_by_name.get(rel.from.as_str()).copied() else {
                tracing::trace!(
                    file_path = %file.path,
                    from = %rel.from,
                    "plugin relation source not found, dropping"
                );
                continue;
            };
            let target_id = entity_ids_by_name
                .get(rel.to.as_str())
                .copied()
                .or_else(|| {
                    // Cross-file target: resolve through the entity index by
                    // name. Symbol-table IDs must never be cast into entity
                    // IDs ; a symbol without a real entity (e.g. a
                    // plugin symbol) is not an addressable relation target.
                    let registered = symbols.get_by_simple_name(&rel.to).or_else(|| {
                        symbols.get_by_qualified_name(&format!("{}::{}", file.path, rel.to))
                    });
                    registered?;
                    self.index
                        .get_function_ids_by_name(&rel.to)
                        .into_iter()
                        .next()
                });
            let Some(to_id) = target_id else {
                tracing::trace!(
                    file_path = %file.path,
                    to = %rel.to,
                    "plugin relation target not found, dropping"
                );
                continue;
            };
            let relation_type = match rel.relation_type.as_str() {
                "call" | "calls" | "injects" | "injection" | "inject" => {
                    cce_types::RelationType::DirectCall
                }
                "import" | "imports" => cce_types::RelationType::ImportStandard,
                "extends" | "inherits" => cce_types::RelationType::Inheritance,
                "implements" => cce_types::RelationType::Implementation,
                "contains" => cce_types::RelationType::Contains,
                "uses" | "reference" | "references" | "type_reference" => {
                    cce_types::RelationType::TypeReference
                }
                _ => cce_types::RelationType::DirectCall,
            };
            let key = (from_id, Some(to_id), relation_type);
            if !seen.insert(key) {
                tracing::trace!(
                    file_path = %file.path,
                    from = %rel.from,
                    to = %rel.to,
                    "duplicate plugin relation skipped"
                );
                continue;
            }
            let resolved = cce_types::ResolvedRelation {
                caller: from_id,
                callee_id: Some(to_id),
                callee_name: rel.to.clone(),
                relation_type,
                span: Default::default(),
                is_external: false,
                external_type: None,
                callee_symbol: None,
                stdlib_category: None,
                owner_type: None,
                call_context: CallContext::Direct,
                overload_signature: None,
            };
            self.index.add_resolved_relation(resolved);
            injected += 1;
        }
        injected
    }
}
