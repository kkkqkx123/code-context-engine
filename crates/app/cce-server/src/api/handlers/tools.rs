//! Tools API handlers
//!
//! This module provides HTTP handlers for tool APIs that support programming tasks.
//! These tools offer functionality similar to LSP features but operate on-demand
//! without side effects.
//!
//! # Available Endpoints
//!
//! - **Compression**: `/api/tools/compress`, `/api/tools/compress/batch`
//! - **AST Diagnosis**: `/api/tools/diagnose`
//! - **File Fold**: `/api/tools/fold`, `/api/tools/fold/batch`
//! - **Keyword Search**: `/api/tools/keyword-search`
//! - **Symbol Lookup**: `/api/tools/symbols`, `/api/tools/references`, `/api/tools/definition`

pub mod compression;
pub mod diagnosis;
pub mod fold;
pub mod keyword;
pub mod symbol;

pub use compression::{handle_compress, handle_compress_batch};
pub use diagnosis::handle_diagnose;
pub use fold::{handle_fold, handle_fold_batch};
pub use keyword::handle_keyword_search;
pub use symbol::{handle_find_references, handle_get_symbols, handle_goto_definition};

/// Explicit orchestrator to wire model conversions.
///
/// Each function destructures its input exhaustively so added orchestrator
/// fields fail compilation instead of dropping silently.
pub(crate) fn keyword_result_from_orch(
    response: cce_orchestrator::KeywordSearchResponse,
) -> cce_api::models::KeywordSearchResult {
    let cce_orchestrator::KeywordSearchResponse {
        query,
        total,
        results,
    } = response;
    cce_api::models::KeywordSearchResult {
        query,
        total,
        results: results
            .into_iter()
            .map(|item| {
                let cce_orchestrator::KeywordSearchItem {
                    chunk_id,
                    score,
                    file_path,
                    title,
                    snippet,
                    start_line,
                    end_line,
                } = item;
                cce_api::models::KeywordSearchItem {
                    chunk_id,
                    score,
                    file_path,
                    title,
                    snippet,
                    start_line,
                    end_line,
                }
            })
            .collect(),
    }
}

fn position_from_view(
    value: cce_server_shared::services::tools::PositionView,
) -> cce_api::models::PositionInfo {
    cce_api::models::PositionInfo {
        row: value.row,
        column: value.column,
    }
}

fn span_from_view(
    value: cce_server_shared::services::tools::SpanView,
) -> cce_api::models::SpanInfo {
    cce_api::models::SpanInfo {
        start_byte: value.start_byte,
        end_byte: value.end_byte,
        start_position: position_from_view(value.start),
        end_position: position_from_view(value.end),
    }
}

fn ast_node_from_view(
    value: cce_server_shared::services::tools::AstNodeView,
) -> cce_api::models::AstNodeInfo {
    cce_api::models::AstNodeInfo {
        kind: value.kind,
        text: value.text,
        span: span_from_view(value.span),
        children: value.children.into_iter().map(ast_node_from_view).collect(),
    }
}

pub(crate) fn diagnose_result_from_view(
    view: cce_server_shared::services::tools::DiagnosisView,
) -> cce_api::models::DiagnoseResult {
    cce_api::models::DiagnoseResult {
        language: view.language,
        is_valid: view.is_valid,
        ast: view.ast.map(ast_node_from_view),
        diagnostics: view
            .diagnostics
            .into_iter()
            .map(|entry| cce_api::models::DiagnosticEntry {
                kind: entry.kind,
                position: position_from_view(entry.position),
                span: entry.span.map(span_from_view),
                message: entry.message,
                precision: entry.precision,
            })
            .collect(),
    }
}

fn symbol_kind_name(kind: cce_orchestrator::SymbolKind) -> String {
    format!("{kind:?}")
}

fn reference_location_from_orch(
    value: cce_orchestrator::ReferenceLocation,
) -> cce_api::models::ReferenceLocation {
    let cce_orchestrator::ReferenceLocation {
        path,
        line,
        column,
        end_line,
        end_column,
        snippet,
        caller_entity,
        callee_file,
        callee_line,
        callee_end_line,
    } = value;
    cce_api::models::ReferenceLocation {
        path,
        line,
        column,
        end_line,
        end_column,
        snippet,
        caller_entity: caller_entity.map(|caller| cce_api::models::CallerEntityInfo {
            name: caller.name,
            kind: symbol_kind_name(caller.kind),
            entity_id: caller.entity_id.0,
        }),
        callee_file,
        callee_line,
        callee_end_line,
    }
}

fn symbol_info_from_orch(value: cce_orchestrator::SymbolInfo) -> cce_api::models::SymbolInfo {
    let cce_orchestrator::SymbolInfo {
        name,
        kind,
        line,
        end_line,
        detail,
        children,
    } = value;
    cce_api::models::SymbolInfo {
        name,
        kind: symbol_kind_name(kind),
        line,
        end_line,
        detail,
        children: children.map(|items| items.into_iter().map(symbol_info_from_orch).collect()),
    }
}

pub(crate) fn find_refs_result_from_orch(
    response: cce_orchestrator::FindReferencesResponse,
) -> cce_api::models::FindReferencesResult {
    let cce_orchestrator::FindReferencesResponse {
        symbol,
        total_count,
        file_count,
        references,
    } = response;
    cce_api::models::FindReferencesResult {
        symbol,
        total_count,
        file_count,
        references: references
            .into_iter()
            .map(|group| {
                let cce_orchestrator::GroupedReferences {
                    path,
                    count,
                    references,
                } = group;
                cce_api::models::GroupedReferences {
                    path,
                    count,
                    references: references
                        .into_iter()
                        .map(reference_location_from_orch)
                        .collect(),
                }
            })
            .collect(),
    }
}

pub(crate) fn get_symbols_result_from_orch(
    response: cce_orchestrator::GetSymbolsResponse,
) -> cce_api::models::GetSymbolsResult {
    let cce_orchestrator::GetSymbolsResponse {
        results,
        success_count,
        fail_count,
    } = response;
    cce_api::models::GetSymbolsResult {
        results: results
            .into_iter()
            .map(|entry| {
                let cce_orchestrator::FileSymbolResult {
                    path,
                    success,
                    symbol_count,
                    symbols,
                    error,
                } = entry;
                cce_api::models::FileSymbolResult {
                    path,
                    success,
                    symbol_count,
                    symbols: symbols
                        .map(|items| items.into_iter().map(symbol_info_from_orch).collect()),
                    error,
                }
            })
            .collect(),
        success_count,
        fail_count,
    }
}

pub(crate) fn goto_definition_result_from_orch(
    response: cce_orchestrator::GotoDefinitionResponse,
) -> cce_api::models::GotoDefinitionResult {
    let cce_orchestrator::GotoDefinitionResponse {
        symbol,
        definitions,
    } = response;
    cce_api::models::GotoDefinitionResult {
        symbol,
        definitions: definitions
            .into_iter()
            .map(|entry| {
                let cce_orchestrator::DefinitionCode {
                    location,
                    name,
                    kind,
                    code,
                    signature,
                } = entry;
                let cce_orchestrator::DefinitionLocation {
                    path,
                    entity_id,
                    line,
                    end_line,
                } = location;
                cce_api::models::DefinitionCode {
                    location: cce_api::models::DefinitionLocation {
                        path,
                        entity_id: entity_id.0,
                        line,
                        end_line,
                    },
                    name,
                    kind: symbol_kind_name(kind),
                    code,
                    signature,
                }
            })
            .collect(),
    }
}
