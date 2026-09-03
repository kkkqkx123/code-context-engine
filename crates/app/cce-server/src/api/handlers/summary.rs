//! File summary handler
//!
//! Provides temporary file summary generation without storage.
//! Supports single files, multiple files, and directory scanning.

use axum::{Json, extract::State};
use std::path::PathBuf;
use tracing::{debug, info};

use cce_api::models::{
    ErrorResponse, FileSummaryItem, SummaryRequest, SummaryResponse, error_codes,
};
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::summary::RuleBasedGenerator;
use cce_scanner::{FSScanner, ScanOptions};
use cce_utils::file::read_file_to_utf8_async;

use crate::api::response::ApiResult;

/// Unified response type for summary handler
pub type SummaryApiResponse = ApiResult<SummaryResponse>;

/// Handle summary generation request
///
/// Generates temporary file summaries without storing them.
/// Supports single files, multiple files, and directory scanning.
pub async fn handle_summary(
    State(state): State<crate::api::state::AppState>,
    Json(request): Json<SummaryRequest>,
) -> SummaryApiResponse {
    let start = std::time::Instant::now();

    // Validate request
    if request.file_paths.is_empty() && request.directory_paths.is_empty() {
        return SummaryApiResponse::Error(ErrorResponse::new(
            error_codes::INVALID_REQUEST,
            "Must provide file_paths or directory_paths",
        ));
    }

    // Collect all files to process
    let mut files_to_process = Vec::new();
    let mut warnings = Vec::new();

    // Add individual files
    for path in &request.file_paths {
        let path_buf = PathBuf::from(path);
        if path_buf.exists() && path_buf.is_file() {
            files_to_process.push(path_buf);
        } else {
            warnings.push(format!("File does not exist or is not a file: {}", path));
        }
    }

    // Scan directories
    for dir_path in &request.directory_paths {
        let dir_path_buf = PathBuf::from(dir_path);
        if !dir_path_buf.exists() {
            warnings.push(format!("Directory does not exist: {}", dir_path));
            continue;
        }

        if !dir_path_buf.is_dir() {
            warnings.push(format!("Path is not a directory: {}", dir_path));
            continue;
        }

        // Build scan options
        let mut include_patterns = Vec::new();
        if !request.extensions.is_empty() {
            for ext in &request.extensions {
                include_patterns.push(format!("**/*.{}", ext));
            }
        }

        let scan_opts = ScanOptions {
            root_path: dir_path.clone(),
            include_patterns,
            exclude_patterns: request.exclude_dirs.clone(),
            respect_gitignore: request.respect_gitignore,
            gitignore_patterns: request.ignore_patterns.clone(),
            follow_symlinks: false,
            gitignore_path: None,
            max_content_size: None,
            max_file_size: None,
        };

        let mut scanner = FSScanner::new();
        match scanner.scan(&scan_opts) {
            Ok(entries) => {
                debug!(count = entries.len(), dir = %dir_path, "Scanned directory");
                for entry in entries {
                    files_to_process.push(entry.path);
                }
            }
            Err(e) => {
                warnings.push(format!("Scanning directory failed {}: {}", dir_path, e));
            }
        }
    }

    // Check max files limit
    if files_to_process.len() > request.max_files {
        warnings.push(format!(
            "The number of files exceeds the limit {}, only the first {} files will be processed",
            request.max_files,
            files_to_process.len()
        ));
        files_to_process.truncate(request.max_files);
    }

    // Remove duplicates while preserving order
    let mut seen = std::collections::HashSet::new();
    files_to_process.retain(|p| {
        let key = p.to_string_lossy().to_string();
        seen.insert(key)
    });

    info!(
        count = files_to_process.len(),
        "Processing files for summary"
    );

    // Generate summaries
    let mut summaries = Vec::new();
    let mut success_count = 0;
    let mut failed_count = 0;

    let generator = RuleBasedGenerator::new();
    let preprocessing_pipeline = PreprocessingPipeline::new();
    let mut parser = state.parser.lock().await;

    for file_path in files_to_process {
        let file_path_str = file_path.to_string_lossy().to_string();
        debug!(path = %file_path_str, "Processing file");

        // Read file content
        let content = match read_file_to_utf8_async(&file_path).await {
            Ok(content) => content,
            Err(e) => {
                summaries.push(FileSummaryItem {
                    file_path: file_path_str,
                    language: String::new(),
                    summary: String::new(),
                    main_entities: Vec::new(),
                    imports: Vec::new(),
                    exports: Vec::new(),
                    entity_count: 0,
                    line_count: 0,
                    tags: Vec::new(),
                    importance_level: "unknown".to_string(),
                    success: false,
                    error: Some(format!("fail to read file: {}", e)),
                });
                failed_count += 1;
                continue;
            }
        };

        // Parse file
        let parsed = match parser.parse(&file_path_str, &content) {
            Ok(parsed) => parsed,
            Err(e) => {
                summaries.push(FileSummaryItem {
                    file_path: file_path_str,
                    language: String::new(),
                    summary: String::new(),
                    main_entities: Vec::new(),
                    imports: Vec::new(),
                    exports: Vec::new(),
                    entity_count: 0,
                    line_count: 0,
                    tags: Vec::new(),
                    importance_level: "unknown".to_string(),
                    success: false,
                    error: Some(format!("fail to parse file: {}", e)),
                });
                failed_count += 1;
                continue;
            }
        };

        // Generate processing result with grouper enrichment for higher-quality summaries
        let processing_result = preprocessing_pipeline.process(&parsed);

        // Generate summary using enriched group information
        let file_summary = generator
            .generate_with_groups(&parsed, &processing_result)
            .await;

        summaries.push(FileSummaryItem {
            file_path: file_path_str,
            language: file_summary.language,
            summary: file_summary.summary_text,
            main_entities: file_summary.main_entities,
            imports: file_summary.imports,
            exports: file_summary.exports,
            entity_count: file_summary.entity_count,
            line_count: file_summary.line_count,
            tags: file_summary.tags,
            importance_level: format!("{:?}", file_summary.importance_level).to_lowercase(),
            success: true,
            error: None,
        });
        success_count += 1;
    }

    drop(parser); // Release lock

    let elapsed_ms = start.elapsed().as_millis() as u64;

    info!(
        total = summaries.len(),
        success = success_count,
        failed = failed_count,
        elapsed_ms = elapsed_ms,
        "Summary generation completed"
    );

    SummaryApiResponse::Success(SummaryResponse {
        success: failed_count == 0 || success_count > 0,
        total_files: summaries.len(),
        success_count,
        failed_count,
        summaries,
        elapsed_ms,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_request_validation() {
        let request = SummaryRequest {
            file_paths: vec![],
            directory_paths: vec![],
            extensions: vec![],
            exclude_dirs: vec![],
            respect_gitignore: true,
            ignore_patterns: vec![],
            recursive: true,
            max_files: 100,
        };

        assert!(request.file_paths.is_empty() && request.directory_paths.is_empty());
    }
}
