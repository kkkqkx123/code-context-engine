//! Export type-inference visualization for every `type_inference` fixture.
//!
//! Each fixture directory is scanned and processed exactly once via
//! [`FileProcessor`](cce_orchestrator::index::FileProcessor); the resulting
//! tree-sitter [`ParsedFile`] snapshots feed two visualization paths that
//! share identical input:
//!
//! - NL docs via [`DirectExporter`](cce_orchestrator::export::DirectExporter)
//!   (`outputs/scenarios/<lang>/summary/<case>/`), following the
//!   `ChunkedResult -> FileAggregator -> FileNlDocument -> RelationEnhancer
//!   -> MarkdownFormatter` pipeline described in
//!   `crates/app/cce-orchestrator/src/export.rs`.
//! - Structured symbol / relation / type-inference reports via
//!   [`StructuredOutputWriter`](cce_e2e_tests::structured_output::StructuredOutputWriter)
//!   (`outputs/scenarios/<lang>/structured/<case>/` plus a standalone
//!   `TYPE_INFERENCE.md` rendered by
//!   [`render_type_inference`](cce_e2e_tests::structured_output::render_type_inference)).
//!
//! The markdown tables (`Variables | Inferred Type | Origin | Priority |
//! Shape | Span`, `Function Returns`, `Control-Flow Narrowing`,
//! `Type Shapes`) are the human-readable counterpart of the machine-checked
//! [`collect_type_bindings`](cce_e2e_tests::type_inference_assert::collect_type_bindings)
//! snapshot used by `tests/regression/type_inference/`.
//!
//! All output goes to the gitignored `outputs/` tree for human inspection
//! only. Regression tests never read these files; they assert on in-memory
//! bindings instead (see `tests/regression/type_inference/common.rs`).

use std::path::Path;
use std::sync::Arc;

use cce_codegraph::IndexBuilder;
use cce_codegraph::index::{EntityIndexOps, RelationQueryOps};
use cce_orchestrator::export::{DirectExporter, ExportConfig};
use cce_orchestrator::index::FileProcessor;
use cce_parser::ast_to_nl::ConversionRequest;
use cce_scanner::{FSScanner, ScanOptions};
use cce_types::{OutputMode, ParsedFile};

use cce_e2e_tests::structured_output::StructuredOutputWriter;
use cce_e2e_tests::type_inference_cases::{TypeInferenceCase, all_type_inference_cases};
use cce_e2e_tests::{OutputCategory, OutputManager, TestFixture, init_minimal_logging};

#[tokio::main]
async fn main() {
    init_minimal_logging();

    let cases = all_type_inference_cases();

    for case in &cases {
        match TestFixture::load(case.spec.clone()) {
            Ok(fixture) => export_case(case, fixture).await,
            Err(e) => eprintln!("skip {}/{}: {e}", case.language, case.scenario),
        }
    }

    println!("\n=== All type-inference visualizations generated ===");
}

async fn export_case(case: &TypeInferenceCase, fixture: TestFixture) {
    println!("\n=== Exporting {}/{} ===", case.language, case.scenario);
    let project_root = fixture.root_path().to_path_buf();

    let export_config = ExportConfig::new(project_root.clone(), 1);
    let direct_exporter = Arc::new(DirectExporter::new(export_config));

    let mut scanner = FSScanner::new();
    let scan_opts = ScanOptions {
        root_path: project_root.to_string_lossy().to_string(),
        include_patterns: case.patterns.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    };
    let file_entries = scanner
        .scan(&scan_opts)
        .expect("Failed to scan fixture directory");

    let mut file_processor = FileProcessor::new();
    let mut parsed_files: Vec<ParsedFile> = Vec::new();
    let mut indexed_files = 0;

    for entry in &file_entries {
        let file_path = entry.path.to_string_lossy().to_string();
        match file_processor
            .process_file_complete(entry, OutputMode::Embedding)
            .await
        {
            Ok(result) => {
                let parsed = result.parsed_file.clone();
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
                            Err(e) => eprintln!("  Export error: {e}"),
                        }
                    }
                }
                parsed_files.push(parsed);
            }
            Err(e) => eprintln!("  Process error: {e}"),
        }
    }

    copy_nl_docs_to_output(case, &project_root, indexed_files);

    if parsed_files.is_empty() {
        eprintln!(
            "  {}: no parsed files, skipping structured output",
            case.scenario
        );
        return;
    }

    let builder = IndexBuilder::new();
    let project_symbols = builder.create_project_symbol_table(&project_root);
    for pf in &parsed_files {
        builder.add_file_symbols(pf, &project_symbols);
    }
    for pf in &parsed_files {
        builder.register_file_entities(pf);
    }
    for pf in &parsed_files {
        builder.resolve_file_relations(pf, &project_symbols);
    }
    let index = builder.build();

    let writer = StructuredOutputWriter::for_scenarios(case.language, case.scenario);
    match writer.write_all(&index, &parsed_files, &Default::default()) {
        Ok(paths) => println!(
            "  structured: {} files ({} entities, {} relations)",
            paths.len(),
            index.function_count(),
            index.resolved_relation_count()
        ),
        Err(e) => eprintln!("  structured error: {e}"),
    }
    match writer.write_type_inference(&parsed_files) {
        Ok(path) => println!("  type inference: {}", path.display()),
        Err(e) => eprintln!("  type inference error: {e}"),
    }
}

fn copy_nl_docs_to_output(case: &TypeInferenceCase, project_root: &Path, indexed_files: usize) {
    let cce_dir = project_root.join(".cce").join("nl_docs");
    let output_mgr = OutputManager::builder()
        .category(OutputCategory::Scenarios)
        .language(case.language)
        .scenario(format!("summary/{}", case.scenario))
        .build();
    let Ok(output_dir) = output_mgr.ensure_output_dir() else {
        return;
    };
    let mut copied = 0;
    let mut stack = vec![cce_dir.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                if let Ok(rel) = path.strip_prefix(&cce_dir) {
                    let target = output_dir.join(rel);
                    if let Some(parent) = target.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if std::fs::copy(&path, &target).is_ok() {
                        copied += 1;
                    }
                }
            }
        }
    }
    let summary = format!(
        "# Type-inference export for {}/{}\n\n**Indexed files:** {}\n**Exported files:** {}\n",
        case.language, case.scenario, indexed_files, copied
    );
    let _ = output_mgr.write("SUMMARY.md", &summary);
    println!("  summary: {copied} files");
}
