//! Storage command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::output::{format_duration, print_error, print_success};
use cce_api::models::{BatchDeleteRequest, ClearIndexRequest};
use cce_client::Client;

pub async fn execute_with_client(
    cmd: &StorageCommands,
    client: &Client,
    verbose: bool,
) -> Result<()> {
    match cmd {
        StorageCommands::Status => storage_status(client, verbose).await,
        StorageCommands::Stats { project_id } => index_stats(*project_id, client, verbose).await,
        StorageCommands::Clear {
            project_id,
            vectors,
            bm25,
            relations,
            cache,
        } => {
            clear_index(
                *project_id,
                client,
                *vectors,
                *bm25,
                *relations,
                *cache,
                verbose,
            )
            .await
        }
        StorageCommands::DeleteFile { project_id, path } => {
            delete_file(*project_id, client, path, verbose).await
        }
        StorageCommands::DeleteEntity { project_id, id } => {
            delete_entity(*project_id, client, id, verbose).await
        }
        StorageCommands::BatchDelete {
            project_id,
            files,
            entities,
        } => batch_delete(*project_id, client, files, entities, verbose).await,
    }
}

async fn storage_status(client: &Client, verbose: bool) -> Result<()> {
    if verbose {
        println!("Fetching storage status...");
    }

    let response = client.storage_status().await?;

    if response.success {
        let status = &response.status;
        println!("Storage status:");
        println!();
        println!("Vector storage:");
        print_component_status(&status.vector_storage);
        println!();
        println!("BM25 storage:");
        print_component_status(&status.bm25_storage);
        println!();
        println!("Relation storage:");
        print_component_status(&status.relation_storage);
        println!();
        println!("Total disk usage: {:.2} MB", status.total_disk_usage_mb);

        if let Some(ref process) = status.process_status {
            println!();
            println!("Qdrant process:");
            println!(
                "  Management:   {}",
                if process.managed {
                    "Enabled"
                } else {
                    "Disabled"
                }
            );
            println!("  Status:       {}", process.status);
            println!("  Running:      {}", process.running);
        }
    } else {
        print_error("Failed to get storage status");
    }

    Ok(())
}

fn print_component_status(component: &cce_api::models::StorageComponentStatus) {
    println!("  Backend:      {}", component.backend);
    println!("  Connected:    {}", component.connected);
    println!("  Items:        {}", component.item_count);
    println!("  Disk usage:   {:.2} MB", component.disk_usage_mb);

    if let Some(ref version) = component.version {
        println!("  Version:      v{}", version);
    }

    if let Some(ref error) = component.last_error {
        println!("  Last error:   {}", error);
    }
}

async fn index_stats(project_id: i64, client: &Client, verbose: bool) -> Result<()> {
    if verbose {
        println!("Fetching index statistics...");
    }

    let response = client.index_stats(project_id).await?;

    if response.success {
        let stats = &response.statistics;
        println!("Index statistics:");
        println!();
        println!("  Files:               {}", stats.total_files);
        println!("  Entities:            {}", stats.total_entities);
        println!("  Relations:           {}", stats.total_relations);
        println!("  Vectors:             {}", stats.total_vectors);
        println!("  BM25 documents:      {}", stats.total_bm25_documents);
        println!(
            "  Elapsed:             {}",
            format_duration(response.elapsed_ms)
        );
    } else {
        print_error("Failed to get index statistics");
    }

    Ok(())
}

async fn clear_index(
    project_id: i64,
    client: &Client,
    vectors: bool,
    bm25: bool,
    relations: bool,
    cache: bool,
    verbose: bool,
) -> Result<()> {
    let request = ClearIndexRequest {
        project_id,
        vectors,
        bm25,
        relations,
        cache,
    };

    if verbose {
        println!("Clearing index...");
    }

    let response = client.clear_index(&request).await?;

    if response.success {
        print_success(&response.message);
        println!();
        println!("  Backend results:");
        for backend in &response.backends {
            let status = if backend.ok { "OK" } else { "FAIL" };
            println!("    - {}: [{}] {}", backend.backend, status, backend.detail);
        }
        println!(
            "  Elapsed:           {}",
            format_duration(response.elapsed_ms)
        );
    } else {
        print_error("Failed to clear index");
    }

    Ok(())
}

async fn delete_file(project_id: i64, client: &Client, path: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Deleting file: {}", path);
    }

    let response = client.delete_file(project_id, path).await?;

    if response.success {
        print_success(&response.message);
        println!();
        println!("  Vectors deleted:   {}", response.vectors_deleted);
        println!("  BM25 docs deleted: {}", response.bm25_documents_deleted);
        println!("  Relations deleted: {}", response.relations_deleted);
        println!(
            "  Elapsed:           {}",
            format_duration(response.elapsed_ms)
        );
    } else {
        print_error("Failed to delete file");
    }

    Ok(())
}

async fn delete_entity(project_id: i64, client: &Client, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Deleting entity: {}", id);
    }

    let entity_id: u64 = id
        .parse()
        .map_err(|_| anyhow::anyhow!("entity id must be a number: {id}"))?;
    let response = client.delete_entity(project_id, entity_id).await?;

    if response.success {
        print_success(&response.message);
        println!();
        println!("  Entity ID:         {}", response.entity_id);
        println!("  Vectors deleted:   {}", response.vectors_deleted);
        println!("  BM25 docs deleted: {}", response.bm25_documents_deleted);
        println!("  Relations deleted: {}", response.relations_deleted);
        println!(
            "  Elapsed:           {}",
            format_duration(response.elapsed_ms)
        );
    } else {
        print_error("Failed to delete entity");
    }

    Ok(())
}

async fn batch_delete(
    project_id: i64,
    client: &Client,
    files: &Option<String>,
    entities: &Option<String>,
    verbose: bool,
) -> Result<()> {
    let file_paths: Vec<String> = files
        .as_ref()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default();

    let entity_ids: Vec<u64> = entities
        .as_ref()
        .map(|s| {
            s.split(',')
                .filter_map(|id| id.trim().parse::<u64>().ok())
                .collect()
        })
        .unwrap_or_default();

    let request = BatchDeleteRequest {
        file_paths,
        entity_ids,
    };

    if verbose {
        println!("Executing batch delete...");
    }

    let response = client.batch_delete(project_id, &request).await?;

    if response.success {
        print_success("Batch delete completed");
        println!();
        println!("  Files deleted:    {}", response.files_deleted);
        println!("  Entities deleted: {}", response.entities_deleted);
        println!(
            "  Elapsed:          {}",
            format_duration(response.elapsed_ms)
        );

        if !response.errors.is_empty() {
            println!();
            println!("Errors:");
            for error in &response.errors {
                print_error(error);
            }
        }
    } else {
        print_error("Batch delete failed");
    }

    Ok(())
}

/// Storage commands
#[derive(Subcommand)]
pub enum StorageCommands {
    /// Get storage status
    Status,

    /// Get index statistics
    Stats {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Clear index
    Clear {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Clear vectors
        #[arg(long, default_value = "true")]
        vectors: bool,

        /// Clear BM25 index
        #[arg(long, default_value = "true")]
        bm25: bool,

        /// Clear relations
        #[arg(long, default_value = "true")]
        relations: bool,

        /// Clear cache
        #[arg(long, default_value = "true")]
        cache: bool,
    },

    /// Delete a file from index
    DeleteFile {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// File path
        path: String,
    },

    /// Delete an entity from index
    DeleteEntity {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Entity ID
        id: String,
    },

    /// Batch delete
    BatchDelete {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// File paths to delete (comma-separated)
        #[arg(long)]
        files: Option<String>,

        /// Entity IDs to delete (comma-separated)
        #[arg(long)]
        entities: Option<String>,
    },
}
