use std::ffi::{CStr, CString};

use crate::error::PluginError;
use crate::types::PluginMetadata;
use cce_plugin::CodePlugin;
use cce_types::grouper::EntityGroup;

use super::ffi_helpers::SendPtr;
use super::ffi_helpers::{
    PluginStringFn, PluginStringFn2, PluginStringFn3, call_owned_string_fn1, call_plugin_string,
    call_plugin_string2, call_plugin_string3, parse_ffi_json_result,
};
use super::native_plugin::NativePlugin;

impl CodePlugin for NativePlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn supports_bm25(&self) -> bool {
        (self.generate_bm25_fn.is_some() || self.generate_bm25_batch_fn.is_some()) && self.has_bm25
    }

    fn supports_embedding(&self) -> bool {
        (self.generate_embedding_fn.is_some() || self.generate_embedding_batch_fn.is_some())
            && self.has_embedding
    }

    fn supports_parse(&self) -> bool {
        self.parse_document_fn.is_some() && self.has_parse
    }

    fn supports_extract(&self) -> bool {
        self.extract_entities_fn.is_some() && self.has_extract
    }

    fn supports_group(&self) -> bool {
        self.post_group_fn.is_some() && self.has_group
    }

    fn supports_group_override(&self) -> bool {
        self.group_fn.is_some() && self.has_group_override
    }

    fn supports_chunk(&self) -> bool {
        self.chunk_fn.is_some() && self.has_chunk
    }

    fn supports_rerank(&self) -> bool {
        self.rerank_fn.is_some() && self.has_rerank
    }

    fn supports_ast_language(&self) -> bool {
        self.tree_sitter_language_fn.is_some() && self.has_ast_language
    }

    fn supports_language_remap(&self) -> bool {
        self.has_language_remap
            && self.remap_grammar_language_fn.is_some()
            && self.remap_grammar_language.is_some()
    }

    fn supports_relation_extract(&self) -> bool {
        (self.extract_symbols_fn.is_some() || self.extract_relations_fn.is_some())
            && self.has_relation_extract
    }

    fn supports_symbol_extract(&self) -> bool {
        (self.extract_imports_fn.is_some() || self.extract_exports_fn.is_some())
            && self.has_symbol_extract
    }

    fn supports_query_rewrite(&self) -> bool {
        self.rewrite_query_fn.is_some() && self.has_query_rewrite
    }

    fn supports_fusion(&self) -> bool {
        self.fusion_weights_fn.is_some() && self.has_fusion
    }

    fn supports_result_filter(&self) -> bool {
        self.filter_results_fn.is_some() && self.has_result_filter
    }

    fn supports_file_filter(&self) -> bool {
        self.filter_file_fn.is_some() && self.has_file_filter
    }

    fn tree_sitter_language(&self) -> Option<*const std::ffi::c_void> {
        let f = self.tree_sitter_language_fn?;
        if !self.has_ast_language {
            return None;
        }
        // SAFETY: `f` is a valid function pointer from the same library.
        let ptr = unsafe { f() };
        if ptr.is_null() { None } else { Some(ptr) }
    }

    fn query_scheme(&self, query_type: cce_types::QueryType) -> Option<String> {
        self.query_schemes.get(&query_type).cloned()
    }

    fn language_name(&self) -> Option<String> {
        self.language_name.clone()
    }

    fn language_extensions(&self) -> Vec<String> {
        self.language_extensions.clone()
    }

    fn remap_grammar_language(&self) -> Option<String> {
        if self.supports_language_remap() {
            self.remap_grammar_language.clone()
        } else {
            None
        }
    }

    // ── LangHeuristics ───────────────────────────────────────────────

    fn supports_any_heuristic(&self) -> bool {
        self.has_stdlib_heuristic || self.has_test_file_heuristic || self.has_entity_kind_heuristic
    }

    fn supports_stdlib_heuristic(&self) -> bool {
        self.classify_stdlib_fn.is_some() && self.has_stdlib_heuristic
    }

    fn classify_stdlib(&self, module_path: &str) -> Result<Option<String>, PluginError> {
        let Some(func) = self.classify_stdlib_fn else {
            return Ok(None);
        };
        if !self.has_stdlib_heuristic {
            return Ok(None);
        }
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        let module_path = module_path.to_string();
        self.execute_with_timeout("classify_stdlib", move || {
            let c_module = std::ffi::CString::new(module_path.as_str()).map_err(|e| {
                PluginError::InvalidOutput(format!("module_path contains NUL: {e}"))
            })?;
            // SAFETY: `func` is a valid function pointer from the same
            // library; `call_owned_string_fn1` reads and frees its output.
            unsafe {
                call_owned_string_fn1(|ctx| func(ctx, c_module.as_ptr()), free_string_fn, ctx)
            }
        })
    }

    fn supports_test_file_heuristic(&self) -> bool {
        self.is_test_file_fn.is_some() && self.has_test_file_heuristic
    }

    fn is_test_file(&self, file_path: &str, content: &str) -> Result<Option<bool>, PluginError> {
        let Some(func) = self.is_test_file_fn else {
            return Ok(None);
        };
        if !self.has_test_file_heuristic {
            return Ok(None);
        }
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        let file_path = file_path.to_string();
        let content = content.to_string();
        self.execute_with_timeout("is_test_file", move || {
            let c_path = std::ffi::CString::new(file_path.as_str())
                .map_err(|e| PluginError::InvalidOutput(format!("file_path contains NUL: {e}")))?;
            let c_content = std::ffi::CString::new(content.as_str())
                .map_err(|e| PluginError::InvalidOutput(format!("content contains NUL: {e}")))?;
            // SAFETY: same contract as `call_owned_string_fn1` (two-arg form).
            let raw = unsafe {
                let ret_ptr = func(
                    ctx.map(|c| c.0).unwrap_or(std::ptr::null_mut()),
                    c_path.as_ptr(),
                    c_content.as_ptr(),
                );
                if ret_ptr.is_null() {
                    return Ok(None);
                }
                let c_str = std::ffi::CStr::from_ptr(ret_ptr);
                let owned = c_str
                    .to_str()
                    .map_err(|_| {
                        PluginError::InvalidOutput("Plugin returned non-UTF-8 string".to_string())
                    })?
                    .to_string();
                free_string_fn(ret_ptr);
                Some(owned)
            };
            match raw.as_deref() {
                Some("true") => Ok(Some(true)),
                Some("false") => Ok(Some(false)),
                _ => Ok(None),
            }
        })
    }

    fn supports_entity_kind_heuristic(&self) -> bool {
        self.entity_kind_fn.is_some() && self.has_entity_kind_heuristic
    }

    fn entity_kind(&self, query_capture_name: &str) -> Result<Option<String>, PluginError> {
        let Some(func) = self.entity_kind_fn else {
            return Ok(None);
        };
        if !self.has_entity_kind_heuristic {
            return Ok(None);
        }
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        let capture = query_capture_name.to_string();
        self.execute_with_timeout("entity_kind", move || {
            let c_capture = std::ffi::CString::new(capture.as_str())
                .map_err(|e| PluginError::InvalidOutput(format!("capture contains NUL: {e}")))?;
            // SAFETY: `func` is a valid function pointer from the same library.
            unsafe {
                call_owned_string_fn1(|ctx| func(ctx, c_capture.as_ptr()), free_string_fn, ctx)
            }
        })
    }

    fn generate_bm25(&self, group: &EntityGroup) -> Result<Option<String>, PluginError> {
        if !self.supports_bm25() {
            return Ok(None);
        }
        let group_json = serde_json::to_string(group).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to serialize EntityGroup: {}", e))
        })?;
        // Prefer the dedicated single entry point.
        if let Some(func) = self.generate_bm25_fn {
            return self.call_single_fn(func, "generate_bm25", group_json, "BM25");
        }
        // Fall back to the batch entry point with a single-element array.
        if let Some(func) = self.generate_bm25_batch_fn {
            let arr = format!("[{group_json}]");
            let mut results = self.call_batch_fn(func, "generate_bm25_batch", arr, 1, "BM25")?;
            return Ok(results.pop().unwrap_or(None));
        }
        Ok(None)
    }

    fn generate_embedding(&self, group: &EntityGroup) -> Result<Option<String>, PluginError> {
        if !self.supports_embedding() {
            return Ok(None);
        }
        let group_json = serde_json::to_string(group).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to serialize EntityGroup: {}", e))
        })?;
        if let Some(func) = self.generate_embedding_fn {
            return self.call_single_fn(func, "generate_embedding", group_json, "embedding");
        }
        if let Some(func) = self.generate_embedding_batch_fn {
            let arr = format!("[{group_json}]");
            let mut results =
                self.call_batch_fn(func, "generate_embedding_batch", arr, 1, "embedding")?;
            return Ok(results.pop().unwrap_or(None));
        }
        Ok(None)
    }

    fn generate_bm25_batch(
        &self,
        groups: &[&EntityGroup],
    ) -> Result<Vec<Option<String>>, PluginError> {
        if !self.supports_bm25() {
            return Ok(vec![None; groups.len()]);
        }
        if let Some(func) = self.generate_bm25_batch_fn {
            let groups_json = serde_json::to_string(groups).map_err(|e| {
                PluginError::InvalidOutput(format!("Failed to serialize EntityGroup batch: {}", e))
            })?;
            return self.call_batch_fn(
                func,
                "generate_bm25_batch",
                groups_json,
                groups.len(),
                "BM25",
            );
        }
        // No batch entry point: fall back to per-group calls.
        let mut results = Vec::with_capacity(groups.len());
        for group in groups {
            results.push(self.generate_bm25(group)?);
        }
        Ok(results)
    }

    fn generate_embedding_batch(
        &self,
        groups: &[&EntityGroup],
    ) -> Result<Vec<Option<String>>, PluginError> {
        if !self.supports_embedding() {
            return Ok(vec![None; groups.len()]);
        }
        if let Some(func) = self.generate_embedding_batch_fn {
            let groups_json = serde_json::to_string(groups).map_err(|e| {
                PluginError::InvalidOutput(format!("Failed to serialize EntityGroup batch: {}", e))
            })?;
            return self.call_batch_fn(
                func,
                "generate_embedding_batch",
                groups_json,
                groups.len(),
                "embedding",
            );
        }
        // No batch entry point: fall back to per-group calls.
        let mut results = Vec::with_capacity(groups.len());
        for group in groups {
            results.push(self.generate_embedding(group)?);
        }
        Ok(results)
    }

    // ── FormatParse ───────────────────────────────────────────────────

    fn parse_document(
        &self,
        content: &str,
        file_path: &str,
    ) -> Result<Option<cce_types::PluginDocument>, PluginError> {
        if !self.supports_parse() {
            return Ok(None);
        }
        let Some(func) = self.parse_document_fn else {
            return Ok(None);
        };
        let value = self.call_ffi_string2(
            func,
            "parse_document",
            content.to_string(),
            file_path.to_string(),
        )?;
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| PluginError::InvalidOutput(format!("Invalid PluginDocument: {e}"))),
            None => Ok(None),
        }
    }

    // ── EntityExtract ─────────────────────────────────────────────────

    fn extract_entities(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<cce_types::PluginEntity>>, PluginError> {
        if !self.supports_extract() {
            return Ok(None);
        }
        let Some(func) = self.extract_entities_fn else {
            return Ok(None);
        };
        let value = self.call_ffi_string3(
            func,
            "extract_entities",
            content.to_string(),
            file_path.to_string(),
            language.to_string(),
        )?;
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| PluginError::InvalidOutput(format!("Invalid PluginEntity list: {e}"))),
            None => Ok(None),
        }
    }

    // ── Group ─────────────────────────────────────────────────────────

    fn post_group(
        &self,
        groups: Vec<EntityGroup>,
        context: cce_types::plugin::GroupPluginContext,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        if !self.supports_group() {
            return Ok(None);
        }
        let Some(func) = self.post_group_fn else {
            return Ok(None);
        };
        let groups_json = serde_json::to_string(&groups).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to serialize EntityGroup list: {e}"))
        })?;
        let context_json = serde_json::to_string(&context).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to serialize GroupPluginContext: {e}"))
        })?;
        let value = self.call_ffi_string2(func, "post_group", groups_json, context_json)?;
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| PluginError::InvalidOutput(format!("Invalid EntityGroup list: {e}"))),
            None => Ok(None),
        }
    }

    // ── Chunk ─────────────────────────────────────────────────────────

    fn chunk(
        &self,
        conversions: Vec<cce_types::GroupConversions>,
        file_path: &str,
    ) -> Result<Option<Vec<cce_types::ChunkedResult>>, PluginError> {
        if !self.supports_chunk() {
            return Ok(None);
        }
        let Some(func) = self.chunk_fn else {
            return Ok(None);
        };
        let conversions_json = serde_json::to_string(&conversions).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to serialize GroupConversions: {e}"))
        })?;
        let value =
            self.call_ffi_string2(func, "chunk", conversions_json, file_path.to_string())?;
        match value {
            Some(v) => serde_json::from_value(v).map(Some).map_err(|e| {
                PluginError::InvalidOutput(format!("Invalid ChunkedResult list: {e}"))
            }),
            None => Ok(None),
        }
    }

    // ── Rerank ────────────────────────────────────────────────────────

    fn rerank(
        &self,
        query: &str,
        candidates: Vec<cce_types::RerankCandidate>,
    ) -> Result<Option<cce_types::RerankResult>, PluginError> {
        if !self.supports_rerank() {
            return Ok(None);
        }
        let Some(func) = self.rerank_fn else {
            return Ok(None);
        };
        let candidates_json = serde_json::to_string(&candidates).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to serialize RerankCandidate list: {e}"))
        })?;
        let value = self.call_ffi_string2(func, "rerank", query.to_string(), candidates_json)?;
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| PluginError::InvalidOutput(format!("Invalid RerankResult: {e}"))),
            None => Ok(None),
        }
    }

    // ── Group override tier ───────────────────────────────────────────

    fn group(
        &self,
        context: cce_types::plugin::GroupPluginContext,
    ) -> Result<Option<Vec<EntityGroup>>, PluginError> {
        if !self.supports_group_override() {
            return Ok(None);
        }
        let Some(func) = self.group_fn else {
            return Ok(None);
        };
        let context_json = serde_json::to_string(&context).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to serialize GroupPluginContext: {e}"))
        })?;
        let value = self.call_ffi_string(func, "group", context_json)?;
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| PluginError::InvalidOutput(format!("Invalid EntityGroup list: {e}"))),
            None => Ok(None),
        }
    }

    // ── RelationExtract ───────────────────────────────────────────────

    fn extract_symbols(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<cce_types::plugin::PluginSymbol>>, PluginError> {
        if !self.supports_relation_extract() {
            return Ok(None);
        }
        let Some(func) = self.extract_symbols_fn else {
            return Ok(None);
        };
        let value = self.call_ffi_string3(
            func,
            "extract_symbols",
            content.to_string(),
            file_path.to_string(),
            language.to_string(),
        )?;
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| PluginError::InvalidOutput(format!("Invalid PluginSymbol list: {e}"))),
            None => Ok(None),
        }
    }

    fn extract_relations(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<cce_types::plugin::PluginRelation>>, PluginError> {
        if !self.supports_relation_extract() {
            return Ok(None);
        }
        let Some(func) = self.extract_relations_fn else {
            return Ok(None);
        };
        let value = self.call_ffi_string3(
            func,
            "extract_relations",
            content.to_string(),
            file_path.to_string(),
            language.to_string(),
        )?;
        match value {
            Some(v) => serde_json::from_value(v).map(Some).map_err(|e| {
                PluginError::InvalidOutput(format!("Invalid PluginRelation list: {e}"))
            }),
            None => Ok(None),
        }
    }

    // ── SymbolExtract ─────────────────────────────────────────────────

    fn extract_imports(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<cce_types::plugin::PluginImport>>, PluginError> {
        if !self.supports_symbol_extract() {
            return Ok(None);
        }
        let Some(func) = self.extract_imports_fn else {
            return Ok(None);
        };
        let value = self.call_ffi_string3(
            func,
            "extract_imports",
            content.to_string(),
            file_path.to_string(),
            language.to_string(),
        )?;
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| PluginError::InvalidOutput(format!("Invalid PluginImport list: {e}"))),
            None => Ok(None),
        }
    }

    fn extract_exports(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Option<Vec<cce_types::plugin::PluginExport>>, PluginError> {
        if !self.supports_symbol_extract() {
            return Ok(None);
        }
        let Some(func) = self.extract_exports_fn else {
            return Ok(None);
        };
        let value = self.call_ffi_string3(
            func,
            "extract_exports",
            content.to_string(),
            file_path.to_string(),
            language.to_string(),
        )?;
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| PluginError::InvalidOutput(format!("Invalid PluginExport list: {e}"))),
            None => Ok(None),
        }
    }

    // ── QueryRewrite ──────────────────────────────────────────────────

    fn rewrite_query(
        &self,
        query: &str,
    ) -> Result<Option<cce_types::plugin::QueryRewriteResult>, PluginError> {
        if !self.supports_query_rewrite() {
            return Ok(None);
        }
        let Some(func) = self.rewrite_query_fn else {
            return Ok(None);
        };
        let value = self.call_ffi_string(func, "rewrite_query", query.to_string())?;
        match value {
            Some(v) => serde_json::from_value(v).map(Some).map_err(|e| {
                PluginError::InvalidOutput(format!("Invalid QueryRewriteResult: {e}"))
            }),
            None => Ok(None),
        }
    }

    // ── Fusion ────────────────────────────────────────────────────────

    fn fusion_weights(
        &self,
        query: &str,
        vector_count: usize,
        bm25_count: usize,
    ) -> Result<Option<cce_types::plugin::FusionWeights>, PluginError> {
        if !self.supports_fusion() {
            return Ok(None);
        }
        let Some(func) = self.fusion_weights_fn else {
            return Ok(None);
        };
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        let query = query.to_string();
        let value = self.execute_with_timeout("fusion_weights", move || {
            let c_query = CString::new(query.as_str())
                .map_err(|_| PluginError::InvalidOutput("Query contains null byte".to_string()))?;
            let ctx_ptr = ctx.map(|c| c.0).unwrap_or(std::ptr::null_mut());
            // SAFETY: `func` is a valid function pointer from the same library.
            let ret_ptr = unsafe { func(ctx_ptr, c_query.as_ptr(), vector_count, bm25_count) };
            if ret_ptr.is_null() {
                return Err(PluginError::ScriptError(
                    "Plugin returned null pointer".to_string(),
                ));
            }
            let c_str = unsafe { CStr::from_ptr(ret_ptr) };
            let json_str = c_str.to_str().map_err(|_| {
                PluginError::InvalidOutput("Plugin returned non-UTF-8 string".to_string())
            })?;
            let owned = json_str.to_string();
            // SAFETY: `ret_ptr` was allocated by the plugin; `free_string_fn`
            // comes from the same library.
            unsafe { free_string_fn(ret_ptr) };
            parse_ffi_json_result(&owned)
        })?;
        match value {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|e| PluginError::InvalidOutput(format!("Invalid FusionWeights: {e}"))),
            None => Ok(None),
        }
    }

    // ── ResultFilter ──────────────────────────────────────────────────

    fn filter_results(
        &self,
        query: &str,
        results: Vec<cce_types::RerankCandidate>,
    ) -> Result<Option<Vec<cce_types::plugin::ResultFilterEntry>>, PluginError> {
        if !self.supports_result_filter() {
            return Ok(None);
        }
        let Some(func) = self.filter_results_fn else {
            return Ok(None);
        };
        let results_json = serde_json::to_string(&results).map_err(|e| {
            PluginError::InvalidOutput(format!("Failed to serialize RerankCandidate list: {e}"))
        })?;
        let value =
            self.call_ffi_string2(func, "filter_results", query.to_string(), results_json)?;
        match value {
            Some(v) => serde_json::from_value(v).map(Some).map_err(|e| {
                PluginError::InvalidOutput(format!("Invalid ResultFilterEntry list: {e}"))
            }),
            None => Ok(None),
        }
    }

    // ── FileFilter ────────────────────────────────────────────────────

    fn filter_file(
        &self,
        file_path: &str,
        is_directory: bool,
        size: u64,
    ) -> Result<Option<cce_types::plugin::FileFilterDecision>, PluginError> {
        if !self.supports_file_filter() {
            return Ok(None);
        }
        let Some(func) = self.filter_file_fn else {
            return Ok(None);
        };
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        let file_path = file_path.to_string();
        let value = self.execute_with_timeout("filter_file", move || {
            let c_path = CString::new(file_path.as_str())
                .map_err(|_| PluginError::InvalidOutput("Path contains null byte".to_string()))?;
            let ctx_ptr = ctx.map(|c| c.0).unwrap_or(std::ptr::null_mut());
            // SAFETY: `func` is a valid function pointer from the same library.
            let ret_ptr = unsafe { func(ctx_ptr, c_path.as_ptr(), is_directory, size) };
            if ret_ptr.is_null() {
                return Err(PluginError::ScriptError(
                    "Plugin returned null pointer".to_string(),
                ));
            }
            let c_str = unsafe { CStr::from_ptr(ret_ptr) };
            let json_str = c_str.to_str().map_err(|_| {
                PluginError::InvalidOutput("Plugin returned non-UTF-8 string".to_string())
            })?;
            let owned = json_str.to_string();
            // SAFETY: `ret_ptr` was allocated by the plugin; `free_string_fn`
            // comes from the same library.
            unsafe { free_string_fn(ret_ptr) };
            parse_ffi_json_result(&owned)
        })?;
        match value {
            Some(v) => serde_json::from_value(v).map(Some).map_err(|e| {
                PluginError::InvalidOutput(format!("Invalid FileFilterDecision: {e}"))
            }),
            None => Ok(None),
        }
    }
}

impl NativePlugin {
    /// Call a single-argument FFI function and parse the JSON envelope value.
    fn call_ffi_string(
        &self,
        func: PluginStringFn,
        operation: &str,
        arg: String,
    ) -> Result<Option<serde_json::Value>, PluginError> {
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        self.execute_with_timeout(operation, move || {
            let json_str = unsafe { call_plugin_string(func, free_string_fn, ctx, &arg) }?;
            parse_ffi_json_result(&json_str)
        })
    }

    /// Call a two-argument FFI function and parse the JSON envelope value.
    fn call_ffi_string2(
        &self,
        func: PluginStringFn2,
        operation: &str,
        arg1: String,
        arg2: String,
    ) -> Result<Option<serde_json::Value>, PluginError> {
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        self.execute_with_timeout(operation, move || {
            let json_str = unsafe { call_plugin_string2(func, free_string_fn, ctx, &arg1, &arg2) }?;
            parse_ffi_json_result(&json_str)
        })
    }

    /// Call a three-argument FFI function and parse the JSON envelope value.
    fn call_ffi_string3(
        &self,
        func: PluginStringFn3,
        operation: &str,
        arg1: String,
        arg2: String,
        arg3: String,
    ) -> Result<Option<serde_json::Value>, PluginError> {
        let free_string_fn = self.free_string_fn;
        let ctx = self.context.as_ref().map(|c| SendPtr(c.0));
        self.execute_with_timeout(operation, move || {
            let json_str =
                unsafe { call_plugin_string3(func, free_string_fn, ctx, &arg1, &arg2, &arg3) }?;
            parse_ffi_json_result(&json_str)
        })
    }
}
