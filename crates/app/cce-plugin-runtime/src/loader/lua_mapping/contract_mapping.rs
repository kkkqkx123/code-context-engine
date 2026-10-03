//! Plugin extension contract conversions between Rust and Lua tables.

use mlua::{Lua, Table, Value};

use cce_types::ast_to_nl::{
    ChunkContentType, ChunkMetadata, ChunkPath, DocumentSpecificMetadata, GroupConversions,
    RerankResult, RerankedCandidate, SourceSpanKind,
};
use cce_types::grouper::GroupType;
use cce_types::plugin::{
    GroupPluginContext, PluginExport, PluginImport, PluginRelation, PluginSymbol, ResultFilterEntry,
};
use cce_types::{
    ChunkedResult, ConversionResult, Language, PluginDocument, PluginEntity, Position, Span,
};

use super::entity_mapping::entity_group_to_lua_table;
use super::table_accessors::{
    get_f32, get_i32, get_string, get_string_array, get_string_map, get_u64,
};

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
        truncated: false,
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
