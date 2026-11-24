//! Export file-fold results for the tool fixture group.
//!
//! Reads each file under `fixtures/tools/file-fold`, folds it through the
//! stateless orchestrator fold tool, and writes human-review skeletons under
//! `outputs/tools/file-fold`.
//!
//! Usage:
//! ```shell
//! cargo run -p cce-e2e-tests --example export_file_fold
//! ```
//!
//! Output: outputs/tools/file-fold/SUMMARY.md
//! Output: outputs/tools/file-fold/folded/<flattened-file>.<mode>.md

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use cce_e2e_tests::{OutputCategory, OutputManager, TestFixture, init_minimal_logging};
use cce_orchestrator::{FileFoldMode, FileFoldRequest, FileFoldResponse, FileFoldTool};
use cce_parser::parser::ParseCoordinator;

struct FoldVariant {
    name: &'static str,
    mode: FileFoldMode,
    max_tokens: usize,
    description: &'static str,
}

const VARIANTS: [FoldVariant; 3] = [
    FoldVariant {
        name: "detailed",
        mode: FileFoldMode::Detailed,
        max_tokens: 600,
        description: "Keeps signatures and shows normal skeleton size.",
    },
    FoldVariant {
        name: "minimal",
        mode: FileFoldMode::Minimal,
        max_tokens: 600,
        description: "Keeps names only and shows signature reduction.",
    },
    FoldVariant {
        name: "tight",
        mode: FileFoldMode::Detailed,
        max_tokens: 80,
        description: "Uses a small budget to expose section dropping or truncation.",
    },
];

fn main() -> Result<(), Box<dyn Error>> {
    init_minimal_logging();

    let fixture = TestFixture::tools_file_fold()?;
    let root = fixture.root_path().to_path_buf();
    let files = collect_relative_files(&root)?;

    if files.is_empty() {
        eprintln!("No file-fold fixtures found under fixtures/tools/file-fold");
        return Ok(());
    }

    let output_mgr = OutputManager::builder()
        .category(OutputCategory::Tools)
        .scenario("file-fold")
        .build();
    output_mgr.clean().ok();
    let base = output_mgr.ensure_output_dir()?;
    let folded_dir = base.join("folded");
    fs::create_dir_all(&folded_dir)?;

    let mut coordinator = ParseCoordinator::new();
    let mut summary_rows = Vec::new();

    for rel_path in &files {
        let abs_path = fixture.file(rel_path);
        let text = fs::read_to_string(&abs_path)?;
        let base_variant = &VARIANTS[0];
        let base_response = fold_file(
            &mut coordinator,
            &text,
            rel_path,
            base_variant,
            &folded_dir,
            &mut summary_rows,
        )?;

        for variant in VARIANTS.iter().skip(1) {
            if variant.name == "minimal" && !base_response.structure_known {
                continue;
            }
            if variant.name == "tight" && variant.max_tokens >= base_response.folded_tokens {
                continue;
            }

            fold_file(
                &mut coordinator,
                &text,
                rel_path,
                variant,
                &folded_dir,
                &mut summary_rows,
            )?;
        }
    }

    let summary = render_summary(&files.len(), &summary_rows);
    output_mgr.write("SUMMARY.md", &summary)?;
    println!("✓ File-fold export written to: {}", base.display());
    Ok(())
}

fn fold_file(
    coordinator: &mut ParseCoordinator,
    text: &str,
    rel_path: &Path,
    variant: &FoldVariant,
    folded_dir: &Path,
    summary_rows: &mut Vec<String>,
) -> Result<FileFoldResponse, Box<dyn Error>> {
    let request = FileFoldRequest::new(text.to_string())
        .with_file_name(rel_path.to_string_lossy().to_string())
        .with_max_tokens(variant.max_tokens)
        .with_mode(variant.mode);
    let response = FileFoldTool::fold(coordinator, request);

    let output_file = format!("{}.{}.md", safe_output_name(rel_path), variant.name);
    let content = render_fold_export(rel_path, variant, &response);
    write_nested(folded_dir, &output_file, &content)?;

    summary_rows.push(format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        rel_path.to_string_lossy(),
        variant.name,
        variant.max_tokens,
        response.language,
        response.structure_known,
        response.original_tokens,
        response.folded_tokens,
        response.kept_sections,
        response.dropped_sections,
    ));

    Ok(response)
}

fn render_summary(file_count: &usize, rows: &[String]) -> String {
    let mut out = String::new();
    out.push_str("# File Fold Export\n\n");
    out.push_str("Fixture group: `fixtures/tools/file-fold`\n\n");
    out.push_str("Variants:\n\n");
    for variant in VARIANTS.iter() {
        out.push_str(&format!(
            "- `{}`: max_tokens={} - {}\n",
            variant.name, variant.max_tokens, variant.description
        ));
    }
    out.push_str(&format!("\nFixture files: {file_count}\n\n"));
    out.push_str("`minimal` is emitted only when the `detailed` fold has known structure; `tight` is emitted only when the smaller budget can further reduce the `detailed` result.\n\n");
    out.push_str(
        "| File | Mode | Budget | Language | Structure | Original | Folded | Kept | Dropped |\n",
    );
    out.push_str("|---|---|---:|---|---|---:|---:|---:|---:|\n");
    for row in rows {
        out.push_str(row);
        out.push('\n');
    }
    out.push_str(
        "\nEach row is generated by `FileFoldTool::fold` with only raw text and file-name hints.\n",
    );
    out
}

fn render_fold_export(
    rel_path: &Path,
    variant: &FoldVariant,
    response: &cce_orchestrator::FileFoldResponse,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Fold export: {}\n\n",
        rel_path.to_string_lossy()
    ));
    out.push_str(&format!("- Mode: {}\n", variant.name));
    out.push_str(&format!("- Max tokens: {}\n", variant.max_tokens));
    out.push_str(&format!("- Language: {}\n", response.language));
    out.push_str(&format!(
        "- Structure known: {}\n",
        response.structure_known
    ));
    out.push_str(&format!(
        "- Tokens: {} -> {}\n",
        response.original_tokens, response.folded_tokens
    ));
    out.push_str(&format!(
        "- Sections: kept={}, dropped={}\n\n",
        response.kept_sections, response.dropped_sections
    ));
    out.push_str("```text\n");
    out.push_str(&response.folded_text);
    if !response.folded_text.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```\n");
    out
}

fn collect_relative_files(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut files = Vec::new();
    collect_dir(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_dir(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_dir(root, &path, files)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            files.push(rel.to_path_buf());
        }
    }
    Ok(())
}

fn safe_output_name(path: &Path) -> String {
    path.to_string_lossy().replace(['/', '\\'], "__")
}

fn write_nested(dir: &Path, file_name: &str, content: &str) -> Result<(), Box<dyn Error>> {
    let path = dir.join(file_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}
