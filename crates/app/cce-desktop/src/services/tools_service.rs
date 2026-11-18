//! Source-level tool commands: folding, compression, diagnosis, keyword
//! search and symbol navigation.
//!
//! Snippet tools (fold, diagnose) take the text the panel holds; file tools
//! (compress, symbols, references, definition) take project paths and resolve
//! against the index on the server.

use cce_api::models::{
    BatchCompressRequest, BatchCompressResponse, BatchFoldItem, BatchFoldRequest,
    BatchFoldResponse, CompressApiResponse, CompressRequest, DiagnoseApiResponse, DiagnoseRequest,
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

/// Split a multi-snippet field into batch fold items.
///
/// A line of `---` separates snippets, and a snippet may open with a
/// `# name` line naming it. Both rules are visible in the placeholder text, so
/// the panel never asks the caller to guess an undocumented format.
pub fn batch_items(text: &str, language: Option<String>) -> Vec<BatchFoldItem> {
    let mut items = Vec::new();
    for (index, block) in text.split("\n---\n").enumerate() {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let (name, body) = match block.strip_prefix("# ") {
            Some(rest) => match rest.split_once('\n') {
                Some((name, body)) => (Some(name.trim().to_string()), body.trim().to_string()),
                None => (Some(rest.trim().to_string()), String::new()),
            },
            None => (None, block.to_string()),
        };
        if body.is_empty() {
            continue;
        }
        items.push(BatchFoldItem {
            id: name.unwrap_or_else(|| format!("item-{}", index + 1)),
            text: body,
            language: language.clone(),
            file_name: None,
            max_tokens: None,
            mode: None,
        });
    }
    items
}

pub fn fold_batch(
    cx: &App,
    client: Client,
    text: String,
    language: Option<String>,
) -> Task<Result<BatchFoldResponse, ClientError>> {
    let items = batch_items(&text, language.clone());
    runtime::spawn(cx, async move {
        let request = BatchFoldRequest {
            items,
            language,
            max_tokens: None,
            mode: None,
            max_concurrency: None,
        };
        client.fold_batch(&request).await
    })
}

pub fn compress_batch(
    cx: &App,
    client: Client,
    paths: Vec<String>,
) -> Task<Result<BatchCompressResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = BatchCompressRequest {
            file_paths: paths,
            include_entities: Some(true),
            include_groups: None,
            max_concurrency: 4,
        };
        client.compress_batch(&request).await
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
