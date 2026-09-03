//! Lua mapping utilities for converting Rust types to Lua tables
//!
//! This module provides helper functions to convert EntityGroup and related types
//! into Lua-compatible table structures for plugin interaction.

use mlua::{Lua, Table, Value};

use cce_types::entity::{EntityId, GroupedEntity};
use cce_types::grouper::{EntityGroup, GroupType};
use cce_types::plugin::{
    GroupPluginContext, PluginExport, PluginImport, PluginRelation, PluginSymbol, ResultFilterEntry,
};
use cce_types::{
    ChunkedResult, ConversionResult, Language, PluginDocument, PluginEntity, Position, Span,
};

use cce_types::ast_to_nl::{
    ChunkContentType, ChunkMetadata, ChunkPath, DocumentSpecificMetadata, GroupConversions,
    RerankResult, RerankedCandidate, SourceSpanKind,
};

/// Convert an EntityGroup to a Lua table
///
/// This function creates a Lua table representation of the EntityGroup that can be
/// passed to Lua plugins. It includes all relevant fields including metadata.
pub fn entity_group_to_lua_table(lua: &Lua, group: &EntityGroup) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;

    // Basic group info
    table.set("group_id", group.group_id.as_str())?;
    table.set("group_type", format!("{:?}", group.group_type))?;
    table.set("name", group.name.as_str())?;
    table.set("kind", format!("{:?}", group.kind))?;
    table.set("language", format!("{:?}", group.language))?;

    // Header entity
    if let Some(ref header) = group.header {
        let header_table = grouped_entity_to_lua_table(lua, header)?;
        table.set("header", header_table)?;
    } else {
        table.set("header", mlua::Nil)?;
    }

    // Member entities
    let members_table = lua.create_table()?;
    for (idx, member) in group.members.iter().enumerate() {
        let member_table = grouped_entity_to_lua_table(lua, member)?;
        members_table.set(idx + 1, member_table)?;
    }
    table.set("members", members_table)?;

    // Pattern info
    table.set("pattern_info", format!("{:?}", group.pattern_info))?;

    // Metadata - entity-specific information
    let metadata_table = lua.create_table()?;
    for (key, value) in &group.metadata {
        metadata_table.set(key.as_str(), value.as_str())?;
    }
    table.set("metadata", metadata_table)?;

    // Nesting info
    table.set("nesting_level", group.nesting_level)?;
    table.set("has_significant_nested", group.has_significant_nested)?;
    if let Some(ref parent_id) = group.parent_group_id {
        table.set("parent_group_id", parent_id.as_str())?;
    } else {
        table.set("parent_group_id", mlua::Nil)?;
    }

    // Header reference
    if let Some(ref header_id) = group.header_id {
        let id_table = lua.create_table()?;
        id_table.set("id", header_id.0)?;
        table.set("header_id", id_table)?;
    } else {
        table.set("header_id", mlua::Nil)?;
    }

    // Member reference IDs
    let member_ids_table = lua.create_table()?;
    for (idx, member_id) in group.member_ids.iter().enumerate() {
        member_ids_table.set(idx + 1, member_id.0)?;
    }
    table.set("member_ids", member_ids_table)?;

    // Member roles
    let member_roles_table = lua.create_table()?;
    for (idx, (entity_id, role)) in group.member_roles.iter().enumerate() {
        let role_table = lua.create_table()?;
        role_table.set("entity_id", entity_id.0)?;
        role_table.set("role", role.to_string())?;
        member_roles_table.set(idx + 1, role_table)?;
    }
    table.set("member_roles", member_roles_table)?;

    // Source span
    let span_table = lua.create_table()?;
    span_table.set("start_byte", group.span.start_byte)?;
    span_table.set("end_byte", group.span.end_byte)?;
    let start_pos = lua.create_table()?;
    start_pos.set("row", group.span.start_position.row)?;
    start_pos.set("column", group.span.start_position.column)?;
    span_table.set("start_position", start_pos)?;
    let end_pos = lua.create_table()?;
    end_pos.set("row", group.span.end_position.row)?;
    end_pos.set("column", group.span.end_position.column)?;
    span_table.set("end_position", end_pos)?;
    table.set("span", span_table)?;

    // Nested groups (recursive conversion)
    let nested_table = lua.create_table()?;
    for (idx, nested) in group.nested_groups.iter().enumerate() {
        let nested_group_table = entity_group_to_lua_table(lua, nested)?;
        nested_table.set(idx + 1, nested_group_table)?;
    }
    table.set("nested_groups", nested_table)?;

    Ok(table)
}

/// Convert a GroupedEntity to a Lua table
///
/// Creates a Lua table representation of a single entity with all its properties
/// including entity-specific metadata.
pub fn grouped_entity_to_lua_table(
    lua: &Lua,
    entity: &GroupedEntity,
) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;

    // Basic entity info
    table.set("id", entity.id.0)?;
    table.set("name", entity.name.as_str())?;
    table.set("kind", format!("{:?}", entity.kind))?;
    table.set("signature", entity.signature.as_str())?;

    // Parameters
    let params_table = lua.create_table()?;
    for (idx, (name, ty)) in entity.parameters.iter().enumerate() {
        let param_table = lua.create_table()?;
        param_table.set("name", name.as_str())?;
        if let Some(type_str) = ty {
            param_table.set("type", type_str.as_str())?;
        } else {
            param_table.set("type", mlua::Nil)?;
        }
        params_table.set(idx + 1, param_table)?;
    }
    table.set("parameters", params_table)?;

    // Return type
    if let Some(ref ret_type) = entity.return_type {
        table.set("return_type", ret_type.as_str())?;
    } else {
        table.set("return_type", mlua::Nil)?;
    }

    // Doc comment
    if let Some(ref doc) = entity.doc_comment {
        table.set("doc_comment", doc.as_str())?;
    } else {
        table.set("doc_comment", mlua::Nil)?;
    }

    // Stdlib info
    table.set("is_stdlib", entity.is_stdlib)?;
    if let Some(ref category) = entity.stdlib_category {
        table.set("stdlib_category", format!("{:?}", category))?;
    } else {
        table.set("stdlib_category", mlua::Nil)?;
    }

    // Metadata - important for NL generation
    let metadata_table = lua.create_table()?;
    for (key, value) in &entity.metadata {
        metadata_table.set(key.as_str(), value.as_str())?;
    }
    table.set("metadata", metadata_table)?;

    Ok(table)
}

// ── Plugin extension contract mappings ────────────────────────────────

/// Convert a [`ConversionResult`] to a Lua table (subset of fields).
pub fn conversion_result_to_lua_table(
    lua: &Lua,
    result: &ConversionResult,
) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;
    table.set("entity_id", result.entity_id.0)?;
    table.set("kind", format!("{:?}", result.kind))?;
    table.set("name", result.name.as_str())?;
    table.set("file_path", result.file_path.as_str())?;
    table.set("bm25_text", result.bm25_text.as_deref().unwrap_or(""))?;
    table.set(
        "embedding_text",
        result.embedding_text.as_deref().unwrap_or(""),
    )?;
    let keywords = lua.create_table()?;
    for (idx, kw) in result.keywords.iter().enumerate() {
        keywords.set(idx + 1, kw.as_str())?;
    }
    table.set("keywords", keywords)?;
    Ok(table)
}

/// Convert a slice of [`GroupConversions`] to a 1-indexed Lua array.
pub fn group_conversions_to_lua_table(
    lua: &Lua,
    conversions: &[GroupConversions],
) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;
    for (idx, conv) in conversions.iter().enumerate() {
        let item = lua.create_table()?;
        item.set("group", entity_group_to_lua_table(lua, &conv.group)?)?;
        if let Some(ref header) = conv.header_conversion {
            item.set(
                "header_conversion",
                conversion_result_to_lua_table(lua, header)?,
            )?;
        } else {
            item.set("header_conversion", mlua::Nil)?;
        }
        let members = lua.create_table()?;
        for (midx, m) in conv.member_conversions.iter().enumerate() {
            members.set(midx + 1, conversion_result_to_lua_table(lua, m)?)?;
        }
        item.set("member_conversions", members)?;
        table.set(idx + 1, item)?;
    }
    Ok(table)
}

/// Convert a [`GroupPluginContext`] to a Lua table.
pub fn group_plugin_context_to_lua_table(
    lua: &Lua,
    context: &GroupPluginContext,
) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;
    table.set("file_path", context.file_path.as_str())?;
    table.set("language", context.language.as_str())?;
    table.set("source", context.source.as_str())?;
    let entities = lua.create_table()?;
    for (idx, entity) in context.entities.iter().enumerate() {
        entities.set(idx + 1, plugin_entity_to_lua_table(lua, entity)?)?;
    }
    table.set("entities", entities)?;
    let relations = lua.create_table()?;
    for (idx, relation) in context.relations.iter().enumerate() {
        let t = lua.create_table()?;
        t.set("from", relation.from.as_str())?;
        t.set("to", relation.to.as_str())?;
        t.set("relation_type", relation.relation_type.as_str())?;
        let metadata = lua.create_table()?;
        for (k, v) in &relation.metadata {
            metadata.set(k.as_str(), v.as_str())?;
        }
        t.set("metadata", metadata)?;
        relations.set(idx + 1, t)?;
    }
    table.set("relations", relations)?;
    Ok(table)
}

/// Convert a [`PluginEntity`] to a Lua table.
///
/// Part of the plugin-facing mapping contract (documented for plugin authors);
/// not exercised by the host pipeline directly.
#[allow(dead_code)]
pub fn plugin_entity_to_lua_table(lua: &Lua, entity: &PluginEntity) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;
    table.set("id", entity.id.as_str())?;
    table.set("kind", entity.kind.as_str())?;
    table.set("name", entity.name.as_str())?;
    table.set("signature", entity.signature.as_deref().unwrap_or(""))?;
    table.set("doc_comment", entity.doc_comment.as_deref().unwrap_or(""))?;
    let metadata = lua.create_table()?;
    for (key, value) in &entity.metadata {
        metadata.set(key.as_str(), value.as_str())?;
    }
    table.set("metadata", metadata)?;
    if let Some(span) = entity.span {
        table.set("span", span_to_lua_table(lua, span)?)?;
    } else {
        table.set("span", mlua::Nil)?;
    }
    let children = lua.create_table()?;
    for (idx, child) in entity.children.iter().enumerate() {
        children.set(idx + 1, plugin_entity_to_lua_table(lua, child)?)?;
    }
    table.set("children", children)?;
    Ok(table)
}

/// Convert a [`PluginDocument`] to a Lua table.
///
/// Part of the plugin-facing mapping contract (documented for plugin authors);
/// not exercised by the host pipeline directly.
#[allow(dead_code)]
pub fn plugin_document_to_lua_table(lua: &Lua, doc: &PluginDocument) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;
    table.set("title", doc.title.as_deref().unwrap_or(""))?;
    table.set("language", doc.language.as_deref().unwrap_or(""))?;
    let entities = lua.create_table()?;
    for (idx, entity) in doc.entities.iter().enumerate() {
        entities.set(idx + 1, plugin_entity_to_lua_table(lua, entity)?)?;
    }
    table.set("entities", entities)?;
    Ok(table)
}

/// Build a [`Span`] from a Lua table `{start_byte, end_byte, start_position, end_position}`.
pub fn span_from_lua_table(table: &Table) -> Result<Span, mlua::Error> {
    let start_byte = table.get::<Option<u64>>("start_byte")?.unwrap_or(0) as usize;
    let end_byte = table.get::<Option<u64>>("end_byte")?.unwrap_or(0) as usize;
    let start_position = position_from_lua_table(&table.get::<Table>("start_position")?)?;
    let end_position = position_from_lua_table(&table.get::<Table>("end_position")?)?;
    Ok(Span {
        start_byte,
        end_byte,
        start_position,
        end_position,
    })
}

fn span_to_lua_table(lua: &Lua, span: Span) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;
    table.set("start_byte", span.start_byte)?;
    table.set("end_byte", span.end_byte)?;
    let start_pos = lua.create_table()?;
    start_pos.set("row", span.start_position.row)?;
    start_pos.set("column", span.start_position.column)?;
    table.set("start_position", start_pos)?;
    let end_pos = lua.create_table()?;
    end_pos.set("row", span.end_position.row)?;
    end_pos.set("column", span.end_position.column)?;
    table.set("end_position", end_pos)?;
    Ok(table)
}

fn position_from_lua_table(table: &Table) -> Result<Position, mlua::Error> {
    Ok(Position {
        row: table.get::<Option<u64>>("row")?.unwrap_or(0) as usize,
        column: table.get::<Option<u64>>("column")?.unwrap_or(0) as usize,
    })
}

/// Parse a [`PluginEntity`] from a Lua table.
pub fn lua_table_to_plugin_entity(table: &Table) -> Result<PluginEntity, mlua::Error> {
    let mut children = Vec::new();
    if let Some(children_table) = table.get::<Option<Table>>("children")? {
        for pair in children_table.pairs::<Value, Value>() {
            let (_, value) = pair?;
            if let Value::Table(child) = value {
                if let Ok(entity) = lua_table_to_plugin_entity(&child) {
                    children.push(entity);
                }
            }
        }
    }
    Ok(PluginEntity {
        id: get_string(table, "id").unwrap_or_default(),
        kind: get_string(table, "kind").unwrap_or_else(|| "entity".to_string()),
        name: get_string(table, "name").unwrap_or_default(),
        signature: get_string(table, "signature"),
        doc_comment: get_string(table, "doc_comment"),
        metadata: get_string_map(table, "metadata"),
        span: match table.get::<Option<Table>>("span")? {
            Some(span_table) => Some(span_from_lua_table(&span_table)?),
            None => None,
        },
        children,
    })
}

/// Parse a [`PluginDocument`] from a Lua table.
pub fn lua_table_to_plugin_document(table: &Table) -> Result<PluginDocument, mlua::Error> {
    let mut entities = Vec::new();
    if let Some(entities_table) = table.get::<Option<Table>>("entities")? {
        for pair in entities_table.pairs::<Value, Value>() {
            let (_, value) = pair?;
            if let Value::Table(entity) = value {
                if let Ok(e) = lua_table_to_plugin_entity(&entity) {
                    entities.push(e);
                }
            }
        }
    }
    Ok(PluginDocument {
        title: get_string(table, "title"),
        language: get_string(table, "language"),
        entities,
    })
}

/// Parse a [`ChunkedResult`] from a Lua table.
///
/// Metadata fields that are difficult to round-trip through Lua (`test_info`,
/// `file_category`, `merged_group_ids`, overlap regions) are defaulted; the
/// host fills `file_path`/`source_span`/`segment_id` where required.
pub fn lua_table_to_chunked_result(table: &Table) -> Result<ChunkedResult, mlua::Error> {
    let path = match get_string(table, "path").as_deref() {
        Some("bm25") => ChunkPath::Bm25,
        _ => ChunkPath::Embedding,
    };
    let group_type = match get_string(table, "group_type") {
        Some(s) => serde_json::from_value::<GroupType>(serde_json::Value::String(s))
            .unwrap_or(GroupType::Standalone),
        None => GroupType::Standalone,
    };
    let content_type = match get_string(table, "content_type").as_deref() {
        Some("config") => ChunkContentType::Config {
            format: get_string(table, "format").unwrap_or_default(),
        },
        Some("document") => ChunkContentType::Document,
        Some("plaintext") => ChunkContentType::PlainText,
        _ => ChunkContentType::Code {
            language: Language::Unknown,
        },
    };

    let mut metadata = ChunkMetadata {
        file_category: content_type.file_category(),
        content_type,
        file_path: get_string(table, "file_path").unwrap_or_default(),
        source_span: Span::default(),
        source_ranges: Vec::new(),
        source_span_kind: SourceSpanKind::Unavailable,
        bm25_word_count: None,
        segment_id: get_string(table, "segment_id").unwrap_or_default(),
        merged_group_ids: Vec::new(),
        test_info: cce_types::TestInfo::unknown(),
        code_metadata: None,
        doc_metadata: Some(DocumentSpecificMetadata {
            doc_structure: get_string(table, "doc_structure"),
            doc_node_ids: get_string_array(table, "doc_node_ids").unwrap_or_default(),
        }),
    };
    if let Some(span_table) = table.get::<Option<Table>>("source_span")? {
        metadata.source_span = span_from_lua_table(&span_table)?;
        metadata.source_ranges = vec![metadata.source_span];
        metadata.source_span_kind = SourceSpanKind::ExactEntities;
    }

    Ok(ChunkedResult {
        chunk_id: get_string(table, "chunk_id").unwrap_or_default(),
        source_group_id: get_string(table, "source_group_id").unwrap_or_default(),
        path,
        group_type,
        chunk_index: get_u64(table, "chunk_index") as usize,
        total_chunks: get_u64(table, "total_chunks") as usize,
        text: get_string(table, "text").unwrap_or_default(),
        bm25_title: get_string(table, "bm25_title"),
        bm25_keywords: get_string_array(table, "bm25_keywords").unwrap_or_default(),
        token_count: get_u64(table, "token_count") as usize,
        start_byte: get_u64(table, "start_byte") as usize,
        end_byte: get_u64(table, "end_byte") as usize,
        prev_overlap: None,
        next_overlap: None,
        related_groups: Vec::new(),
        self_contained: table
            .get::<Option<bool>>("self_contained")?
            .unwrap_or(false),
        metadata,
    })
}

/// Parse a [`RerankResult`] from a Lua table.
pub fn lua_table_to_rerank_result(table: &Table) -> Result<RerankResult, mlua::Error> {
    let mut reranked_candidates = Vec::new();
    if let Some(candidates) = table.get::<Option<Table>>("reranked_candidates")? {
        for pair in candidates.pairs::<Value, Value>() {
            let (_, value) = pair?;
            if let Value::Table(c) = value {
                reranked_candidates.push(RerankedCandidate {
                    id: get_string(&c, "id").unwrap_or_default(),
                    rerank_score: get_f32(&c, "rerank_score").unwrap_or(0.0),
                    initial_score: get_f32(&c, "initial_score").unwrap_or(0.0),
                    final_score: get_f32(&c, "final_score").unwrap_or_else(|| {
                        // Default: use the plugin's rerank score if no final_score given.
                        get_f32(&c, "rerank_score").unwrap_or(0.0)
                    }),
                    rank_change: get_i32(&c, "rank_change").unwrap_or(0),
                    reasoning: get_string(&c, "reasoning"),
                });
            }
        }
    }
    Ok(RerankResult::new(reranked_candidates))
}

/// Patch an [`EntityGroup`] from a Lua table, using `fallback` for any field
/// the plugin did not provide.
pub fn lua_table_to_entity_group(
    table: &Table,
    fallback: EntityGroup,
) -> Result<EntityGroup, mlua::Error> {
    let mut group = fallback;

    if let Some(s) = get_string(table, "group_id") {
        group.group_id = compact_str::CompactString::from(s);
    }
    if let Some(s) = get_string(table, "group_type") {
        if let Ok(gt) = serde_json::from_value::<GroupType>(serde_json::Value::String(s)) {
            group.group_type = gt;
        }
    }
    if let Some(s) = get_string(table, "name") {
        group.name = compact_str::CompactString::from(s);
    }
    if let Some(s) = get_string(table, "kind") {
        if let Some(kind) = entity_kind_from_string(&s) {
            group.kind = kind;
        }
    }
    if let Some(s) = get_string(table, "language") {
        if let Ok(lang) = serde_json::from_value::<Language>(serde_json::Value::String(s)) {
            group.language = lang;
        }
    }
    if let Some(s) = get_string(table, "pattern_info") {
        if let Ok(pi) = serde_json::from_str(&s) {
            group.pattern_info = pi;
        }
    }
    group.metadata = get_string_map(table, "metadata");
    if let Some(s) = get_string(table, "parent_group_id") {
        group.parent_group_id = Some(compact_str::CompactString::from(s));
    }
    if let Some(n) = table.get::<Option<u64>>("nesting_level")? {
        group.nesting_level = n as usize;
    }
    if let Some(header_table) = table.get::<Option<Table>>("header")? {
        let current = group.header.take();
        if let Some(header) = lua_table_to_grouped_entity(&header_table, current)? {
            group.header = Some(header);
        }
    }
    if let Some(members_table) = table.get::<Option<Table>>("members")? {
        let mut members = Vec::new();
        let mut member_ids = Vec::new();
        for pair in members_table.pairs::<Value, Value>() {
            let (_, value) = pair?;
            if let Value::Table(member) = value {
                if let Ok(Some(ge)) = lua_table_to_grouped_entity(&member, None) {
                    member_ids.push(ge.id);
                    members.push(ge);
                }
            }
        }
        group.members = members.into();
        group.member_ids = member_ids.into();
    }
    Ok(group)
}

/// Patch a [`GroupedEntity`] from a Lua table, using `fallback` for missing fields.
pub fn lua_table_to_grouped_entity(
    table: &Table,
    fallback: Option<GroupedEntity>,
) -> Result<Option<GroupedEntity>, mlua::Error> {
    if table.is_empty() {
        return Ok(fallback);
    }
    let mut entity = fallback.unwrap_or_default();

    if let Some(id) = table.get::<Option<u64>>("id")? {
        entity.id = EntityId(id);
    }
    if let Some(s) = get_string(table, "name") {
        entity.name = s;
    }
    if let Some(s) = get_string(table, "kind") {
        if let Some(kind) = entity_kind_from_string(&s) {
            entity.kind = kind;
        }
    }
    if let Some(s) = get_string(table, "signature") {
        entity.signature = s;
    }
    if let Some(s) = get_string(table, "doc_comment") {
        entity.doc_comment = Some(s);
    }
    entity.metadata = get_string_map(table, "metadata");
    if let Some(mods) = get_string_array(table, "modifiers") {
        entity.modifiers = mods;
    }
    if let Some(s) = get_string(table, "subtype") {
        entity.subtype = Some(s);
    }
    if let Some(b) = table.get::<Option<bool>>("is_stdlib")? {
        entity.is_stdlib = b;
    }
    Ok(Some(entity))
}

/// Best-effort map from a string (Debug or serde snake_case form) to [`EntityKind`].
fn entity_kind_from_string(s: &str) -> Option<cce_types::EntityKind> {
    use cce_types::EntityKind;
    // Try the Debug form first (what `grouped_entity_to_lua_table` emits).
    if let Ok(kind) = serde_json::from_value::<EntityKind>(serde_json::Value::String(s.to_string()))
    {
        return Some(kind);
    }
    // Fall back to snake_case conversion.
    let snake = snake_case(s);
    serde_json::from_value::<EntityKind>(serde_json::Value::String(snake)).ok()
}

/// Convert a camel/PascalCase identifier to snake_case (ASCII).
fn snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

// ── Low-level table field accessors ────────────────────────────────────

/// Read an optional string field from a Lua table.
fn get_string(table: &Table, key: &str) -> Option<String> {
    match table.get::<Value>(key).ok()? {
        Value::String(s) => Some(s.to_string_lossy().to_string()),
        Value::Integer(i) => Some(i.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Read a string→string map field from a Lua table.
fn get_string_map(table: &Table, key: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    if let Ok(Some(map)) = table.get::<Option<Table>>(key) {
        for (k, v) in map.pairs::<String, Value>().flatten() {
            let value = match v {
                Value::String(s) => s.to_string_lossy().to_string(),
                Value::Integer(i) => i.to_string(),
                Value::Number(n) => n.to_string(),
                Value::Boolean(b) => b.to_string(),
                _ => continue,
            };
            out.insert(k, value);
        }
    }
    out
}

/// Read an optional array-of-strings field from a Lua table.
fn get_string_array(table: &Table, key: &str) -> Option<Vec<String>> {
    let array = table.get::<Option<Table>>(key).ok()??;
    let mut out = Vec::new();
    for pair in array.pairs::<Value, Value>() {
        if let Ok((_, Value::String(s))) = pair {
            out.push(s.to_string_lossy().to_string());
        }
    }
    Some(out)
}

/// Read an optional u64 field.
fn get_u64(table: &Table, key: &str) -> u64 {
    table.get::<Option<u64>>(key).ok().flatten().unwrap_or(0)
}

/// Read an optional f32 field.
fn get_f32(table: &Table, key: &str) -> Option<f32> {
    table.get::<Option<f32>>(key).ok().flatten()
}

/// Read an optional i32 field.
fn get_i32(table: &Table, key: &str) -> Option<i32> {
    table.get::<Option<i32>>(key).ok().flatten()
}

// ---------------------------------------------------------------------------
// Contract conversions
// ---------------------------------------------------------------------------

/// Convert a Lua array-of-tables into [`PluginSymbol`]s.
pub fn lua_table_to_plugin_symbols(table: &Table) -> Result<Vec<PluginSymbol>, mlua::Error> {
    let mut out = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        if let Ok((_, Value::Table(t))) = pair {
            if let Ok(symbol) = lua_table_to_plugin_symbol(&t) {
                out.push(symbol);
            }
        }
    }
    Ok(out)
}

/// Convert a single Lua table into a [`PluginSymbol`].
pub fn lua_table_to_plugin_symbol(table: &Table) -> Result<PluginSymbol, mlua::Error> {
    let id: String = get_string(table, "id")
        .or_else(|| get_string(table, "name"))
        .unwrap_or_default();
    let name: String = get_string(table, "name").unwrap_or_default();
    let kind: String = get_string(table, "kind").unwrap_or_default();
    let mut symbol = PluginSymbol {
        id,
        name,
        kind,
        visibility: get_string(table, "visibility"),
        module_path: get_string(table, "module_path"),
        location: None,
        metadata: get_string_map(table, "metadata"),
        children: Vec::new(),
    };
    if let Some(children_table) = table.get::<Option<Table>>("children")? {
        symbol.children = lua_table_to_plugin_symbols(&children_table)?;
    }
    Ok(symbol)
}

/// Convert a Lua array-of-tables into [`PluginRelation`]s.
pub fn lua_table_to_plugin_relations(table: &Table) -> Result<Vec<PluginRelation>, mlua::Error> {
    let mut out = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        if let Ok((_, Value::Table(t))) = pair {
            if let Ok(relation) = lua_table_to_plugin_relation(&t) {
                out.push(relation);
            }
        }
    }
    Ok(out)
}

/// Convert a single Lua table into a [`PluginRelation`].
pub fn lua_table_to_plugin_relation(table: &Table) -> Result<PluginRelation, mlua::Error> {
    Ok(PluginRelation {
        from: get_string(table, "from").unwrap_or_default(),
        to: get_string(table, "to").unwrap_or_default(),
        relation_type: get_string(table, "relation_type").unwrap_or_default(),
        metadata: get_string_map(table, "metadata"),
    })
}

/// Convert a Lua array-of-tables into [`ResultFilterEntry`]s.
pub fn lua_table_to_filter_entries(table: &Table) -> Result<Vec<ResultFilterEntry>, mlua::Error> {
    let mut out = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        if let Ok((_, Value::Table(t))) = pair {
            out.push(ResultFilterEntry {
                id: get_string(&t, "id").unwrap_or_default(),
                remove: t.get::<Option<bool>>("remove")?.unwrap_or(false),
                boost: get_f32(&t, "boost"),
                note: get_string(&t, "note"),
            });
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// SymbolExtract contract conversions
// ---------------------------------------------------------------------------

/// Convert a Lua array-of-tables into [`PluginImport`]s (`SymbolExtract`).
pub fn lua_table_to_plugin_imports(table: &Table) -> Result<Vec<PluginImport>, mlua::Error> {
    let mut out = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        if let Ok((_, Value::Table(t))) = pair {
            let path = get_string(&t, "path").unwrap_or_default();
            if path.is_empty() {
                continue;
            }
            out.push(PluginImport {
                path,
                symbols: get_string_array(&t, "symbols"),
                alias: get_string(&t, "alias"),
                is_wildcard: t.get::<Option<bool>>("is_wildcard")?.unwrap_or(false),
                metadata: get_string_map(&t, "metadata"),
            });
        }
    }
    Ok(out)
}

/// Convert a Lua array-of-tables into [`PluginExport`]s (`SymbolExtract`).
pub fn lua_table_to_plugin_exports(table: &Table) -> Result<Vec<PluginExport>, mlua::Error> {
    let mut out = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        if let Ok((_, Value::Table(t))) = pair {
            let name = get_string(&t, "name").unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            out.push(PluginExport {
                name,
                kind: get_string(&t, "kind").unwrap_or_default(),
                visibility: get_string(&t, "visibility"),
                location: None,
                metadata: get_string_map(&t, "metadata"),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::CompactString;
    use smallvec::SmallVec;
    use std::collections::HashMap;

    use cce_types::Span;
    use cce_types::grouper::{GroupType, PatternInfo};

    #[test]
    fn test_grouped_entity_to_lua_table() {
        let lua = Lua::new();
        let entity = GroupedEntity {
            id: cce_types::entity::EntityId(1),
            kind: cce_types::entity::EntityKind::Function,
            name: "test_function".to_string(),
            signature: "fn test_function() -> i32".to_string(),
            parameters: SmallVec::new(),
            return_type: Some("i32".to_string()),
            doc_comment: Some("A test function".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("endpoint".to_string(), "/api/test".to_string());
                m.insert("methods".to_string(), "GET,POST".to_string());
                m
            },
        };

        let table = grouped_entity_to_lua_table(&lua, &entity).expect("Failed to convert");

        // Verify basic fields
        let name: String = table.get("name").expect("Missing name");
        assert_eq!(name, "test_function");

        let kind: String = table.get("kind").expect("Missing kind");
        assert!(kind.contains("Function"));

        // Verify metadata
        let metadata: Table = table.get("metadata").expect("Missing metadata");
        let endpoint: String = metadata.get("endpoint").expect("Missing endpoint");
        assert_eq!(endpoint, "/api/test");

        let methods: String = metadata.get("methods").expect("Missing methods");
        assert_eq!(methods, "GET,POST");
    }

    #[test]
    fn test_entity_group_to_lua_table_with_metadata() {
        let lua = Lua::new();
        let group = EntityGroup {
            group_id: CompactString::from("test_group"),
            group_type: GroupType::ClassWithMethods,
            header: None,
            header_id: None,
            members: SmallVec::new(),
            member_ids: SmallVec::new(),
            entity_spans: HashMap::new(),
            combined_source: None,
            combined_source_lazy: std::sync::OnceLock::new(),
            span: Span::default(),
            kind: cce_types::entity::EntityKind::Class,
            name: CompactString::from("TestClass"),
            language: cce_types::language::Language::Python,
            pattern_info: PatternInfo::None,
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: {
                let mut m = HashMap::new();
                m.insert("route_pattern".to_string(), "/api/users".to_string());
                m
            },
            test_info: cce_types::TestInfo::unknown(),
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");

        // Verify group info
        let name: String = table.get("name").expect("Missing name");
        assert_eq!(name, "TestClass");

        let group_type: String = table.get("group_type").expect("Missing group_type");
        assert!(group_type.contains("ClassWithMethods"));

        // Verify metadata
        let metadata: Table = table.get("metadata").expect("Missing metadata");
        let route: String = metadata
            .get("route_pattern")
            .expect("Missing route_pattern");
        assert_eq!(route, "/api/users");

        // Verify new nesting fields
        let nesting_level: usize = table.get("nesting_level").expect("Missing nesting_level");
        assert_eq!(nesting_level, 0);

        let has_significant_nested: bool = table
            .get("has_significant_nested")
            .expect("Missing has_significant_nested");
        assert!(!has_significant_nested);

        let parent_group_id: mlua::Value = table
            .get("parent_group_id")
            .expect("Missing parent_group_id");
        assert!(matches!(parent_group_id, mlua::Value::Nil));

        // Verify header_id is nil when None
        let header_id: mlua::Value = table.get("header_id").expect("Missing header_id");
        assert!(matches!(header_id, mlua::Value::Nil));

        // Verify empty member_ids
        let member_ids: Table = table.get("member_ids").expect("Missing member_ids");
        let member_ids_len: i64 = member_ids.len().expect("Failed to get length");
        assert_eq!(member_ids_len, 0);

        // Verify empty member_roles
        let member_roles: Table = table.get("member_roles").expect("Missing member_roles");
        let member_roles_len: i64 = member_roles.len().expect("Failed to get length");
        assert_eq!(member_roles_len, 0);

        // Verify span
        let span: Table = table.get("span").expect("Missing span");
        let start_byte: usize = span.get("start_byte").expect("Missing start_byte");
        assert_eq!(start_byte, 0);

        // Verify nested_groups empty
        let nested_groups: Table = table.get("nested_groups").expect("Missing nested_groups");
        let nested_len: i64 = nested_groups.len().expect("Failed to get length");
        assert_eq!(nested_len, 0);
    }

    #[test]
    fn test_grouped_entity_with_none_optionals() {
        let lua = Lua::new();
        let entity = GroupedEntity {
            id: cce_types::entity::EntityId(2),
            kind: cce_types::entity::EntityKind::Function,
            name: "minimal".to_string(),
            signature: "fn minimal()".to_string(),
            parameters: SmallVec::new(),
            return_type: None,
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: HashMap::new(),
        };

        let table = grouped_entity_to_lua_table(&lua, &entity).expect("Failed to convert");

        let return_type: mlua::Value = table.get("return_type").expect("Missing return_type");
        assert!(matches!(return_type, mlua::Value::Nil));

        let doc_comment: mlua::Value = table.get("doc_comment").expect("Missing doc_comment");
        assert!(matches!(doc_comment, mlua::Value::Nil));
    }

    #[test]
    fn test_grouped_entity_with_parameters() {
        let lua = Lua::new();
        let entity = GroupedEntity {
            id: cce_types::entity::EntityId(3),
            kind: cce_types::entity::EntityKind::Function,
            name: "with_params".to_string(),
            signature: "fn(x: i32, y: String)".to_string(),
            parameters: {
                let mut params = SmallVec::new();
                params.push((CompactString::from("x"), Some(CompactString::from("i32"))));
                params.push((
                    CompactString::from("y"),
                    Some(CompactString::from("String")),
                ));
                params
            },
            return_type: Some("bool".to_string()),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: HashMap::new(),
        };

        let table = grouped_entity_to_lua_table(&lua, &entity).expect("Failed to convert");
        let params: Table = table.get("parameters").expect("Missing parameters");
        let len: i64 = params.len().expect("Failed to get length");
        assert_eq!(len, 2);

        let p1: Table = params.get(1).expect("Missing param 1");
        let name: String = p1.get("name").expect("Missing name");
        assert_eq!(name, "x");
    }

    #[test]
    fn test_entity_group_with_header_and_members() {
        let lua = Lua::new();
        let header = GroupedEntity::new(
            cce_types::entity::EntityId(10),
            cce_types::entity::EntityKind::Class,
            "MyClass".to_string(),
            "class MyClass".to_string(),
        );
        let member = GroupedEntity::new(
            cce_types::entity::EntityId(11),
            cce_types::entity::EntityKind::Method,
            "my_method".to_string(),
            "fn my_method()".to_string(),
        );

        let group = EntityGroup {
            group_id: CompactString::from("g1"),
            group_type: GroupType::ClassWithMethods,
            header: Some(header),
            header_id: Some(cce_types::entity::EntityId(10)),
            members: smallvec::smallvec![member],
            member_ids: smallvec::smallvec![cce_types::entity::EntityId(11)],
            name: CompactString::from("MyClass"),
            kind: cce_types::entity::EntityKind::Class,
            language: cce_types::language::Language::Python,
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");

        // Verify header
        let header_table: Table = table.get("header").expect("Missing header");
        let header_name: String = header_table.get("name").expect("Missing header name");
        assert_eq!(header_name, "MyClass");

        // Verify members
        let members_table: Table = table.get("members").expect("Missing members");
        let member_len: i64 = members_table.len().expect("Failed to get length");
        assert_eq!(member_len, 1);

        // Verify header_id
        let header_id_table: Table = table.get("header_id").expect("Missing header_id");
        let header_id_val: u64 = header_id_table.get("id").expect("Missing id");
        assert_eq!(header_id_val, 10);
    }

    #[test]
    fn test_entity_group_with_nested_groups() {
        let lua = Lua::new();
        let nested = EntityGroup {
            group_id: CompactString::from("nested1"),
            name: CompactString::from("NestedClass"),
            nesting_level: 1,
            parent_group_id: Some(CompactString::from("parent")),
            ..Default::default()
        };

        let group = EntityGroup {
            group_id: CompactString::from("parent"),
            name: CompactString::from("ParentClass"),
            nested_groups: Box::new([nested]),
            nesting_level: 0,
            has_significant_nested: true,
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");

        let nested_tables: Table = table.get("nested_groups").expect("Missing nested_groups");
        let len: i64 = nested_tables.len().expect("Failed to get length");
        assert_eq!(len, 1);

        let nested_t: Table = nested_tables.get(1).expect("Missing nested table");
        let nested_name: String = nested_t.get("name").expect("Missing name");
        assert_eq!(nested_name, "NestedClass");

        let is_nested: bool = table.get("has_significant_nested").expect("Missing flag");
        assert!(is_nested);
    }

    #[test]
    fn test_entity_group_with_member_roles() {
        let lua = Lua::new();
        use cce_types::grouper::MemberRole;

        let mut roles = SmallVec::new();
        roles.push((
            cce_types::entity::EntityId(1),
            MemberRole::BoilerplateMethod,
        ));
        roles.push((
            cce_types::entity::EntityId(2),
            MemberRole::BoilerplateMethod,
        ));

        let group = EntityGroup {
            group_id: CompactString::from("roles_group"),
            name: CompactString::from("RolesGroup"),
            member_roles: roles,
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");
        let roles_table: Table = table.get("member_roles").expect("Missing member_roles");
        let len: i64 = roles_table.len().expect("Failed to get length");
        assert_eq!(len, 2);

        let role1: Table = roles_table.get(1).expect("Missing role 1");
        let role_str: String = role1.get("role").expect("Missing role");
        assert_eq!(role_str, "boilerplate_method");
    }

    #[test]
    fn test_entity_group_with_non_default_span() {
        let lua = Lua::new();
        use cce_types::{Position, Span};

        let span = Span {
            start_byte: 100,
            end_byte: 500,
            start_position: Position { row: 5, column: 0 },
            end_position: Position {
                row: 20,
                column: 10,
            },
        };

        let group = EntityGroup {
            group_id: CompactString::from("span_group"),
            name: CompactString::from("SpanGroup"),
            span,
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");
        let span_table: Table = table.get("span").expect("Missing span");

        let start_byte: usize = span_table.get("start_byte").expect("Missing start_byte");
        assert_eq!(start_byte, 100);

        let end_byte: usize = span_table.get("end_byte").expect("Missing end_byte");
        assert_eq!(end_byte, 500);

        let start_pos: Table = span_table.get("start_position").expect("Missing start_pos");
        let row: usize = start_pos.get("row").expect("Missing row");
        assert_eq!(row, 5);
    }

    #[test]
    fn test_entity_group_with_member_ids() {
        let lua = Lua::new();
        use cce_types::entity::EntityId;

        let group = EntityGroup {
            group_id: CompactString::from("id_group"),
            name: CompactString::from("IdGroup"),
            member_ids: smallvec::smallvec![EntityId(101), EntityId(102), EntityId(103)],
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");
        let member_ids: Table = table.get("member_ids").expect("Missing member_ids");
        let len: i64 = member_ids.len().expect("Failed to get length");
        assert_eq!(len, 3);

        let id1: u64 = member_ids.get(1).expect("Missing id 1");
        assert_eq!(id1, 101);
    }
}
