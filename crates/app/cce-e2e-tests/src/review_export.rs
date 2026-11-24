//! Shared review-fixture export pipeline for E2E examples.
//!
//! Each fixture is extracted, scanned and processed exactly once; the results
//! feed markdown NL-doc summary, chunk segmentation, and structured symbol /
//! relation outputs. Tree-sitter parse products (`ParsedFile`) are reused
//! between the NL export path and the relation / type-inference path so that
//! both pipelines operate on identical snapshots.
//!
//! Output (per language / fixture):
//!   outputs/scenarios/<lang>/summary/{fixture}/      — markdown NL docs
//!   outputs/scenarios/<lang>/chunks/{fixture}/emb/   — chunk segmentation (Embedding)
//!   outputs/scenarios/<lang>/chunks/{fixture}/bm25/  — chunk segmentation (BM25)
//!   outputs/scenarios/<lang>/structured/{fixture}/   — SUMMARY.md + per-file reports

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use cce_codegraph::IndexBuilder;
use cce_codegraph::index::{EntityIndexOps, RelationQueryOps};
use cce_orchestrator::export::{DirectExporter, ExportConfig};
use cce_parser::ast_to_nl::ConversionRequest;
use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_scanner::{FSScanner, ScanOptions};
use cce_types::{OutputMode, ParsedFile};

use crate::bench_gen::file_processor_for_root;
use crate::review_filter::ReviewFilterOptions;
use crate::structured_output::StructuredOutputWriter;
use crate::{FixtureSpec, OutputCategory, OutputManager, TestFixture};

/// One review-fixture export job.
pub struct ReviewExportJob {
    /// Language directory under `outputs/scenarios/` (e.g. `"rust"`).
    pub language: &'static str,
    /// Fixture directory name used in output paths (e.g. `"once_cell"`).
    pub fixture_name: &'static str,
    /// Fixture specification used by `TestFixture::load`.
    pub spec: FixtureSpec,
    /// Glob patterns used by the scanner (e.g. `&["*.rs"]`).
    pub include_patterns: &'static [&'static str],
    /// Output-side filter applied when writing reports (indexing stays
    /// exhaustive; filtering happens at write time).
    pub filter: ReviewFilterOptions,
}

/// Load and export every job, printing a completion banner at the end.
pub async fn export_jobs(jobs: &[ReviewExportJob]) {
    for job in jobs {
        let fixture = TestFixture::load(job.spec.clone()).unwrap_or_else(|e| {
            panic!(
                "Failed to load {}/{}: {}",
                job.language, job.fixture_name, e
            )
        });
        export_fixture(
            job.language,
            job.fixture_name,
            fixture,
            job.include_patterns,
            job.filter.clone(),
        )
        .await;
    }

    println!("\n=== All outputs generated ===");
}

/// Export summary, chunks, and structured outputs for one fixture.
pub async fn export_fixture(
    language: &str,
    fixture_name: &str,
    fixture: TestFixture,
    include_patterns: &[&str],
    filter: ReviewFilterOptions,
) {
    println!("\n=== Exporting fixture: {language}/{fixture_name} ===");

    let project_root = fixture.root_path().to_path_buf();
    let project_root_str = project_root.to_string_lossy().replace('\\', "/");

    let export_config = ExportConfig::new(project_root.clone(), 1);
    let direct_exporter = Arc::new(DirectExporter::new(export_config));

    let mut scanner = FSScanner::new();
    let scan_opts = ScanOptions {
        root_path: project_root.to_string_lossy().to_string(),
        include_patterns: include_patterns.iter().map(|s| (*s).to_string()).collect(),
        ..Default::default()
    };
    let file_entries = scanner
        .scan(&scan_opts)
        .expect("Failed to scan fixture directory");

    let mut file_processor = file_processor_for_root(&project_root);
    let mut indexed_files = 0;
    let mut emb_by_file: BTreeMap<String, Vec<ChunkedResult>> = BTreeMap::new();
    let mut bm25_by_file: BTreeMap<String, Vec<ChunkedResult>> = BTreeMap::new();
    let mut parsed_files: Vec<ParsedFile> = Vec::new();

    for entry in &file_entries {
        let file_path = entry.path.to_string_lossy().to_string();
        let relative_path = normalize_file_path(&file_path, &project_root_str);

        match file_processor
            .process_file_complete(entry, OutputMode::Embedding)
            .await
        {
            Ok(result) => {
                // Retain the ParsedFile for the structured (relation / type-inference)
                // path before it is moved into conversion. This reuses the same
                // tree-sitter snapshot that produced the NL chunks.
                let parsed_for_structured = result.parsed_file.clone();

                if let Some(processing_result) = result.processing_result {
                    if !processing_result.groups.is_empty() {
                        let converter = file_processor.converter();
                        let source = &*result.parsed_file.source;
                        let request = ConversionRequest {
                            force_mode: Some(OutputMode::Embedding),
                        };
                        let conversions = converter
                            .convert_entity_groups(
                                &processing_result.groups,
                                &file_path,
                                Some(&request),
                                Some(&processing_result),
                                Some(source),
                            )
                            .groups;
                        match direct_exporter
                            .export_groups(&conversions, &file_path)
                            .await
                        {
                            Ok(_) => indexed_files += 1,
                            Err(e) => eprintln!("  Export error: {}", e),
                        }
                    }
                }
                if !result.chunks.is_empty() {
                    emb_by_file
                        .entry(relative_path.clone())
                        .or_default()
                        .extend(result.chunks);
                }
                // Keep the parse product for relation construction even when the
                // file yielded no groups or entities (empty type-inference tables).
                parsed_files.push(parsed_for_structured);
            }
            Err(e) => eprintln!("  Process error: {}", e),
        }

        if let Ok(result) = file_processor
            .process_file_complete(entry, OutputMode::Bm25)
            .await
        {
            if !result.chunks.is_empty() {
                bm25_by_file
                    .entry(relative_path)
                    .or_default()
                    .extend(result.chunks);
            }
        }
    }

    sort_chunks_by_line(&mut emb_by_file);
    sort_chunks_by_line(&mut bm25_by_file);

    write_summary_output(
        language,
        fixture_name,
        &project_root,
        indexed_files,
        &filter,
    );
    let total_emb = write_chunk_outputs(
        language,
        &emb_by_file,
        &format!("chunks/{fixture_name}/emb"),
        &filter,
    );
    let total_bm25 = write_chunk_outputs(
        language,
        &bm25_by_file,
        &format!("chunks/{fixture_name}/bm25"),
        &filter,
    );
    println!("  {fixture_name} chunks: emb={total_emb} bm25={total_bm25}");

    // Structured output: reuse the same ParsedFile snapshots for relation
    // graph construction and type inference so that export and relation
    // analysis do not re-parse the files.
    write_structured_output(
        language,
        fixture_name,
        &project_root,
        &parsed_files,
        &filter,
    );
}

fn sort_chunks_by_line(chunks_by_file: &mut BTreeMap<String, Vec<ChunkedResult>>) {
    for chunks in chunks_by_file.values_mut() {
        chunks.sort_by(|a, b| {
            a.metadata
                .source_span
                .line_range_opt()
                .map(|range| range.0)
                .unwrap_or(usize::MAX)
                .cmp(
                    &b.metadata
                        .source_span
                        .line_range_opt()
                        .map(|range| range.0)
                        .unwrap_or(usize::MAX),
                )
        });
    }
}

fn write_summary_output(
    language: &str,
    fixture_name: &str,
    project_root: &Path,
    indexed_files: usize,
    filter: &ReviewFilterOptions,
) {
    let cce_dir = project_root.join(".cce").join("nl_docs");
    let exported_files = collect_md_files(&cce_dir);

    let output_mgr = OutputManager::builder()
        .category(OutputCategory::Scenarios)
        .language(language)
        .scenario(format!("summary/{fixture_name}"))
        .build();
    let output_dir = output_mgr
        .ensure_output_dir()
        .expect("Failed to create output dir");

    let mut copied_files = Vec::new();
    for file in &exported_files {
        let relative = file.strip_prefix(&cce_dir).unwrap_or(file);
        // NL doc files mirror the source path with a `.md` suffix; the filter
        // rules are defined on source paths.
        let source_relative = relative
            .to_string_lossy()
            .trim_end_matches(".md")
            .to_string();
        if filter.is_excluded_path(&source_relative) {
            continue;
        }
        copy_to_output(&cce_dir, &output_dir, file).ok();
        copied_files.push(relative.to_path_buf());
    }

    let mut summary = String::new();
    summary.push_str(&format!("# Export Output for {fixture_name} Fixture\n\n"));
    summary.push_str(&format!("**Indexed files:** {}\n", indexed_files));
    summary.push_str(&format!("**Exported files:** {}\n\n", copied_files.len()));
    summary.push_str("## Files\n\n");
    copied_files.sort();
    for rel_path in &copied_files {
        summary.push_str(&format!(
            "- {}\n",
            rel_path.to_string_lossy().replace('\\', "/")
        ));
    }
    output_mgr.write("SUMMARY.md", &summary).ok();

    println!("  {fixture_name} summary: {} files", copied_files.len());
}

/// Build a relation index from the same `ParsedFile` snapshots that fed the
/// NL export, then emit structured symbol / relation / type-inference files.
///
/// This reuses the tree-sitter parse without a second filesystem read; the
/// `ParsedFile` list is already the canonical source for both pipelines.
fn write_structured_output(
    language: &str,
    fixture_name: &str,
    project_root: &Path,
    parsed_files: &[ParsedFile],
    filter: &ReviewFilterOptions,
) {
    if parsed_files.is_empty() {
        eprintln!("  {fixture_name} structured: no parsed files, skipping");
        return;
    }

    // Build a full relation index from the in-memory parse products.
    let builder = IndexBuilder::new();
    let project_symbols = builder.create_project_symbol_table(project_root);
    for pf in parsed_files {
        builder.add_file_symbols(pf, &project_symbols);
    }
    for pf in parsed_files {
        builder.register_file_entities(pf);
    }
    for pf in parsed_files {
        builder.resolve_file_relations(pf, &project_symbols);
    }
    let index = builder.build();

    let writer = StructuredOutputWriter::for_scenarios(language, fixture_name);
    match writer.write_all(&index, parsed_files, filter) {
        Ok(paths) => {
            println!(
                "  {fixture_name} structured: {} files ({} entities, {} relations) -> {}",
                paths.len(),
                index.function_count(),
                index.resolved_relation_count(),
                writer.ensure_dir().unwrap_or_default().display()
            );
            for p in &paths {
                println!("    - {}", p.display());
            }
        }
        Err(e) => eprintln!("  {fixture_name} structured error: {e}"),
    }
}

/// Write chunk segmentation files for the given scenario. Returns the total
/// number of chunks written.
fn write_chunk_outputs(
    language: &str,
    chunks_by_file: &BTreeMap<String, Vec<ChunkedResult>>,
    scenario: &str,
    filter: &ReviewFilterOptions,
) -> usize {
    let chunking_mgr = OutputManager::builder()
        .category(OutputCategory::Scenarios)
        .language(language)
        .scenario(scenario)
        .build();
    let output_dir = chunking_mgr
        .ensure_output_dir()
        .expect("Failed to create chunking output dir");

    for (file_path, chunks) in chunks_by_file {
        if filter.is_excluded_path(file_path) {
            continue;
        }
        let path = Path::new(file_path);
        let content = render_chunking_file(chunks);
        let file_name = path
            .file_name()
            .map(|f| format!("{}.txt", f.to_string_lossy()))
            .unwrap_or_else(|| "unknown.txt".to_string());
        let file_dir = path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let target_dir = output_dir.join(&file_dir);
        std::fs::create_dir_all(&target_dir).expect("Failed to create target dir");
        let target_path = target_dir.join(&file_name);
        std::fs::write(&target_path, &content)
            .unwrap_or_else(|e| panic!("Failed to write {file_name}: {e}"));
    }

    chunks_by_file.values().map(Vec::len).sum()
}

fn collect_files_by_extension(dir: &Path, extension: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if !dir.exists() {
        return files;
    }
    collect_files_recursive(dir, extension, &mut files);
    files
}

fn collect_md_files(dir: &Path) -> Vec<std::path::PathBuf> {
    collect_files_by_extension(dir, "md")
}

fn collect_files_recursive(dir: &Path, extension: &str, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files_recursive(&path, extension, files);
            } else if path.extension().is_some_and(|e| e == extension) {
                files.push(path);
            }
        }
    }
}

fn copy_to_output(cce_dir: &Path, output_dir: &Path, file: &Path) -> std::io::Result<()> {
    let relative = file
        .strip_prefix(cce_dir)
        .unwrap_or_else(|_| file.file_name().map(Path::new).unwrap_or(file));
    let target = output_dir.join(relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(file, &target)?;
    Ok(())
}

fn normalize_file_path(abs_path: &str, manifest_dir: &str) -> String {
    let abs = abs_path.replace('\\', "/");
    let manifest_dir = manifest_dir.replace('\\', "/");
    if let Some(stripped) = abs.strip_prefix(&manifest_dir) {
        let stripped = stripped.trim_start_matches('/');
        if !stripped.is_empty() {
            return stripped.to_string();
        }
    }
    abs
}

fn render_chunking_file(chunks: &[cce_parser::ast_to_nl::chunker::ChunkedResult]) -> String {
    let mut out = String::new();
    let total = chunks.len();

    out.push_str(&format!("total chunks: {total}\n"));
    out.push_str("---\n");

    for (i, chunk) in chunks.iter().enumerate() {
        let (start_line, end_line) = chunk
            .metadata
            .source_span
            .line_range_opt()
            .unwrap_or((0, 0));
        let code_meta = chunk.metadata.code_metadata.as_ref();

        out.push_str(&format!(
            "\n=== CHUNK {}/{} (lines {start_line}-{end_line}) ===\n",
            i + 1,
            total,
        ));
        out.push_str(&format!(
            "chunk_id: {} | group: {} | kind: {}\n",
            chunk.chunk_id,
            chunk.source_group_id,
            code_meta
                .map(|m| format!("{:?}", m.entity_kind))
                .unwrap_or_else(|| "n/a".to_string())
        ));

        let is_fragment = code_meta.is_some_and(|m| m.is_fragment);
        if is_fragment {
            let frag_idx = code_meta.and_then(|m| m.fragment_index).unwrap_or(0);
            let total_frags = code_meta.and_then(|m| m.total_fragments).unwrap_or(0);
            out.push_str(&format!(
                "fragment: {}/{total_frags} | split_reason: {}\n",
                frag_idx + 1,
                code_meta
                    .map(|m| format!("{:?}", m.split_reason))
                    .unwrap_or_else(|| "n/a".to_string())
            ));
        } else {
            out.push_str(&format!(
                "split_reason: {}\n",
                code_meta
                    .map(|m| format!("{:?}", m.split_reason))
                    .unwrap_or_else(|| "n/a".to_string())
            ));
        }

        if chunk.path == cce_parser::ast_to_nl::chunker::ChunkPath::Bm25 {
            out.push_str(&format!(
                "title: {} | tokens: {}\n",
                chunk.bm25_title.as_deref().unwrap_or("n/a"),
                chunk.token_count
            ));
            out.push_str(&format!("keywords: [{}]\n", chunk.bm25_keywords.join(", ")));
        } else {
            out.push_str(&format!("tokens: {}\n", chunk.token_count));
        }

        if !chunk.related_groups.is_empty() {
            let rel_str: Vec<String> = chunk
                .related_groups
                .iter()
                .map(|rel| format!("{} ({})", rel.group_id, rel.relation_type))
                .collect();
            out.push_str(&format!("related: {}\n", rel_str.join(", ")));
        }

        if let Some(o) = &chunk.prev_overlap {
            out.push_str(&format!(
                "prev_overlap: {} tokens from {}\n",
                o.token_count, o.source_chunk_id
            ));
        }
        if let Some(o) = &chunk.next_overlap {
            out.push_str(&format!(
                "next_overlap: {} tokens from {}\n",
                o.token_count, o.source_chunk_id
            ));
        }

        out.push('\n');
        out.push_str(&chunk.text);
        out.push('\n');
    }

    out
}
