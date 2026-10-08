//! Fulltext retrieval contract (backend-neutral).
//!
//! Groups the operations that the embedded Tantivy branch serves today:
//! batch writes, scoped deletes, keyword retrieval, snapshot readback,
//! generation enumeration, per-project counts, and clear/rebuild. Batch
//! writes are idempotent per document id (delete-term then add), so
//! replaying a batch after a transient failure is safe. The body keyword
//! field is indexed but not stored, so it never participates in readback.
//!
//! Transaction boundary: every named operation is atomic on its own scope.
//! Batch writes commit once per batch; a transient failure retries the whole
//! batch, never a partial prefix.

use crate::{Bm25Client, Bm25Document, Bm25Error, Bm25SearchOptions, Bm25SearchResult};

/// Backend-neutral fulltext document (same shape as the local branch).
pub type FulltextDocument = Bm25Document;

/// Backend-neutral retrieval options (same shape as the local branch).
pub type FulltextSearchOptions = Bm25SearchOptions;

/// Backend-neutral retrieval hit (same shape as the local branch).
pub type FulltextHit = Bm25SearchResult;

/// Backend-neutral fulltext error (same classification as the local branch).
pub type FulltextError = Bm25Error;

/// Fulltext storage contract implemented by the local Tantivy branch and
/// the remote search-service branch.
///
/// Callers hold the backend enum and call through this contract; no caller
/// touches a concrete client or index manager.
pub trait FulltextStorage: Clone + Send + Sync + 'static {
    /// Index a batch of documents into the configured index.
    fn batch_index(
        &mut self,
        index_name: &str,
        documents: &[Bm25Document],
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send;

    /// Run a keyword retrieval with project and generation filters.
    ///
    /// The output carries only the stored readback fields (document and
    /// chunk ids, title, file path, alignment ids); the indexed-only body
    /// and keyword fields never participate in readback on any branch.
    fn search(
        &self,
        query: &str,
        options: &Bm25SearchOptions,
    ) -> impl Future<Output = Result<Vec<Bm25SearchResult>, Bm25Error>> + Send;

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

    /// Make recent writes visible to counts and snapshot readbacks.
    ///
    /// The local branch reloads its reader per batch, so this is a no-op
    /// there; the remote branch refreshes the index. Call it after the last
    /// batch before a generation is marked ready or activated.
    fn flush(&self) -> impl Future<Output = Result<(), Bm25Error>> + Send;

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

    fn search(
        &self,
        query: &str,
        options: &Bm25SearchOptions,
    ) -> impl Future<Output = Result<Vec<Bm25SearchResult>, Bm25Error>> + Send {
        Bm25Client::search(self, query, options)
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

    async fn flush(&self) -> Result<(), Bm25Error> {
        Ok(())
    }

    fn is_enabled(&self) -> bool {
        Bm25Client::is_enabled(self)
    }
}

/// Compile-time assertion that a type implements the contract.
pub fn assert_fulltext_storage<T: FulltextStorage>() {}

/// The remote search-service branch implements the same contract surface.
///
/// Method-for-method delegation keeps the two branches substitutable behind
/// the backend enum; behavior notes (refresh timing, phrase approximation)
/// live on [`crate::ElasticsearchClient`].
impl FulltextStorage for crate::ElasticsearchClient {
    fn batch_index(
        &mut self,
        index_name: &str,
        documents: &[Bm25Document],
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        crate::ElasticsearchClient::batch_index(self, index_name, documents)
    }

    fn search(
        &self,
        query: &str,
        options: &Bm25SearchOptions,
    ) -> impl Future<Output = Result<Vec<Bm25SearchResult>, Bm25Error>> + Send {
        crate::ElasticsearchClient::search(self, query, options)
    }

    fn delete_by_file_path_scoped(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        crate::ElasticsearchClient::delete_by_file_path_scoped(
            self, index_name, file_path, project_id,
        )
    }

    fn delete_by_file_path_scoped_epoch(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        crate::ElasticsearchClient::delete_by_file_path_scoped_epoch(
            self, index_name, file_path, project_id, epoch,
        )
    }

    fn delete_by_project_epoch(
        &mut self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        crate::ElasticsearchClient::delete_by_project_epoch(self, index_name, project_id, epoch)
    }

    fn delete_all_project_docs(
        &mut self,
        index_name: &str,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        crate::ElasticsearchClient::delete_all_project_docs(self, index_name, project_id)
    }

    fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> impl Future<Output = Result<Vec<Bm25Document>, Bm25Error>> + Send {
        crate::ElasticsearchClient::snapshot_documents(self, project_id, epoch)
    }

    fn document_count(&self) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        crate::ElasticsearchClient::document_count(self)
    }

    fn document_count_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        crate::ElasticsearchClient::document_count_by_project(self, project_id)
    }

    fn epochs_by_project(
        &self,
        project_id: i64,
    ) -> impl Future<Output = Result<Vec<i64>, Bm25Error>> + Send {
        crate::ElasticsearchClient::epochs_by_project(self, project_id)
    }

    fn clear_index(
        &mut self,
        index_name: &str,
    ) -> impl Future<Output = Result<usize, Bm25Error>> + Send {
        crate::ElasticsearchClient::clear_index(self, index_name)
    }

    fn is_enabled(&self) -> bool {
        crate::ElasticsearchClient::is_enabled(self)
    }

    fn backend_name(&self) -> &'static str {
        crate::ElasticsearchClient::backend_name(self)
    }

    fn flush(&self) -> impl Future<Output = Result<(), Bm25Error>> + Send {
        crate::ElasticsearchClient::flush(self)
    }
}

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

    #[test]
    fn remote_client_satisfies_fulltext_contract() {
        use crate::ElasticsearchConfig;
        assert_fulltext_storage::<crate::ElasticsearchClient>();
        let config = ElasticsearchConfig {
            base_url: "http://localhost:9200".to_string(),
            api_key: None,
            username: None,
            password: None,
            index_name: "code_index".to_string(),
            bulk_size: 500,
            request_timeout: std::time::Duration::from_millis(1000),
            refresh_interval: None,
            k1: 1.8,
            b: 0.6,
        };
        let client =
            crate::ElasticsearchClient::new(config).expect("remote client builds without I/O");
        assert_eq!(FulltextStorage::backend_name(&client), "remote");
        assert!(FulltextStorage::is_enabled(&client));
    }

    #[test]
    fn neutral_aliases_match_local_shapes() {
        fn assert_document(doc: FulltextDocument) -> Bm25Document {
            doc
        }
        fn assert_hit(hit: FulltextHit) -> Bm25SearchResult {
            hit
        }
        let doc = assert_document(Bm25Document::new("d"));
        assert_eq!(doc.document_id, "d");
        let hit = assert_hit(Bm25SearchResult {
            document_id: "d".to_string(),
            score: 1.0,
            fields: std::collections::HashMap::new(),
        });
        assert_eq!(hit.score, 1.0);
    }
}
