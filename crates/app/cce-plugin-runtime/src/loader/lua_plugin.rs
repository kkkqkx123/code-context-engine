use mlua::{HookTriggers, Lua, Table, VmState};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};
use tracing::warn;

use crate::error::PluginError;
use crate::lua_mapping::entity_group_to_lua_table;
use crate::pattern::{CompiledPattern, PatternDeclaration, compile_patterns};
use crate::types::PluginMetadata;
use crate::utils::CancellationToken;
use cce_metrics::PluginMetrics;
use cce_types::QueryType;
use std::sync::atomic::{AtomicU64, Ordering};

use super::lua_helpers::{capability_label, parse_query_type, read_lua_priority};
use super::lua_vm_pool::LuaVmPool;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_MEMORY_LIMIT_KB: usize = 64 * 1024;
const HOOK_INSTRUCTION_INTERVAL: u32 = 10_000;

static LUA_PLUGIN_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct LuaPlugin {
    pub(crate) metadata: PluginMetadata,
    pub(crate) vm_pool: Arc<LuaVmPool>,
    pub(crate) timeout: Duration,
    pub(crate) memory_limit_kb: usize,
    pub(crate) generate_bm25_fn_name: Option<String>,
    pub(crate) generate_embedding_fn_name: Option<String>,
    pub(crate) generate_bm25_batch_fn_name: Option<String>,
    pub(crate) generate_embedding_batch_fn_name: Option<String>,
    pub(crate) parse_document_fn_name: Option<String>,
    pub(crate) extract_entities_fn_name: Option<String>,
    pub(crate) post_group_fn_name: Option<String>,
    pub(crate) group_fn_name: Option<String>,
    pub(crate) chunk_fn_name: Option<String>,
    pub(crate) rerank_fn_name: Option<String>,
    pub(crate) extract_symbols_fn_name: Option<String>,
    pub(crate) extract_relations_fn_name: Option<String>,
    pub(crate) extract_imports_fn_name: Option<String>,
    pub(crate) extract_exports_fn_name: Option<String>,
    pub(crate) rewrite_query_fn_name: Option<String>,
    pub(crate) fusion_weights_fn_name: Option<String>,
    pub(crate) filter_results_fn_name: Option<String>,
    pub(crate) filter_file_fn_name: Option<String>,
    /// Custom language name (`LanguageRemap`, e.g. "mytpl").
    pub(crate) language_name: Option<String>,
    /// File extensions for the remapped custom language (e.g. ["tplx"]).
    pub(crate) language_extensions: Vec<String>,
    /// Host built-in grammar reference (`LanguageRemap`, e.g. "JavaScript").
    pub(crate) remap_grammar_language: Option<String>,
    /// Query schemes keyed by [`QueryType`] (`plugin.query_schemes`).
    pub(crate) query_schemes: HashMap<QueryType, String>,
    /// `plugin.classify_stdlib(module_path)` (`LangHeuristics`).
    pub(crate) classify_stdlib_fn_name: Option<String>,
    /// `plugin.is_test_file(file_path, content)` (`LangHeuristics`).
    pub(crate) is_test_file_fn_name: Option<String>,
    /// `plugin.entity_kind(capture_name)` (`LangHeuristics`).
    pub(crate) entity_kind_fn_name: Option<String>,
    /// Host-side compiled regex patterns for entity extraction (`plugin.patterns`).
    pub(crate) patterns: Vec<CompiledPattern>,
    pub(crate) metrics: Option<Arc<PluginMetrics>>,
}

impl LuaPlugin {
    pub fn from_script(script: &str) -> Result<Self, PluginError> {
        Self::with_timeout(script, Duration::from_millis(DEFAULT_TIMEOUT_MS))
    }

    pub fn with_timeout(script: &str, timeout: Duration) -> Result<Self, PluginError> {
        Self::with_options(script, timeout, DEFAULT_MEMORY_LIMIT_KB)
    }

    pub fn with_options(
        script: &str,
        timeout: Duration,
        memory_limit_kb: usize,
    ) -> Result<Self, PluginError> {
        // Validate the script by loading it into a temporary VM.
        // The pool will create its own states lazily on first use.
        let lua = LuaVmPool::create(script)?;

        let plugin_table: Table = lua
            .globals()
            .get("plugin")
            .map_err(|e| PluginError::ScriptError(e.to_string()))?;

        let id: String = plugin_table.get("id").unwrap_or_else(|_| {
            let n = LUA_PLUGIN_COUNTER.fetch_add(1, Ordering::SeqCst);
            format!("lua_plugin_{n}")
        });
        let name: String = plugin_table.get("name").unwrap_or_else(|_| id.clone());
        let version: String = plugin_table
            .get("version")
            .unwrap_or_else(|_| "0.1.0".to_string());
        let priority: i32 = read_lua_priority(&plugin_table)?;
        let description: Option<String> = plugin_table.get("description").ok();
        let capabilities: Vec<String> = plugin_table
            .get::<Option<Vec<String>>>("capabilities")
            .ok()
            .flatten()
            .unwrap_or_default();
        let capability_priorities: std::collections::HashMap<String, i32> = plugin_table
            .get::<Option<std::collections::HashMap<String, i32>>>("capability_priorities")
            .ok()
            .flatten()
            .unwrap_or_default();

        let metadata = PluginMetadata {
            id,
            name,
            version,
            priority,
            capability_priorities,
            description,
            capabilities,
        };

        let generate_bm25_fn_name = plugin_table
            .get::<Option<mlua::Function>>("generate_bm25")
            .ok()
            .flatten()
            .map(|_| "generate_bm25".to_string());

        let generate_embedding_fn_name = plugin_table
            .get::<Option<mlua::Function>>("generate_embedding")
            .ok()
            .flatten()
            .map(|_| "generate_embedding".to_string());

        let generate_bm25_batch_fn_name = plugin_table
            .get::<Option<mlua::Function>>("generate_bm25_batch")
            .ok()
            .flatten()
            .map(|_| "generate_bm25_batch".to_string());

        let generate_embedding_batch_fn_name = plugin_table
            .get::<Option<mlua::Function>>("generate_embedding_batch")
            .ok()
            .flatten()
            .map(|_| "generate_embedding_batch".to_string());

        let parse_document_fn_name = plugin_table
            .get::<Option<mlua::Function>>("parse_document")
            .ok()
            .flatten()
            .map(|_| "parse_document".to_string());

        let extract_entities_fn_name = plugin_table
            .get::<Option<mlua::Function>>("extract_entities")
            .ok()
            .flatten()
            .map(|_| "extract_entities".to_string());

        let post_group_fn_name = plugin_table
            .get::<Option<mlua::Function>>("post_group")
            .ok()
            .flatten()
            .map(|_| "post_group".to_string());

        let group_fn_name = plugin_table
            .get::<Option<mlua::Function>>("group")
            .ok()
            .flatten()
            .map(|_| "group".to_string());

        let chunk_fn_name = plugin_table
            .get::<Option<mlua::Function>>("chunk")
            .ok()
            .flatten()
            .map(|_| "chunk".to_string());

        let rerank_fn_name = plugin_table
            .get::<Option<mlua::Function>>("rerank")
            .ok()
            .flatten()
            .map(|_| "rerank".to_string());

        let extract_symbols_fn_name = plugin_table
            .get::<Option<mlua::Function>>("extract_symbols")
            .ok()
            .flatten()
            .map(|_| "extract_symbols".to_string());

        let extract_relations_fn_name = plugin_table
            .get::<Option<mlua::Function>>("extract_relations")
            .ok()
            .flatten()
            .map(|_| "extract_relations".to_string());

        let extract_imports_fn_name = plugin_table
            .get::<Option<mlua::Function>>("extract_imports")
            .ok()
            .flatten()
            .map(|_| "extract_imports".to_string());

        let extract_exports_fn_name = plugin_table
            .get::<Option<mlua::Function>>("extract_exports")
            .ok()
            .flatten()
            .map(|_| "extract_exports".to_string());

        let rewrite_query_fn_name = plugin_table
            .get::<Option<mlua::Function>>("rewrite_query")
            .ok()
            .flatten()
            .map(|_| "rewrite_query".to_string());

        let fusion_weights_fn_name = plugin_table
            .get::<Option<mlua::Function>>("fusion_weights")
            .ok()
            .flatten()
            .map(|_| "fusion_weights".to_string());

        let filter_results_fn_name = plugin_table
            .get::<Option<mlua::Function>>("filter_results")
            .ok()
            .flatten()
            .map(|_| "filter_results".to_string());

        let filter_file_fn_name = plugin_table
            .get::<Option<mlua::Function>>("filter_file")
            .ok()
            .flatten()
            .map(|_| "filter_file".to_string());

        // `LanguageRemap` declarations: `plugin.language_name`,
        // `plugin.language_extensions`, `plugin.remap_grammar_language`,
        // optional `plugin.query_schemes = { entity = "...", ... }`.
        let language_name = plugin_table
            .get::<Option<String>>("language_name")
            .ok()
            .flatten()
            .or_else(|| {
                plugin_table
                    .get::<Option<String>>("language")
                    .ok()
                    .flatten()
            });
        let language_extensions: Vec<String> = plugin_table
            .get::<Option<Vec<String>>>("language_extensions")
            .ok()
            .flatten()
            .or_else(|| {
                plugin_table
                    .get::<Option<Vec<String>>>("extensions")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
        let remap_grammar_language = plugin_table
            .get::<Option<String>>("remap_grammar_language")
            .ok()
            .flatten();

        // `LangHeuristics` function declarations (each independently probed).
        let classify_stdlib_fn_name = plugin_table
            .get::<Option<mlua::Function>>("classify_stdlib")
            .ok()
            .flatten()
            .map(|_| "classify_stdlib".to_string());
        let is_test_file_fn_name = plugin_table
            .get::<Option<mlua::Function>>("is_test_file")
            .ok()
            .flatten()
            .map(|_| "is_test_file".to_string());
        let entity_kind_fn_name = plugin_table
            .get::<Option<mlua::Function>>("entity_kind")
            .ok()
            .flatten()
            .map(|_| "entity_kind".to_string());
        let query_schemes = match plugin_table.get::<Option<Table>>("query_schemes") {
            Ok(Some(t)) => {
                let mut schemes = HashMap::new();
                for (key, value) in t.pairs::<String, String>().filter_map(Result::ok) {
                    if let Some(qt) = parse_query_type(&key) {
                        schemes.insert(qt, value);
                    } else {
                        warn!(
                            plugin = %metadata.name,
                            query_type = %key,
                            "LanguageRemap plugin declares an unknown query type; ignoring it"
                        );
                    }
                }
                schemes
            }
            _ => HashMap::new(),
        };

        // Host-side pattern declarations (`plugin.patterns`).
        let patterns = match plugin_table.get::<Option<Vec<Table>>>("patterns") {
            Ok(Some(pattern_tables)) => {
                let mut decls = Vec::with_capacity(pattern_tables.len());
                for t in &pattern_tables {
                    let name: String = t.get("name").unwrap_or_else(|_| "pattern".to_string());
                    let regex: String = match t.get("regex") {
                        Ok(r) => r,
                        Err(e) => {
                            return Err(PluginError::InvalidOutput(format!(
                                "Pattern '{name}' is missing a valid 'regex': {e}"
                            )));
                        }
                    };
                    let kind: String = t.get("kind").unwrap_or_default();
                    decls.push(PatternDeclaration { name, regex, kind });
                }
                match compile_patterns(&decls) {
                    Ok(compiled) => compiled,
                    Err(e) => {
                        return Err(PluginError::InvalidOutput(format!(
                            "Plugin '{}' declares invalid patterns: {e}",
                            metadata.name
                        )));
                    }
                }
            }
            _ => Vec::new(),
        };

        Ok(Self {
            metadata,
            vm_pool: Arc::new(LuaVmPool::new(Arc::new(script.to_string()))),
            timeout,
            memory_limit_kb,
            generate_bm25_fn_name,
            generate_embedding_fn_name,
            generate_bm25_batch_fn_name,
            generate_embedding_batch_fn_name,
            parse_document_fn_name,
            extract_entities_fn_name,
            post_group_fn_name,
            group_fn_name,
            chunk_fn_name,
            rerank_fn_name,
            extract_symbols_fn_name,
            extract_relations_fn_name,
            extract_imports_fn_name,
            extract_exports_fn_name,
            rewrite_query_fn_name,
            fusion_weights_fn_name,
            filter_results_fn_name,
            filter_file_fn_name,
            language_name,
            language_extensions,
            remap_grammar_language,
            query_schemes,
            classify_stdlib_fn_name,
            is_test_file_fn_name,
            entity_kind_fn_name,
            patterns,
            metrics: None,
        })
    }

    /// Attach an optional metrics sink for execution accounting.
    pub fn with_metrics(mut self, metrics: Arc<PluginMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Set a debug hook that fires every [`HOOK_INSTRUCTION_INTERVAL`]
    /// instructions.  The hook checks the cancellation token and the
    /// current memory usage; if either exceeds its limit the Lua
    /// execution is interrupted with a Lua error which terminates the
    /// worker thread promptly.
    pub(crate) fn set_protection_hook(
        lua: &Lua,
        token: CancellationToken,
        max_kb: usize,
    ) -> Result<(), PluginError> {
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(HOOK_INSTRUCTION_INTERVAL),
            move |l, _| {
                if token.is_cancelled() {
                    return Err(mlua::Error::RuntimeError(
                        "Plugin execution timed out".to_string(),
                    ));
                }
                let used_kb = l.used_memory() / 1024;
                if used_kb > max_kb {
                    return Err(mlua::Error::RuntimeError(format!(
                        "Memory limit exceeded: {used_kb} KB > {max_kb} KB"
                    )));
                }
                Ok(VmState::Continue)
            },
        )
        .map_err(|e| PluginError::ScriptError(format!("Failed to set hook: {e}")))?;
        Ok(())
    }

    /// Run `f` on a Lua VM from the pool, with a hard timeout.
    ///
    /// Lua states are pooled and reused across calls — each state has
    /// the plugin script pre-loaded.  The debug hook fires periodically
    /// to check the cancellation token and memory limit, so the worker
    /// thread terminates shortly after the caller gives up waiting
    /// (rather than running to completion).  After the call finishes
    /// the VM is returned to the pool for reuse.
    pub(crate) fn execute_with_timeout<R>(
        &self,
        operation: &str,
        f: impl FnOnce(&Lua) -> Result<R, PluginError> + Send + 'static,
    ) -> Result<R, PluginError>
    where
        R: Send + 'static,
    {
        let plugin_id = self.metadata.id.clone();
        let start = Instant::now();
        let result = self.run_with_timeout(operation, f);
        if let Some(m) = &self.metrics {
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
            match &result {
                Ok(_) => {
                    m.record_capability_execution(capability_label(operation), latency_ms, true)
                }
                Err(_) => {
                    m.record_capability_execution(capability_label(operation), latency_ms, false);
                    m.record_execution_error(&plugin_id);
                }
            }
        }
        result
    }

    /// Inner execution on a dedicated thread with a hard timeout.
    pub(crate) fn run_with_timeout<R>(
        &self,
        operation: &str,
        f: impl FnOnce(&Lua) -> Result<R, PluginError> + Send + 'static,
    ) -> Result<R, PluginError>
    where
        R: Send + 'static,
    {
        let (tx, rx) = channel::<Result<R, PluginError>>();
        let plugin_id = self.metadata.id.clone();
        let timeout = self.timeout;
        let vm_pool = self.vm_pool.clone();
        let max_kb = self.memory_limit_kb;
        let token = CancellationToken::new();
        let token_for_thread = token.clone();

        std::thread::spawn(move || {
            let lua = match vm_pool.acquire() {
                Ok(lua) => lua,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };

            if let Err(e) = Self::set_protection_hook(&lua, token_for_thread, max_kb) {
                vm_pool.release(lua);
                let _ = tx.send(Err(e));
                return;
            }

            let result = f(&lua);
            vm_pool.release(lua);
            let _ = tx.send(result);
        });

        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                token.cancel();
                warn!(
                    "Plugin {plugin_id} timed out during {operation} ({}ms)",
                    timeout.as_millis()
                );
                Err(PluginError::Timeout)
            }
            Err(RecvTimeoutError::Disconnected) => Err(PluginError::ExecutionFailed(format!(
                "Plugin {plugin_id} thread terminated unexpectedly during {operation}"
            ))),
        }
    }
}

impl LuaPlugin {
    pub(crate) fn call_batch_function(
        &self,
        fn_name: &str,
        groups: &[&cce_types::grouper::EntityGroup],
        operation: &str,
    ) -> Result<Vec<Option<String>>, PluginError> {
        let groups: Vec<cce_types::grouper::EntityGroup> =
            groups.iter().map(|&g| g.clone()).collect();
        let fn_name = fn_name.to_string();
        self.execute_with_timeout(operation, move |lua| {
            let plugin_table: Table = lua
                .globals()
                .get("plugin")
                .map_err(|e| PluginError::ScriptError(e.to_string()))?;
            let func: mlua::Function = plugin_table.get(fn_name.as_str()).map_err(|e| {
                PluginError::LogicError(format!("Failed to get {fn_name} function: {e}"))
            })?;

            let mut group_tables = Vec::with_capacity(groups.len());
            for group in &groups {
                let group_table = entity_group_to_lua_table(lua, group).map_err(|e| {
                    PluginError::InvalidOutput(format!("Failed to convert EntityGroup: {e}"))
                })?;
                group_tables.push(group_table);
            }

            let batch_table = lua.create_table().map_err(|e| {
                PluginError::ScriptError(format!("Failed to create batch table: {e}"))
            })?;
            for (i, group_table) in group_tables.into_iter().enumerate() {
                batch_table.set(i + 1, group_table).map_err(|e| {
                    PluginError::ScriptError(format!("Failed to set batch table entry: {e}"))
                })?;
            }

            let results = func
                .call::<Vec<Option<String>>>(batch_table)
                .map_err(|e| PluginError::ScriptError(format!("Batch call error: {e}")))?;

            if results.len() != groups.len() {
                return Err(PluginError::InvalidOutput(format!(
                    "Batch result length mismatch: expected {}, got {}",
                    groups.len(),
                    results.len()
                )));
            }

            Ok(results)
        })
    }
}
