//! Fulltext retrieval contract (phase 1 skeleton).
//!
//! Groups the operations that today live on the embedded Tantivy client:
//! batch writes, scoped deletes, keyword retrieval inputs, snapshot
//! readback, generation enumeration, per-project counts, and clear/rebuild.
//! Batch writes are idempotent per document id (delete-term then add), so
//! replaying a batch after a transient failure is safe. The body keyword
//! field is indexed but not stored, so it never participates in readback.

use crate::{Bm25Client, Bm25Document, Bm25Error};

/// Fulltext storage contract implemented by the local Tantivy branch.
///
/// Phase 1 keeps the existing client as the internal implementation;
/// callers switch to this contract plus the backend enum in phase 2, and a
/// future search-service branch implements the same surface in phase 3.
pub trait FulltextStorage: Clone + Send + Sync + 'static {
    /// Index a batch of documents into the configured index.
    fn batch_index(
        &mut self,
        index_name: &str,
        documents: &[Bm25Document],
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send;

    /// Delete documents for one file within one project.
    fn delete_by_file_path_scoped(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send;

    /// Delete documents for one file in one data epoch.
    fn delete_by_file_path_scoped_epoch(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send;

    /// Delete all documents for one project and data epoch.
    fn delete_by_project_epoch(
        &mut self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send;

    /// Delete all documents for a project.
    fn delete_all_project_docs(
        &mut self,
        index_name: &str,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send;

    /// Read back the stored fields needed to copy an epoch into a
    /// candidate generation.
    fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Vec<Bm25Document>, Bm25Error>> + Send;

    /// Count all documents in the index.
    fn document_count(&self) -> impl Future<Output = Result<usize, Bm25Error>> + Send;

    /// Count documents belonging to one project.
    fn document_count_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send;

    /// List data epochs currently present for a project.
    fn epochs_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<Vec<i64>, Bm25Error>> + Send;

    /// Recreate the index from scratch (generation rebuild/cleanup).
    fn clear_index(
        &mut self,
        index_name: &str,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send;

    /// Whether the branch is enabled and connected.
    fn is_enabled(&self) -> bool;

    /// Backend name for logging (`local` for the embedded branch).
    fn backend_name(&self) -> &'static str {
        "local"
    }
}

impl FulltextStorage for Bm25Client {
    fn batch_index(
        &mut self,
        index_name: &str,
        documents: &[Bm25Document],
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        Bm25Client::batch_index(self, index_name, documents)
    }

    fn delete_by_file_path_scoped(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        Bm25Client::delete_by_file_path_scoped(self, index_name, file_path, project_id)
    }

    fn delete_by_file_path_scoped_epoch(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        Bm25Client::delete_by_file_path_scoped_epoch(self, index_name, file_path, project_id, epoch)
    }

    fn delete_by_project_epoch(
        &mut self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        Bm25Client::delete_by_project_epoch(self, index_name, project_id, epoch)
    }

    fn delete_all_project_docs(
        &mut self,
        index_name: &str,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        Bm25Client::delete_all_project_docs(self, index_name, project_id)
    }

    fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Vec<Bm25Document>, Bm25Error>> + Send {
        Bm25Client::snapshot_documents(self, project_id, epoch)
    }

    fn document_count(&self) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        Bm25Client::document_count(self)
    }

    fn document_count_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        Bm25Client::document_count_by_project(self, project_id)
    }

    fn epochs_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<Vec<i64>, Bm25Error>> + Send {
        Bm25Client::epochs_by_project(self, project_id)
    }

    fn clear_index(
        &mut self,
        index_name: &str,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        Bm25Client::clear_index(self, index_name)
    }

    fn is_enabled(&self) -> bool {
        Bm25Client::is_enabled(self)
    }
}

/// Compile-time assertion that a type implements the contract.
pub fn assert_fulltext_storage<T: FulltextStorage>() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_client_satisfies_fulltext_contract() {
        assert_fulltext_storage::<Bm25Client>();
        let client = Bm25Client::default_client();
        assert_eq!(FulltextStorage::backend_name(&client), "local");
        assert!(!FulltextStorage::is_enabled(&client));
    }
}
