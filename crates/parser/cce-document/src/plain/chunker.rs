//! Plain text chunker
//!
//! Chunks plain text content using two-tier approach:
//! 1. Logical units (paragraphs, lines, sections) become raw groups
//! 2. Each unit produces one embedding chunk (truncated) + N BM25 sub-chunks (by word count)

mod csv_chunker;
mod docker_chunker;
mod ini_chunker;
mod log_chunker;
mod make_chunker;
mod rst_chunker;
mod text_chunker;

use crate::common::chunker::{TwoTierParams, two_tier_chunking};
use crate::types::DocumentClassification;
use cce_config::modules::ChunkingConfig;
use cce_types::ChunkedResult;
use cce_types::GroupType;
use cce_types::Span;
use cce_types::ast_to_nl::options::OutputMode;
use cce_types::language::LanguageInfo;
use cce_utils::token_estimation::TokenEstimator;

/// Plain text file subtype for chunking strategy selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlainTextKind {
    /// Generic plain text (.txt)
    Text,
    /// Log file (.log)
    Log,
    /// INI-style config (.ini)
    Ini,
    /// CSV data file (.csv)
    Csv,
    /// RST document (.rst)
    Rst,
    /// Makefile / GNUmakefile
    Make,
    /// Dockerfile
    Docker,
}

impl PlainTextKind {
    /// Detect kind from a lowercased file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "log" => PlainTextKind::Log,
            "ini" => PlainTextKind::Ini,
            "csv" => PlainTextKind::Csv,
            "rst" => PlainTextKind::Rst,
            "mk" | "makefile" => PlainTextKind::Make,
            "dockerfile" => PlainTextKind::Docker,
            _ => PlainTextKind::Text,
        }
    }

    /// Detect from a file path's name when extension is absent (Makefile/Dockerfile).
    pub fn from_path(file_path: &str) -> Self {
        let lower = cce_types::path::file_name_str(file_path).to_ascii_lowercase();
        if lower == "makefile"
            || lower == "gnumakefile"
            || lower.starts_with("makefile.")
            || lower.ends_with(".mk")
            || lower.ends_with(".makefile")
        {
            return PlainTextKind::Make;
        }
        if lower == "dockerfile"
            || lower.starts_with("dockerfile.")
            || lower.ends_with(".dockerfile")
        {
            return PlainTextKind::Docker;
        }
        let ext = cce_types::path::extension_lower(file_path).unwrap_or_default();
        Self::from_extension(&ext)
    }

    /// Detect kind from the entry-passed detection result (single source).
    pub fn from_language_info(info: &LanguageInfo) -> Self {
        let ext = info.extensions.first().map(String::as_str).unwrap_or("");
        let kind = Self::from_extension(&ext.to_lowercase());
        if kind != PlainTextKind::Text {
            return kind;
        }
        // Extensionless build files have empty extension; fall back to path name
        // check via payload format.
        let fmt = info.payload_format();
        match fmt.as_str() {
            "make" => PlainTextKind::Make,
            "docker" => PlainTextKind::Docker,
            _ => PlainTextKind::Text,
        }
    }
}

/// Plain text chunker
pub struct PlainTextChunker {
    pub(crate) config: ChunkingConfig,
    pub(crate) estimator: TokenEstimator,
}

impl PlainTextChunker {
    /// Create a new plain text chunker
    pub fn new(config: ChunkingConfig) -> Self {
        Self {
            config,
            estimator: TokenEstimator::default(),
        }
    }

    /// Chunk plain text content directly into ChunkedResults.
    ///
    /// Each logical unit (paragraph / line / section / row-group) produces:
    /// - 1 Embedding chunk (truncated if exceeding max_tokens)
    /// - N BM25 sub-chunks (split by max_bm25_words, all sharing source_group_id)
    pub fn chunk(
        &self,
        content: &str,
        file_path: &str,
        kind: PlainTextKind,
        classification: &DocumentClassification,
    ) -> Vec<ChunkedResult> {
        match kind {
            PlainTextKind::Log => log_chunker::chunk_log(self, content, file_path, classification),
            PlainTextKind::Ini => ini_chunker::chunk_ini(self, content, file_path, classification),
            PlainTextKind::Csv => csv_chunker::chunk_csv(self, content, file_path, classification),
            PlainTextKind::Rst => rst_chunker::chunk_rst(self, content, file_path, classification),
            PlainTextKind::Make => {
                make_chunker::chunk_make(self, content, file_path, classification)
            }
            PlainTextKind::Docker => {
                docker_chunker::chunk_docker(self, content, file_path, classification)
            }
            PlainTextKind::Text => {
                text_chunker::chunk_text(self, content, file_path, classification)
            }
        }
    }

    /// Produce embedding + BM25 sub-chunks for one logical unit.
    ///
    /// Payload type and business category are the entry-passed pair derived
    /// once at the pipeline entry — no re-derivation from the path here.
    pub(crate) fn produce_unit(
        &self,
        text: &str,
        source_span: Span,
        source_base: &str,
        unit_idx: usize,
        file_path: &str,
        classification: &DocumentClassification,
    ) -> Vec<ChunkedResult> {
        let gid = format!("{}_{}", source_base, unit_idx);
        let bm25_title = std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from);

        two_tier_chunking(TwoTierParams {
            embedding_text: text,
            bm25_text: text,
            source_span,
            source_group_id: &gid,
            file_path,
            config: &self.config,
            estimator: &self.estimator,
            group_type: GroupType::Standalone,
            bm25_title,
            output_mode: OutputMode::Both,
            content_type: classification.payload().clone(),
            file_category: classification.category(),
        })
    }

    #[allow(dead_code)]
    pub(crate) fn is_rst_underline(line: &str) -> bool {
        rst_chunker::is_rst_underline(line)
    }

    #[allow(dead_code)]
    pub(crate) fn is_rst_heading_paragraph(text: &str) -> bool {
        rst_chunker::is_rst_heading_paragraph(text)
    }
}

impl Default for PlainTextChunker {
    fn default() -> Self {
        Self::new(ChunkingConfig::default())
    }
}

/// Merge a range of paragraphs (by indices) into a single (start, end) byte range.
pub(crate) fn merge_paragraph_ranges(
    paras: &[(usize, usize)],
    start_idx: usize,
    end_idx: usize,
) -> (usize, usize) {
    if start_idx >= end_idx || start_idx >= paras.len() {
        return (0, 0);
    }
    let start = paras[start_idx].0;
    let end = paras[end_idx.saturating_sub(1).min(paras.len().saturating_sub(1))].1;
    (start, end)
}

pub(crate) fn non_empty_line_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let text_end = offset + line.trim_end_matches(['\r', '\n']).len();
        if !content[offset..text_end].trim().is_empty() {
            ranges.push((offset, text_end));
        }
        offset += line.len();
    }
    ranges
}

pub(crate) fn paragraph_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut paragraph_start = None;
    let mut paragraph_end = 0;

    for (start, end) in non_empty_and_blank_line_ranges(content) {
        if content[start..end].trim().is_empty() {
            if let Some(paragraph_start) = paragraph_start.take() {
                ranges.push((paragraph_start, paragraph_end));
            }
        } else {
            paragraph_start.get_or_insert(start);
            paragraph_end = end;
        }
    }
    if let Some(paragraph_start) = paragraph_start {
        ranges.push((paragraph_start, paragraph_end));
    }
    ranges
}

pub(crate) fn non_empty_and_blank_line_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let end = offset + line.trim_end_matches(['\r', '\n']).len();
        ranges.push((offset, end));
        offset += line.len();
    }
    ranges
}

/// Byte offset of the start of the `line_idx`-th line (0-indexed).
pub(crate) fn line_start(content: &str, line_idx: usize) -> usize {
    let mut offset = 0;
    for line in content.split_inclusive('\n').take(line_idx) {
        offset += line.len();
    }
    offset
}

/// Byte offset just past the end of the `line_idx`-th line (0-indexed, exclusive).
pub(crate) fn line_end(content: &str, line_idx: usize) -> usize {
    let start = line_start(content, line_idx);
    let rest = &content[start..];
    let line_len = rest.find('\n').map_or(rest.len(), |i| i);
    start + line_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::ChunkPath;
    fn count_emb(chunks: &[ChunkedResult]) -> usize {
        chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .count()
    }

    fn count_bm25(chunks: &[ChunkedResult]) -> usize {
        chunks.iter().filter(|c| c.path == ChunkPath::Bm25).count()
    }

    #[test]
    fn test_chunk_text_single_paragraph() {
        let chunker = PlainTextChunker::default();
        let text = "This is a short paragraph.";
        let chunks = chunker.chunk(
            text,
            "test.txt",
            PlainTextKind::Text,
            &DocumentClassification::detect("test.txt"),
        );

        // 1 embedding + BM25 (fits within limits → 1 sub-chunk)
        assert_eq!(count_emb(&chunks), 1);
        assert_eq!(count_bm25(&chunks), 1);
        // BM25 sub-chunk has the same source_group_id as embedding
        let emb = chunks
            .iter()
            .find(|c| c.path == ChunkPath::Embedding)
            .unwrap();
        let bm25 = chunks.iter().find(|c| c.path == ChunkPath::Bm25).unwrap();
        assert_eq!(emb.source_group_id, bm25.source_group_id);
    }

    #[test]
    fn test_chunk_text_multiple_paragraphs() {
        let chunker = PlainTextChunker::default();
        let text = "First paragraph with some content.\n\nSecond paragraph with more content.\n\nThird paragraph.";
        let chunks = chunker.chunk(
            text,
            "test.txt",
            PlainTextKind::Text,
            &DocumentClassification::detect("test.txt"),
        );

        // 3 paragraphs → 3 embedding chunks
        assert_eq!(count_emb(&chunks), 3);
        // Each has BM25 sub-chunks
        assert_eq!(count_bm25(&chunks), 3);
    }

    #[test]
    fn text_chunks_keep_exact_source_ranges() {
        let chunker = PlainTextChunker::default();
        let text =
            "Copyright 2010 Pallets\n\nRedistribution and use\nare permitted.\n\nLast paragraph.";
        let chunks = chunker.chunk(
            text,
            "LICENSE.txt",
            PlainTextKind::Text,
            &DocumentClassification::detect("LICENSE.txt"),
        );
        let embedding_ranges: Vec<_> = chunks
            .iter()
            .filter(|chunk| chunk.path == ChunkPath::Embedding)
            .map(|chunk| chunk.metadata.source_span.line_range_opt())
            .collect();

        assert_eq!(
            embedding_ranges,
            vec![Some((1, 1)), Some((3, 4)), Some((6, 6))]
        );
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.metadata.source_span.is_available())
        );
    }

    #[test]
    fn test_chunk_text_bm25_sub_split() {
        let config = ChunkingConfig {
            max_bm25_words: 3,
            max_tokens: 500,
            ..Default::default()
        };
        let chunker = PlainTextChunker::new(config);
        let text = "one two three four five six seven eight nine ten";
        let chunks = chunker.chunk(
            text,
            "test.txt",
            PlainTextKind::Text,
            &DocumentClassification::detect("test.txt"),
        );

        // 1 embedding + 4 BM25 sub-chunks (10 words / 3 = ~4 with overlap)
        assert_eq!(count_emb(&chunks), 1);
        assert!(
            count_bm25(&chunks) >= 3,
            "Expected >=3 BM25 chunks, got {}",
            count_bm25(&chunks)
        );

        // All BM25 sub-chunks share source_group_id with embedding
        let emb_gid = &chunks
            .iter()
            .find(|c| c.path == ChunkPath::Embedding)
            .unwrap()
            .source_group_id;
        for c in &chunks {
            assert_eq!(&c.source_group_id, emb_gid);
        }
    }

    #[test]
    fn test_chunk_log() {
        let chunker = PlainTextChunker::default();
        let log = "2024-01-01 10:00:00 INFO Starting\n2024-01-01 10:00:01 DEBUG Loading config\n2024-01-01 10:00:02 INFO Ready";
        let chunks = chunker.chunk(
            log,
            "app.log",
            PlainTextKind::Log,
            &DocumentClassification::detect("app.log"),
        );

        assert_eq!(count_emb(&chunks), 3);
        // Each line has a BM25 chunk
        assert_eq!(count_bm25(&chunks), 3);
        for chunk in &chunks {
            let text = &chunk.text;
            assert!(text.contains("2024"), "missing timestamp in: {}", text);
        }
    }

    #[test]
    fn test_chunk_ini() {
        let chunker = PlainTextChunker::default();
        let ini = "[database]\nhost=localhost\nport=5432\n\n[server]\nport=8080\ndebug=true";
        let chunks = chunker.chunk(
            ini,
            "config.ini",
            PlainTextKind::Ini,
            &DocumentClassification::detect("config.ini"),
        );

        // 2 sections → 2 embedding + 2 BM25
        assert_eq!(count_emb(&chunks), 2);
        assert_eq!(count_bm25(&chunks), 2);
    }

    #[test]
    fn ini_chunks_keep_exact_source_ranges() {
        let chunker = PlainTextChunker::default();
        let ini = "[database]\nhost=localhost\nport=5432\n\n[server]\nport=8080\ndebug=true";
        let chunks = chunker.chunk(
            ini,
            "config.ini",
            PlainTextKind::Ini,
            &DocumentClassification::detect("config.ini"),
        );
        let embedding_ranges: Vec<_> = chunks
            .iter()
            .filter(|chunk| chunk.path == ChunkPath::Embedding)
            .map(|chunk| chunk.metadata.source_span.line_range_opt())
            .collect();

        // [database] section spans lines 1-3, [server] section spans lines 5-7
        assert_eq!(embedding_ranges, vec![Some((1, 3)), Some((5, 7))]);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.metadata.source_span.is_available())
        );
    }

    #[test]
    fn test_chunk_text_empty() {
        let chunker = PlainTextChunker::default();
        let chunks = chunker.chunk(
            "",
            "empty.txt",
            PlainTextKind::Text,
            &DocumentClassification::detect("empty.txt"),
        );
        assert!(chunks.is_empty(), "empty content should produce no chunks");
    }

    #[test]
    fn test_kind_detection() {
        assert_eq!(PlainTextKind::from_extension("log"), PlainTextKind::Log);
        assert_eq!(PlainTextKind::from_extension("ini"), PlainTextKind::Ini);
        assert_eq!(PlainTextKind::from_extension("rst"), PlainTextKind::Rst);
        assert_eq!(PlainTextKind::from_extension("txt"), PlainTextKind::Text);
        assert_eq!(
            PlainTextKind::from_extension("unknown"),
            PlainTextKind::Text
        );
    }

    #[test]
    fn test_rst_headings_grouped_into_sections() {
        let chunker = PlainTextChunker::default();
        let rst = "Version 3.2.0\n-------------\n\nUnreleased\n\n- Drop support for Python 3.9.\n- Remove deprecated code.\n\nVersion 3.1.0\n-------------\n\nReleased 2024-01-01\n\n- Fix bug.\n- Add feature.";
        let chunks = chunker.chunk(
            rst,
            "CHANGES.rst",
            PlainTextKind::Rst,
            &DocumentClassification::detect("CHANGES.rst"),
        );

        // 2 heading sections → 2 embedding + BM25 per section
        assert_eq!(
            count_emb(&chunks),
            2,
            "expected 2 embedding chunks (one per section)"
        );
        assert_eq!(
            count_bm25(&chunks),
            2,
            "expected 2 BM25 chunks (one per section)"
        );

        // First section: "Version 3.2.0\n-------------" + "Unreleased" + changelist
        let emb0 = chunks
            .iter()
            .find(|c| c.path == ChunkPath::Embedding && c.source_group_id.ends_with("_0"))
            .unwrap();
        assert!(
            emb0.text.contains("Version 3.2.0"),
            "first section should contain heading"
        );
        assert!(
            emb0.text.contains("Unreleased"),
            "first section should contain body"
        );
        assert!(
            emb0.text.contains("Drop support"),
            "first section should contain changelist"
        );

        // Second section: "Version 3.1.0\n-------------" + "Released" + changelist
        let emb1 = chunks
            .iter()
            .find(|c| c.path == ChunkPath::Embedding && c.source_group_id.ends_with("_1"))
            .unwrap();
        assert!(
            emb1.text.contains("Version 3.1.0"),
            "second section should contain heading"
        );
        assert!(
            emb1.text.contains("Released 2024-01-01"),
            "second section should contain body"
        );
        assert!(
            emb1.text.contains("Fix bug"),
            "second section should contain changelist"
        );
    }

    #[test]
    fn test_rst_fallback_when_no_headings() {
        let chunker = PlainTextChunker::default();
        let text = "Just some\nplain text.\n\nNo headings here.";
        let chunks = chunker.chunk(
            text,
            "plain.rst",
            PlainTextKind::Rst,
            &DocumentClassification::detect("plain.rst"),
        );

        assert_eq!(count_emb(&chunks), 2);
        assert_eq!(count_bm25(&chunks), 2);
    }

    #[test]
    fn test_rst_preamble_before_first_heading() {
        let chunker = PlainTextChunker::default();
        let rst = "Preamble text before any heading.\n\nVersion 1.0\n-----------\n\nContent.";
        let chunks = chunker.chunk(
            rst,
            "CHANGES.rst",
            PlainTextKind::Rst,
            &DocumentClassification::detect("CHANGES.rst"),
        );

        assert_eq!(count_emb(&chunks), 2);
        assert_eq!(count_bm25(&chunks), 2);
    }

    #[test]
    fn test_rst_heading_detection() {
        assert!(PlainTextChunker::is_rst_underline("========="));
        assert!(PlainTextChunker::is_rst_underline("-------------"));
        assert!(PlainTextChunker::is_rst_underline("~~~~~~~~~~~~~"));
        assert!(!PlainTextChunker::is_rst_underline("not an underline"));
        assert!(!PlainTextChunker::is_rst_underline("== mixed =="));
        assert!(!PlainTextChunker::is_rst_underline("--"));

        assert!(PlainTextChunker::is_rst_heading_paragraph("Heading\n====="));
        assert!(PlainTextChunker::is_rst_heading_paragraph(
            "Version 2.0\n-------------"
        ));
        assert!(!PlainTextChunker::is_rst_heading_paragraph(
            "Just a normal paragraph."
        ));
        assert!(!PlainTextChunker::is_rst_heading_paragraph(""));
    }

    /// Chunk payload type and business category must be derived from the
    /// file path's category (single-source), so plain-text files keep
    /// `PlainText`/`Other`, INI keeps `Config`, RST stays documentation and
    /// schema files are not downgraded.
    #[test]
    fn test_chunk_metadata_derives_category_from_path() {
        use cce_types::ChunkContentType;
        use cce_types::ast_to_nl::{FileCategory, SourceSpanKind};

        let chunker = PlainTextChunker::default();

        // .txt → PlainText payload + Other category
        let chunks = chunker.chunk(
            "hello world",
            "notes.txt",
            PlainTextKind::Text,
            &DocumentClassification::detect("notes.txt"),
        );
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.metadata.category_consistent());
            assert_eq!(chunk.metadata.content_type, ChunkContentType::PlainText);
            assert_eq!(chunk.metadata.file_category, FileCategory::Other);
        }
        let txt = chunks.first().expect("chunks non-empty");
        assert_eq!(txt.metadata.source_span_kind, SourceSpanKind::Unavailable);

        // .ini → Config payload + Config category
        let chunks = chunker.chunk(
            "[core]\nname=x\n",
            "conf/app.ini",
            PlainTextKind::Ini,
            &DocumentClassification::detect("conf/app.ini"),
        );
        for chunk in &chunks {
            assert_eq!(
                chunk.metadata.content_type,
                ChunkContentType::Config {
                    format: "ini".to_string()
                }
            );
            assert_eq!(chunk.metadata.file_category, FileCategory::Config);
        }

        // .rst → Document payload + Documentation category
        let chunks = chunker.chunk(
            "Title\n=====\n\ntext",
            "docs/x.rst",
            PlainTextKind::Rst,
            &DocumentClassification::detect("docs/x.rst"),
        );
        for chunk in &chunks {
            assert_eq!(chunk.metadata.content_type, ChunkContentType::Document);
            assert_eq!(chunk.metadata.file_category, FileCategory::Documentation);
        }
    }

    /// `.proto` routes through the plain-text pipeline (no AST grammar) but
    /// its schema semantics must survive in the stored category.
    #[test]
    fn test_proto_files_keep_schema_category() {
        use cce_types::ast_to_nl::FileCategory;

        let chunker = PlainTextChunker::default();
        let proto = "syntax = \"proto3\";\nmessage User { string name = 1; }\n";
        let chunks = chunker.chunk(
            proto,
            "api/user.proto",
            PlainTextKind::Text,
            &DocumentClassification::detect("api/user.proto"),
        );

        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.metadata.category_consistent());
            assert_eq!(chunk.metadata.file_category, FileCategory::Schema);
        }
    }
}
