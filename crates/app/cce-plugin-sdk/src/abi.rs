//! ABI version management for the native plugin SDK.

/// The ABI version this SDK targets.
///
/// The project is in the development stage, so the version history has been
/// reset to 1. Bump it on breaking ABI upgrades; the host (`cce_plugin_runtime
/// ::native`) rejects plugins below its minimum and warns on plugins
/// newer than its current version.
pub const CCE_ABI_VERSION: u32 = 1;

#[macro_export]
macro_rules! declare_plugin {
    // ── Primary arm: explicit initializer ──
    ($plugin_type:ty, $init:expr) => {
        // ── Static singleton ──────────────────────────────────────────
        static PLUGIN: std::sync::LazyLock<$plugin_type> = std::sync::LazyLock::new(|| $init);

        // ── Required exports ──────────────────────────────────────────

        /// Return the ABI version this plugin targets.
        #[no_mangle]
        pub extern "C" fn cce_plugin_abi_version() -> u32 {
            $crate::CCE_ABI_VERSION
        }

        /// Return plugin metadata as a JSON C string.
        ///
        /// The caller must free the returned string via [`cce_plugin_free_string`].
        #[no_mangle]
        pub extern "C" fn cce_plugin_metadata() -> *mut std::ffi::c_char {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                $crate::serde_json::to_string(&PLUGIN.metadata())
            }));
            let json = match result {
                Ok(Ok(meta)) => meta,
                _ => r#"{"id":"unknown","name":"Unknown","version":"0.0.0","priority":0}"#
                    .to_string(),
            };
            std::ffi::CString::new(json).unwrap().into_raw()
        }

        /// Whether this plugin implements BM25 NL generation.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_bm25_generation() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.supports_bm25()))
                .unwrap_or(false)
        }

        /// Whether this plugin implements embedding NL generation.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_embedding_generation() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.supports_embedding()))
                .unwrap_or(false)
        }

        /// Whether this plugin implements lifecycle management.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_lifecycle() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.supports_lifecycle()))
                .unwrap_or(false)
        }

        /// Whether this plugin implements the `GroupOverride` full-override tier.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_group_override() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_group_override()
            }))
            .unwrap_or(false)
        }

        /// Whether this plugin implements `RelationExtract`.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_relation_extract() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_relation_extract()
            }))
            .unwrap_or(false)
        }

        /// Whether this plugin implements `SymbolExtract`.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_symbol_extract() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_symbol_extract()
            }))
            .unwrap_or(false)
        }

        /// Whether this plugin implements `QueryRewrite`.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_query_rewrite() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_query_rewrite()
            }))
            .unwrap_or(false)
        }

        /// Whether this plugin implements `Fusion`.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_fusion() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.supports_fusion()))
                .unwrap_or(false)
        }

        /// Whether this plugin implements `ResultFilter`.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_result_filter() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_result_filter()
            }))
            .unwrap_or(false)
        }

        /// Whether this plugin implements `FileFilter`.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_file_filter() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_file_filter()
            }))
            .unwrap_or(false)
        }

        /// Create a plugin context (opaque pointer).
        ///
        /// Returns null if the plugin does not require context.
        #[no_mangle]
        pub extern "C" fn cce_plugin_create() -> *mut std::ffi::c_void {
            let ctx =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.create_context()))
                    .unwrap_or(None);
            match ctx {
                Some(ptr) => ptr,
                None => std::ptr::null_mut(),
            }
        }

        /// Destroy a plugin context previously created by [`cce_plugin_create`].
        ///
        /// # Safety
        ///
        /// `ctx` must have been returned by `cce_plugin_create` and not yet freed.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_destroy(ctx: *mut std::ffi::c_void) {
            if !ctx.is_null() {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                    PLUGIN.destroy_context(ctx)
                }));
            }
        }

        /// Free a C string previously returned by this plugin.
        ///
        /// # Safety
        ///
        /// `ptr` must have been allocated by a plugin function via
        /// `CString::into_raw()`.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_free_string(ptr: *mut std::ffi::c_char) {
            if !ptr.is_null() {
                drop(unsafe { std::ffi::CString::from_raw(ptr) });
            }
        }

        // ── Optional exports ─────────────────────────────────────────

        /// Generate BM25 NL text for an entity group.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_generate_bm25(
            ctx: *mut std::ffi::c_void,
            group_json: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let json_str = match unsafe { $crate::ffi::read_c_str(group_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| PLUGIN.generate_bm25(ctx, &json_str));
            $crate::ffi::result_to_c_string::<String>(&result)
        }

        /// Generate embedding NL text for an entity group.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_generate_embedding(
            ctx: *mut std::ffi::c_void,
            group_json: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let json_str = match unsafe { $crate::ffi::read_c_str(group_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| PLUGIN.generate_embedding(ctx, &json_str));
            $crate::ffi::result_to_c_string::<String>(&result)
        }

        /// Generate BM25 NL text for a batch of entity groups.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_generate_bm25_batch(
            ctx: *mut std::ffi::c_void,
            groups_json: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let json_str = match unsafe { $crate::ffi::read_c_str(groups_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| PLUGIN.generate_bm25_batch(ctx, &json_str));
            $crate::ffi::vec_result_to_c_string(&result)
        }

        /// Generate embedding NL text for a batch of entity groups.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_generate_embedding_batch(
            ctx: *mut std::ffi::c_void,
            groups_json: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let json_str = match unsafe { $crate::ffi::read_c_str(groups_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result =
                $crate::ffi::catch_unwind(|| PLUGIN.generate_embedding_batch(ctx, &json_str));
            $crate::ffi::vec_result_to_c_string(&result)
        }

        // ── Capability guards ───────────────────────────────────────

        /// Whether this plugin implements `FormatParse`.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_parse() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.supports_parse()))
                .unwrap_or(false)
        }

        /// Whether this plugin implements `EntityExtract`.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_extract() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.supports_extract()))
                .unwrap_or(false)
        }

        /// Whether this plugin implements the `Group` post-processing hook.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_group() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.supports_group()))
                .unwrap_or(false)
        }

        /// Whether this plugin implements the `Chunk` override.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_chunk() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.supports_chunk()))
                .unwrap_or(false)
        }

        /// Whether this plugin implements `Rerank`.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_rerank() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.supports_rerank()))
                .unwrap_or(false)
        }

        /// Whether this plugin provides a custom tree-sitter language.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_ast_language() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_ast_language()
            }))
            .unwrap_or(false)
        }

        /// Whether this plugin remaps a custom language onto a host built-in
        /// grammar.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_language_remap() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_language_remap()
            }))
            .unwrap_or(false)
        }

        /// Whether this plugin maps module paths to stdlib categories.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_stdlib_heuristic() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_stdlib_heuristic()
            }))
            .unwrap_or(false)
        }

        /// Whether this plugin can decide test-file status.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_test_file_heuristic() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_test_file_heuristic()
            }))
            .unwrap_or(false)
        }

        /// Whether this plugin maps capture names to entity kinds.
        #[no_mangle]
        pub extern "C" fn cce_plugin_has_entity_kind_heuristic() -> bool {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.supports_entity_kind_heuristic()
            }))
            .unwrap_or(false)
        }

        // ── Capability entry points ─────────────────────────────────

        /// Parse a document into a `PluginDocument`.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_parse_document(
            ctx: *mut std::ffi::c_void,
            content: *const std::ffi::c_char,
            file_path: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let content_str = match unsafe { $crate::ffi::read_c_str(content) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let path_str = match unsafe { $crate::ffi::read_c_str(file_path) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result =
                $crate::ffi::catch_unwind(|| PLUGIN.parse_document(ctx, &content_str, &path_str));
            $crate::ffi::result_to_c_string::<$crate::PluginDocument>(&result)
        }

        /// Extract supplementary entities from a code file.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_extract_entities(
            ctx: *mut std::ffi::c_void,
            content: *const std::ffi::c_char,
            file_path: *const std::ffi::c_char,
            language: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let content_str = match unsafe { $crate::ffi::read_c_str(content) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let path_str = match unsafe { $crate::ffi::read_c_str(file_path) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let lang_str = match unsafe { $crate::ffi::read_c_str(language) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| {
                PLUGIN.extract_entities(ctx, &content_str, &path_str, &lang_str)
            });
            $crate::ffi::opt_vec_result_to_c_string::<$crate::PluginEntity>(&result)
        }

        /// Post-process groups after built-in grouping.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_post_group(
            ctx: *mut std::ffi::c_void,
            groups_json: *const std::ffi::c_char,
            context_json: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let groups_str = match unsafe { $crate::ffi::read_c_str(groups_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let context_str = match unsafe { $crate::ffi::read_c_str(context_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result =
                $crate::ffi::catch_unwind(|| PLUGIN.post_group(ctx, &groups_str, &context_str));
            $crate::ffi::opt_vec_result_to_c_string::<$crate::EntityGroup>(&result)
        }

        /// Override chunking for converted groups.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_chunk(
            ctx: *mut std::ffi::c_void,
            conversions_json: *const std::ffi::c_char,
            file_path: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let conversions_str = match unsafe { $crate::ffi::read_c_str(conversions_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let path_str = match unsafe { $crate::ffi::read_c_str(file_path) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result =
                $crate::ffi::catch_unwind(|| PLUGIN.chunk(ctx, &conversions_str, &path_str));
            $crate::ffi::opt_vec_result_to_c_string::<$crate::ChunkedResult>(&result)
        }

        /// Rerank query candidates.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_rerank(
            ctx: *mut std::ffi::c_void,
            query: *const std::ffi::c_char,
            candidates_json: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let query_str = match unsafe { $crate::ffi::read_c_str(query) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let candidates_str = match unsafe { $crate::ffi::read_c_str(candidates_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result =
                $crate::ffi::catch_unwind(|| PLUGIN.rerank(ctx, &query_str, &candidates_str));
            $crate::ffi::result_to_c_string::<$crate::RerankResult>(&result)
        }

        /// Fully replace built-in grouping for a parsed file.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_group(
            ctx: *mut std::ffi::c_void,
            context_json: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let context_str = match unsafe { $crate::ffi::read_c_str(context_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| PLUGIN.group(ctx, &context_str));
            $crate::ffi::opt_vec_result_to_c_string::<$crate::EntityGroup>(&result)
        }

        /// Extract supplementary symbols from a code file.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_extract_symbols(
            ctx: *mut std::ffi::c_void,
            content: *const std::ffi::c_char,
            file_path: *const std::ffi::c_char,
            language: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let content_str = match unsafe { $crate::ffi::read_c_str(content) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let path_str = match unsafe { $crate::ffi::read_c_str(file_path) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let lang_str = match unsafe { $crate::ffi::read_c_str(language) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| {
                PLUGIN.extract_symbols(ctx, &content_str, &path_str, &lang_str)
            });
            $crate::ffi::opt_vec_result_to_c_string::<$crate::PluginSymbol>(&result)
        }

        /// Extract explicit relations between symbols.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_extract_relations(
            ctx: *mut std::ffi::c_void,
            content: *const std::ffi::c_char,
            file_path: *const std::ffi::c_char,
            language: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let content_str = match unsafe { $crate::ffi::read_c_str(content) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let path_str = match unsafe { $crate::ffi::read_c_str(file_path) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let lang_str = match unsafe { $crate::ffi::read_c_str(language) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| {
                PLUGIN.extract_relations(ctx, &content_str, &path_str, &lang_str)
            });
            $crate::ffi::opt_vec_result_to_c_string::<$crate::PluginRelation>(&result)
        }

        /// Extract import statements from a code file.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_extract_imports(
            ctx: *mut std::ffi::c_void,
            content: *const std::ffi::c_char,
            file_path: *const std::ffi::c_char,
            language: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let content_str = match unsafe { $crate::ffi::read_c_str(content) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let path_str = match unsafe { $crate::ffi::read_c_str(file_path) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let lang_str = match unsafe { $crate::ffi::read_c_str(language) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| {
                PLUGIN.extract_imports(ctx, &content_str, &path_str, &lang_str)
            });
            $crate::ffi::opt_vec_result_to_c_string::<$crate::PluginImport>(&result)
        }

        /// Extract export declarations from a code file.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_extract_exports(
            ctx: *mut std::ffi::c_void,
            content: *const std::ffi::c_char,
            file_path: *const std::ffi::c_char,
            language: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let content_str = match unsafe { $crate::ffi::read_c_str(content) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let path_str = match unsafe { $crate::ffi::read_c_str(file_path) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let lang_str = match unsafe { $crate::ffi::read_c_str(language) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| {
                PLUGIN.extract_exports(ctx, &content_str, &path_str, &lang_str)
            });
            $crate::ffi::opt_vec_result_to_c_string::<$crate::PluginExport>(&result)
        }

        /// Rewrite / expand a query before recall.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_rewrite_query(
            ctx: *mut std::ffi::c_void,
            query: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let query_str = match unsafe { $crate::ffi::read_c_str(query) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| PLUGIN.rewrite_query(ctx, &query_str));
            $crate::ffi::result_to_c_string::<$crate::QueryRewriteResult>(&result)
        }

        /// Override hybrid fusion weights.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_fusion_weights(
            ctx: *mut std::ffi::c_void,
            query: *const std::ffi::c_char,
            vector_count: usize,
            bm25_count: usize,
        ) -> *mut std::ffi::c_char {
            let query_str = match unsafe { $crate::ffi::read_c_str(query) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| {
                PLUGIN.fusion_weights(ctx, &query_str, vector_count, bm25_count)
            });
            $crate::ffi::result_to_c_string::<$crate::FusionWeights>(&result)
        }

        /// Filter / boost / annotate candidates after reranking.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_filter_results(
            ctx: *mut std::ffi::c_void,
            query: *const std::ffi::c_char,
            results_json: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let query_str = match unsafe { $crate::ffi::read_c_str(query) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let results_str = match unsafe { $crate::ffi::read_c_str(results_json) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result =
                $crate::ffi::catch_unwind(|| PLUGIN.filter_results(ctx, &query_str, &results_str));
            $crate::ffi::opt_vec_result_to_c_string::<$crate::ResultFilterEntry>(&result)
        }

        /// Decide whether a path should be included/excluded during scanning.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_filter_file(
            ctx: *mut std::ffi::c_void,
            file_path: *const std::ffi::c_char,
            is_directory: bool,
            size: u64,
        ) -> *mut std::ffi::c_char {
            let path_str = match unsafe { $crate::ffi::read_c_str(file_path) } {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return $crate::ffi::error_to_c_string(&$crate::PluginError::ExecutionFailed(
                        e,
                    ));
                }
            };
            let result = $crate::ffi::catch_unwind(|| {
                PLUGIN.filter_file(ctx, &path_str, is_directory, size)
            });
            $crate::ffi::result_to_c_string::<$crate::FileFilterDecision>(&result)
        }

        /// Return the tree-sitter query string for a query type, or null.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_query_scheme(
            ctx: *mut std::ffi::c_void,
            query_type: u32,
        ) -> *mut std::ffi::c_char {
            let scheme = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.query_scheme(ctx, query_type)
            }))
            .unwrap_or(None);
            match scheme {
                Some(s) => std::ffi::CString::new(s)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut()),
                None => std::ptr::null_mut(),
            }
        }

        /// Return a raw pointer to the tree-sitter `TSLanguage`, or null.
        #[no_mangle]
        pub extern "C" fn cce_plugin_tree_sitter_language() -> *mut std::ffi::c_void {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.tree_sitter_language()
            }))
            .unwrap_or(None)
            .unwrap_or(std::ptr::null_mut())
        }

        /// Return the custom language name, or null.
        #[no_mangle]
        pub extern "C" fn cce_plugin_language_name() -> *mut std::ffi::c_char {
            let name =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| PLUGIN.language_name()))
                    .unwrap_or(None);
            match name {
                Some(s) => std::ffi::CString::new(s)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut()),
                None => std::ptr::null_mut(),
            }
        }

        /// Return the custom language extensions as a JSON array, or null.
        #[no_mangle]
        pub extern "C" fn cce_plugin_language_extensions() -> *mut std::ffi::c_char {
            let exts = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.language_extensions()
            }))
            .unwrap_or_default();
            let json = $crate::serde_json::to_string(&exts).unwrap_or_else(|_| "[]".to_string());
            std::ffi::CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }

        /// Return the host built-in language name backing the remapped custom
        /// language, or null.
        #[no_mangle]
        pub extern "C" fn cce_plugin_remap_grammar_language() -> *mut std::ffi::c_char {
            let name = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.remap_grammar_language()
            }))
            .unwrap_or(None);
            match name {
                Some(s) => std::ffi::CString::new(s)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut()),
                None => std::ptr::null_mut(),
            }
        }

        /// Classify `module_path` as a standard-library item, or null.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_classify_stdlib(
            ctx: *mut std::ffi::c_void,
            module_path: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let module_str = match unsafe { $crate::ffi::read_c_str(module_path) } {
                Ok(s) => s.to_string(),
                Err(_) => return std::ptr::null_mut(),
            };
            let category = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.classify_stdlib(ctx, &module_str)
            }))
            .unwrap_or(None);
            match category {
                Some(s) => std::ffi::CString::new(s)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut()),
                None => std::ptr::null_mut(),
            }
        }

        /// Decide test-file status, returning "true"/"false", or null.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_is_test_file(
            ctx: *mut std::ffi::c_void,
            file_path: *const std::ffi::c_char,
            content: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let path_str = match unsafe { $crate::ffi::read_c_str(file_path) } {
                Ok(s) => s.to_string(),
                Err(_) => return std::ptr::null_mut(),
            };
            let content_str = match unsafe { $crate::ffi::read_c_str(content) } {
                Ok(s) => s.to_string(),
                Err(_) => return std::ptr::null_mut(),
            };
            let decision = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.is_test_file(ctx, &path_str, &content_str)
            }))
            .unwrap_or(None);
            match decision {
                Some(true) => std::ffi::CString::new("true")
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut()),
                Some(false) => std::ffi::CString::new("false")
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut()),
                None => std::ptr::null_mut(),
            }
        }

        /// Map a tree-sitter capture name to an entity kind, or null.
        #[no_mangle]
        pub unsafe extern "C" fn cce_plugin_entity_kind(
            ctx: *mut std::ffi::c_void,
            capture_name: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_char {
            let capture_str = match unsafe { $crate::ffi::read_c_str(capture_name) } {
                Ok(s) => s.to_string(),
                Err(_) => return std::ptr::null_mut(),
            };
            let kind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                PLUGIN.entity_kind(ctx, &capture_str)
            }))
            .unwrap_or(None);
            match kind {
                Some(s) => std::ffi::CString::new(s)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut()),
                None => std::ptr::null_mut(),
            }
        }
    };

    // ── Convenience arm: no initializer → uses Default ──
    ($plugin_type:ty) => {
        $crate::declare_plugin!($plugin_type, <$plugin_type>::default());
    };
}
