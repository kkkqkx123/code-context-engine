use mlua::Table;
use tracing::warn;

use super::lua_helpers::{get_string_field, parse_entity_array};
use super::lua_plugin::LuaPlugin;
use crate::error::PluginError;
use crate::lua_mapping::{
    entity_group_to_lua_table, group_conversions_to_lua_table, group_plugin_context_to_lua_table,
    lua_table_to_chunked_result, lua_table_to_entity_group, lua_table_to_plugin_document,
    lua_table_to_rerank_result,
};
use crate::pattern::extract_entities;
use crate::types::PluginMetadata;
use cce_plugin::CodePlugin;
use cce_types::ast_to_nl::RerankCandidate;
use cce_types::grouper::EntityGroup;
use cce_types::plugin::GroupPluginContext;
use cce_types::{
    ChunkedResult, FileFilterDecision, FusionWeights, GroupConversions, PluginDocument,
    PluginEntity, PluginExport, PluginImport, PluginRelation, PluginSymbol, QueryRewriteResult,
    RerankResult, ResultFilterEntry,
};

impl CodePlugin for LuaPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn generate_bm25(
        &self,
        group: &cce_types::grouper::EntityGroup,
    ) -> Result<Option<String>, PluginError> {
        let fn_name = match &self.generate_bm25_fn_name {
            Some(name) => name.clone(),
            None => return Ok(None),
        };
        let group = group.clone();
        let plugin_id = self.metadata.id.clone();
        self.execute_with_timeout("generate_bm25", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get generate_bm25 function: {e}"))
            })?;

            let group_table = entity_group_to_lua_table(lua, &group).map_err(|e| {
                PluginError::InvalidOutput(format!("Failed to convert EntityGroup: {e}"))
            })?;

            match func.call::<Option<String>>(group_table) {
                Ok(result) => Ok(result),
                Err(e) => {
                    warn!("Lua plugin {plugin_id} generate_bm25 failed: {e}");
                    Err(PluginError::ScriptError(format!(
                        "Generate BM25 function error: {e}"
                    )))
                }
            }
        })
    }

    fn generate_embedding(
        &self,
        group: &cce_types::grouper::EntityGroup,
    ) -> Result<Option<String>, PluginError> {
        let fn_name = match &self.generate_embedding_fn_name {
            Some(name) => name.clone(),
            None => return Ok(None),
        };
        let group = group.clone();
        let plugin_id = self.metadata.id.clone();
        self.execute_with_timeout("generate_embedding", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get generate_embedding function: {e}"))
            })?;

            let group_table = entity_group_to_lua_table(lua, &group).map_err(|e| {
                PluginError::InvalidOutput(format!("Failed to convert EntityGroup: {e}"))
            })?;

            match func.call::<Option<String>>(group_table) {
                Ok(result) => Ok(result),
                Err(e) => {
                    warn!("Lua plugin {plugin_id} generate_embedding failed: {e}");
                    Err(PluginError::ScriptError(format!(
                        "Generate embedding function error: {e}"
                    )))
                }
            }
        })
    }

    fn supports_bm25(&self) -> bool {
        self.generate_bm25_fn_name.is_some() || self.generate_bm25_batch_fn_name.is_some()
    }

    fn supports_embedding(&self) -> bool {
        self.generate_embedding_fn_name.is_some() || self.generate_embedding_batch_fn_name.is_some()
    }

    fn generate_bm25_batch(
        &self,
        groups: &[&cce_types::grouper::EntityGroup],
    ) -> Result<Vec<Option<String>>, PluginError> {
        if let Some(fn_name) = &self.generate_bm25_batch_fn_name {
            return self.call_batch_function(fn_name, groups, "generate_bm25_batch");
        }
        let mut results = Vec::with_capacity(groups.len());
        for group in groups {
            results.push(self.generate_bm25(group)?);
        }
        Ok(results)
    }

    fn generate_embedding_batch(
        &self,
        groups: &[&cce_types::grouper::EntityGroup],
    ) -> Result<Vec<Option<String>>, PluginError> {
        if let Some(fn_name) = &self.generate_embedding_batch_fn_name {
            return self.call_batch_function(fn_name, groups, "generate_embedding_batch");
        }
        let mut results = Vec::with_capacity(groups.len());
        for group in groups {
            results.push(self.generate_embedding(group)?);
        }
        Ok(results)
    }

    // ── FormatParse ───────────────────────────────────────────────────

    fn supports_parse(&self) -> bool {
        self.parse_document_fn_name.is_some() || !self.patterns.is_empty()
    }

    fn parse_document(
        &self,
        content: &str,
        file_path: &str,
    ) -> Result<Option<PluginDocument>, PluginError> {
        let Some(fn_name) = &self.parse_document_fn_name else {
            // Pattern fallback: no function, but patterns are declared.
            if !self.patterns.is_empty() {
                let entities = extract_entities(content, &self.patterns);
                if entities.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(PluginDocument {
                    title: None,
                    language: None,
                    entities,
                }));
            }
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let content = content.to_string();
        let file_path = file_path.to_string();
        self.execute_with_timeout("parse_document", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get parse_document function: {e}"))
            })?;
            let content_val = lua
                .create_string(&content)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let path_val = lua
                .create_string(&file_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func
                .call((content_val, path_val))
                .map_err(|e| PluginError::ScriptError(format!("parse_document error: {e}")))?;
            let value_table = match result {
                mlua::Value::Table(t) => t,
                _ => return Ok(None),
            };
            // Accept either `{title, language, entities}` or a bare array of entities.
            let doc = lua_table_to_plugin_document(&value_table).map_err(|e| {
                PluginError::InvalidOutput(format!("Invalid parse_document result: {e}"))
            })?;
            if doc.entities.is_empty() {
                let entities = parse_entity_array(&value_table);
                if entities.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(PluginDocument {
                    title: doc.title,
                    language: doc.language,
                    entities,
                }));
            }
            Ok(Some(doc))
        })
    }

    // ── EntityExtract ─────────────────────────────────────────────────

    fn supports_extract(&self) -> bool {
        self.extract_entities_fn_name.is_some() || !self.patterns.is_empty()
    }

    // ── LanguageRemap (no FFI; host built-in grammar) ─────────────────

    fn supports_language_remap(&self) -> bool {
        self.remap_grammar_language.is_some()
            && self.language_name.is_some()
            && !self.language_extensions.is_empty()
    }

    fn language_name(&self) -> Option<String> {
        self.language_name.clone()
    }

    fn language_extensions(&self) -> Vec<String> {
        self.language_extensions.clone()
    }

    fn remap_grammar_language(&self) -> Option<String> {
        self.remap_grammar_language.clone()
    }

    fn query_scheme(&self, query_type: cce_types::QueryType) -> Option<String> {
        self.query_schemes.get(&query_type).cloned()
    }

    // ── LangHeuristics ───────────────────────────────────────────────

    fn supports_any_heuristic(&self) -> bool {
        self.classify_stdlib_fn_name.is_some()
            || self.is_test_file_fn_name.is_some()
            || self.entity_kind_fn_name.is_some()
    }

    fn supports_stdlib_heuristic(&self) -> bool {
        self.classify_stdlib_fn_name.is_some()
    }

    fn classify_stdlib(&self, module_path: &str) -> Result<Option<String>, PluginError> {
        let Some(fn_name) = &self.classify_stdlib_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let module_path = module_path.to_string();
        let plugin_id = self.metadata.id.clone();
        self.execute_with_timeout("classify_stdlib", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get classify_stdlib function: {e}"))
            })?;
            let module_val = lua
                .create_string(&module_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func.call(module_val).map_err(|e| {
                warn!("Lua plugin {plugin_id} classify_stdlib failed: {e}");
                PluginError::ScriptError(format!("classify_stdlib error: {e}"))
            })?;
            match result {
                mlua::Value::String(s) => Ok(Some(
                    s.to_str()
                        .map_err(|e| {
                            PluginError::InvalidOutput(format!(
                                "classify_stdlib non-UTF8 output: {e}"
                            ))
                        })?
                        .to_string(),
                )),
                _ => Ok(None),
            }
        })
    }

    fn supports_test_file_heuristic(&self) -> bool {
        self.is_test_file_fn_name.is_some()
    }

    fn is_test_file(&self, file_path: &str, content: &str) -> Result<Option<bool>, PluginError> {
        let Some(fn_name) = &self.is_test_file_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let file_path = file_path.to_string();
        let content = content.to_string();
        let plugin_id = self.metadata.id.clone();
        self.execute_with_timeout("is_test_file", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get is_test_file function: {e}"))
            })?;
            let path_val = lua
                .create_string(&file_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let content_val = lua
                .create_string(&content)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func.call((path_val, content_val)).map_err(|e| {
                warn!("Lua plugin {plugin_id} is_test_file failed: {e}");
                PluginError::ScriptError(format!("is_test_file error: {e}"))
            })?;
            match result {
                mlua::Value::Boolean(b) => Ok(Some(b)),
                _ => Ok(None),
            }
        })
    }

    fn supports_entity_kind_heuristic(&self) -> bool {
        self.entity_kind_fn_name.is_some()
    }

    fn entity_kind(&self, query_capture_name: &str) -> Result<Option<String>, PluginError> {
        let Some(fn_name) = &self.entity_kind_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let capture = query_capture_name.to_string();
        let plugin_id = self.metadata.id.clone();
        self.execute_with_timeout("entity_kind", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get entity_kind function: {e}"))
            })?;
            let capture_val = lua
                .create_string(&capture)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func.call(capture_val).map_err(|e| {
                warn!("Lua plugin {plugin_id} entity_kind failed: {e}");
                PluginError::ScriptError(format!("entity_kind error: {e}"))
            })?;
            match result {
                mlua::Value::String(s) => Ok(Some(
                    s.to_str()
                        .map_err(|e| {
                            PluginError::InvalidOutput(format!("entity_kind non-UTF8 output: {e}"))
                        })?
                        .to_string(),
                )),
                _ => Ok(None),
            }
        })
    }

    fn extract_entities(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<PluginEntity>>, PluginError> {
        let Some(fn_name) = &self.extract_entities_fn_name else {
            if !self.patterns.is_empty() {
                let entities = extract_entities(content, &self.patterns);
                if entities.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(entities));
            }
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let content = content.to_string();
        let file_path = file_path.to_string();
        let language = language.to_string();
        self.execute_with_timeout("extract_entities", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get extract_entities function: {e}"))
            })?;
            let content_val = lua
                .create_string(&content)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let path_val = lua
                .create_string(&file_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let lang_val = lua
                .create_string(&language)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func
                .call((content_val, path_val, lang_val))
                .map_err(|e| PluginError::ScriptError(format!("extract_entities error: {e}")))?;
            match result {
                mlua::Value::Table(t) => {
                    let entities = parse_entity_array(&t);
                    if entities.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(entities))
                    }
                }
                _ => Ok(None),
            }
        })
    }

    // ── Group ─────────────────────────────────────────────────────────

    fn supports_group(&self) -> bool {
        self.post_group_fn_name.is_some()
    }

    fn post_group(
        &self,
        groups: Vec<EntityGroup>,
        context: GroupPluginContext,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        let Some(fn_name) = &self.post_group_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        self.execute_with_timeout("post_group", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get post_group function: {e}"))
            })?;
            let groups_table = lua
                .create_table()
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            for (i, group) in groups.iter().enumerate() {
                let group_table = entity_group_to_lua_table(lua, group).map_err(|e| {
                    PluginError::InvalidOutput(format!("Failed to convert EntityGroup: {e}"))
                })?;
                groups_table
                    .set(i + 1, group_table)
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            }
            let context_table = group_plugin_context_to_lua_table(lua, &context).map_err(|e| {
                PluginError::InvalidOutput(format!("Failed to convert context: {e}"))
            })?;
            let result: mlua::Value = func
                .call((groups_table, context_table))
                .map_err(|e| PluginError::ScriptError(format!("post_group error: {e}")))?;
            let result_table = match result {
                mlua::Value::Table(t) => t,
                _ => return Ok(None),
            };
            let mut out = Vec::new();
            for pair in result_table.pairs::<mlua::Value, mlua::Value>() {
                let (_, value) = pair.map_err(|e| {
                    PluginError::ScriptError(format!("post_group result iteration error: {e}"))
                })?;
                if let mlua::Value::Table(group_table) = value {
                    let fallback = groups
                        .iter()
                        .find(|g| {
                            get_string_field(&group_table, "group_id").as_deref()
                                == Some(g.group_id.as_str())
                        })
                        .cloned()
                        .unwrap_or_else(|| {
                            // Keep the current group by index order if ids don't match.
                            let idx = out.len().min(groups.len().saturating_sub(1));
                            groups[idx].clone()
                        });
                    let group = lua_table_to_entity_group(&group_table, fallback).map_err(|e| {
                        PluginError::InvalidOutput(format!("Invalid post_group group: {e}"))
                    })?;
                    out.push(group);
                }
            }
            if out.is_empty() {
                Ok(None)
            } else {
                Ok(Some(out))
            }
        })
    }

    // ── Chunk ─────────────────────────────────────────────────────────

    fn supports_chunk(&self) -> bool {
        self.chunk_fn_name.is_some()
    }

    fn chunk(
        &self,
        conversions: Vec<GroupConversions>,
        file_path: &str,
    ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
        let Some(fn_name) = &self.chunk_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let file_path = file_path.to_string();
        self.execute_with_timeout("chunk", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get chunk function: {e}"))
            })?;
            let conversions_table =
                group_conversions_to_lua_table(lua, &conversions).map_err(|e| {
                    PluginError::InvalidOutput(format!("Failed to convert conversions: {e}"))
                })?;
            let path_val = lua
                .create_string(&file_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func
                .call((conversions_table, path_val))
                .map_err(|e| PluginError::ScriptError(format!("chunk error: {e}")))?;
            let result_table = match result {
                mlua::Value::Table(t) => t,
                _ => return Ok(None),
            };
            let mut out = Vec::new();
            for pair in result_table.pairs::<mlua::Value, mlua::Value>() {
                let (_, value) = pair.map_err(|e| {
                    PluginError::ScriptError(format!("chunk result iteration error: {e}"))
                })?;
                if let mlua::Value::Table(chunk_table) = value {
                    if let Ok(chunk) = lua_table_to_chunked_result(&chunk_table) {
                        out.push(chunk);
                    }
                }
            }
            if out.is_empty() {
                Ok(None)
            } else {
                Ok(Some(out))
            }
        })
    }

    // ── Rerank ────────────────────────────────────────────────────────

    fn supports_rerank(&self) -> bool {
        self.rerank_fn_name.is_some()
    }

    fn rerank(
        &self,
        query: &str,
        candidates: Vec<RerankCandidate>,
    ) -> Result<Option<RerankResult>, PluginError> {
        let Some(fn_name) = &self.rerank_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let query = query.to_string();
        self.execute_with_timeout("rerank", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get rerank function: {e}"))
            })?;
            let candidates_table = lua
                .create_table()
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            for (i, c) in candidates.iter().enumerate() {
                let t = lua
                    .create_table()
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                t.set("id", c.id.as_str())
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                t.set("content", c.content.as_str())
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                t.set("file_path", c.file_path.as_str())
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                t.set("initial_score", c.initial_score)
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                if let Some(et) = &c.entity_type {
                    t.set("entity_type", et.as_str())
                        .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                }
                let md = lua
                    .create_table()
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                for (k, v) in &c.metadata {
                    md.set(k.as_str(), v.as_str())
                        .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                }
                t.set("metadata", md)
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                candidates_table
                    .set(i + 1, t)
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            }
            let query_val = lua
                .create_string(&query)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func
                .call((query_val, candidates_table))
                .map_err(|e| PluginError::ScriptError(format!("rerank error: {e}")))?;
            match result {
                mlua::Value::Table(t) => {
                    let rerank_result = lua_table_to_rerank_result(&t).map_err(|e| {
                        PluginError::InvalidOutput(format!("Invalid rerank result: {e}"))
                    })?;
                    if rerank_result.reranked_candidates.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(rerank_result))
                    }
                }
                _ => Ok(None),
            }
        })
    }

    // ── Group override tier ───────────────────────────────────────────

    fn supports_group_override(&self) -> bool {
        self.group_fn_name.is_some()
    }

    fn group(&self, context: GroupPluginContext) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        let Some(fn_name) = &self.group_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        self.execute_with_timeout("group", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get group function: {e}"))
            })?;
            let context_table = group_plugin_context_to_lua_table(lua, &context).map_err(|e| {
                PluginError::InvalidOutput(format!("Failed to convert context: {e}"))
            })?;
            let result: mlua::Value = func
                .call(context_table)
                .map_err(|e| PluginError::ScriptError(format!("group error: {e}")))?;
            let result_table = match result {
                mlua::Value::Table(t) => t,
                _ => return Ok(None),
            };
            let mut out = Vec::new();
            for pair in result_table.pairs::<mlua::Value, mlua::Value>() {
                let (_, value) = pair.map_err(|e| {
                    PluginError::ScriptError(format!("group result iteration error: {e}"))
                })?;
                if let mlua::Value::Table(group_table) = value {
                    let group = lua_table_to_entity_group(
                        &group_table,
                        cce_types::grouper::EntityGroup::default(),
                    )
                    .map_err(|e| {
                        PluginError::InvalidOutput(format!("Invalid group override result: {e}"))
                    })?;
                    out.push(group);
                }
            }
            if out.is_empty() {
                Ok(None)
            } else {
                Ok(Some(out))
            }
        })
    }

    // ── RelationExtract ───────────────────────────────────────────────

    fn supports_relation_extract(&self) -> bool {
        self.extract_symbols_fn_name.is_some() || self.extract_relations_fn_name.is_some()
    }

    fn extract_symbols(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<PluginSymbol>>, PluginError> {
        let Some(fn_name) = &self.extract_symbols_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let content = content.to_string();
        let file_path = file_path.to_string();
        let language = language.to_string();
        let plugin_id = self.metadata.id.clone();
        self.execute_with_timeout("extract_symbols", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get extract_symbols function: {e}"))
            })?;
            let content_val = lua
                .create_string(&content)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let path_val = lua
                .create_string(&file_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let lang_val = lua
                .create_string(&language)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value =
                func.call((content_val, path_val, lang_val)).map_err(|e| {
                    warn!("Lua plugin {plugin_id} extract_symbols failed: {e}");
                    PluginError::ScriptError(format!("extract_symbols error: {e}"))
                })?;
            match result {
                mlua::Value::Table(t) => {
                    let symbols = crate::lua_mapping::lua_table_to_plugin_symbols(&t)
                        .map_err(|e| PluginError::InvalidOutput(format!("Invalid symbols: {e}")))?;
                    if symbols.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(symbols))
                    }
                }
                _ => Ok(None),
            }
        })
    }

    fn extract_relations(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<PluginRelation>>, PluginError> {
        let Some(fn_name) = &self.extract_relations_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let content = content.to_string();
        let file_path = file_path.to_string();
        let language = language.to_string();
        let plugin_id = self.metadata.id.clone();
        self.execute_with_timeout("extract_relations", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get extract_relations function: {e}"))
            })?;
            let content_val = lua
                .create_string(&content)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let path_val = lua
                .create_string(&file_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let lang_val = lua
                .create_string(&language)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value =
                func.call((content_val, path_val, lang_val)).map_err(|e| {
                    warn!("Lua plugin {plugin_id} extract_relations failed: {e}");
                    PluginError::ScriptError(format!("extract_relations error: {e}"))
                })?;
            match result {
                mlua::Value::Table(t) => {
                    let relations =
                        crate::lua_mapping::lua_table_to_plugin_relations(&t).map_err(|e| {
                            PluginError::InvalidOutput(format!("Invalid relations: {e}"))
                        })?;
                    if relations.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(relations))
                    }
                }
                _ => Ok(None),
            }
        })
    }

    // ── SymbolExtract ─────────────────────────────────────────────────

    fn supports_symbol_extract(&self) -> bool {
        self.extract_imports_fn_name.is_some() || self.extract_exports_fn_name.is_some()
    }

    fn extract_imports(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<PluginImport>>, PluginError> {
        let Some(fn_name) = &self.extract_imports_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let content = content.to_string();
        let file_path = file_path.to_string();
        let language = language.to_string();
        let plugin_id = self.metadata.id.clone();
        self.execute_with_timeout("extract_imports", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get extract_imports function: {e}"))
            })?;
            let content_val = lua
                .create_string(&content)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let path_val = lua
                .create_string(&file_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let lang_val = lua
                .create_string(&language)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value =
                func.call((content_val, path_val, lang_val)).map_err(|e| {
                    warn!("Lua plugin {plugin_id} extract_imports failed: {e}");
                    PluginError::ScriptError(format!("extract_imports error: {e}"))
                })?;
            match result {
                mlua::Value::Table(t) => {
                    let imports = crate::lua_mapping::lua_table_to_plugin_imports(&t)
                        .map_err(|e| PluginError::InvalidOutput(format!("Invalid imports: {e}")))?;
                    if imports.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(imports))
                    }
                }
                _ => Ok(None),
            }
        })
    }

    fn extract_exports(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<PluginExport>>, PluginError> {
        let Some(fn_name) = &self.extract_exports_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let content = content.to_string();
        let file_path = file_path.to_string();
        let language = language.to_string();
        let plugin_id = self.metadata.id.clone();
        self.execute_with_timeout("extract_exports", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get extract_exports function: {e}"))
            })?;
            let content_val = lua
                .create_string(&content)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let path_val = lua
                .create_string(&file_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let lang_val = lua
                .create_string(&language)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value =
                func.call((content_val, path_val, lang_val)).map_err(|e| {
                    warn!("Lua plugin {plugin_id} extract_exports failed: {e}");
                    PluginError::ScriptError(format!("extract_exports error: {e}"))
                })?;
            match result {
                mlua::Value::Table(t) => {
                    let exports = crate::lua_mapping::lua_table_to_plugin_exports(&t)
                        .map_err(|e| PluginError::InvalidOutput(format!("Invalid exports: {e}")))?;
                    if exports.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(exports))
                    }
                }
                _ => Ok(None),
            }
        })
    }

    // ── QueryRewrite ──────────────────────────────────────────────────

    fn supports_query_rewrite(&self) -> bool {
        self.rewrite_query_fn_name.is_some()
    }

    fn rewrite_query(&self, query: &str) -> Result<Option<QueryRewriteResult>, PluginError> {
        let Some(fn_name) = &self.rewrite_query_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let query = query.to_string();
        self.execute_with_timeout("rewrite_query", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get rewrite_query function: {e}"))
            })?;
            let query_val = lua
                .create_string(&query)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func
                .call(query_val)
                .map_err(|e| PluginError::ScriptError(format!("rewrite_query error: {e}")))?;
            match result {
                mlua::Value::Table(t) => {
                    let rw: QueryRewriteResult = t
                        .get("rewritten_query")
                        .ok()
                        .map(|s: String| QueryRewriteResult {
                            rewritten_query: s,
                            expansion_terms: t
                                .get::<Option<Vec<String>>>("expansion_terms")
                                .ok()
                                .flatten()
                                .unwrap_or_default(),
                        })
                        .ok_or_else(|| {
                            PluginError::InvalidOutput(
                                "rewrite_query missing rewritten_query".into(),
                            )
                        })?;
                    if rw.rewritten_query.trim().is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(rw))
                    }
                }
                _ => Ok(None),
            }
        })
    }

    // ── Fusion ────────────────────────────────────────────────────────

    fn supports_fusion(&self) -> bool {
        self.fusion_weights_fn_name.is_some()
    }

    fn fusion_weights(
        &self,
        query: &str,
        vector_count: usize,
        bm25_count: usize,
    ) -> Result<Option<FusionWeights>, PluginError> {
        let Some(fn_name) = &self.fusion_weights_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let query = query.to_string();
        self.execute_with_timeout("fusion_weights", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get fusion_weights function: {e}"))
            })?;
            let query_val = lua
                .create_string(&query)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func
                .call((query_val, vector_count as i64, bm25_count as i64))
                .map_err(|e| PluginError::ScriptError(format!("fusion_weights error: {e}")))?;
            match result {
                mlua::Value::Table(t) => {
                    let weights = FusionWeights {
                        vector_weight: t.get("vector_weight").ok().flatten(),
                        bm25_weight: t.get("bm25_weight").ok().flatten(),
                        min_score: t.get("min_score").ok().flatten(),
                    };
                    if weights.vector_weight.is_none()
                        && weights.bm25_weight.is_none()
                        && weights.min_score.is_none()
                    {
                        Ok(None)
                    } else {
                        Ok(Some(weights))
                    }
                }
                _ => Ok(None),
            }
        })
    }

    // ── ResultFilter ──────────────────────────────────────────────────

    fn supports_result_filter(&self) -> bool {
        self.filter_results_fn_name.is_some()
    }

    fn filter_results(
        &self,
        query: &str,
        results: Vec<RerankCandidate>,
    ) -> Result<Option<Vec<ResultFilterEntry>>, PluginError> {
        let Some(fn_name) = &self.filter_results_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let query = query.to_string();
        self.execute_with_timeout("filter_results", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get filter_results function: {e}"))
            })?;
            let candidates_table = lua
                .create_table()
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            for (i, c) in results.iter().enumerate() {
                let t = lua
                    .create_table()
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                t.set("id", c.id.as_str())
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                t.set("content", c.content.as_str())
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                t.set("file_path", c.file_path.as_str())
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                t.set("initial_score", c.initial_score)
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
                candidates_table
                    .set(i + 1, t)
                    .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            }
            let query_val = lua
                .create_string(&query)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func
                .call((query_val, candidates_table))
                .map_err(|e| PluginError::ScriptError(format!("filter_results error: {e}")))?;
            match result {
                mlua::Value::Table(t) => {
                    let entries =
                        crate::lua_mapping::lua_table_to_filter_entries(&t).map_err(|e| {
                            PluginError::InvalidOutput(format!("Invalid filter entries: {e}"))
                        })?;
                    if entries.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(entries))
                    }
                }
                _ => Ok(None),
            }
        })
    }

    // ── FileFilter ────────────────────────────────────────────────────

    fn supports_file_filter(&self) -> bool {
        self.filter_file_fn_name.is_some()
    }

    fn filter_file(
        &self,
        file_path: &str,
        is_directory: bool,
        size: u64,
    ) -> Result<Option<FileFilterDecision>, PluginError> {
        let Some(fn_name) = &self.filter_file_fn_name else {
            return Ok(None);
        };
        let fn_name = fn_name.clone();
        let file_path = file_path.to_string();
        self.execute_with_timeout("filter_file", move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get filter_file function: {e}"))
            })?;
            let path_val = lua
                .create_string(&file_path)
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let result: mlua::Value = func
                .call((path_val, is_directory, size as i64))
                .map_err(|e| PluginError::ScriptError(format!("filter_file error: {e}")))?;
            match result {
                mlua::Value::String(s) => match s.to_string_lossy().as_ref() {
                    "include" => Ok(Some(FileFilterDecision::Include)),
                    "exclude" => Ok(Some(FileFilterDecision::Exclude)),
                    _ => Ok(None),
                },
                _ => Ok(None),
            }
        })
    }
}
