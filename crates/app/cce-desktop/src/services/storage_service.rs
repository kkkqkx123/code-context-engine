//! Storage inspection and removal commands.
//!
//! Removal is destructive, so the panel asks for confirmation before calling
//! these; the service itself performs exactly the request it is given and
//! never widens the scope.

use cce_api::models::{
    BatchDeleteRequest, BatchDeleteResponse, ClearIndexRequest, ClearIndexResponse,
    DeleteEntityResponse, DeleteFileResponse, IndexStatsResponse, StorageStatusResponse,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

/// Which storage components a clear request drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearScope {
    pub vectors: bool,
    pub bm25: bool,
    pub relations: bool,
    pub cache: bool,
}

impl ClearScope {
    /// Nothing selected: the request would be a no-op, so the panel refuses it.
    pub fn is_empty(self) -> bool {
        !(self.vectors || self.bm25 || self.relations || self.cache)
    }
}

pub fn status(cx: &App, client: Client) -> Task<Result<StorageStatusResponse, ClientError>> {
    runtime::spawn(cx, async move { client.storage_status().await })
}

pub fn stats(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<IndexStatsResponse, ClientError>> {
    runtime::spawn(cx, async move { client.index_stats(project_id).await })
}

pub fn clear(
    cx: &App,
    client: Client,
    project_id: i64,
    scope: ClearScope,
) -> Task<Result<ClearIndexResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = ClearIndexRequest {
            project_id,
            vectors: scope.vectors,
            bm25: scope.bm25,
            relations: scope.relations,
            cache: scope.cache,
        };
        client.clear_index(&request).await
    })
}

pub fn delete_file(
    cx: &App,
    client: Client,
    project_id: i64,
    file_path: String,
) -> Task<Result<DeleteFileResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.delete_file(project_id, &file_path).await
    })
}

pub fn delete_entity(
    cx: &App,
    client: Client,
    project_id: i64,
    entity_id: u64,
) -> Task<Result<DeleteEntityResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.delete_entity(project_id, entity_id).await
    })
}

pub fn batch_delete(
    cx: &App,
    client: Client,
    project_id: i64,
    files: Vec<String>,
    entities: Vec<u64>,
) -> Task<Result<BatchDeleteResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = BatchDeleteRequest {
            file_paths: files,
            entity_ids: entities,
        };
        client.batch_delete(project_id, &request).await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scope_is_rejected() {
        let scope = ClearScope {
            vectors: false,
            bm25: false,
            relations: false,
            cache: false,
        };
        assert!(scope.is_empty());
        let scope = ClearScope {
            cache: true,
            ..scope
        };
        assert!(!scope.is_empty());
    }
}
