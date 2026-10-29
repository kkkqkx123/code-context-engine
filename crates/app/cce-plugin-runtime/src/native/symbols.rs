//! Native plugin ABI symbol table.
//!
//! Every dynamic symbol the host loads from a plugin library is declared
//! exactly once here. `NativePlugin::load` resolves through these constants
//! so a renamed or dropped ABI symbol fails in one place instead of hiding
//! inside scattered byte-string literals.
//!
//! The table must stay in sync with the authoritative ABI definition in
//! `cce-plugin-sdk/include/cce_plugin.h` (and the `declare_plugin!` macro
//! plus the symbol table in `cce-plugin-sdk/docs/guide.md`); the test below
//! enforces the header side of that contract.

/// One ABI symbol: the plain name for diagnostics and the NUL-terminated
/// bytes for library resolution.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AbiSymbol {
    /// Plain symbol name, used in diagnostics.
    pub name: &'static str,
    /// NUL-terminated bytes passed to library resolution.
    pub bytes: &'static [u8],
}

// ── Loader handshake: version, metadata, memory ──
pub(crate) const ABI_VERSION: AbiSymbol = AbiSymbol {
    name: "cce_plugin_abi_version",
    bytes: b"cce_plugin_abi_version\0",
};
pub(crate) const METADATA: AbiSymbol = AbiSymbol {
    name: "cce_plugin_metadata",
    bytes: b"cce_plugin_metadata\0",
};
pub(crate) const FREE_STRING: AbiSymbol = AbiSymbol {
    name: "cce_plugin_free_string",
    bytes: b"cce_plugin_free_string\0",
};
pub(crate) const HAS_LIFECYCLE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_lifecycle",
    bytes: b"cce_plugin_has_lifecycle\0",
};
pub(crate) const CREATE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_create",
    bytes: b"cce_plugin_create\0",
};
pub(crate) const DESTROY: AbiSymbol = AbiSymbol {
    name: "cce_plugin_destroy",
    bytes: b"cce_plugin_destroy\0",
};

// ── Text generation ──
pub(crate) const HAS_BM25_GENERATION: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_bm25_generation",
    bytes: b"cce_plugin_has_bm25_generation\0",
};
pub(crate) const HAS_EMBEDDING_GENERATION: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_embedding_generation",
    bytes: b"cce_plugin_has_embedding_generation\0",
};
pub(crate) const GENERATE_BM25: AbiSymbol = AbiSymbol {
    name: "cce_plugin_generate_bm25",
    bytes: b"cce_plugin_generate_bm25\0",
};
pub(crate) const GENERATE_EMBEDDING: AbiSymbol = AbiSymbol {
    name: "cce_plugin_generate_embedding",
    bytes: b"cce_plugin_generate_embedding\0",
};
pub(crate) const GENERATE_BM25_BATCH: AbiSymbol = AbiSymbol {
    name: "cce_plugin_generate_bm25_batch",
    bytes: b"cce_plugin_generate_bm25_batch\0",
};
pub(crate) const GENERATE_EMBEDDING_BATCH: AbiSymbol = AbiSymbol {
    name: "cce_plugin_generate_embedding_batch",
    bytes: b"cce_plugin_generate_embedding_batch\0",
};

// ── Document pipeline: parse, extract, group, chunk, rerank ──
pub(crate) const HAS_PARSE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_parse",
    bytes: b"cce_plugin_has_parse\0",
};
pub(crate) const PARSE_DOCUMENT: AbiSymbol = AbiSymbol {
    name: "cce_plugin_parse_document",
    bytes: b"cce_plugin_parse_document\0",
};
pub(crate) const HAS_EXTRACT: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_extract",
    bytes: b"cce_plugin_has_extract\0",
};
pub(crate) const EXTRACT_ENTITIES: AbiSymbol = AbiSymbol {
    name: "cce_plugin_extract_entities",
    bytes: b"cce_plugin_extract_entities\0",
};
pub(crate) const HAS_GROUP: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_group",
    bytes: b"cce_plugin_has_group\0",
};
pub(crate) const POST_GROUP: AbiSymbol = AbiSymbol {
    name: "cce_plugin_post_group",
    bytes: b"cce_plugin_post_group\0",
};
pub(crate) const HAS_GROUP_OVERRIDE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_group_override",
    bytes: b"cce_plugin_has_group_override\0",
};
pub(crate) const GROUP: AbiSymbol = AbiSymbol {
    name: "cce_plugin_group",
    bytes: b"cce_plugin_group\0",
};
pub(crate) const HAS_CHUNK: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_chunk",
    bytes: b"cce_plugin_has_chunk\0",
};
pub(crate) const CHUNK: AbiSymbol = AbiSymbol {
    name: "cce_plugin_chunk",
    bytes: b"cce_plugin_chunk\0",
};
pub(crate) const HAS_RERANK: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_rerank",
    bytes: b"cce_plugin_has_rerank\0",
};
pub(crate) const RERANK: AbiSymbol = AbiSymbol {
    name: "cce_plugin_rerank",
    bytes: b"cce_plugin_rerank\0",
};

// ── Custom language support ──
pub(crate) const HAS_AST_LANGUAGE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_ast_language",
    bytes: b"cce_plugin_has_ast_language\0",
};
pub(crate) const TREE_SITTER_LANGUAGE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_tree_sitter_language",
    bytes: b"cce_plugin_tree_sitter_language\0",
};
pub(crate) const LANGUAGE_NAME: AbiSymbol = AbiSymbol {
    name: "cce_plugin_language_name",
    bytes: b"cce_plugin_language_name\0",
};
pub(crate) const LANGUAGE_EXTENSIONS: AbiSymbol = AbiSymbol {
    name: "cce_plugin_language_extensions",
    bytes: b"cce_plugin_language_extensions\0",
};
pub(crate) const QUERY_SCHEME: AbiSymbol = AbiSymbol {
    name: "cce_plugin_query_scheme",
    bytes: b"cce_plugin_query_scheme\0",
};
pub(crate) const HAS_LANGUAGE_REMAP: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_language_remap",
    bytes: b"cce_plugin_has_language_remap\0",
};
pub(crate) const REMAP_GRAMMAR_LANGUAGE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_remap_grammar_language",
    bytes: b"cce_plugin_remap_grammar_language\0",
};

// ── Language heuristics ──
pub(crate) const HAS_STDLIB_HEURISTIC: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_stdlib_heuristic",
    bytes: b"cce_plugin_has_stdlib_heuristic\0",
};
pub(crate) const CLASSIFY_STDLIB: AbiSymbol = AbiSymbol {
    name: "cce_plugin_classify_stdlib",
    bytes: b"cce_plugin_classify_stdlib\0",
};
pub(crate) const HAS_TEST_FILE_HEURISTIC: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_test_file_heuristic",
    bytes: b"cce_plugin_has_test_file_heuristic\0",
};
pub(crate) const IS_TEST_FILE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_is_test_file",
    bytes: b"cce_plugin_is_test_file\0",
};
pub(crate) const HAS_ENTITY_KIND_HEURISTIC: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_entity_kind_heuristic",
    bytes: b"cce_plugin_has_entity_kind_heuristic\0",
};
pub(crate) const ENTITY_KIND: AbiSymbol = AbiSymbol {
    name: "cce_plugin_entity_kind",
    bytes: b"cce_plugin_entity_kind\0",
};
pub(crate) const HAS_STDLIB_CALL_HEURISTIC: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_stdlib_call_heuristic",
    bytes: b"cce_plugin_has_stdlib_call_heuristic\0",
};
pub(crate) const CLASSIFY_STDLIB_CALL: AbiSymbol = AbiSymbol {
    name: "cce_plugin_classify_stdlib_call",
    bytes: b"cce_plugin_classify_stdlib_call\0",
};
pub(crate) const HAS_BARE_METHODS_HEURISTIC: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_bare_methods_heuristic",
    bytes: b"cce_plugin_has_bare_methods_heuristic\0",
};
pub(crate) const BARE_BOILERPLATE_METHODS: AbiSymbol = AbiSymbol {
    name: "cce_plugin_bare_boilerplate_methods",
    bytes: b"cce_plugin_bare_boilerplate_methods\0",
};

// ── Relation and symbol extraction ──
pub(crate) const HAS_RELATION_EXTRACT: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_relation_extract",
    bytes: b"cce_plugin_has_relation_extract\0",
};
pub(crate) const EXTRACT_RELATIONS: AbiSymbol = AbiSymbol {
    name: "cce_plugin_extract_relations",
    bytes: b"cce_plugin_extract_relations\0",
};
pub(crate) const HAS_SYMBOL_EXTRACT: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_symbol_extract",
    bytes: b"cce_plugin_has_symbol_extract\0",
};
pub(crate) const EXTRACT_SYMBOLS: AbiSymbol = AbiSymbol {
    name: "cce_plugin_extract_symbols",
    bytes: b"cce_plugin_extract_symbols\0",
};
pub(crate) const EXTRACT_IMPORTS: AbiSymbol = AbiSymbol {
    name: "cce_plugin_extract_imports",
    bytes: b"cce_plugin_extract_imports\0",
};
pub(crate) const EXTRACT_EXPORTS: AbiSymbol = AbiSymbol {
    name: "cce_plugin_extract_exports",
    bytes: b"cce_plugin_extract_exports\0",
};

// ── Query pipeline: rewrite, fusion, filters ──
pub(crate) const HAS_QUERY_REWRITE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_query_rewrite",
    bytes: b"cce_plugin_has_query_rewrite\0",
};
pub(crate) const REWRITE_QUERY: AbiSymbol = AbiSymbol {
    name: "cce_plugin_rewrite_query",
    bytes: b"cce_plugin_rewrite_query\0",
};
pub(crate) const HAS_FUSION: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_fusion",
    bytes: b"cce_plugin_has_fusion\0",
};
pub(crate) const FUSION_WEIGHTS: AbiSymbol = AbiSymbol {
    name: "cce_plugin_fusion_weights",
    bytes: b"cce_plugin_fusion_weights\0",
};
pub(crate) const HAS_RESULT_FILTER: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_result_filter",
    bytes: b"cce_plugin_has_result_filter\0",
};
pub(crate) const FILTER_RESULTS: AbiSymbol = AbiSymbol {
    name: "cce_plugin_filter_results",
    bytes: b"cce_plugin_filter_results\0",
};
pub(crate) const HAS_FILE_FILTER: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_file_filter",
    bytes: b"cce_plugin_has_file_filter\0",
};
pub(crate) const FILTER_FILE: AbiSymbol = AbiSymbol {
    name: "cce_plugin_filter_file",
    bytes: b"cce_plugin_filter_file\0",
};

// ── Domain vocabulary ──
pub(crate) const HAS_VOCABULARY: AbiSymbol = AbiSymbol {
    name: "cce_plugin_has_vocabulary",
    bytes: b"cce_plugin_has_vocabulary\0",
};
pub(crate) const VOCABULARY: AbiSymbol = AbiSymbol {
    name: "cce_plugin_vocabulary",
    bytes: b"cce_plugin_vocabulary\0",
};

/// Every symbol the host resolves, in one list for contract tests.
#[cfg(test)]
pub(crate) const ALL_SYMBOLS: &[AbiSymbol] = &[
    ABI_VERSION,
    METADATA,
    FREE_STRING,
    HAS_LIFECYCLE,
    CREATE,
    DESTROY,
    HAS_BM25_GENERATION,
    HAS_EMBEDDING_GENERATION,
    GENERATE_BM25,
    GENERATE_EMBEDDING,
    GENERATE_BM25_BATCH,
    GENERATE_EMBEDDING_BATCH,
    HAS_PARSE,
    PARSE_DOCUMENT,
    HAS_EXTRACT,
    EXTRACT_ENTITIES,
    HAS_GROUP,
    POST_GROUP,
    HAS_GROUP_OVERRIDE,
    GROUP,
    HAS_CHUNK,
    CHUNK,
    HAS_RERANK,
    RERANK,
    HAS_AST_LANGUAGE,
    TREE_SITTER_LANGUAGE,
    LANGUAGE_NAME,
    LANGUAGE_EXTENSIONS,
    QUERY_SCHEME,
    HAS_LANGUAGE_REMAP,
    REMAP_GRAMMAR_LANGUAGE,
    HAS_STDLIB_HEURISTIC,
    CLASSIFY_STDLIB,
    HAS_TEST_FILE_HEURISTIC,
    IS_TEST_FILE,
    HAS_ENTITY_KIND_HEURISTIC,
    ENTITY_KIND,
    HAS_RELATION_EXTRACT,
    EXTRACT_RELATIONS,
    HAS_SYMBOL_EXTRACT,
    EXTRACT_SYMBOLS,
    EXTRACT_IMPORTS,
    EXTRACT_EXPORTS,
    HAS_QUERY_REWRITE,
    REWRITE_QUERY,
    HAS_FUSION,
    FUSION_WEIGHTS,
    HAS_RESULT_FILTER,
    FILTER_RESULTS,
    HAS_FILE_FILTER,
    FILTER_FILE,
    HAS_VOCABULARY,
    VOCABULARY,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The SDK header next to this workspace is the authoritative ABI
    /// definition; every symbol the host resolves must be declared there.
    /// This mirrors the SDK-side header test and closes the header, macro,
    /// host-loader, and guide contract from the host side.
    #[test]
    fn host_symbol_table_matches_sdk_header() {
        const HEADER: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../cce-plugin-sdk/include/cce_plugin.h"
        ));
        for symbol in ALL_SYMBOLS {
            assert!(
                HEADER.contains(symbol.name),
                "SDK header must declare host-loaded symbol {}",
                symbol.name
            );
        }
    }

    #[test]
    fn symbol_bytes_are_nul_terminated_names() {
        for symbol in ALL_SYMBOLS {
            assert_eq!(
                symbol.bytes,
                format!("{}\0", symbol.name).as_bytes(),
                "bytes of {} must be the NUL-terminated name",
                symbol.name
            );
        }
    }
}
