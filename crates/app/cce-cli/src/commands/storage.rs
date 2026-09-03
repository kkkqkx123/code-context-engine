//! Storage command handlers

use anyhow::Result;

use crate::cli::StorageCommands;
use crate::client::ApiClient;
use crate::output::{format_duration, print_error, print_success};
use cce_api::models::{
    BatchDeleteRequest, ClearIndexRequest, ClearIndexResponse, DeleteEntityResponse,
    DeleteFileResponse, IndexStatsResponse, StorageStatusResponse,
};

pub async fn execute(cmd: &StorageCommands, server: &str, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;

    match cmd {
        StorageCommands::Status => storage_status(&client, verbose).await,
        StorageCommands::Stats { project_id } => index_stats(*project_id, &client, verbose).await,
        StorageCommands::Clear {
            project_id,
            vectors,
            bm25,
            relations,
            cache,
        } => {
            clear_index(
                *project_id,
                &client,
                *vectors,
                *bm25,
                *relations,
                *cache,
                verbose,
            )
            .await
        }
        StorageCommands::DeleteFile { project_id, path } => {
            delete_file(*project_id, &client, path, verbose).await
        }
        StorageCommands::DeleteEntity { project_id, id } => {
            delete_entity(*project_id, &client, id, verbose).await
        }
        StorageCommands::BatchDelete {
            project_id,
            files,
            entities,
        } => batch_delete(*project_id, &client, files, entities, verbose).await,
    }
}

async fn storage_status(client: &ApiClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("Fetching storage status...");
    }

    let response: StorageStatusResponse = client.get("/api/storage/status").await?;

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

async fn index_stats(project_id: i64, client: &ApiClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("Fetching index statistics...");
    }

    let url = format!("/api/index/stats?project_id={}", project_id);
    let response: IndexStatsResponse = client.get(&url).await?;

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
    client: &ApiClient,
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

    let url = format!("/api/index?project_id={}", project_id);
    let response: ClearIndexResponse = client.delete_with_body(&url, &request).await?;

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

async fn delete_file(project_id: i64, client: &ApiClient, path: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Deleting file: {}", path);
    }

    let url = format!(
        "/api/index/file/{}?project_id={}",
        urlencoding::encode(path),
        project_id
    );
    let response: DeleteFileResponse = client.delete(&url).await?;

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

async fn delete_entity(project_id: i64, client: &ApiClient, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Deleting entity: {}", id);
    }

    let url = format!("/api/index/entity/{}?project_id={}", id, project_id);
    let response: DeleteEntityResponse = client.delete(&url).await?;

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
    client: &ApiClient,
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

    let url = format!("/api/index/batch?project_id={}", project_id);
    let response: cce_api::models::BatchDeleteResponse =
        client.delete_with_body(&url, &request).await?;

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
