//! Plain text exporter for displaying natural language conversion in E2E tests.
//!
//! This utility is display-only. It formats the converted text in a layout that
//! stays close to the original source code and uses `---` as the section
//! separator.

use std::collections::HashMap;
use std::path::PathBuf;

use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_parser::ast_to_nl::clean_comment_content;
use cce_parser::summary::FileSummary;

/// Export result for the plain text display output.
#[derive(Debug, Clone, Default)]
pub struct NlTextExportResult {
    /// Number of files exported.
    pub exported_count: usize,
    /// Failed files.
    pub failed: Vec<(PathBuf, String)>,
    /// Output paths.
    pub output_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct DisplaySection {
    entity_name: String,
    line_info: String,
    code_texts: Vec<String>,
    nl_texts: Vec<String>,
    keywords: Vec<String>,
    start_line: usize,
    group_type: String,
    entity_kind: String,
    fragment_info: Option<String>,
    split_reason: Option<String>,
    chunk_paths: Vec<String>,
    related_groups: Vec<(String, String, f32)>,
}

/// Plain text exporter for natural language conversion display.
pub struct NlTextDocumentExporter {
    /// Project root directory.
    project_root: PathBuf,
    /// Whether to export BM25 path output.
    export_bm25: bool,
    /// Whether to export Embedding path output.
    export_embedding: bool,
}

impl NlTextDocumentExporter {
    /// Create a new exporter with both BM25 and Embedding paths enabled.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            export_bm25: true,
            export_embedding: true,
        }
    }

    /// Create an exporter for Embedding path only.
    pub fn embedding_only(project_root: PathBuf) -> Self {
        Self {
            project_root,
            export_bm25: false,
            export_embedding: true,
        }
    }

    /// Create an exporter for BM25 path only.
    pub fn bm25_only(project_root: PathBuf) -> Self {
        Self {
            project_root,
            export_bm25: true,
            export_embedding: false,
        }
    }

    /// Get the output directory for the display text files.
    /// Returns different subdirectories based on which paths are enabled.
    pub fn output_dir(&self) -> PathBuf {
        let base_dir = self.project_root.join(".cce");

        match (self.export_bm25, self.export_embedding) {
            (true, true) => base_dir.join("nl_text_docs"),
            (true, false) => base_dir.join("nl_text_docs_bm25"),
            (false, true) => base_dir.join("nl_text_docs_emb"),
            (false, false) => base_dir.join("nl_text_docs"), // fallback
        }
    }

    /// Get the output directory for BM25 path.
    pub fn bm25_output_dir(&self) -> PathBuf {
        self.project_root.join(".cce").join("nl_text_docs_bm25")
    }

    /// Get the output directory for Embedding path.
    pub fn embedding_output_dir(&self) -> PathBuf {
        self.project_root.join(".cce").join("nl_text_docs_emb")
    }

    /// Check if exporting BM25 path.
    pub fn is_exporting_bm25(&self) -> bool {
        self.export_bm25
    }

    /// Check if exporting Embedding path.
    pub fn is_exporting_embedding(&self) -> bool {
        self.export_embedding
    }

    /// Export a single file.
    pub fn export_file(&self, chunks: &[ChunkedResult]) -> Result<PathBuf, String> {
        self.export_file_with_summary(chunks, None)
    }

    /// Export a single file with an optional file summary.
    pub fn export_file_with_summary(
        &self,
        chunks: &[ChunkedResult],
        summary: Option<&FileSummary>,
    ) -> Result<PathBuf, String> {
        if chunks.is_empty() {
            return Err("No chunks to export".to_string());
        }

        let file_path = chunks[0].metadata.file_path.clone();
        let content = self.format_display_doc(chunks, summary, self.default_filter_path())?;
        let output_path = self.compute_output_path(&file_path);

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        std::fs::write(&output_path, &content)
            .map_err(|e| format!("Failed to write file: {}", e))?;
        Ok(output_path)
    }

    /// Export a single file for a specific chunk path with an optional file summary.
    pub fn export_file_with_path(
        &self,
        chunks: &[ChunkedResult],
        path: ChunkPath,
        summary: Option<&FileSummary>,
    ) -> Result<PathBuf, String> {
        if chunks.is_empty() {
            return Err("No chunks to export".to_string());
        }

        let file_path = chunks[0].metadata.file_path.clone();
        let content = self.format_display_doc(chunks, summary, Some(path))?;
        let output_path = self.compute_output_path_for_path(&file_path, &path);

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        std::fs::write(&output_path, &content)
            .map_err(|e| format!("Failed to write file: {}", e))?;
        Ok(output_path)
    }

    /// Export multiple files.
    pub fn export_batch(
        &self,
        file_chunks: &HashMap<String, Vec<ChunkedResult>>,
    ) -> NlTextExportResult {
        self.export_batch_with_summaries(file_chunks, &HashMap::new())
    }

    /// Export multiple files with optional summaries.
    pub fn export_batch_with_summaries(
        &self,
        file_chunks: &HashMap<String, Vec<ChunkedResult>>,
        summaries: &HashMap<String, FileSummary>,
    ) -> NlTextExportResult {
        let mut result = NlTextExportResult::default();

        for (file_path, chunks) in file_chunks {
            let summary = summaries.get(file_path);
            match self.export_file_with_summary(chunks, summary) {
                Ok(output_path) => {
                    result.exported_count += 1;
                    result.output_paths.push(output_path);
                }
                Err(err) => {
                    result.failed.push((PathBuf::from(file_path), err));
                }
            }
        }

        result
    }

    fn format_display_doc(
        &self,
        chunks: &[ChunkedResult],
        summary: Option<&FileSummary>,
        filter_path: Option<ChunkPath>,
    ) -> Result<String, String> {
        let file_path = chunks[0].metadata.file_path.clone();
        let display_path = self.relative_source_path(&file_path);
        let groups = self.group_by_source(chunks);
        let include_keywords = match filter_path {
            Some(ChunkPath::Embedding) => false,
            _ => self.export_bm25,
        };
        let sections = self.build_sections(&groups, filter_path, include_keywords);

        if sections.is_empty() {
            return Err("No display content found in chunks".to_string());
        }

        let mut output = String::new();
        output.push_str(&format!("{}\n", display_path.display()));

        if let Some(summary) = summary {
            if let Some(file_doc) = summary.file_doc_comment.as_deref() {
                let cleaned = clean_comment_content(file_doc);
                if !cleaned.trim().is_empty() {
                    output.push_str("doc:\n");
                    output.push_str(&cleaned);
                    output.push('\n');
                }
            }
        }

        output.push_str("---\n");

        for (index, section) in sections.iter().enumerate() {
            if index > 0 {
                output.push_str("---\n");
            }

            if section.entity_name.is_empty() {
                output.push_str(&format!("({})\n", section.line_info));
            } else {
                output.push_str(&format!(
                    "{} ({})\n",
                    section.entity_name, section.line_info
                ));
            }

            if !section.entity_kind.is_empty() {
                output.push_str(&format!("  kind: {}\n", section.entity_kind));
            }
            if !section.group_type.is_empty() {
                output.push_str(&format!("  group: {}\n", section.group_type));
            }
            if !section.chunk_paths.is_empty() {
                output.push_str(&format!(
                    "  chunk_path: {}\n",
                    section.chunk_paths.join(", ")
                ));
            }
            if let Some(fi) = &section.fragment_info {
                output.push_str(&format!("  {}\n", fi));
            }
            if let Some(sr) = &section.split_reason {
                if sr != "not_split" {
                    output.push_str(&format!("  split_reason: {}\n", sr));
                }
            }
            if !section.related_groups.is_empty() {
                let rel_str: Vec<String> = section
                    .related_groups
                    .iter()
                    .map(|(id, rt, _str)| format!("{} ({})", id, rt))
                    .collect();
                output.push_str(&format!("  related_groups: {}\n", rel_str.join(", ")));
            }

            if !section.keywords.is_empty() {
                output.push_str(&format!("keywords: {}\n", section.keywords.join(", ")));
            }

            if !section.code_texts.is_empty() {
                output.push_str("code:\n");
                for (i, text) in section.code_texts.iter().enumerate() {
                    output.push_str(text);
                    if !text.ends_with('\n') {
                        output.push('\n');
                    }
                    if i + 1 < section.code_texts.len() {
                        output.push('\n');
                    }
                }
            }

            if !section.nl_texts.is_empty() {
                if !section.code_texts.is_empty() && filter_path != Some(ChunkPath::Embedding) {
                    output.push('\n');
                }
                if filter_path != Some(ChunkPath::Embedding) {
                    output.push_str("nl:\n");
                }
                for (i, text) in section.nl_texts.iter().enumerate() {
                    output.push_str(text);
                    if !text.ends_with('\n') {
                        output.push('\n');
                    }
                    if i + 1 < section.nl_texts.len() {
                        output.push('\n');
                    }
                }
            }
        }

        Ok(output)
    }

    fn build_sections(
        &self,
        groups: &[(String, Vec<&ChunkedResult>)],
        filter_path: Option<ChunkPath>,
        include_keywords: bool,
    ) -> Vec<DisplaySection> {
        let mut sections = Vec::new();

        for (_group_id, group_chunks) in groups {
            let span = group_chunks[0].metadata.source_span;
            let line_info = match span.line_range_opt() {
                None => "source range unavailable".to_string(),
                Some((start_line, end_line)) if start_line == end_line => {
                    format!("line {start_line}")
                }
                Some((start_line, end_line)) => format!("lines {start_line}-{end_line}"),
            };
            let start_line = span.line_range_opt().map(|(s, _)| s).unwrap_or(0);

            let entity_name = group_chunks
                .iter()
                .find_map(|chunk| chunk.bm25_title.clone())
                .unwrap_or_default();

            let code_chunks: Vec<&ChunkedResult> = match filter_path {
                Some(ChunkPath::Bm25) => group_chunks
                    .iter()
                    .copied()
                    .filter(|chunk| chunk.path == ChunkPath::Bm25)
                    .collect(),
                Some(ChunkPath::Embedding) => Vec::new(),
                None => group_chunks
                    .iter()
                    .copied()
                    .filter(|chunk| chunk.path == ChunkPath::Bm25)
                    .collect(),
            };

            let nl_chunks: Vec<&ChunkedResult> = match filter_path {
                Some(ChunkPath::Bm25) => Vec::new(),
                Some(ChunkPath::Embedding) => group_chunks
                    .iter()
                    .copied()
                    .filter(|chunk| chunk.path == ChunkPath::Embedding)
                    .collect(),
                None => {
                    let emb_chunks: Vec<&ChunkedResult> = group_chunks
                        .iter()
                        .copied()
                        .filter(|chunk| chunk.path == ChunkPath::Embedding)
                        .collect();

                    if emb_chunks.is_empty() {
                        group_chunks
                            .iter()
                            .copied()
                            .filter(|chunk| chunk.path == ChunkPath::Bm25)
                            .collect()
                    } else {
                        emb_chunks
                    }
                }
            };

            if code_chunks.is_empty() && nl_chunks.is_empty() {
                continue;
            }

            let mut code_chunks = code_chunks;
            code_chunks.sort_by_key(|chunk| chunk.chunk_index);
            let mut nl_chunks = nl_chunks;
            nl_chunks.sort_by_key(|chunk| chunk.chunk_index);

            let code_texts = code_chunks
                .iter()
                .map(|chunk| chunk.text.trim())
                .filter(|text| !text.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();

            let nl_texts = nl_chunks
                .iter()
                .map(|chunk| chunk.text.trim())
                .filter(|text| !text.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();

            let keywords = if include_keywords {
                self.collect_keywords(group_chunks)
            } else {
                Vec::new()
            };

            let group_type = group_chunks
                .first()
                .map(|c| c.group_type.to_string())
                .unwrap_or_default();

            let entity_kind = group_chunks
                .iter()
                .find_map(|c| {
                    c.metadata
                        .code_metadata
                        .as_ref()
                        .map(|m| m.entity_kind.to_string())
                })
                .unwrap_or_default();

            let fragment_info = group_chunks.iter().find_map(|c| {
                c.metadata.code_metadata.as_ref().and_then(|m| {
                    if m.is_fragment {
                        Some(format!(
                            "fragment {}/{}",
                            m.fragment_index.unwrap_or(0) + 1,
                            m.total_fragments.unwrap_or(0)
                        ))
                    } else {
                        None
                    }
                })
            });

            let split_reason = group_chunks.iter().find_map(|c| {
                c.metadata
                    .code_metadata
                    .as_ref()
                    .map(|m| m.split_reason.to_string())
            });

            let mut chunk_paths: Vec<String> =
                group_chunks.iter().map(|c| c.path.to_string()).collect();
            chunk_paths.sort();
            chunk_paths.dedup();

            let mut related_groups: Vec<(String, String, f32)> = group_chunks
                .iter()
                .flat_map(|c| c.related_groups.iter())
                .map(|rg| {
                    (
                        rg.group_id.clone(),
                        rg.relation_type.to_string(),
                        rg.strength,
                    )
                })
                .collect();
            related_groups.sort_by(|a, b| a.0.cmp(&b.0));
            related_groups.dedup();

            sections.push(DisplaySection {
                entity_name,
                line_info,
                code_texts,
                nl_texts,
                keywords,
                start_line,
                group_type,
                entity_kind,
                fragment_info,
                split_reason,
                chunk_paths,
                related_groups,
            });
        }

        sections.sort_by_key(|section| section.start_line);

        // Display output should stay compact even when the same entity is
        // surfaced through multiple groups or chunk paths.
        self.deduplicate_sections(sections)
    }

    fn collect_keywords(&self, group_chunks: &[&ChunkedResult]) -> Vec<String> {
        let mut keywords = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for keyword in group_chunks
            .iter()
            .flat_map(|chunk| chunk.bm25_keywords.iter())
        {
            let normalized = keyword.to_lowercase();
            if seen.insert(normalized) {
                keywords.push(keyword.clone());
            }
        }

        keywords
    }

    fn deduplicate_sections(&self, sections: Vec<DisplaySection>) -> Vec<DisplaySection> {
        use std::collections::hash_map::Entry;

        let mut best_by_key: HashMap<(String, String), (usize, DisplaySection)> = HashMap::new();

        for section in sections {
            let key = (section.entity_name.clone(), section.line_info.clone());
            let score = self.section_score(&section);

            match best_by_key.entry(key) {
                Entry::Vacant(entry) => {
                    entry.insert((score, section));
                }
                Entry::Occupied(mut entry) => {
                    if score > entry.get().0 {
                        entry.insert((score, section));
                    }
                }
            }
        }

        let mut deduped: Vec<DisplaySection> = best_by_key
            .into_values()
            .map(|(_, section)| section)
            .collect();
        deduped.sort_by_key(|section| section.start_line);
        deduped
    }

    fn section_score(&self, section: &DisplaySection) -> usize {
        section
            .code_texts
            .iter()
            .chain(section.nl_texts.iter())
            .chain(section.keywords.iter())
            .map(|text| text.len())
            .sum()
    }

    fn default_filter_path(&self) -> Option<ChunkPath> {
        match (self.export_bm25, self.export_embedding) {
            (true, false) => Some(ChunkPath::Bm25),
            (false, true) => Some(ChunkPath::Embedding),
            _ => None,
        }
    }

    fn group_by_source<'a>(
        &self,
        chunks: &'a [ChunkedResult],
    ) -> Vec<(String, Vec<&'a ChunkedResult>)> {
        let mut group_map: HashMap<String, Vec<&ChunkedResult>> = HashMap::new();

        for chunk in chunks {
            group_map
                .entry(chunk.source_group_id.clone())
                .or_default()
                .push(chunk);
        }

        let mut groups: Vec<(String, Vec<&ChunkedResult>)> = group_map.into_iter().collect();
        groups.sort_by(|a, b| {
            let a_line =
                a.1.iter()
                    .map(|chunk| chunk.metadata.source_span.start_position.row)
                    .min();
            let b_line =
                b.1.iter()
                    .map(|chunk| chunk.metadata.source_span.start_position.row)
                    .min();
            a_line.cmp(&b_line)
        });

        groups
    }

    fn compute_output_path(&self, source_path: &str) -> PathBuf {
        let mut output_path = self.output_dir();
        let relative = self.relative_source_path(source_path);
        output_path.push(relative);
        output_path.set_extension("txt");
        output_path
    }

    fn compute_output_path_for_path(&self, source_path: &str, path: &ChunkPath) -> PathBuf {
        let mut output_path = match path {
            ChunkPath::Bm25 => self.bm25_output_dir(),
            ChunkPath::Embedding => self.embedding_output_dir(),
        };
        let relative = self.relative_source_path(source_path);
        output_path.push(relative);
        output_path.set_extension("txt");
        output_path
    }

    fn relative_source_path(&self, source_path: &str) -> PathBuf {
        let project_root = self
            .project_root
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string();

        let normalized = source_path
            .replace('\\', "/")
            .trim_start_matches("\\\\?\\")
            .trim_start_matches("//?/")
            .to_string();

        if let Some(stripped) = normalized.strip_prefix(&project_root) {
            let relative = stripped.trim_start_matches('/');
            if !relative.is_empty() {
                return PathBuf::from(relative);
            }
        }

        PathBuf::from(source_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_parser::ast_to_nl::chunker::{ChunkMetadata, CodeSpecificMetadata};
    use cce_parser::grouper::GroupType;
    use cce_parser::summary::{FileSummary, ImportanceLevel};
    use cce_types::language::Language;
    use cce_types::{EntityId, EntityKind, Span};

    /// Builder for constructing test chunks with fluent API
    struct ChunkBuilder {
        file_path: String,
        group_id: String,
        path: ChunkPath,
        text: String,
        keywords: Vec<String>,
        title: Option<String>,
        start_line: u32,
        end_line: u32,
    }

    impl ChunkBuilder {
        fn new(file_path: &str, group_id: &str) -> Self {
            Self {
                file_path: file_path.to_string(),
                group_id: group_id.to_string(),
                path: ChunkPath::Embedding,
                text: String::new(),
                keywords: Vec::new(),
                title: None,
                start_line: 1,
                end_line: 1,
            }
        }

        fn path(mut self, path: ChunkPath) -> Self {
            self.path = path;
            self
        }

        fn text(mut self, text: &str) -> Self {
            self.text = text.to_string();
            self
        }

        fn keywords(mut self, keywords: &[&str]) -> Self {
            self.keywords = keywords.iter().map(|s| s.to_string()).collect();
            self
        }

        fn title(mut self, title: Option<&str>) -> Self {
            self.title = title.map(|s| s.to_string());
            self
        }

        fn line_range(mut self, start: u32, end: u32) -> Self {
            self.start_line = start;
            self.end_line = end;
            self
        }

        fn build(self) -> ChunkedResult {
            let path_str = match self.path {
                ChunkPath::Bm25 => "bm25",
                ChunkPath::Embedding => "emb",
            };
            let chunk_id = format!("{}_{}_0", self.group_id, path_str);
            let mut chunk = ChunkedResult::new(chunk_id, self.group_id.clone(), self.path, 0, 1);
            chunk.text = self.text.clone();
            chunk.token_count = self.text.split_whitespace().count();
            chunk.start_byte = 0;
            chunk.end_byte = self.text.len();
            chunk.group_type = GroupType::Standalone;
            chunk.bm25_title = self.title;
            chunk.bm25_keywords = self.keywords;
            chunk.metadata = ChunkMetadata::for_code(
                self.file_path,
                Span::from_lines(
                    (self.start_line.saturating_sub(1)) as usize,
                    self.end_line as usize,
                ),
                Language::Rust,
                CodeSpecificMetadata {
                    content_entity_ids: vec![EntityId(1)],
                    entity_kind: EntityKind::Function,
                    ..Default::default()
                },
            );
            chunk
        }
    }

    #[test]
    fn test_nl_text_exporter_single_file() {
        let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let exporter = NlTextDocumentExporter::new(tmp_dir.path().to_path_buf());

        let chunks = vec![
            ChunkBuilder::new("src/main.rs", "group_main")
                .path(ChunkPath::Embedding)
                .text("The main function serves as the entry point.")
                .keywords(&["main"])
                .title(Some("main"))
                .line_range(1, 3)
                .build(),
            ChunkBuilder::new("src/main.rs", "group_main")
                .path(ChunkPath::Bm25)
                .text("fn main() { println!(\"Hello\"); }")
                .keywords(&["main", "println"])
                .title(Some("main"))
                .line_range(1, 3)
                .build(),
        ];

        let path = exporter
            .export_file(&chunks)
            .expect("Display export should succeed");

        assert!(path.exists(), "Output file should exist");
        assert!(
            path.to_string_lossy().contains("nl_text_docs"),
            "Output should be under nl_text_docs"
        );

        let content = std::fs::read_to_string(&path).expect("Should read output");
        assert!(content.contains("main"));
        assert!(content.contains("keywords:"));
        assert!(content.contains("nl:"));
        assert!(content.contains("entry point"));
    }

    #[test]
    fn test_nl_text_exporter_includes_summary() {
        let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let exporter = NlTextDocumentExporter::new(tmp_dir.path().to_path_buf());

        let chunks = vec![
            ChunkBuilder::new("src/lib.rs", "group_docs")
                .path(ChunkPath::Embedding)
                .text("Documentation text.")
                .keywords(&["docs"])
                .title(Some("docs"))
                .line_range(1, 3)
                .build(),
        ];

        let summary = FileSummary::new("src/lib.rs")
            .with_importance_level(ImportanceLevel::Medium)
            .with_file_doc_comment(Some("# Overview\n\ncrate docs".to_string()));

        let path = exporter
            .export_file_with_summary(&chunks, Some(&summary))
            .expect("Display export should succeed");

        let content = std::fs::read_to_string(&path).expect("Should read output");
        assert!(content.contains("doc:"));
        assert!(content.contains("Overview"));
        assert!(content.contains("crate docs"));
    }

    #[test]
    fn test_nl_text_exporter_batch() {
        let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let exporter = NlTextDocumentExporter::new(tmp_dir.path().to_path_buf());

        let mut file_chunks: HashMap<String, Vec<ChunkedResult>> = HashMap::new();

        file_chunks.insert(
            "src/module_a.rs".to_string(),
            vec![
                ChunkBuilder::new("src/module_a.rs", "group_a")
                    .path(ChunkPath::Embedding)
                    .text("The compute function multiplies two numbers.")
                    .keywords(&["compute", "multiply"])
                    .title(Some("compute"))
                    .line_range(1, 5)
                    .build(),
            ],
        );

        file_chunks.insert(
            "src/module_b.rs".to_string(),
            vec![
                ChunkBuilder::new("src/module_b.rs", "group_b")
                    .path(ChunkPath::Embedding)
                    .text("The util function increments the input.")
                    .keywords(&["util"])
                    .title(Some("util"))
                    .line_range(1, 3)
                    .build(),
            ],
        );

        let result = exporter.export_batch(&file_chunks);

        assert_eq!(result.exported_count, 2, "Both files should export");
        assert!(result.failed.is_empty(), "No failures");
        assert_eq!(result.output_paths.len(), 2);
    }

    #[test]
    fn test_nl_text_exporter_empty_chunks() {
        let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let exporter = NlTextDocumentExporter::new(tmp_dir.path().to_path_buf());

        let result = exporter.export_file(&[]);
        assert!(result.is_err(), "Empty chunks should fail");
    }

    #[test]
    fn test_nl_text_exporter_deduplicates_duplicate_sections() {
        let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let exporter = NlTextDocumentExporter::new(tmp_dir.path().to_path_buf());

        let chunks = vec![
            ChunkBuilder::new("src/lib.rs", "group_a")
                .path(ChunkPath::Bm25)
                .text("alpha short code")
                .keywords(&["alpha"])
                .title(Some("duplicate_entity"))
                .line_range(1, 1)
                .build(),
            ChunkBuilder::new("src/lib.rs", "group_a")
                .path(ChunkPath::Embedding)
                .text("alpha short semantic")
                .keywords(&["alpha"])
                .title(Some("duplicate_entity"))
                .line_range(1, 1)
                .build(),
            ChunkBuilder::new("src/lib.rs", "group_b")
                .path(ChunkPath::Bm25)
                .text("beta very detailed code description with extra terms")
                .keywords(&["beta"])
                .title(Some("duplicate_entity"))
                .line_range(1, 1)
                .build(),
            ChunkBuilder::new("src/lib.rs", "group_b")
                .path(ChunkPath::Embedding)
                .text("beta very detailed semantic description with extra terms")
                .keywords(&["beta"])
                .title(Some("duplicate_entity"))
                .line_range(1, 1)
                .build(),
        ];

        let path = exporter
            .export_file(&chunks)
            .expect("Display export should succeed");
        let content = std::fs::read_to_string(&path).expect("Should read output");

        assert_eq!(
            content.matches("duplicate_entity (lines 1-2)").count(),
            1,
            "duplicate sections should be merged"
        );
        assert!(
            content.contains("beta very detailed"),
            "the richer section should be retained"
        );
        assert!(
            !content.contains("alpha short code"),
            "the weaker duplicate should be dropped"
        );
    }
}
