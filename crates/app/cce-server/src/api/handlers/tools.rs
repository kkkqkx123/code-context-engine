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

fn position_from_orch(value: cce_types::position::Position) -> cce_api::models::PositionInfo {
    let cce_types::position::Position { row, column } = value;
    cce_api::models::PositionInfo { row, column }
}

fn span_from_orch(value: cce_types::position::Span) -> cce_api::models::SpanInfo {
    let cce_types::position::Span {
        start_byte,
        end_byte,
        start_position,
        end_position,
    } = value;
    cce_api::models::SpanInfo {
        start_byte,
        end_byte,
        start_position: position_from_orch(start_position),
        end_position: position_from_orch(end_position),
    }
}

fn ast_node_from_orch(
    value: cce_parser::parser::ast_parser::AstNode,
) -> cce_api::models::AstNodeInfo {
    let cce_parser::parser::ast_parser::AstNode {
        kind,
        text,
        span,
        children,
    } = value;
    cce_api::models::AstNodeInfo {
        kind,
        text,
        span: span_from_orch(span),
        children: children.into_iter().map(ast_node_from_orch).collect(),
    }
}

pub(crate) fn diagnose_result_from_orch(
    response: cce_orchestrator::DiagnosisResponse,
) -> cce_api::models::DiagnoseResult {
    let cce_orchestrator::DiagnosisResponse {
        language,
        is_valid,
        ast,
        diagnostics,
    } = response;
    cce_api::models::DiagnoseResult {
        language,
        is_valid,
        ast: ast.map(ast_node_from_orch),
        diagnostics: diagnostics
            .into_iter()
            .map(|entry| {
                let cce_orchestrator::Diagnostic {
                    kind,
                    position,
                    span,
                    message,
                    precision,
                } = entry;
                cce_api::models::DiagnosticEntry {
                    kind: format!("{kind:?}"),
                    position: position_from_orch(position),
                    span: span.map(span_from_orch),
                    message,
                    precision: format!("{precision:?}"),
                }
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
