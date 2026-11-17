//! Indexing commands: full pass, incremental update, single-file parse and
//! summary generation.
//!
//! The full pass and the summary run on the long-task budget inside the
//! client; the single-file parse is interactive. The panel never sets a
//! timeout of its own.

use cce_api::models::{
    IncrementalIndexRequest, IncrementalIndexResponse, IndexRequest, IndexResponse,
    IndexStatsResponse, ParseRequest, ParseResponse, SummaryRequest, SummaryResponse,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

pub fn full(
    cx: &App,
    client: Client,
    project_id: i64,
    path: String,
    extensions: Vec<String>,
) -> Task<Result<IndexResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = IndexRequest {
            project_id,
            path,
            extensions,
            exclude_dirs: Vec::new(),
            respect_gitignore: true,
            ignore_patterns: Vec::new(),
            custom_gitignore: None,
        };
        client.index(&request).await
    })
}

pub fn incremental(
    cx: &App,
    client: Client,
    project_id: i64,
    add: Vec<String>,
    remove: Vec<String>,
    force: bool,
) -> Task<Result<IncrementalIndexResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = IncrementalIndexRequest {
            project_id,
            files_to_index: add,
            files_to_remove: remove,
            force_reindex: force,
        };
        client.incremental_index(&request).await
    })
}

pub fn parse(
    cx: &App,
    client: Client,
    file_path: String,
) -> Task<Result<ParseResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.parse_file(&ParseRequest { file_path }).await
    })
}

pub fn summary(
    cx: &App,
    client: Client,
    files: Vec<String>,
    directories: Vec<String>,
    recursive: bool,
) -> Task<Result<SummaryResponse, ClientError>> {
    let request = summary_request(files, directories, recursive);
    runtime::spawn(cx, async move { client.summary(&request).await })
}

/// Summary request for the panel's two path fields.
///
/// The file cap mirrors the contract default: asking for more would only make
/// the server truncate, so the panel does not offer it.
fn summary_request(
    files: Vec<String>,
    directories: Vec<String>,
    recursive: bool,
) -> SummaryRequest {
    SummaryRequest {
        file_paths: files,
        directory_paths: directories,
        extensions: Vec::new(),
        exclude_dirs: Vec::new(),
        respect_gitignore: true,
        ignore_patterns: Vec::new(),
        recursive,
        max_files: 100,
    }
}

pub fn stats(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<IndexStatsResponse, ClientError>> {
    runtime::spawn(cx, async move { client.index_stats(project_id).await })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_request_keeps_both_path_fields() {
        let request = summary_request(
            vec!["src/main.rs".to_string()],
            vec!["src/parser".to_string()],
            false,
        );
        assert_eq!(request.file_paths, vec!["src/main.rs".to_string()]);
        assert_eq!(request.directory_paths, vec!["src/parser".to_string()]);
        assert!(!request.recursive);
        assert_eq!(request.max_files, 100);
        assert!(request.respect_gitignore);
    }
}
