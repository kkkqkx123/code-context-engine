//! Text pipeline trait and router
//!
//! This module provides the TextPipeline trait for document processing
//! and PipelineRouter for routing to appropriate implementations.
//!
//! # Design
//!
//! Uses global singleton pattern with `std::sync::OnceLock` for:
//! - Single PipelineRouter instance shared across the entire application
//! - Eliminates redundant initialization in tests and production
//!
//! This design is lightweight since document parsers have minimal initialization cost
//! (unlike tree-sitter which requires loading native libraries and compiling queries).

use std::sync::OnceLock;

use tracing::{debug, info, warn};

use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::ParseError;
use cce_types::TestInfo;
use cce_types::ast_to_nl::options::OutputMode;

use super::types::{DocSummary, DocType, DocumentClassification};

/// Which concrete pipeline handles a [`DocType::Config`] file.
///
/// Selected by the detected language instead of re-matching extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigPipeline {
    Json,
    Toml,
    Yaml,
    Xml,
    Plain,
}

/// Text processing pipeline interface
///
/// Each document type has its own pipeline implementation.
pub trait TextPipeline {
    /// Parsed node type
    type ParsedNode: Clone;
    /// Group type
    type Group;

    /// Parse document content
    fn parse(&self, content: &str) -> Result<Vec<Self::ParsedNode>, ParseError>;

    /// Group parsed nodes
    fn group(
        &self,
        nodes: Vec<Self::ParsedNode>,
        file_path: &str,
    ) -> Result<Vec<Self::Group>, ParseError>;

    /// Chunk groups into final results.
    ///
    /// `classification` is derived once per file at the pipeline entry (see
    /// [`TextPipeline::process`]); chunkers must reuse it verbatim instead of
    /// re-deriving labels from the path.
    fn chunk(
        &self,
        groups: Vec<Self::Group>,
        config: &ChunkingConfig,
        file_path: &str,
        output_mode: OutputMode,
        classification: &DocumentClassification,
    ) -> Result<Vec<ChunkedResult>, ParseError>;

    /// Generate document summary
    fn summarize(
        &self,
        nodes: &[Self::ParsedNode],
        groups: &[Self::Group],
        file_path: &str,
    ) -> Option<DocSummary>;

    /// Generate only the document summary, without producing chunks.
    ///
    /// Runs the parse → group → summarize stages so callers that need just
    /// the [`DocSummary`] (e.g. the hot-update summary processor) share the
    /// exact same summary source as the full-index chunking stage.
    fn summarize_document(&self, content: &str, file_path: &str) -> Option<DocSummary> {
        let nodes = self.parse(content).ok()?;
        let groups = self.group(nodes.clone(), file_path).ok()?;
        self.summarize(&nodes, &groups, file_path)
    }

    /// Process document (full pipeline)
    fn process(
        &self,
        content: &str,
        file_path: &str,
        config: &ChunkingConfig,
        output_mode: OutputMode,
    ) -> Result<(Vec<ChunkedResult>, Option<DocSummary>), ParseError> {
        let nodes = self.parse(content)?;

        let groups = self.group(nodes.clone(), file_path)?;

        let summary = self.summarize(&nodes, &groups, file_path);

        // Single classification derivation per file: every downstream stage
        // reuses these labels.
        let classification = DocumentClassification::detect(file_path);
        let mut chunks = self.chunk(groups, config, file_path, output_mode, &classification)?;

        // Document/config/plain-text chunks carry no AST-level signal, so the
        // generic `tests/` path rule is applied once at the pipeline entry.
        let file_test_info = TestInfo::from_path(None, file_path);
        for chunk in &mut chunks {
            chunk.metadata.test_info = chunk.metadata.test_info.merge(&file_test_info);
        }

        Ok((chunks, summary))
    }
}

/// Global singleton PipelineRouter instance
///
/// This ensures all parts of the application (including tests) share the same
/// PipelineRouter instance, eliminating redundant initialization.
static GLOBAL_ROUTER: OnceLock<PipelineRouter> = OnceLock::new();

/// Pipeline router
///
/// Routes to appropriate pipeline based on file extension.
///
/// # Global Singleton
///
/// Use `PipelineRouter::global()` to access the global instance instead of creating
/// new instances. This ensures efficient resource usage across tests and production.
#[derive(Clone)]
pub struct PipelineRouter {
    /// Markdown pipeline
    markdown: super::markdown::MarkdownPipeline,
    /// Plain text pipeline
    plain: super::plain::PlainTextPipeline,
    /// JSON pipeline
    json: super::json::JsonPipeline,
    /// XML pipeline
    xml: super::xml::XmlPipeline,
    /// TOML pipeline
    toml: super::toml::TomlPipeline,
    /// YAML pipeline
    yaml: super::yaml::YamlPipeline,
}

impl PipelineRouter {
    /// Get the global PipelineRouter instance
    ///
    /// This is the recommended way to access PipelineRouter in both production
    /// and tests. Using the global instance ensures:
    /// - Single instance shared across all code
    /// - No redundant initialization
    /// - Efficient memory usage
    pub fn global() -> &'static Self {
        GLOBAL_ROUTER.get_or_init(|| {
            info!("PipelineRouter initialized");
            Self::new()
        })
    }

    /// Create a new pipeline router
    ///
    /// Note: Prefer using `PipelineRouter::global()` instead of this method.
    /// This constructor is kept for backward compatibility and special cases.
    pub fn new() -> Self {
        debug!("Creating PipelineRouter");
        Self {
            markdown: super::markdown::MarkdownPipeline::new(),
            plain: super::plain::PlainTextPipeline::new(),
            json: super::json::JsonPipeline::new(),
            xml: super::xml::XmlPipeline::new(),
            toml: super::toml::TomlPipeline::new(),
            yaml: super::yaml::YamlPipeline::new(),
        }
    }

    /// Get document type from file path.
    ///
    /// Pure delegation to the unified detection chain
    /// ([`cce_types::LanguageInfo::detect_from_path`]): the router no
    /// longer keeps its own extension table. Structured config formats
    /// (JSON/YAML/TOML/XML) report [`DocType::Config`]; the concrete
    /// sub-pipeline is selected by the detected language at
    /// summarize/process time (see [`Self::select_config_pipeline`]).
    pub fn get_doc_type(file_path: &str) -> DocType {
        Self::doc_type_for(&cce_types::LanguageInfo::detect_from_path(file_path))
    }

    /// Doc type derived from already-detected language information.
    fn doc_type_for(info: &cce_types::LanguageInfo) -> DocType {
        use cce_types::{FileType, Language};

        match info.file_type {
            FileType::Config => match info.language {
                Language::Json | Language::Toml | Language::Yaml | Language::Xml => DocType::Config,
                _ => DocType::PlainText,
            },
            FileType::Documentation => {
                // Markdown rules: `.md`/`.markdown` extensions or well-known
                // extensionless doc names (README, LICENSE, ...). Other
                // documentation formats (rst/adoc) stay on the plain-text
                // pipeline — their dedicated RST chunking lives there.
                let markdown_ext = info
                    .extensions
                    .first()
                    .map(|ext| ext == "md" || ext == "markdown")
                    .unwrap_or(true);
                if markdown_ext {
                    DocType::Markdown
                } else {
                    DocType::PlainText
                }
            }
            _ => DocType::PlainText,
        }
    }

    /// Which concrete pipeline handles a [`DocType::Config`] file.
    ///
    /// Selected by the detected language instead of re-matching extensions.
    fn select_config_pipeline(file_path: &str) -> ConfigPipeline {
        use cce_types::Language;

        match cce_types::LanguageInfo::detect_from_path(file_path).language {
            Language::Json => ConfigPipeline::Json,
            Language::Toml => ConfigPipeline::Toml,
            Language::Yaml => ConfigPipeline::Yaml,
            Language::Xml => ConfigPipeline::Xml,
            _ => ConfigPipeline::Plain,
        }
    }

    /// Generate only the document summary for `content`, dispatching to the
    /// pipeline selected by [`Self::get_doc_type`]. Returns `None` when the
    /// selected pipeline fails to parse or groups the content.
    pub fn summarize_only(&self, content: &str, file_path: &str) -> Option<DocSummary> {
        let doc_type = Self::get_doc_type(file_path);
        match doc_type {
            DocType::Markdown => self.markdown.summarize_document(content, file_path),
            DocType::Xml => self.xml.summarize_document(content, file_path),
            DocType::Config => match Self::select_config_pipeline(file_path) {
                ConfigPipeline::Json => self.json.summarize_document(content, file_path),
                ConfigPipeline::Toml => self.toml.summarize_document(content, file_path),
                ConfigPipeline::Yaml => self.yaml.summarize_document(content, file_path),
                ConfigPipeline::Xml => self.xml.summarize_document(content, file_path),
                ConfigPipeline::Plain => self.plain.summarize_document(content, file_path),
            },
            DocType::PlainText => self.plain.summarize_document(content, file_path),
        }
    }

    /// Process document using appropriate pipeline
    pub fn process(
        &self,
        content: &str,
        file_path: &str,
        config: &ChunkingConfig,
        output_mode: OutputMode,
    ) -> Result<(Vec<ChunkedResult>, Option<DocSummary>), ParseError> {
        let doc_type = Self::get_doc_type(file_path);

        let result = match doc_type {
            DocType::Markdown => self
                .markdown
                .process(content, file_path, config, output_mode),
            DocType::Xml => self.xml.process(content, file_path, config, output_mode),
            DocType::Config => {
                // Route to the specific config pipeline by detected language
                // (single-source; no extension rematch).
                match Self::select_config_pipeline(file_path) {
                    ConfigPipeline::Json => {
                        self.json.process(content, file_path, config, output_mode)
                    }
                    ConfigPipeline::Toml => {
                        self.toml.process(content, file_path, config, output_mode)
                    }
                    ConfigPipeline::Yaml => {
                        self.yaml.process(content, file_path, config, output_mode)
                    }
                    ConfigPipeline::Xml => {
                        self.xml.process(content, file_path, config, output_mode)
                    }
                    ConfigPipeline::Plain => {
                        self.plain.process(content, file_path, config, output_mode)
                    }
                }
            }
            DocType::PlainText => self.plain.process(content, file_path, config, output_mode),
        };

        if let Err(e) = &result {
            warn!(
                file_path = %file_path,
                doc_type = ?doc_type,
                error = %e,
                "Document processing failed"
            );
        }

        result
    }

    /// Process document and return only chunks (for backward compatibility)
    pub fn process_chunks_only(
        &self,
        content: &str,
        file_path: &str,
        config: &ChunkingConfig,
    ) -> Result<Vec<ChunkedResult>, ParseError> {
        let (chunks, _) = self.process(content, file_path, config, OutputMode::default())?;
        Ok(chunks)
    }

    /// Process a document, consulting `FormatParse` plugins first.
    ///
    /// Three-tier order: override-tier plugins (priority ≥ 0, first
    /// non-empty [`PluginDocument`] wins) → built-in 6-pipeline routing →
    /// below-builtin fallback plugins (negative priority, only when the
    /// built-in produced no chunks).
    pub fn process_with_plugins(
        &self,
        content: &str,
        file_path: &str,
        config: &ChunkingConfig,
        output_mode: OutputMode,
        registry: &cce_plugin::PluginRegistry,
    ) -> Result<(Vec<ChunkedResult>, Option<DocSummary>), ParseError> {
        // Documents carry no language; plugins are filtered by file pattern only.
        let (above, below) = registry.get_override_plugins(
            cce_plugin::PluginCapability::FormatParse,
            Some(file_path),
            None,
        );

        if let Some(parsed) =
            self.process_with_plugin_list(content, file_path, config, output_mode, &above)
        {
            return Ok(parsed);
        }

        let builtin = self.process(content, file_path, config, output_mode)?;

        if builtin.0.is_empty() {
            if let Some(parsed) =
                self.process_with_plugin_list(content, file_path, config, output_mode, &below)
            {
                return Ok(parsed);
            }
        }
        Ok(builtin)
    }

    /// Run a pre-filtered `FormatParse` plugin list; returns `None` when no
    /// plugin produced a non-empty document.
    fn process_with_plugin_list(
        &self,
        content: &str,
        file_path: &str,
        config: &ChunkingConfig,
        output_mode: OutputMode,
        parsers: &[&std::sync::Arc<dyn cce_plugin::CodePlugin>],
    ) -> Option<(Vec<ChunkedResult>, Option<DocSummary>)> {
        for plugin in parsers {
            match plugin.parse_document(content, file_path) {
                Ok(Some(doc)) if !doc.entities.is_empty() => {
                    let pipeline = super::plugin::PluginDocumentPipeline::new(config.clone());
                    let mut chunks = pipeline.process(&doc, file_path, output_mode);

                    // Same file-level test-info rule the built-in pipelines apply.
                    let file_test_info = TestInfo::from_path(None, file_path);
                    for chunk in &mut chunks {
                        chunk.metadata.test_info = chunk.metadata.test_info.merge(&file_test_info);
                    }

                    return Some((chunks, None));
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(
                        file_path = %file_path,
                        plugin = %plugin.metadata().id,
                        error = %e,
                        "Plugin parse_document failed; falling back to built-in pipeline"
                    );
                }
            }
        }
        None
    }
}

impl Default for PipelineRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_doc_type() {
        assert_eq!(PipelineRouter::get_doc_type("test.md"), DocType::Markdown);
        assert_eq!(
            PipelineRouter::get_doc_type("test.markdown"),
            DocType::Markdown
        );
        // XML is a structured config format at the routing layer; its
        // dedicated sub-pipeline is selected by language at process time.
        assert_eq!(PipelineRouter::get_doc_type("test.xml"), DocType::Config);
        assert_eq!(PipelineRouter::get_doc_type("test.toml"), DocType::Config);
        assert_eq!(PipelineRouter::get_doc_type("test.yaml"), DocType::Config);
        assert_eq!(PipelineRouter::get_doc_type("test.json"), DocType::Config);
        // Build-config names without a structured-config language stay on
        // the plain-text pipeline (their chunking has no dedicated parser).
        assert_eq!(
            PipelineRouter::get_doc_type("CMakeLists.txt"),
            DocType::PlainText
        );
        assert_eq!(PipelineRouter::get_doc_type("Makefile"), DocType::PlainText);
        assert_eq!(
            PipelineRouter::get_doc_type("conf/app.ini"),
            DocType::PlainText
        );
        assert_eq!(PipelineRouter::get_doc_type("test.txt"), DocType::PlainText);
        assert_eq!(PipelineRouter::get_doc_type("test.log"), DocType::PlainText);
        // RST/ADOC documentation keeps the plain-text pipeline (RST-specific
        // chunking lives there); schema definitions have no dedicated
        // document pipeline either.
        assert_eq!(
            PipelineRouter::get_doc_type("docs/x.rst"),
            DocType::PlainText
        );
        assert_eq!(
            PipelineRouter::get_doc_type("docs/y.adoc"),
            DocType::PlainText
        );
        assert_eq!(
            PipelineRouter::get_doc_type("api/user.proto"),
            DocType::PlainText
        );
        assert_eq!(
            PipelineRouter::get_doc_type("test.unknown"),
            DocType::PlainText
        );
        // HTML files are handled as source code files, not documents
        assert_eq!(
            PipelineRouter::get_doc_type("test.html"),
            DocType::PlainText
        );
        assert_eq!(PipelineRouter::get_doc_type("test.htm"), DocType::PlainText);
    }

    #[test]
    fn test_get_doc_type_extensionless_docs() {
        assert_eq!(PipelineRouter::get_doc_type("README"), DocType::Markdown);
        assert_eq!(PipelineRouter::get_doc_type("readme"), DocType::Markdown);
        assert_eq!(PipelineRouter::get_doc_type("CHANGELOG"), DocType::Markdown);
        assert_eq!(PipelineRouter::get_doc_type("LICENSE"), DocType::Markdown);
        assert_eq!(
            PipelineRouter::get_doc_type("docs/COPYING"),
            DocType::Markdown
        );
        assert_eq!(
            PipelineRouter::get_doc_type("docs/guide"),
            DocType::PlainText
        );
    }

    #[test]
    fn test_global_singleton() {
        // Get global instance twice - should be the same instance
        let router1 = PipelineRouter::global();
        let router2 = PipelineRouter::global();

        // Both references should point to the same instance
        // (We can't compare references directly, but we can verify they work)
        let config = ChunkingConfig::default();
        let content = "# Test\n\nContent";
        let result1 = router1.process(content, "test.md", &config, OutputMode::default());
        let result2 = router2.process(content, "test.md", &config, OutputMode::default());

        // Both should succeed and produce the same result
        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert_eq!(result1.unwrap().0.len(), result2.unwrap().0.len());
    }

    // ── FormatParse three-tier chain (above → builtin → below) ──

    use cce_plugin::{CodePlugin, PluginBundle, PluginError, PluginMetadata, PluginRegistry};
    use cce_types::PluginDocument;

    type ParseFn = fn(&str, &str) -> Result<Option<PluginDocument>, PluginError>;

    /// Configurable `CodePlugin` test double for the `FormatParse` capability.
    struct ParseMockPlugin {
        meta: PluginMetadata,
        parse: Option<ParseFn>,
    }

    impl ParseMockPlugin {
        fn with_id(id: &str, priority: i32) -> Self {
            Self {
                meta: PluginMetadata {
                    id: id.to_string(),
                    name: id.to_string(),
                    version: "0.1.0".to_string(),
                    priority,
                    capabilities: Vec::new(),
                    capability_priorities: std::collections::HashMap::new(),
                    description: None,
                },
                parse: None,
            }
        }

        fn parser(mut self, f: ParseFn) -> Self {
            self.parse = Some(f);
            self
        }
    }

    impl CodePlugin for ParseMockPlugin {
        fn metadata(&self) -> &PluginMetadata {
            &self.meta
        }
        fn supports_parse(&self) -> bool {
            self.parse.is_some()
        }
        fn parse_document(
            &self,
            content: &str,
            file_path: &str,
        ) -> Result<Option<PluginDocument>, PluginError> {
            match self.parse {
                Some(f) => f(content, file_path),
                None => Ok(None),
            }
        }
    }

    fn parse_register(
        registry: &mut PluginRegistry,
        plugin: ParseMockPlugin,
        patterns: Option<Vec<&str>>,
    ) {
        let mut bundle = PluginBundle::new(std::sync::Arc::new(plugin));
        if let Some(patterns) = patterns {
            bundle =
                bundle.with_file_patterns(patterns.into_iter().map(|p| p.to_string()).collect());
        }
        registry.register_bundle(bundle);
    }

    fn router_with_plugins(
        plugins: Vec<(ParseMockPlugin, Option<Vec<&str>>)>,
    ) -> (PipelineRouter, PluginRegistry) {
        let mut registry = PluginRegistry::new();
        for (plugin, patterns) in plugins {
            parse_register(&mut registry, plugin, patterns);
        }
        (PipelineRouter::new(), registry)
    }

    fn plugin_doc(name: &str) -> PluginDocument {
        PluginDocument {
            title: Some(name.to_string()),
            language: None,
            entities: vec![cce_types::PluginEntity::new("1", "section", name)],
        }
    }

    #[test]
    fn test_format_parse_above_plugin_wins_over_builtin() {
        fn parse(_content: &str, _file_path: &str) -> Result<Option<PluginDocument>, PluginError> {
            Ok(Some(plugin_doc("PLUGIN-SECTION")))
        }
        let (router, registry) = router_with_plugins(vec![(
            ParseMockPlugin::with_id("custom", 100).parser(parse),
            Some(vec!["*.proto"]),
        )]);
        let config = ChunkingConfig::default();

        let (chunks, _) = router
            .process_with_plugins(
                "syntax = \"proto3\";",
                "api.proto",
                &config,
                OutputMode::default(),
                &registry,
            )
            .expect("plugin-format file must be parsed");

        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.metadata.is_document()));
        assert!(
            chunks.iter().any(|c| c.text.contains("PLUGIN-SECTION")),
            "chunk content must originate from the plugin document"
        );
    }

    #[test]
    fn test_format_parse_above_all_decline_uses_builtin() {
        fn decline(
            _content: &str,
            _file_path: &str,
        ) -> Result<Option<PluginDocument>, PluginError> {
            Ok(None)
        }
        let (router, registry) = router_with_plugins(vec![(
            ParseMockPlugin::with_id("decline", 100).parser(decline),
            Some(vec!["*.proto"]),
        )]);
        let config = ChunkingConfig::default();

        // `.md` file: plugin not matched by pattern; built-in markdown runs.
        let (chunks, _) = router
            .process_with_plugins(
                "# Title\n\nBody text",
                "readme.md",
                &config,
                OutputMode::default(),
                &registry,
            )
            .expect("built-in pipeline must handle unmatched files");
        assert!(!chunks.is_empty());
        assert!(
            chunks.iter().all(|c| !c.text.contains("PLUGIN-SECTION")),
            "declined plugin must not influence output"
        );
    }

    #[test]
    fn test_format_parse_empty_entities_is_decline() {
        fn empty_doc(
            _content: &str,
            _file_path: &str,
        ) -> Result<Option<PluginDocument>, PluginError> {
            Ok(Some(PluginDocument::default()))
        }
        let (router, registry) = router_with_plugins(vec![(
            ParseMockPlugin::with_id("empty", 100).parser(empty_doc),
            None,
        )]);
        let config = ChunkingConfig::default();

        // An empty plugin document counts as decline → built-in pipeline.
        let (chunks, _) = router
            .process_with_plugins(
                "# Heading\n\nparagraph",
                "doc.md",
                &config,
                OutputMode::default(),
                &registry,
            )
            .expect("built-in pipeline must take over");
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_format_parse_error_falls_back_to_builtin() {
        fn fail(_content: &str, _file_path: &str) -> Result<Option<PluginDocument>, PluginError> {
            Err(PluginError::ExecutionFailed("broken".to_string()))
        }
        let (router, registry) = router_with_plugins(vec![(
            ParseMockPlugin::with_id("broken", 100).parser(fail),
            None,
        )]);
        let config = ChunkingConfig::default();

        let (chunks, _) = router
            .process_with_plugins(
                "# Heading\n\nparagraph",
                "doc.md",
                &config,
                OutputMode::default(),
                &registry,
            )
            .expect("failed plugin must fall back to the built-in pipeline");
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_format_parse_below_plugin_used_only_when_builtin_empty() {
        fn below_parse(
            _content: &str,
            _file_path: &str,
        ) -> Result<Option<PluginDocument>, PluginError> {
            Ok(Some(plugin_doc("BELOW-FALLBACK")))
        }
        let (router, registry) = router_with_plugins(vec![(
            ParseMockPlugin::with_id("below", -1).parser(below_parse),
            None,
        )]);
        let config = ChunkingConfig::default();

        // Non-empty built-in result → below-tier plugin stays silent.
        let (chunks, _) = router
            .process_with_plugins(
                "# Heading\n\nparagraph",
                "doc.md",
                &config,
                OutputMode::default(),
                &registry,
            )
            .expect("built-in pipeline must handle the file");
        assert!(!chunks.is_empty());
        assert!(
            chunks.iter().all(|c| !c.text.contains("BELOW-FALLBACK")),
            "below-tier plugin must stay silent when the built-in produced chunks"
        );

        // Empty built-in result (empty plain text) → below-tier plugin runs.
        let (chunks, _) = router
            .process_with_plugins("", "empty.txt", &config, OutputMode::default(), &registry)
            .expect("below-tier plugin must handle the file");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("BELOW-FALLBACK"));
    }

    #[test]
    fn test_format_parse_priority_order_first_non_empty_wins() {
        fn plugin_a(
            _content: &str,
            _file_path: &str,
        ) -> Result<Option<PluginDocument>, PluginError> {
            Ok(Some(plugin_doc("PLUGIN-A")))
        }
        fn plugin_b(
            _content: &str,
            _file_path: &str,
        ) -> Result<Option<PluginDocument>, PluginError> {
            Ok(Some(plugin_doc("PLUGIN-B")))
        }
        let (router, registry) = router_with_plugins(vec![
            (ParseMockPlugin::with_id("a", 100).parser(plugin_a), None),
            (ParseMockPlugin::with_id("b", 10).parser(plugin_b), None),
        ]);
        let config = ChunkingConfig::default();

        let (chunks, _) = router
            .process_with_plugins(
                "raw content",
                "doc.xyz",
                &config,
                OutputMode::default(),
                &registry,
            )
            .expect("plugin tier must handle the file");
        assert!(!chunks.is_empty());
        assert!(
            chunks.iter().any(|c| c.text.contains("PLUGIN-A")),
            "the highest-priority non-declining plugin must win"
        );
    }
}
