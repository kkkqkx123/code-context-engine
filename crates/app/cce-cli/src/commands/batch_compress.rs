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
                for (i, (file_path, result)) in response.successes.iter().enumerate() {
                    println!("  {}. {}", i + 1, file_path);
                    if result.success {
                        let ratio = if result.original_size > 0 {
                            (result.compressed_size as f64 / result.original_size as f64) * 100.0
                        } else {
                            0.0
                        };
                        println!("     Original size: {} bytes", result.original_size);
                        println!("     Compressed size: {} bytes", result.compressed_size);
                        println!("     Compression ratio: {:.1}%", ratio);
                        println!();
                        println!("     Compressed code:");
                        for line in result.compressed.lines() {
                            println!("     {}", line);
                        }
                    } else {
                        print_error(&format!("     Failed: {}", result.compressed));
                    }
                }
            }

            // Display failures
            if !response.failures.is_empty() {
                if !response.successes.is_empty() {
                    println!();
                }
                println!("Failed:");
                for (i, (file_path, error)) in response.failures.iter().enumerate() {
                    println!("  {}. {} - {}", i + 1, file_path, error);
                }
            }
        }
        Err(e) => {
            print_error(&format!("Failed to compress files: {}", e));
        }
    }

    Ok(())
}
