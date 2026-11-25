use super::IndexBuilder;
use crate::index::build_view::BuildIndexSink;
use crate::index::core::RelationIndex;
use crate::symbol_table::ProjectSymbolTable;
use cce_plugin::{PluginCapability, PluginRegistry};
use cce_types::relation::CallContext;
use cce_types::{
    EntityId, ParsedFile, PluginRelation, PluginSymbol, RelationType, normalize_project_path,
};
use std::collections::HashMap;
use std::path::Path;

impl IndexBuilder {
    pub fn register_file_plugin_symbols(&self, file: &ParsedFile, symbols: &ProjectSymbolTable) {
        register_file_plugin_symbols(&self.index, self.plugin_registry.as_deref(), file, symbols);
    }

    pub fn inject_plugin_relations(&self, file: &ParsedFile, symbols: &ProjectSymbolTable) {
        inject_plugin_relations(&self.index, self.plugin_registry.as_deref(), file, symbols);
    }
}

/// Register plugin symbols (`RelationExtract`) into the project symbol table.
pub(crate) fn register_file_plugin_symbols<I: BuildIndexSink>(
    sink: &I,
    registry: Option<&PluginRegistry>,
    file: &ParsedFile,
    symbols: &ProjectSymbolTable,
) {
    let Some(registry) = registry else {
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
                let count = inject_plugin_symbols(sink, file, symbols, &plugin_symbols);
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
                    "extract_symbols failed (relation.plugin_symbols_enabled must be true), skipping plugin symbols"
                );
            }
        }
    }
    if any_injected {
        symbols.rebuild_resolution_cache();
    }
}

fn inject_plugin_symbols<I: BuildIndexSink>(
    sink: &I,
    file: &ParsedFile,
    symbols: &ProjectSymbolTable,
    plugin_symbols: &[PluginSymbol],
) -> usize {
    let fallback_module = cce_parser_core::determine_module_path(
        Path::new(&file.path),
        &symbols.root_path,
        file.language,
    )
    .unwrap_or_default();
    let mut count = 0;
    let mut queue: Vec<(&PluginSymbol, String)> = plugin_symbols
        .iter()
        .map(|sym| (sym, normalize_project_path(&file.path)))
        .collect();
    while let Some((sym, parent_qualifier)) = queue.pop() {
        if sym.name.is_empty() {
            tracing::trace!(
                file_path = %file.path,
                "plugin symbol with empty name skipped"
            );
            continue;
        }
        let entity_id = EntityId(RelationIndex::SYNTHETIC_MARK | sink.allocate_entity_id().0);
        let module_path = sym
            .module_path
            .clone()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| fallback_module.clone());
        let qualified_name = format!("{parent_qualifier}::{}", sym.name);
        symbols.insert_symbol(
            qualified_name.clone(),
            entity_id,
            file.path.clone(),
            module_path.clone(),
        );
        let effective_id = if sym.id.is_empty() {
            &sym.name
        } else {
            &sym.id
        };
        if effective_id != &sym.name {
            symbols.insert_symbol(
                format!("{parent_qualifier}::{effective_id}"),
                entity_id,
                file.path.clone(),
                module_path,
            );
        }
        count += 1;
        for child in &sym.children {
            queue.push((child, qualified_name.clone()));
        }
    }
    count
}

/// Inject plugin relations (`RelationExtract`) into the build sink.
pub(crate) fn inject_plugin_relations<I: BuildIndexSink>(
    sink: &I,
    registry: Option<&PluginRegistry>,
    file: &ParsedFile,
    symbols: &ProjectSymbolTable,
) {
    let Some(registry) = registry else {
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
                resolve_plugin_relations(sink, file, symbols, &relations, &plugin.metadata().id);
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    plugin = %plugin.metadata().id,
                    file_path = %file.path,
                    error = %e,
                    "extract_relations failed (relation.plugin_symbols_enabled must be true), skipping plugin relations"
                );
            }
        }
    }
}

fn resolve_plugin_ref<I: BuildIndexSink>(
    sink: &I,
    file: &ParsedFile,
    symbols: &ProjectSymbolTable,
    entity_ids_by_name: &HashMap<String, EntityId>,
    reference: &str,
) -> Option<EntityId> {
    let normalized_path = normalize_project_path(&file.path);
    if let Some(id) = entity_ids_by_name.get(reference) {
        return Some(*id);
    }
    if let Some(id) = entity_ids_by_name.get(&format!("{normalized_path}::{reference}")) {
        return Some(*id);
    }
    if reference.contains("::") {
        if let Some(id) = entity_ids_by_name.get(&normalize_project_path(reference)) {
            return Some(*id);
        }
        if let Some(id) = symbols
            .get_by_qualified_name(reference)
            .or_else(|| symbols.get_by_qualified_name(&normalize_project_path(reference)))
        {
            return Some(id);
        }
        let bare = reference.rsplit("::").next().unwrap_or(reference);
        if let Some(id) = entity_ids_by_name.get(bare) {
            return Some(*id);
        }
        if let Some(id) = symbols.get_by_simple_name(bare) {
            return Some(id);
        }
    } else {
        if let Some(id) = symbols.get_by_qualified_name(&format!("{normalized_path}::{reference}"))
        {
            return Some(id);
        }
        if let Some(id) = symbols.get_by_simple_name(reference) {
            return Some(id);
        }
    }
    sink.get_function_ids_by_name(reference)
        .into_iter()
        .min_by_key(|id| id.0)
}

fn resolve_plugin_relations<I: BuildIndexSink>(
    sink: &I,
    file: &ParsedFile,
    symbols: &ProjectSymbolTable,
    relations: &[PluginRelation],
    plugin_id: &str,
) -> usize {
    let normalized_path = normalize_project_path(&file.path);
    let file_remap = sink.entity_id_remap_for(&normalized_path);
    let remap_id = |id: EntityId| -> EntityId {
        file_remap
            .as_ref()
            .and_then(|m| m.get(&id))
            .copied()
            .unwrap_or(id)
    };

    let mut sorted_entities: Vec<_> = file.entities.iter().collect();
    sorted_entities.sort_by_key(|e| e.id.0);
    let mut entity_ids_by_name: HashMap<String, EntityId> = HashMap::new();
    for entity in sorted_entities {
        let global = remap_id(entity.id);
        entity_ids_by_name
            .entry(entity.name.clone())
            .or_insert(global);
        entity_ids_by_name
            .entry(format!("{normalized_path}::{}", entity.name))
            .or_insert(global);
    }
    let mut seen: std::collections::HashSet<(EntityId, Option<EntityId>, RelationType)> = sink
        .get_resolved_relations_by_file(&file.path)
        .into_iter()
        .flat_map(|(caller, rels)| {
            rels.into_iter()
                .map(move |r| (caller, r.callee_id, r.relation_type))
        })
        .collect();
    let mut injected = 0;
    for rel in relations {
        let Some(from_id) = resolve_plugin_ref(sink, file, symbols, &entity_ids_by_name, &rel.from)
        else {
            tracing::warn!(
                plugin = %plugin_id,
                file_path = %file.path,
                from = %rel.from,
                "plugin relation source not found, dropping (relation.plugin_symbols_enabled must be true and the source must name a file entity or plugin symbol)"
            );
            continue;
        };
        let Some(to_id) = resolve_plugin_ref(sink, file, symbols, &entity_ids_by_name, &rel.to)
        else {
            tracing::warn!(
                plugin = %plugin_id,
                file_path = %file.path,
                to = %rel.to,
                "plugin relation target not found, dropping (relation.plugin_symbols_enabled must be true and the target must name a file entity or plugin symbol)"
            );
            continue;
        };
        let relation_type = match rel.relation_type.as_str() {
            "call" | "calls" | "injects" | "injection" | "inject" => RelationType::DirectCall,
            "import" | "imports" => RelationType::ImportStandard,
            "extends" | "inherits" => RelationType::Inheritance,
            "implements" => RelationType::Implementation,
            "contains" => RelationType::Contains,
            "uses" | "reference" | "references" | "type_reference" => RelationType::TypeReference,
            unknown => {
                tracing::warn!(
                    plugin = %plugin_id,
                    file_path = %file.path,
                    relation_type = %unknown,
                    "unknown plugin relation type, defaulting to DirectCall"
                );
                RelationType::DirectCall
            }
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
            call_frequency: 1,
            cfg_condition: None,
        };
        sink.add_resolved_relation(resolved);
        injected += 1;
    }
    injected
}
