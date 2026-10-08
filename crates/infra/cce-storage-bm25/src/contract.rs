//! Fulltext retrieval contract implementations.
//!
//! The contract itself lives in `cce_storage_common::fulltext`; this module
//! implements it for the embedded Tantivy branch and the remote
//! search-service branch, and re-exports the contract surface so branch code
//! keeps a single import path.

pub use cce_storage_common::fulltext::{
    FulltextDocument, FulltextError, FulltextHit, FulltextSearchOptions, FulltextStorage,
    assert_fulltext_storage,
};

use crate::{Bm25Document, Bm25Error, Bm25SearchOptions, Bm25SearchResult};

#[cfg(feature = "local")]
use crate::Bm25Client;

#[cfg(feature = "local")]
impl FulltextStorage for Bm25Client {
    async fn batch_index(
        &self,
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
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        Bm25Client::delete_by_file_path_scoped(self, index_name, file_path, project_id).await
    }

    async fn delete_by_file_path_scoped_epoch(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        Bm25Client::delete_by_file_path_scoped_epoch(self, index_name, file_path, project_id, epoch)
            .await
    }

    async fn delete_by_project_epoch(
        &self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        Bm25Client::delete_by_project_epoch(self, index_name, project_id, epoch).await
    }

    async fn delete_all_project_docs(
        &self,
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

    async fn clear_index(&self, index_name: &str) -> Result<usize, Bm25Error> {
        Bm25Client::clear_index(self, index_name).await
    }

    async fn flush(&self) -> Result<(), Bm25Error> {
        Ok(())
    }

    fn is_enabled(&self) -> bool {
        Bm25Client::is_enabled(self)
    }
}

/// The remote search-service branch implements the same contract surface.
///
/// Method-for-method delegation keeps the two branches substitutable behind
/// the backend enum; behavior notes (refresh timing, phrase approximation)
/// live on [`crate::ElasticsearchClient`].
#[cfg(feature = "remote")]
impl FulltextStorage for crate::ElasticsearchClient {
    async fn batch_index(
        &self,
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
        &self,
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
        &self,
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
        &self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        crate::ElasticsearchClient::delete_by_project_epoch(self, index_name, project_id, epoch)
            .await
    }

    async fn delete_all_project_docs(
        &self,
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

    async fn clear_index(&self, index_name: &str) -> Result<usize, Bm25Error> {
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
}
