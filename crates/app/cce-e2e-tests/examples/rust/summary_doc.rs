//! Export file-level summaries for structured document formats (JSON, XML, YAML, TOML).
//!
//! Processes each fixture file through the document pipeline and exports
//! the file-level DocSummary to disk as markdown.
//!
//! Usage:
//! ```shell
//! cargo run --example summary_doc -p cce-e2e-tests
//! ```
//!
//! Output: outputs/scenarios/documents/summary/{format}/{file_name}.md

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

fn render_summary(summary: &cce_parser::document::DocSummary, file_path: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!("# {}\n\n", file_path));

    if let Some(ref title) = summary.title {
        out.push_str(&format!("**Title:** {}  \n", title));
    }

    out.push_str(&format!("**Type:** {:?}  \n", summary.doc_type));
    out.push_str(&format!(
        "**Lines:** {} | **Headings:** {} | **Code Blocks:** {}  \n",
        summary.line_count, summary.heading_count, summary.code_block_count
    ));

    out.push('\n');

    if !summary.main_headings.is_empty() {
        out.push_str("## Main Sections\n\n");
        for heading in &summary.main_headings {
            out.push_str(&format!("- {}\n", heading));
        }
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
        .scenario("summary")
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
            "json" | "xml" | "yaml" | "yml" | "toml" => {}
            _ => continue,
        }

        let normalized = normalize_file_path(&rel_path);
        println!("\nProcessing [{}]: {}", format, normalized);

        let result = processor
            .process_file_complete(entry, OutputMode::Embedding)
            .await;

        match result {
            Ok(process_result) => {
                if let Some(doc_summary) = process_result.doc_summary {
                    let format_dir = base.join(&format);
                    std::fs::create_dir_all(&format_dir)?;

                    let file_stem = Path::new(&rel_path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown");

                    let content = render_summary(&doc_summary, &normalized);
                    let out_path = format_dir.join(format!("{}.md", file_stem));
                    std::fs::write(&out_path, &content)?;
                    println!("  Wrote summary to {}", out_path.display());
                } else {
                    println!("  No doc summary produced, skipping");
                }
            }
            Err(e) => {
                eprintln!("  Failed to process: {}", e);
            }
        }
    }

    println!("\n✓ Document summaries written to: {}", base.display());
    Ok(())
}
