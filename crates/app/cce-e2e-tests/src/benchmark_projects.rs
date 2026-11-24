//! Central registry for benchmark projects.
//!
//! Each variant bundles the fixture pair and the relevance judgments that
//! were previously paired manually at every call site. Tools iterate over
//! the registry instead of importing each judgment module separately.

use crate::FixtureSpec;
use crate::annotation_review::{AnnotationReviewConfig, ContentMode, RecallMode};
use crate::bench_data::{QueryData, RelevanceJudgment};
use crate::bench_gen::ProjectConfig;
use crate::query_review::QueryReviewConfig;
use crate::relation_review::RelationReviewConfig;
use crate::review_filter::ReviewFilterOptions;

/// Benchmark project with its fixture pair and relevance judgments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BenchmarkProject {
    Flask,
    OnceCell,
    Ripgrep,
    Gin,
    Express,
    SpringBoot,
    JacksonCore,
    Mediatr,
    MediaServer,
}

impl BenchmarkProject {
    /// Stable directory and report name for the project.
    pub fn name(self) -> &'static str {
        match self {
            Self::Flask => "flask",
            Self::OnceCell => "once_cell",
            Self::Ripgrep => "ripgrep",
            Self::Gin => "gin",
            Self::Express => "express",
            Self::SpringBoot => "spring_boot",
            Self::JacksonCore => "jackson_core",
            Self::Mediatr => "mediatr",
            Self::MediaServer => "media_server",
        }
    }

    /// Primary fixture under test.
    pub fn target_spec(self) -> FixtureSpec {
        match self {
            Self::Flask => FixtureSpec::python_flask(),
            Self::OnceCell => FixtureSpec::rust_once_cell(),
            Self::Ripgrep => FixtureSpec::rust_ripgrep(),
            Self::Gin => FixtureSpec::go_gin(),
            Self::Express => FixtureSpec::javascript_express(),
            Self::SpringBoot => FixtureSpec::java_spring_boot(),
            Self::JacksonCore => FixtureSpec::java_jackson_core(),
            Self::Mediatr => FixtureSpec::csharp_mediatr(),
            Self::MediaServer => FixtureSpec::cpp_media_server(),
        }
    }

    /// Distractor fixture paired with the target.
    pub fn distractor_spec(self) -> FixtureSpec {
        match self {
            Self::Flask => FixtureSpec::python_basic(),
            Self::OnceCell | Self::Ripgrep => FixtureSpec::rust_distractor(),
            Self::Gin => FixtureSpec::go_basic(),
            Self::Express => FixtureSpec::javascript_basic(),
            Self::SpringBoot | Self::JacksonCore => FixtureSpec::java_basic(),
            Self::Mediatr => FixtureSpec::csharp_basic(),
            Self::MediaServer => FixtureSpec::cpp_basic(),
        }
    }

    /// Ground truth judgments for the project.
    pub fn judgments(self) -> Vec<RelevanceJudgment> {
        match self {
            Self::Flask => crate::judgments::flask::flask_relevance_judgments(),
            Self::OnceCell => crate::judgments::once_cell::once_cell_relevance_judgments(),
            Self::Ripgrep => crate::judgments::ripgrep::ripgrep_relevance_judgments(),
            Self::Gin => crate::judgments::gin::gin_relevance_judgments(),
            Self::Express => crate::judgments::express::express_relevance_judgments(),
            Self::SpringBoot => crate::judgments::spring_boot::spring_boot_relevance_judgments(),
            Self::JacksonCore => crate::judgments::jackson_core::jackson_core_relevance_judgments(),
            Self::Mediatr => crate::judgments::mediatr::mediatr_relevance_judgments(),
            Self::MediaServer => crate::judgments::media_server::media_server_relevance_judgments(),
        }
    }

    /// Scenario language directory under `outputs/scenarios/`.
    pub fn language(self) -> &'static str {
        match self {
            Self::Flask => "python",
            Self::OnceCell | Self::Ripgrep => "rust",
            Self::Gin => "go",
            Self::Express => "javascript",
            Self::SpringBoot | Self::JacksonCore => "java",
            Self::Mediatr => "csharp",
            Self::MediaServer => "cpp",
        }
    }

    /// Hub entity names for relation-review ego graphs.
    pub fn hub_names(self) -> Vec<&'static str> {
        match self {
            Self::Flask => vec!["route", "run", "Flask"],
            Self::OnceCell => vec!["new", "get", "set", "get_or_init"],
            Self::Ripgrep => vec!["new", "get", "set", "run"],
            Self::Gin => vec!["GET", "POST", "Group", "Engine"],
            Self::Express => vec!["get", "post", "use", "listen"],
            Self::SpringBoot => vec!["Controller", "Service", "Repository"],
            Self::JacksonCore => vec!["ObjectMapper", "JsonParser", "JsonGenerator"],
            Self::Mediatr => vec!["Send", "Publish", "Mediator"],
            Self::MediaServer => vec!["create", "open", "read", "write"],
        }
    }

    /// Query set converted to benchmark data generation input.
    pub fn query_data(self) -> Vec<QueryData> {
        self.judgments()
            .iter()
            .map(|judgment| QueryData {
                id: judgment.id.clone(),
                text: judgment.query_text.clone(),
                query_type: judgment.query_type,
                relevant_names: Vec::new(),
                irrelevant_names: Vec::new(),
            })
            .collect()
    }

    /// Benchmark data generation config for the project.
    pub fn gen_config(self) -> ProjectConfig {
        ProjectConfig {
            name: self.name(),
            target_spec: self.target_spec(),
            distractor_spec: self.distractor_spec(),
            queries: self.query_data(),
        }
    }

    /// Query-review config with the shared top-k default.
    pub fn query_review_config(self) -> QueryReviewConfig {
        QueryReviewConfig {
            project: self.name(),
            language: self.language(),
            spec: self.target_spec(),
            judgments: self.judgments(),
            top_k: 10,
            scenario_suffix: None,
        }
    }

    /// Relation-review config with the shared traversal defaults.
    pub fn relation_review_config(self) -> RelationReviewConfig {
        RelationReviewConfig {
            project: self.name(),
            language: self.language(),
            spec: self.target_spec(),
            judgments: self.judgments(),
            hub_names: self.hub_names(),
            max_seeds: 14,
            depth: 2,
            limit: 20,
        }
    }

    /// Annotation-review config with the shared content defaults.
    pub fn annotation_review_config(self) -> AnnotationReviewConfig {
        AnnotationReviewConfig {
            project: self.name(),
            language: self.language(),
            spec: self.target_spec(),
            top_k: 10,
            content_mode: ContentMode::Chunk,
            recall: RecallMode::Emb,
            expansion: true,
            filter: ReviewFilterOptions::default(),
        }
    }

    /// Human readable fixture pair label used in sweep reports.
    pub fn fixture_label(self) -> String {
        format!(
            "{} + {} distractor",
            self.name(),
            self.distractor_spec().subdirectory
        )
    }

    /// Every registered project in a stable order.
    pub fn all() -> Vec<Self> {
        vec![
            Self::Flask,
            Self::OnceCell,
            Self::Ripgrep,
            Self::Gin,
            Self::Express,
            Self::SpringBoot,
            Self::JacksonCore,
            Self::Mediatr,
            Self::MediaServer,
        ]
    }

    /// Look up a project by its stable name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|project| project.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_project_resolves_its_fixture_root() {
        for project in BenchmarkProject::all() {
            let root = project.target_spec().source_root();
            assert!(
                root.exists(),
                "missing fixture root for {}: {}",
                project.name(),
                root.display()
            );
            assert!(
                !project.judgments().is_empty(),
                "empty judgment set for {}",
                project.name()
            );
        }
    }

    #[test]
    fn project_names_are_unique() {
        let mut names: Vec<&str> = BenchmarkProject::all()
            .iter()
            .map(|project| project.name())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BenchmarkProject::all().len());
    }

    #[test]
    fn from_name_round_trips() {
        for project in BenchmarkProject::all() {
            assert_eq!(BenchmarkProject::from_name(project.name()), Some(project));
        }
        assert_eq!(BenchmarkProject::from_name("unknown_project"), None);
    }
}
