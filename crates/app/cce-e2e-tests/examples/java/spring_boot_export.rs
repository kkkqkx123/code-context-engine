//! Generates NL document export snapshots for the Spring Boot minimal demo fixture.
//!
//! Output:
//!   outputs/scenarios/java/summary/springboot-minimal-demo/

use std::path::Path;
use std::sync::Arc;

use cce_orchestrator::export::{ExportConfig, NlDocumentExporter};
use cce_orchestrator::{IndexOptions, IndexOrchestrator};

use cce_e2e_tests::{OutputCategory, OutputManager, TestFixture, init_minimal_logging};

fn collect_md_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if !dir.exists() {
        return files;
    }
    collect_md_files_recursive(dir, &mut files);
    files
}

fn collect_md_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_md_files_recursive(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                files.push(path);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    init_minimal_logging();

    let fixture =
        TestFixture::java_spring_boot().expect("Failed to load Spring Boot minimal demo fixture");
    let project_root = fixture.root_path().to_path_buf();

    let export_config = ExportConfig::new(project_root.clone(), 1)
        .with_summary(true)
        .with_relation_enhancement(false);
    let exporter = Arc::new(NlDocumentExporter::new(export_config));

    let mut orchestrator = IndexOrchestrator::new(1)
        .expect("failed to create IndexOrchestrator")
        .with_nl_exporter(exporter);
    let options = IndexOptions {
        root_dir: project_root.clone(),
        extensions: vec![
            "java".to_string(),
            "xml".to_string(),
            "properties".to_string(),
            "md".to_string(),
        ],
        store_vectors: false,
        store_bm25: false,
        build_relations: true,
        ..IndexOptions::new(&project_root)
    };

    let result = orchestrator
        .execute(options)
        .await
        .expect("Index should succeed");

    let export_dir = project_root.join(".cce").join("nl_docs");
    let exported_files = collect_md_files(&export_dir);

    let export_output_mgr = OutputManager::builder()
        .category(OutputCategory::Scenarios)
        .language("java")
        .scenario("summary/springboot-minimal-demo")
        .build();
    let export_output_dir = export_output_mgr
        .ensure_output_dir()
        .expect("Failed to create export output directory");

    let mut copied_export_files = Vec::new();
    for file in &exported_files {
        let relative = file
            .strip_prefix(&export_dir)
            .expect("Exported file should be under .cce/nl_docs");
        let target = export_output_dir.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create parent directory");
        }
        std::fs::copy(file, &target).expect("Failed to copy export output");
        copied_export_files.push(relative.to_path_buf());
    }

    copied_export_files.sort();
    let mut export_summary = String::new();
    export_summary.push_str("# Export Output for Spring Boot Minimal Demo Fixture\n\n");
    export_summary.push_str(&format!("**Indexed files:** {}  \n", result.indexed_files));
    export_summary.push_str(&format!(
        "**Total entities:** {}  \n",
        result.total_entities
    ));
    export_summary.push_str(&format!(
        "**Total relations:** {}  \n",
        result.total_relations
    ));
    export_summary.push_str(&format!(
        "**Exported files:** {}  \n\n",
        copied_export_files.len()
    ));
    export_summary.push_str("## Exported Documents\n\n");
    for rel_path in &copied_export_files {
        let rel_str = rel_path.to_string_lossy().replace('\\', "/");
        export_summary.push_str(&format!("- [{}]({})\n", rel_str, rel_str));
    }
    export_summary.push_str("\n---\n");
    export_summary
        .push_str("_This directory contains auto-generated export output for manual review._\n");
    export_output_mgr
        .write("SUMMARY.md", &export_summary)
        .expect("Failed to write export summary");
}
