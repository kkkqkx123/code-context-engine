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

#[cfg(feature = "local")]
use crate::Bm25Client;
use crate::{Bm25Document, Bm25Error, Bm25SearchOptions, Bm25SearchResult};

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
///
/// Frozen semantics: batch writes are idempotent per document id; deletes
/// report the number of removed documents; `clear_index` removes all
/// documents and reports how many were removed; counts and snapshots only
/// observe flushed writes, so callers must invoke `flush` after the last
/// batch before marking a generation ready. File paths are compared after
/// normalization on every branch.
pub trait FulltextStorage: Clone + Send + Sync + 'static {
    /// Index a batch of documents into the configured index.
    async fn batch_index(
        &mut self,
        index_name: &str,
        documents: &[Bm25Document],
    ) -> Result<usize, Bm25Error>;

    /// Run a keyword retrieval with project and generation filters.
    ///
    /// The output carries only the stored readback fields (document and
    /// chunk ids, title, file path, alignment ids); the indexed-only body
    /// and keyword fields never participate in readback on any branch.
    async fn search(
        &self,
        query: &str,
        options: &Bm25SearchOptions,
    ) -> Result<Vec<Bm25SearchResult>, Bm25Error>;

    /// Delete documents for one file within one project.
    async fn delete_by_file_path_scoped(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error>;

    /// Delete documents for one file in one data epoch.
    async fn delete_by_file_path_scoped_epoch(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error>;

    /// Delete all documents for one project and data epoch.
    async fn delete_by_project_epoch(
        &mut self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error>;

    /// Delete all documents for a project.
    async fn delete_all_project_docs(
        &mut self,
        index_name: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error>;

    /// Read back the stored fields needed to copy an epoch into a
    /// candidate generation.
    async fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<Bm25Document>, Bm25Error>;

    /// Count all documents in the index.
    async fn document_count(&self) -> Result<usize, Bm25Error>;

    /// Count documents belonging to one project.
    async fn document_count_by_project(&self, project_id: i64) -> Result<usize, Bm25Error>;

    /// List data epochs currently present for a project.
    async fn epochs_by_project(&self, project_id: i64) -> Result<Vec<i64>, Bm25Error>;

    /// Recreate the index from scratch (generation rebuild/cleanup).
    async fn clear_index(&mut self, index_name: &str) -> Result<usize, Bm25Error>;

    /// Make recent writes visible to counts and snapshot readbacks.
    ///
    /// The local branch reloads its reader per batch, so this is a no-op
    /// there; the remote branch refreshes the index. Call it after the last
    /// batch before a generation is marked ready or activated.
    async fn flush(&self) -> Result<(), Bm25Error>;

    /// Whether the branch is enabled and connected.
    fn is_enabled(&self) -> bool;

    /// Backend name for logging (`local` for the embedded branch).
    fn backend_name(&self) -> &'static str {
        "local"
    }
}

#[cfg(feature = "local")]
impl FulltextStorage for Bm25Client {
    async fn batch_index(
        &mut self,
        index_name: &str,
        documents: &[Bm25Document],
    ) -> Result<usize, Bm25Error> {
        Bm25Client::batch_index(self, index_name, documents).await
    }

    async fn search(
        &self,
        query: &str,
        options: &Bm25SearchOptions,
    ) -> Result<Vec<Bm25SearchResult>, Bm25Error> {
        Bm25Client::search(self, query, options).await
    }

    async fn delete_by_file_path_scoped(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        Bm25Client::delete_by_file_path_scoped(self, index_name, file_path, project_id).await
    }

    async fn delete_by_file_path_scoped_epoch(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        Bm25Client::delete_by_file_path_scoped_epoch(self, index_name, file_path, project_id, epoch)
            .await
    }

    async fn delete_by_project_epoch(
        &mut self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        Bm25Client::delete_by_project_epoch(self, index_name, project_id, epoch).await
    }

    async fn delete_all_project_docs(
        &mut self,
        index_name: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        Bm25Client::delete_all_project_docs(self, index_name, project_id).await
    }

    async fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<Bm25Document>, Bm25Error> {
        Bm25Client::snapshot_documents(self, project_id, epoch).await
    }

    async fn document_count(&self) -> Result<usize, Bm25Error> {
        Bm25Client::document_count(self).await
    }

    async fn document_count_by_project(&self, project_id: i64) -> Result<usize, Bm25Error> {
        Bm25Client::document_count_by_project(self, project_id).await
    }

    async fn epochs_by_project(&self, project_id: i64) -> Result<Vec<i64>, Bm25Error> {
        Bm25Client::epochs_by_project(self, project_id).await
    }

    async fn clear_index(&mut self, index_name: &str) -> Result<usize, Bm25Error> {
        Bm25Client::clear_index(self, index_name).await
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
#[cfg(feature = "remote")]
impl FulltextStorage for crate::ElasticsearchClient {
    async fn batch_index(
        &mut self,
        index_name: &str,
        documents: &[Bm25Document],
    ) -> Result<usize, Bm25Error> {
        crate::ElasticsearchClient::batch_index(self, index_name, documents).await
    }

    async fn search(
        &self,
        query: &str,
        options: &Bm25SearchOptions,
    ) -> Result<Vec<Bm25SearchResult>, Bm25Error> {
        crate::ElasticsearchClient::search(self, query, options).await
    }

    async fn delete_by_file_path_scoped(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        crate::ElasticsearchClient::delete_by_file_path_scoped(
            self, index_name, file_path, project_id,
        )
        .await
    }

    async fn delete_by_file_path_scoped_epoch(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        crate::ElasticsearchClient::delete_by_file_path_scoped_epoch(
            self, index_name, file_path, project_id, epoch,
        )
        .await
    }

    async fn delete_by_project_epoch(
        &mut self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        crate::ElasticsearchClient::delete_by_project_epoch(self, index_name, project_id, epoch)
            .await
    }

    async fn delete_all_project_docs(
        &mut self,
        index_name: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        crate::ElasticsearchClient::delete_all_project_docs(self, index_name, project_id).await
    }

    async fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<Bm25Document>, Bm25Error> {
        crate::ElasticsearchClient::snapshot_documents(self, project_id, epoch).await
    }

    async fn document_count(&self) -> Result<usize, Bm25Error> {
        crate::ElasticsearchClient::document_count(self).await
    }

    async fn document_count_by_project(&self, project_id: i64) -> Result<usize, Bm25Error> {
        crate::ElasticsearchClient::document_count_by_project(self, project_id).await
    }

    async fn epochs_by_project(&self, project_id: i64) -> Result<Vec<i64>, Bm25Error> {
        crate::ElasticsearchClient::epochs_by_project(self, project_id).await
    }

    async fn clear_index(&mut self, index_name: &str) -> Result<usize, Bm25Error> {
        crate::ElasticsearchClient::clear_index(self, index_name).await
    }

    fn is_enabled(&self) -> bool {
        crate::ElasticsearchClient::is_enabled(self)
    }

    fn backend_name(&self) -> &'static str {
        crate::ElasticsearchClient::backend_name(self)
    }

    async fn flush(&self) -> Result<(), Bm25Error> {
        crate::ElasticsearchClient::flush(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "local")]
    fn local_client_satisfies_fulltext_contract() {
        assert_fulltext_storage::<Bm25Client>();
        let client = Bm25Client::default_client();
        assert_eq!(FulltextStorage::backend_name(&client), "local");
        assert!(!FulltextStorage::is_enabled(&client));
    }

    #[test]
    #[cfg(feature = "remote")]
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
