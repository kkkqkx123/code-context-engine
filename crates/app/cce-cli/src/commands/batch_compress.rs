//! Batch compression command handler
//!
//! Compresses multiple code files in a single request.

use anyhow::Result;

use crate::client::ApiClient;
use crate::output::{print_error, print_success};
use cce_api::models::{BatchCompressRequest, BatchCompressResponse};

pub async fn execute(
    file_paths: &[String],
    include_entities: bool,
    include_groups: bool,
    max_concurrency: usize,
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    if verbose {
        println!("Compressing {} files...", file_paths.len());
    }

    // Validate input
    if file_paths.is_empty() {
        print_error("Must provide at least one file path to compress");
        return Ok(());
    }

    let request = BatchCompressRequest {
        file_paths: file_paths.to_vec(),
        include_entities: Some(include_entities),
        include_groups: Some(include_groups),
        max_concurrency,
    };

    match client
        .post::<BatchCompressRequest, BatchCompressResponse>("/api/tools/compress/batch", &request)
        .await
    {
        Ok(response) => {
            let total = response.successes.len() + response.failures.len();
            print_success(&format!(
                "Batch compression completed: {} succeeded, {} failed ({} total)",
                response.successes.len(),
                response.failures.len(),
                total,
            ));
            println!();

            // Display successes
            if !response.successes.is_empty() {
                println!("Successful:");
                for (i, entry) in response.successes.iter().enumerate() {
                    println!("  {}. {}", i + 1, entry.path);
                    println!("     Language: {}", entry.result.language);
                    if entry.result.from_cache {
                        println!("     (served from cache)");
                    }
                    println!();
                    println!("     Compressed text:");
                    for line in entry.result.semantic_text.lines() {
                        println!("     {}", line);
                    }
                }
            }

            // Display failures
            if !response.failures.is_empty() {
                if !response.successes.is_empty() {
                    println!();
                }
                println!("Failed:");
                for (i, entry) in response.failures.iter().enumerate() {
                    println!("  {}. {} - {}", i + 1, entry.path, entry.error);
                }
            }
        }
        Err(e) => {
            print_error(&format!("Failed to compress files: {}", e));
        }
    }

    Ok(())
}
