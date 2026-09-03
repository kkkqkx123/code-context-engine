use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;

use serde::Deserialize;

use crate::error::PluginError;

// ── C ABI function pointer types ─────────────────────────────────────────

/// `cce_plugin_abi_version()`
pub(crate) type AbiVersionFn = unsafe extern "C" fn() -> u32;

/// `cce_plugin_metadata()`
pub(crate) type MetadataFn = unsafe extern "C" fn() -> *mut c_char;

/// `cce_plugin_has_bm25_generation()` / `cce_plugin_has_embedding_generation()`
pub(crate) type HasCapabilityFn = unsafe extern "C" fn() -> bool;

/// `cce_plugin_free_string(ptr)`
pub(crate) type FreeStringFn = unsafe extern "C" fn(*mut c_char);

/// `cce_plugin_generate_bm25(ctx, json)` / `cce_plugin_generate_embedding(ctx, json)`
/// `cce_plugin_generate_bm25_batch(ctx, json)` / `cce_plugin_generate_embedding_batch(ctx, json)`
pub(crate) type PluginStringFn = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char;

/// `cce_plugin_parse_document(ctx, content, file_path)`
/// `cce_plugin_post_group(ctx, groups_json, context_json)`
/// `cce_plugin_rerank(ctx, query, candidates_json)`
pub(crate) type PluginStringFn2 =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> *mut c_char;

/// `cce_plugin_extract_entities(ctx, content, file_path, language)`
/// `cce_plugin_extract_symbols(ctx, content, file_path, language)`
/// `cce_plugin_extract_relations(ctx, content, file_path, language)`
/// `cce_plugin_extract_imports(ctx, content, file_path, language)`
/// `cce_plugin_extract_exports(ctx, content, file_path, language)`
pub(crate) type PluginStringFn3 =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *const c_char) -> *mut c_char;

/// `cce_plugin_fusion_weights(ctx, query, vector_count, bm25_count)`
pub(crate) type FusionWeightsFn =
    unsafe extern "C" fn(*mut c_void, *const c_char, usize, usize) -> *mut c_char;

/// `cce_plugin_filter_file(ctx, path, is_directory, size)`
pub(crate) type FilterFileFn =
    unsafe extern "C" fn(*mut c_void, *const c_char, bool, u64) -> *mut c_char;

/// `cce_plugin_query_scheme(ctx, query_type)`
pub(crate) type QuerySchemeFn = unsafe extern "C" fn(*mut c_void, u32) -> *mut c_char;

/// `cce_plugin_tree_sitter_language()`
pub(crate) type TreeSitterLangFn = unsafe extern "C" fn() -> *const c_void;

/// `cce_plugin_language_name()` / `cce_plugin_language_extensions()`
pub(crate) type StringOnlyFn = unsafe extern "C" fn() -> *mut c_char;

/// `cce_plugin_create()`
pub(crate) type CreateContextFn = unsafe extern "C" fn() -> *mut c_void;

/// `cce_plugin_destroy(ctx)`
pub(crate) type DestroyContextFn = unsafe extern "C" fn(*mut c_void);

/// An opaque handle to a native plugin's lifecycle context.
///
/// The handle is `Send + Sync` because the host guarantees (via the
/// ABI contract) that the plugin's functions are safe to call from
/// multiple threads.  The pointer is only dereferenced through the
/// vtable extracted from the library and only freed in `Drop`.
pub(crate) struct PluginContext(pub(crate) *mut c_void);

// SAFETY: The host treats the context as an opaque token. All accesses
// go through FFI function pointers obtained from the library itself,
// which the plugin SDK contract requires to be thread-safe.
unsafe impl Send for PluginContext {}
unsafe impl Sync for PluginContext {}

/// A `Send`-able copy of a raw context pointer, used to move the context
/// into a `'static` worker-thread closure.
#[derive(Clone, Copy)]
pub(crate) struct SendPtr(pub(crate) *mut c_void);

// SAFETY: Same contract as `PluginContext` — the pointer is an opaque
// token passed back to the owning library's FFI functions only.
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

// ── FFI result protocol types ─────────────────────────────────────────────

/// Internal result from a native plugin FFI call.
///
/// The plugin returns a JSON string that deserializes into this structure.
#[derive(Deserialize)]
pub(crate) struct FfiResult<T> {
    result: String,
    #[serde(default = "default_none")]
    value: Option<T>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error_type: Option<String>,
}

/// Helper: default for serde skip on Option<T> without T: Default bound.
pub(crate) fn default_none<T>() -> Option<T> {
    None
}

/// Restore a concrete [`PluginError`] from the FFI `error_type` field.
///
/// Unknown or missing `error_type` values fall back to `ScriptError`,
/// preserving the pre-`error_type` behaviour.
pub(crate) fn ffi_error(message: Option<String>, error_type: Option<&str>) -> PluginError {
    let msg = message.unwrap_or_else(|| "Unknown plugin error".to_string());
    match error_type {
        Some("timeout") => PluginError::Timeout,
        Some("invalid_output") => PluginError::InvalidOutput(msg),
        Some("logic") => PluginError::LogicError(msg),
        Some("resource") => PluginError::ResourceError(msg),
        Some("circuit_broken") => PluginError::CircuitBroken,
        Some("not_found") => PluginError::NotFound(msg),
        Some("execution_failed") => PluginError::ExecutionFailed(msg),
        // "script", absent, or unknown → ScriptError
        _ => PluginError::ScriptError(msg),
    }
}

#[cfg(test)]
pub(crate) fn parse_ffi_result<T: serde::de::DeserializeOwned>(
    json_str: &str,
) -> Result<Option<T>, PluginError> {
    let parsed: FfiResult<T> = serde_json::from_str(json_str).map_err(|e| {
        PluginError::InvalidOutput(format!(
            "Failed to parse plugin FFI result: {} (raw: {})",
            e,
            json_str.chars().take(200).collect::<String>()
        ))
    })?;

    match parsed.result.as_str() {
        "ok" => Ok(parsed.value),
        "none" => Ok(None),
        "error" => Err(ffi_error(parsed.message, parsed.error_type.as_deref())),
        other => Err(PluginError::InvalidOutput(format!(
            "Unknown FFI result type '{}'",
            other
        ))),
    }
}

/// Call a C function that takes a context pointer and a string argument,
/// and returns a string allocated by the plugin (freed via `free_string_fn`).
///
/// # Safety
///
/// `func` must be a valid function pointer from the same library and `ctx`
/// must have been created by that library's `cce_plugin_create`. `arg` must be
/// a valid C-compatible string (no interior null bytes).
pub(crate) unsafe fn call_plugin_string(
    func: PluginStringFn,
    free_string_fn: FreeStringFn,
    ctx: Option<SendPtr>,
    arg: &str,
) -> Result<String, PluginError> {
    let c_arg = CString::new(arg)
        .map_err(|_| PluginError::InvalidOutput("Argument contains null byte".to_string()))?;
    let ctx_ptr = ctx.map(|c| c.0).unwrap_or(std::ptr::null_mut());
    // SAFETY: `func` is a valid function pointer from the same library;
    // `c_arg` is a valid C-compatible string (no interior null bytes).
    let ret_ptr = unsafe { func(ctx_ptr, c_arg.as_ptr()) };
    if ret_ptr.is_null() {
        return Err(PluginError::ScriptError(
            "Plugin returned null pointer".to_string(),
        ));
    }
    let c_str = unsafe { CStr::from_ptr(ret_ptr) };
    let result = c_str
        .to_str()
        .map_err(|_| PluginError::InvalidOutput("Plugin returned non-UTF-8 string".to_string()))?;
    let owned = result.to_string();
    // SAFETY: `ret_ptr` was allocated by the plugin; `free_string_fn` comes
    // from the same library.
    unsafe { free_string_fn(ret_ptr) };
    Ok(owned)
}

/// Parse a plugin FFI result JSON string into an optional JSON value.
pub(crate) fn parse_ffi_json_result(
    json_str: &str,
) -> Result<Option<serde_json::Value>, PluginError> {
    let parsed: FfiResult<serde_json::Value> = serde_json::from_str(json_str)
        .map_err(|e| PluginError::InvalidOutput(format!("Failed to parse plugin result: {}", e)))?;
    match parsed.result.as_str() {
        "ok" => Ok(parsed.value),
        "none" => Ok(None),
        "error" => Err(ffi_error(parsed.message, parsed.error_type.as_deref())),
        other => Err(PluginError::InvalidOutput(format!(
            "Unknown FFI result type '{}'",
            other
        ))),
    }
}

/// Call a C function that takes a context pointer and two string arguments.
///
/// # Safety
///
/// `func` must be a valid function pointer from the same library and `ctx`
/// must have been created by that library's `cce_plugin_create`. The string
/// arguments must be valid C-compatible strings (no interior null bytes).
pub(crate) unsafe fn call_plugin_string2(
    func: PluginStringFn2,
    free_string_fn: FreeStringFn,
    ctx: Option<SendPtr>,
    arg1: &str,
    arg2: &str,
) -> Result<String, PluginError> {
    let c1 = CString::new(arg1)
        .map_err(|_| PluginError::InvalidOutput("Argument contains null byte".to_string()))?;
    let c2 = CString::new(arg2)
        .map_err(|_| PluginError::InvalidOutput("Argument contains null byte".to_string()))?;
    let ctx_ptr = ctx.map(|c| c.0).unwrap_or(std::ptr::null_mut());
    // SAFETY: `func` is a valid function pointer from the same library;
    // `c1`/`c2` are valid C-compatible strings (no interior null bytes).
    let ret_ptr = unsafe { func(ctx_ptr, c1.as_ptr(), c2.as_ptr()) };
    read_returned_string(ret_ptr, free_string_fn)
}

/// Call a C function that takes a context pointer and three string arguments.
///
/// # Safety
///
/// Same contract as [`call_plugin_string2`].
pub(crate) unsafe fn call_plugin_string3(
    func: PluginStringFn3,
    free_string_fn: FreeStringFn,
    ctx: Option<SendPtr>,
    arg1: &str,
    arg2: &str,
    arg3: &str,
) -> Result<String, PluginError> {
    let c1 = CString::new(arg1)
        .map_err(|_| PluginError::InvalidOutput("Argument contains null byte".to_string()))?;
    let c2 = CString::new(arg2)
        .map_err(|_| PluginError::InvalidOutput("Argument contains null byte".to_string()))?;
    let c3 = CString::new(arg3)
        .map_err(|_| PluginError::InvalidOutput("Argument contains null byte".to_string()))?;
    let ctx_ptr = ctx.map(|c| c.0).unwrap_or(std::ptr::null_mut());
    // SAFETY: `func` is a valid function pointer from the same library.
    let ret_ptr = unsafe { func(ctx_ptr, c1.as_ptr(), c2.as_ptr(), c3.as_ptr()) };
    read_returned_string(ret_ptr, free_string_fn)
}

/// Read and free a string returned by a plugin FFI function (null = None).
pub(crate) fn read_returned_string(
    ret_ptr: *mut c_char,
    free_string_fn: FreeStringFn,
) -> Result<String, PluginError> {
    if ret_ptr.is_null() {
        return Err(PluginError::ScriptError(
            "Plugin returned null pointer".to_string(),
        ));
    }
    let c_str = unsafe { CStr::from_ptr(ret_ptr) };
    let result = c_str
        .to_str()
        .map_err(|_| PluginError::InvalidOutput("Plugin returned non-UTF-8 string".to_string()))?;
    let owned = result.to_string();
    // SAFETY: `ret_ptr` was allocated by the plugin; `free_string_fn` comes
    // from the same library.
    unsafe { free_string_fn(ret_ptr) };
    Ok(owned)
}

/// Call a no-argument string-returning C function and read/free its output.
///
/// # Safety
///
/// `func` must be a valid function pointer from the same library.
pub(crate) unsafe fn call_owned_string_only(
    func: StringOnlyFn,
    free_string_fn: FreeStringFn,
    _operation: &str,
) -> Result<Option<String>, PluginError> {
    // SAFETY: `func` is a valid function pointer from the same library.
    let ret_ptr = unsafe { func() };
    if ret_ptr.is_null() {
        return Ok(None);
    }
    let c_str = unsafe { CStr::from_ptr(ret_ptr) };
    let result = c_str
        .to_str()
        .map_err(|_| PluginError::InvalidOutput("Plugin returned non-UTF-8 string".to_string()))?;
    let owned = result.to_string();
    // SAFETY: `ret_ptr` was allocated by the plugin; `free_string_fn` comes
    // from the same library.
    unsafe { free_string_fn(ret_ptr) };
    Ok(Some(owned))
}

/// Call a context+one-u32 string-returning C function and read/free its output.
///
/// # Safety
///
/// Same contract as [`call_plugin_string2`].
pub(crate) unsafe fn call_owned_string_fn1(
    f: impl FnOnce(*mut c_void) -> *mut c_char,
    free_string_fn: FreeStringFn,
    ctx: Option<SendPtr>,
) -> Result<Option<String>, PluginError> {
    let ctx_ptr = ctx.map(|c| c.0).unwrap_or(std::ptr::null_mut());
    // SAFETY: `f` is a valid function pointer from the same library.
    let ret_ptr = f(ctx_ptr);
    if ret_ptr.is_null() {
        return Ok(None);
    }
    let c_str = unsafe { CStr::from_ptr(ret_ptr) };
    let result = c_str
        .to_str()
        .map_err(|_| PluginError::InvalidOutput("Plugin returned non-UTF-8 string".to_string()))?;
    let owned = result.to_string();
    // SAFETY: `ret_ptr` was allocated by the plugin; `free_string_fn` comes
    // from the same library.
    unsafe { free_string_fn(ret_ptr) };
    Ok(Some(owned))
}

pub(crate) fn symbol_missing_err(
    path: &std::path::Path,
    symbol: &str,
) -> crate::error::PluginError {
    crate::error::PluginError::ResourceError(format!(
        "Native plugin '{}' is missing required symbol '{}'",
        path.display(),
        symbol
    ))
}

pub(crate) fn capability_label(operation: &str) -> &'static str {
    match operation {
        "generate_bm25"
        | "generate_embedding"
        | "generate_bm25_batch"
        | "generate_embedding_batch" => "text_gen",
        "parse_document" => "format_parse",
        "extract_entities" => "entity_extract",
        "post_group" => "group",
        "group" => "group_override",
        "chunk" => "chunk",
        "rerank" => "rerank",
        "extract_symbols" | "extract_relations" => "relation_extract",
        "extract_imports" | "extract_exports" => "symbol_extract",
        "rewrite_query" => "query_rewrite",
        "fusion_weights" => "fusion",
        "filter_results" => "result_filter",
        "filter_file" => "file_filter",
        _ => "unknown",
    }
}
