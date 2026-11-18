//! Indexing commands: full pass, incremental update, single-file parse and
//! summary generation.
//!
//! The full pass and the summary run on the long-task budget inside the
//! client; the single-file parse is interactive. The panel never sets a
//! timeout of its own.

use cce_api::models::{
    IncrementalIndexRequest, IncrementalIndexResponse, IndexRequest, IndexResponse,
    IndexStatsResponse, ParseRequest, ParseResponse, ProjectConfig, SummaryRequest,
    SummaryResponse,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

/// Index settings taken from the project record.
///
/// The web console treats the project record as the single source of truth for
/// what a project contains. The desktop used to send empty exclusions and a
/// hard-coded `respect_gitignore`, so "index this project" could sweep in
/// directories the record excludes and produce an index the web side never
/// sees. Defaults now come from the record; the panel only supplies overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDefaults {
    pub path: String,
    pub extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub respect_gitignore: bool,
    pub ignore_patterns: Vec<String>,
}

impl IndexDefaults {
    /// `None` when the record has no root: a project without a root cannot be
    /// indexed at all, and guessing a path is exactly the old bug.
    pub fn from_record(project: &ProjectConfig) -> Option<Self> {
        let path = project.root_path.trim();
        if path.is_empty() {
            return None;
        }
        Some(Self {
            path: path.to_string(),
            extensions: project.extensions.clone(),
            exclude_dirs: project.exclude_dirs.clone(),
            respect_gitignore: project.respect_gitignore,
            ignore_patterns: project.ignore_patterns.clone(),
        })
    }

    fn request(&self, project_id: i64, overrides: &IndexOverrides) -> IndexRequest {
        IndexRequest {
            project_id,
            path: overrides.path.clone().unwrap_or_else(|| self.path.clone()),
            extensions: overrides
                .extensions
                .clone()
                .unwrap_or_else(|| self.extensions.clone()),
            exclude_dirs: overrides
                .exclude_dirs
                .clone()
                .unwrap_or_else(|| self.exclude_dirs.clone()),
            respect_gitignore: overrides
                .respect_gitignore
                .unwrap_or(self.respect_gitignore),
            ignore_patterns: overrides
                .ignore_patterns
                .clone()
                .unwrap_or_else(|| self.ignore_patterns.clone()),
            custom_gitignore: None,
        }
    }
}

/// Panel-supplied replacements for the record's defaults.
///
/// Every field is optional on purpose: `None` means "use the record", so an
/// untouched field can never clear an exclusion the record declares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexOverrides {
    pub path: Option<String>,
    pub extensions: Option<Vec<String>>,
    pub exclude_dirs: Option<Vec<String>>,
    pub respect_gitignore: Option<bool>,
    pub ignore_patterns: Option<Vec<String>>,
}

/// Full index pass with the project record as the source of truth.
pub fn full(
    cx: &App,
    client: Client,
    project_id: i64,
    overrides: IndexOverrides,
) -> Task<Result<IndexResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let list = client.list_projects().await?;
        let record = list
            .projects
            .iter()
            .find(|project| record_matches(project, project_id));
        let Some(record) = record else {
            return Err(refused(
                "PROJECT_NOT_FOUND",
                format!("no project record for id {project_id}"),
            ));
        };
        let Some(defaults) = IndexDefaults::from_record(record) else {
            return Err(refused(
                "PROJECT_ROOT_MISSING",
                format!("project {project_id} has no root path to index"),
            ));
        };
        let request = defaults.request(project_id, &overrides);
        client.index(&request).await
    })
}

/// The record's id is a string on the wire while every call takes an integer.
fn record_matches(project: &ProjectConfig, project_id: i64) -> bool {
    project.id.trim().parse::<i64>().ok() == Some(project_id)
}

fn refused(code: &str, message: String) -> ClientError {
    ClientError::Rejected {
        method: cce_client::ApiMethod::Post,
        path: "/api/index".to_string(),
        status: 400,
        code: Some(code.to_string()),
        message,
        details: None,
    }
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

    fn record() -> ProjectConfig {
        ProjectConfig {
            id: "7".to_string(),
            name: "core".to_string(),
            root_path: " /src/core ".to_string(),
            extensions: vec!["rs".to_string()],
            exclude_dirs: vec!["target".to_string()],
            respect_gitignore: true,
            ignore_patterns: vec!["*.snap".to_string()],
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_indexed: None,
        }
    }

    #[test]
    fn defaults_come_from_the_record() {
        let defaults = IndexDefaults::from_record(&record()).expect("root present");
        let request = defaults.request(7, &IndexOverrides::default());
        assert_eq!(request.path, "/src/core");
        assert_eq!(request.extensions, vec!["rs".to_string()]);
        assert_eq!(request.exclude_dirs, vec!["target".to_string()]);
        assert_eq!(request.ignore_patterns, vec!["*.snap".to_string()]);
        assert!(request.respect_gitignore);
    }

    #[test]
    fn overrides_replace_only_the_fields_they_carry() {
        let defaults = IndexDefaults::from_record(&record()).expect("root present");
        let request = defaults.request(
            7,
            &IndexOverrides {
                extensions: Some(vec!["ts".to_string()]),
                ..IndexOverrides::default()
            },
        );
        assert_eq!(request.extensions, vec!["ts".to_string()]);
        assert_eq!(
            request.exclude_dirs,
            vec!["target".to_string()],
            "an untouched exclusion survives the override"
        );
        assert_eq!(request.path, "/src/core");
    }

    #[test]
    fn a_record_without_a_root_cannot_be_indexed() {
        let mut project = record();
        project.root_path = "   ".to_string();
        assert!(IndexDefaults::from_record(&project).is_none());
    }

    #[test]
    fn only_the_matching_record_is_used() {
        let project = record();
        assert!(record_matches(&project, 7));
        assert!(!record_matches(&project, 8));
    }

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
