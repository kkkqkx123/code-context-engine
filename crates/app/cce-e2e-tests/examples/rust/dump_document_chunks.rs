//! Dump chunk texts from document-type fixture files.
//!
//! Processes non-code files (markdown, plain text, log, ini, toml, csv, etc.)
//! through the document pipeline and dumps the resulting chunk texts to disk.
//!
//! Unlike code files, document files use regex-based (markdown) or simple
//! text-splitting (plain text, log, ini, csv) parsing via PipelineRouter,
//! not tree-sitter AST parsing.
//!
//! Usage:
//! ```shell
//! cargo run --example dump_document_chunks -p cce-e2e-tests
//! ```
//!
//! Output: outputs/demo/documents/{file_name}_{type}/chunk_XXXX.txt + metadata.csv

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use cce_orchestrator::index::FileProcessor;
use cce_scanner::{FSScanner, FileEntry, ScanOptions};
use cce_types::OutputMode;

use cce_e2e_tests::init_minimal_logging;

fn fixture_source_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("documents")
}

/// Scan fixture directory without extension restriction so all document types
/// (md, txt, log, ini, toml, csv) are picked up.
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

fn clean_directory(dir: &Path) -> io::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn write_chunks(
    chunks: &[cce_parser::ast_to_nl::chunker::ChunkedResult],
    out_dir: &Path,
) -> io::Result<()> {
    if chunks.is_empty() {
        return Ok(());
    }

    let meta_path = out_dir.join("metadata.csv");
    let mut meta =
        String::from("idx,chunk_id,file_path,start_line,end_line,document_type,text_length\n");

    for (i, cr) in chunks.iter().enumerate() {
        let fname = format!("chunk_{:04}.txt", i);
        let text_path = out_dir.join(&fname);

        let (start_line, end_line) = cr.metadata.source_span.line_range_opt().unwrap_or((0, 0));
        let fpath = normalize_file_path(&cr.metadata.file_path);

        let ext = Path::new(&fpath)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_lowercase();
        let doc_type = match ext.as_str() {
            "md" | "markdown" => "markdown",
            "txt" => "plain_text",
            "log" => "log",
            "ini" => "ini",
            "toml" => "toml",
            "csv" => "csv",
            "json" => "json",
            "yaml" | "yml" => "yaml",
            _ => "other",
        };

        // Write chunk text file with header metadata
        let mut content = String::new();
        content.push_str(&format!("document_type: {}\n", doc_type));
        content.push_str(&format!("file: {} [{}:{}]\n", fpath, start_line, end_line));
        if let Some(title) = &cr.bm25_title {
            content.push_str(&format!("section: {}\n", title));
        }
        content.push_str("---\n");
        content.push_str(&cr.text);

        std::fs::write(&text_path, &content)?;

        meta.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            i,
            cr.chunk_id,
            fpath,
            start_line,
            end_line,
            doc_type,
            cr.text.len()
        ));
    }

    std::fs::write(&meta_path, &meta)?;

    println!("  Wrote {} chunks to {}", chunks.len(), out_dir.display());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_minimal_logging();

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let base = PathBuf::from(manifest_dir)
        .join("outputs")
        .join("demo")
        .join("documents");

    clean_directory(&base)?;
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
        println!("\nProcessing: {}", normalize_file_path(&rel_path));

        // Embedding path result
        match processor
            .process_file_complete(entry, OutputMode::Embedding)
            .await
        {
            Ok(result) => {
                let out_dir = base.join(format!("{}.emb", file_stem(&rel_path)));
                clean_directory(&out_dir).ok();
                std::fs::create_dir_all(&out_dir)?;
                write_chunks(&result.chunks, &out_dir)?;
            }
            Err(e) => {
                eprintln!("  Embedding path failed: {}", e);
            }
        }

        // BM25 path result
        match processor
            .process_file_complete(entry, OutputMode::Bm25)
            .await
        {
            Ok(result) => {
                let out_dir = base.join(format!("{}.bm25", file_stem(&rel_path)));
                clean_directory(&out_dir).ok();
                std::fs::create_dir_all(&out_dir)?;
                write_chunks(&result.chunks, &out_dir)?;
            }
            Err(e) => {
                eprintln!("  BM25 path failed: {}", e);
            }
        }
    }

    // Write a summary overview
    let summary_path = base.join("SUMMARY.md");
    let mut f = std::fs::File::create(&summary_path)?;
    writeln!(f, "# Document Chunk Demo\n")?;
    writeln!(f, "Chunking results for non-code file types.\n")?;
    writeln!(f, "## Files Processed\n")?;
    for entry in &entries {
        let rel = entry.path.to_string_lossy();
        writeln!(f, "- {}", rel)?;
    }
    writeln!(f)?;
    writeln!(f, "## Pipeline\n")?;
    writeln!(f, "- **Markdown**: regex-based section parser")?;
    writeln!(f, "- **Plain text**: paragraph splitting (double newline)")?;
    writeln!(f, "- **Log**: line-by-line segmentation")?;
    writeln!(f, "- **INI**: section-based splitting")?;
    writeln!(f, "- **TOML**: toml crate + GenericChunker")?;
    writeln!(f, "- **CSV**: header + row batches")?;

    println!("\n✓ Document chunk demo written to: {}", base.display());
    Ok(())
}

fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}
