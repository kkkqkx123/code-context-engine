//! Summary command handler
//!
//! Generates temporary file summaries without storage.
//! Supports single files, multiple files, and directory scanning.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::client::ApiClient;
use crate::output::{print_error, print_success, print_warning};

/// Summary request structure (matches API model)
#[derive(Debug, Serialize)]
struct SummaryRequest {
    #[serde(default)]
    file_paths: Vec<String>,
    #[serde(default)]
    directory_paths: Vec<String>,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    exclude_dirs: Vec<String>,
    respect_gitignore: bool,
    #[serde(default)]
    ignore_patterns: Vec<String>,
    recursive: bool,
    max_files: usize,
}

/// File summary item (matches API response model)
#[derive(Debug, Deserialize)]
struct FileSummaryItem {
    file_path: String,
    language: String,
    summary: String,
    main_entities: Vec<String>,
    imports: Vec<String>,
    exports: Vec<String>,
    entity_count: u32,
    line_count: u32,
    tags: Vec<String>,
    importance_level: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Summary response structure (matches API model)
#[derive(Debug, Deserialize)]
struct SummaryResponse {
    success: bool,
    total_files: usize,
    success_count: usize,
    failed_count: usize,
    summaries: Vec<FileSummaryItem>,
    elapsed_ms: u64,
    #[serde(default)]
    warnings: Vec<String>,
}

/// Input paths configuration
#[derive(Debug, Clone)]
pub struct InputPaths {
    pub files: Vec<String>,
    pub directories: Vec<String>,
}

/// File filtering configuration
#[derive(Debug, Clone)]
pub struct FilterConfig {
    pub extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub respect_gitignore: bool,
    pub max_files: usize,
}

/// Execution context
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub server: String,
    pub verbose: bool,
}

/// Summary command options - consolidates all parameters
#[derive(Debug, Clone)]
pub struct SummaryOptions {
    pub input: InputPaths,
    pub filters: FilterConfig,
    pub execution: ExecutionContext,
}

impl SummaryOptions {
    pub fn new(input: InputPaths, filters: FilterConfig, execution: ExecutionContext) -> Self {
        Self {
            input,
            filters,
            execution,
        }
    }
}

pub async fn execute(options: SummaryOptions) -> Result<()> {
    let client = ApiClient::new(&options.execution.server)?;

    if options.execution.verbose {
        println!("Generating summaries...");
        println!("  Files: {}", options.input.files.len());
        println!("  Directories: {}", options.input.directories.len());
        println!("  Extensions: {:?}", options.filters.extensions);
    }

    // Validate input
    if options.input.files.is_empty() && options.input.directories.is_empty() {
        print_error("Must provide at least one file path or directory path");
        return Ok(());
    }

    let request = SummaryRequest {
        file_paths: options.input.files.clone(),
        directory_paths: options.input.directories.clone(),
        extensions: options.filters.extensions.clone(),
        exclude_dirs: options.filters.exclude_dirs.clone(),
        respect_gitignore: options.filters.respect_gitignore,
        ignore_patterns: options.filters.ignore_patterns.clone(),
        recursive: true,
        max_files: options.filters.max_files,
    };

    match client
        .post::<SummaryRequest, SummaryResponse>("/api/summary", &request)
        .await
    {
        Ok(response) => {
            if response.success {
                print_success(&format!(
                    "Summary generation completed in {}ms",
                    response.elapsed_ms
                ));
                println!();
                println!("Total files: {}", response.total_files);
                println!("Successful: {}", response.success_count);
                println!("Failed: {}", response.failed_count);
                println!();

                // Display warnings if any
                if !response.warnings.is_empty() {
                    print_warning("Warnings:");
                    for warning in &response.warnings {
                        println!("  - {}", warning);
                    }
                    println!();
                }

                // Display summaries
                for (i, summary) in response.summaries.iter().enumerate() {
                    if i > 0 {
                        println!("\n{}", "─".repeat(80));
                    }

                    println!("File: {}", summary.file_path);
                    println!("Language: {}", summary.language);
                    println!("Lines: {}", summary.line_count);
                    println!("Entities: {}", summary.entity_count);
                    println!("Importance: {}", summary.importance_level);

                    if !summary.main_entities.is_empty() {
                        println!("Main Entities:");
                        for entity in &summary.main_entities {
                            println!("  - {}", entity);
                        }
                    }

                    if !summary.imports.is_empty() {
                        println!("Imports:");
                        for import in &summary.imports {
                            println!("  - {}", import);
                        }
                    }

                    if !summary.exports.is_empty() {
                        println!("Exports:");
                        for export in &summary.exports {
                            println!("  - {}", export);
                        }
                    }

                    if !summary.tags.is_empty() {
                        println!("Tags: {}", summary.tags.join(", "));
                    }

                    println!("\nSummary:");
                    println!("{}", summary.summary);

                    if !summary.success {
                        if let Some(ref error) = summary.error {
                            print_error(&format!("Error: {}", error));
                        }
                    }
                }
            } else {
                print_error("Summary generation failed");
            }
        }
        Err(e) => {
            print_error(&format!("Failed to generate summaries: {}", e));
        }
    }

    Ok(())
}
