//! Export chunk texts for structured document formats (JSON, XML, YAML, TOML).
//!
//! Processes each fixture file through the document pipeline and exports
//! the resulting chunk texts to disk as markdown summaries.
//!
//! Usage:
//! ```shell
//! cargo run --example export_doc -p cce-e2e-tests
//! ```
//!
//! Output: outputs/scenarios/documents/chunks/{format}/{file_name}.txt

use std::path::{Path, PathBuf};

use cce_orchestrator::index::FileProcessor;
use cce_scanner::{FSScanner, FileEntry, ScanOptions};
use cce_types::OutputMode;

use cce_e2e_tests::{OutputCategory, OutputManager, init_minimal_logging};

fn fixture_source_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("documents")
}

fn scan_documents() -> Result<Vec<FileEntry>, String> {
    let root = fixture_source_path();
    let mut scanner = FSScanner::new();
    let opts = ScanOptions {
        root_path: root.to_string_lossy().to_string(),
        ..Default::default()
    };
    scanner.scan(&opts).map_err(|e| e.to_string())
}

fn normalize_file_path(abs_path: &str) -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let normalized = abs_path.replace('\\', "/");
    let manifest_prefix = manifest_dir.replace('\\', "/");
    if let Some(stripped) = normalized.strip_prefix(&manifest_prefix) {
        let relative = stripped.trim_start_matches('/');
        if !relative.is_empty() {
            return relative.to_string();
        }
    }
    normalized
}

fn format_from_path(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown")
        .to_lowercase()
}

fn render_chunks(
    chunks: &[cce_parser::ast_to_nl::chunker::ChunkedResult],
    file_path: &str,
) -> String {
    let mut out = String::new();
    let total = chunks.len();
    let path = Path::new(file_path);
    let file_name = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string());

    out.push_str(&format!("{}\n", file_name));
    out.push_str(&format!("total chunks: {}\n", total));
    out.push_str("---\n");

    for (i, chunk) in chunks.iter().enumerate() {
        let (start_line, end_line) = chunk
            .metadata
            .source_span
            .line_range_opt()
            .unwrap_or((0, 0));

        let path_type = match chunk.path {
            cce_parser::ast_to_nl::chunker::ChunkPath::Embedding => "embedding",
            cce_parser::ast_to_nl::chunker::ChunkPath::Bm25 => "bm25",
        };

        out.push_str(&format!(
            "\n=== CHUNK {}/{} (lines {}-{}) [{}] ===\n",
            i + 1,
            total,
            start_line,
            end_line,
            path_type,
        ));
        out.push_str(&format!(
            "chunk_id: {} | group: {}\n",
            chunk.chunk_id, chunk.source_group_id,
        ));
        let token_display = match chunk.path {
            cce_parser::ast_to_nl::chunker::ChunkPath::Embedding => chunk.token_count.to_string(),
            cce_parser::ast_to_nl::chunker::ChunkPath::Bm25 => chunk
                .metadata
                .bm25_word_count
                .map(|c| format!("{}w", c))
                .unwrap_or_else(|| "0".to_string()),
        };

        if chunk.path == cce_parser::ast_to_nl::chunker::ChunkPath::Bm25 {
            out.push_str(&format!(
                "title: {} | tokens: {}\n",
                chunk.bm25_title.as_deref().unwrap_or("n/a"),
                token_display
            ));
            if !chunk.bm25_keywords.is_empty() {
                out.push_str(&format!("keywords: [{}]\n", chunk.bm25_keywords.join(", ")));
            }
        } else {
            out.push_str(&format!("tokens: {}\n", token_display));
        }
        out.push('\n');
        out.push_str(&chunk.text);
        out.push('\n');
    }

    out
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_minimal_logging();

    let output_mgr = OutputManager::builder()
        .category(OutputCategory::Scenarios)
        .language("documents")
        .scenario("chunks")
        .build();
    let base = output_mgr.ensure_output_dir()?;
    output_mgr.clean().ok();
    std::fs::create_dir_all(&base)?;

    let entries = scan_documents()?;
    println!("Scanned {} document files", entries.len());

    if entries.is_empty() {
        eprintln!(
            "No document fixtures found in: {}",
            fixture_source_path().display()
        );
        return Ok(());
    }

    let mut processor = FileProcessor::new();

    for entry in &entries {
        let rel_path = entry.path.to_string_lossy().to_string();
        let format = format_from_path(&entry.path);

        match format.as_str() {
            "json" | "xml" | "yaml" | "yml" | "toml" | "md" => {}
            _ => continue,
        }

        let normalized = normalize_file_path(&rel_path);
        println!("\nProcessing [{}]: {}", format, normalized);

        let mut emb_chunks: Vec<cce_parser::ast_to_nl::chunker::ChunkedResult> = Vec::new();
        let mut bm25_chunks: Vec<cce_parser::ast_to_nl::chunker::ChunkedResult> = Vec::new();

        if let Ok(result) = processor
            .process_file_complete(entry, OutputMode::Embedding)
            .await
        {
            emb_chunks = result.chunks;
        }

        if let Ok(result) = processor
            .process_file_complete(entry, OutputMode::Bm25)
            .await
        {
            bm25_chunks = result.chunks;
        }

        if emb_chunks.is_empty() && bm25_chunks.is_empty() {
            println!("  No chunks produced, skipping");
            continue;
        }

        let format_dir = base.join(&format);
        std::fs::create_dir_all(&format_dir)?;

        let file_stem = Path::new(&rel_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        let mut combined = String::new();
        combined.push_str(&format!("# {}\n\n", normalized));
        combined.push_str(&format!("Format: {}\n\n", format));

        if !emb_chunks.is_empty() {
            combined.push_str("## Embedding Path\n\n");
            combined.push_str(&render_chunks(&emb_chunks, &normalized));
        }

        if !bm25_chunks.is_empty() {
            combined.push_str("\n## BM25 Path\n\n");
            combined.push_str(&render_chunks(&bm25_chunks, &normalized));
        }

        let out_path = format_dir.join(format!("{}.txt", file_stem));
        std::fs::write(&out_path, &combined)?;
        println!("  Wrote to {}", out_path.display());
    }

    println!("\n✓ Document chunks written to: {}", base.display());
    Ok(())
}
