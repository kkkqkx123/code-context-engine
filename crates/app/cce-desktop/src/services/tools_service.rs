//! Source-level tool commands: folding, compression, diagnosis, keyword
//! search and symbol navigation.
//!
//! Snippet tools (fold, diagnose) take the text the panel holds; file tools
//! (compress, symbols, references, definition) take project paths and resolve
//! against the index on the server.

use cce_api::models::{
    CompressApiResponse, CompressRequest, DiagnoseApiResponse, DiagnoseRequest,
    FindReferencesRequest, FindReferencesResponse, FoldRequest, FoldResponse, GetSymbolsRequest,
    GetSymbolsResponse, GotoDefinitionRequest, GotoDefinitionResponse, KeywordSearchApiResponse,
    KeywordSearchRequest, KeywordTermOperator,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

pub fn fold(
    cx: &App,
    client: Client,
    text: String,
    language: Option<String>,
) -> Task<Result<FoldResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = FoldRequest {
            text,
            language,
            file_name: None,
            max_tokens: None,
            mode: None,
        };
        client.fold(&request).await
    })
}

pub fn compress(
    cx: &App,
    client: Client,
    file_path: String,
    include_entities: bool,
) -> Task<Result<CompressApiResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = CompressRequest {
            file_path,
            include_entities,
            include_groups: false,
        };
        client.compress(&request).await
    })
}

pub fn diagnose(
    cx: &App,
    client: Client,
    code: String,
    language: Option<String>,
    include_ast: bool,
) -> Task<Result<DiagnoseApiResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = DiagnoseRequest {
            code,
            language,
            file_name: None,
            include_ast,
        };
        client.diagnose(&request).await
    })
}

/// Keyword (BM25) search scoped to the active project.
pub fn keyword(
    cx: &App,
    client: Client,
    project_id: i64,
    query: String,
    top_n: usize,
    all_terms: bool,
) -> Task<Result<KeywordSearchApiResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = KeywordSearchRequest {
            query,
            top_n,
            project_id,
            epoch: None,
            offset: 0,
            term_operator: if all_terms {
                KeywordTermOperator::And
            } else {
                KeywordTermOperator::Or
            },
        };
        client.keyword_search(&request).await
    })
}

pub fn symbols(
    cx: &App,
    client: Client,
    project_id: i64,
    paths: Vec<String>,
) -> Task<Result<GetSymbolsResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = GetSymbolsRequest { project_id, paths };
        client.symbols(&request).await
    })
}

pub fn references(
    cx: &App,
    client: Client,
    project_id: i64,
    path: String,
    line: usize,
    symbol: Option<String>,
) -> Task<Result<FindReferencesResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = FindReferencesRequest {
            project_id,
            path,
            line,
            column: None,
            symbol,
            context_lines: None,
            include_snippet: Some(true),
            include_entity_info: Some(true),
        };
        client.references(&request).await
    })
}

pub fn definition(
    cx: &App,
    client: Client,
    project_id: i64,
    path: String,
    line: usize,
    symbol: Option<String>,
) -> Task<Result<GotoDefinitionResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = GotoDefinitionRequest {
            project_id,
            path,
            line,
            column: None,
            symbol,
            include_body: true,
        };
        client.definition(&request).await
    })
}

/// Parse the 1-based line field of the navigation tools.
///
/// A malformed line is reported as an error instead of defaulting to the first
/// line, which would silently answer about the wrong location.
pub fn parse_line(text: &str) -> Result<usize, &'static str> {
    match text.trim().parse::<usize>() {
        Ok(line) if line > 0 => Ok(line),
        _ => Err("line must be a positive number"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_numbers_must_be_positive() {
        assert_eq!(parse_line("42"), Ok(42));
        assert_eq!(parse_line(" 7 "), Ok(7));
        assert!(parse_line("0").is_err());
        assert!(parse_line("").is_err());
        assert!(parse_line("twelve").is_err());
    }
}
