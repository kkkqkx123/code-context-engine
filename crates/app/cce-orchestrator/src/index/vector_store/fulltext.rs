use std::sync::Arc;

use cce_storage_bm25::{Bm25Client, ElasticsearchClient};
use cce_storage_common::{
    FulltextDocument, FulltextError, FulltextHit, FulltextSearchOptions, FulltextStorage,
};
use cce_types::StorageError;

/// Fulltext backend holding one concrete implementation.
///
/// Local is the embedded Tantivy branch; remote is the Elasticsearch branch.
/// Both branches implement the fulltext contract, so assembly selects them
/// from configuration instead of threading concrete client types through
/// every caller.
#[derive(Clone)]
pub enum FulltextStore {
    /// Embedded Tantivy index.
    Local(Arc<Bm25Client>),
    /// External Elasticsearch service.
    Remote(Arc<ElasticsearchClient>),
}

impl FulltextStore {
    /// Wrap a local BM25 client.
    pub fn local(client: Arc<Bm25Client>) -> Self {
        Self::Local(client)
    }

    /// Wrap a remote Elasticsearch client.
    pub fn remote(client: Arc<ElasticsearchClient>) -> Self {
        Self::Remote(client)
    }

    /// Whether this is the embedded backend.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Whether this is the remote backend.
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    /// Backend name for logging (`local` or `remote`).
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
            Self::Remote(_) => "remote",
        }
    }

    /// Borrow the local client when this is the embedded branch.
    pub fn as_local(&self) -> Option<&Arc<Bm25Client>> {
        match self {
            Self::Local(client) => Some(client),
            _ => None,
        }
    }

    /// Borrow the remote client when this is the remote branch.
    pub fn as_remote(&self) -> Option<&Arc<ElasticsearchClient>> {
        match self {
            Self::Remote(client) => Some(client),
            _ => None,
        }
    }

    /// Unwrap into the local handle when this is the embedded branch.
    pub fn into_local(self) -> Option<Arc<Bm25Client>> {
        match self {
            Self::Local(client) => Some(client),
            _ => None,
        }
    }

    /// Unwrap into the remote handle when this is the remote branch.
    pub fn into_remote(self) -> Option<Arc<ElasticsearchClient>> {
        match self {
            Self::Remote(client) => Some(client),
            _ => None,
        }
    }

    /// Build the store from the resolved database configuration.
    ///
    /// The local branch opens the embedded Tantivy index; the remote branch
    /// constructs an Elasticsearch client from the configured endpoint. Both
    /// branches share the fulltext contract, so callers stay branch-free.
    pub fn from_database_config(
        database: &cce_config::global::DatabaseConfig,
    ) -> Result<Self, StorageError> {
        match database.fulltext_backend {
            cce_config::modules::FulltextBackend::Local => {
                let client = Bm25Client::new(database.bm25.clone());
                Ok(Self::Local(Arc::new(client)))
            }
            cce_config::modules::FulltextBackend::Remote => {
                let client = ElasticsearchClient::from_database_config(database).map_err(|e| {
                    StorageError::query(format!("failed to create elasticsearch client: {e}"))
                })?;
                Ok(Self::Remote(Arc::new(client)))
            }
        }
    }
}

impl std::fmt::Debug for FulltextStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => write!(f, "FulltextStore::Local(..)"),
            Self::Remote(_) => write!(f, "FulltextStore::Remote(..)"),
        }
    }
}

impl FulltextStorage for FulltextStore {
    async fn batch_index(
        &self,
        index_name: &str,
        documents: &[FulltextDocument],
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::batch_index(client.as_ref(), index_name, documents).await
            }
            Self::Remote(client) => {
                FulltextStorage::batch_index(client.as_ref(), index_name, documents).await
            }
        }
    }

    async fn search(
        &self,
        query: &str,
        options: &FulltextSearchOptions,
    ) -> Result<Vec<FulltextHit>, FulltextError> {
        match self {
            Self::Local(client) => FulltextStorage::search(client.as_ref(), query, options).await,
            Self::Remote(client) => FulltextStorage::search(client.as_ref(), query, options).await,
        }
    }

    async fn delete_by_file_path_scoped(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::delete_by_file_path_scoped(
                    client.as_ref(),
                    index_name,
                    file_path,
                    project_id,
                )
                .await
            }
            Self::Remote(client) => {
                FulltextStorage::delete_by_file_path_scoped(
                    client.as_ref(),
                    index_name,
                    file_path,
                    project_id,
                )
                .await
            }
        }
    }

    async fn delete_by_file_path_scoped_epoch(
        &self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::delete_by_file_path_scoped_epoch(
                    client.as_ref(),
                    index_name,
                    file_path,
                    project_id,
                    epoch,
                )
                .await
            }
            Self::Remote(client) => {
                FulltextStorage::delete_by_file_path_scoped_epoch(
                    client.as_ref(),
                    index_name,
                    file_path,
                    project_id,
                    epoch,
                )
                .await
            }
        }
    }

    async fn delete_by_file_paths_scoped_epoch(
        &self,
        index_name: &str,
        file_paths: &[&str],
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::delete_by_file_paths_scoped_epoch(
                    client.as_ref(),
                    index_name,
                    file_paths,
                    project_id,
                    epoch,
                )
                .await
            }
            Self::Remote(client) => {
                FulltextStorage::delete_by_file_paths_scoped_epoch(
                    client.as_ref(),
                    index_name,
                    file_paths,
                    project_id,
                    epoch,
                )
                .await
            }
        }
    }

    async fn delete_by_project_epoch(
        &self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::delete_by_project_epoch(
                    client.as_ref(),
                    index_name,
                    project_id,
                    epoch,
                )
                .await
            }
            Self::Remote(client) => {
                FulltextStorage::delete_by_project_epoch(
                    client.as_ref(),
                    index_name,
                    project_id,
                    epoch,
                )
                .await
            }
        }
    }

    async fn delete_all_project_docs(
        &self,
        index_name: &str,
        project_id: i64,
    ) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::delete_all_project_docs(client.as_ref(), index_name, project_id)
                    .await
            }
            Self::Remote(client) => {
                FulltextStorage::delete_all_project_docs(client.as_ref(), index_name, project_id)
                    .await
            }
        }
    }

    async fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<FulltextDocument>, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::snapshot_documents(client.as_ref(), project_id, epoch).await
            }
            Self::Remote(client) => {
                FulltextStorage::snapshot_documents(client.as_ref(), project_id, epoch).await
            }
        }
    }

    async fn document_count(&self) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => FulltextStorage::document_count(client.as_ref()).await,
            Self::Remote(client) => FulltextStorage::document_count(client.as_ref()).await,
        }
    }

    async fn document_count_by_project(&self, project_id: i64) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::document_count_by_project(client.as_ref(), project_id).await
            }
            Self::Remote(client) => {
                FulltextStorage::document_count_by_project(client.as_ref(), project_id).await
            }
        }
    }

    async fn epochs_by_project(&self, project_id: i64) -> Result<Vec<i64>, FulltextError> {
        match self {
            Self::Local(client) => {
                FulltextStorage::epochs_by_project(client.as_ref(), project_id).await
            }
            Self::Remote(client) => {
                FulltextStorage::epochs_by_project(client.as_ref(), project_id).await
            }
        }
    }

    async fn clear_index(&self, index_name: &str) -> Result<usize, FulltextError> {
        match self {
            Self::Local(client) => FulltextStorage::clear_index(client.as_ref(), index_name).await,
            Self::Remote(client) => FulltextStorage::clear_index(client.as_ref(), index_name).await,
        }
    }

    async fn flush(&self) -> Result<(), FulltextError> {
        match self {
            Self::Local(client) => FulltextStorage::flush(client.as_ref()).await,
            Self::Remote(client) => FulltextStorage::flush(client.as_ref()).await,
        }
    }

    fn is_enabled(&self) -> bool {
        match self {
            Self::Local(client) => FulltextStorage::is_enabled(client.as_ref()),
            Self::Remote(client) => FulltextStorage::is_enabled(client.as_ref()),
        }
    }

    fn backend_name(&self) -> &'static str {
        match self {
            Self::Local(client) => FulltextStorage::backend_name(client.as_ref()),
            Self::Remote(client) => FulltextStorage::backend_name(client.as_ref()),
        }
    }
}

/// Backend-neutral fulltext diagnostics snapshot.
///
/// Mirrors [`VectorDiagnostics`]: the local branch probes its document
/// count, the remote branch probes the search service over HTTP. Probe
/// failures fold into the snapshot so handlers stay branch-free.
#[derive(Debug, Clone)]
pub struct FulltextDiagnostics {
    /// Whether the backend is reachable and serving.
    pub reachable: bool,
    /// Backend version, when the backend exposes one.
    pub version: Option<String>,
    /// Whether the index exists and answers reads.
    pub index_exists: bool,
    /// Number of documents in the index.
    pub documents_count: u64,
    /// Human-readable failure detail, when unhealthy.
    pub error: Option<String>,
}

impl FulltextStore {
    /// Configured index name of the active branch.
    ///
    /// Both branches validate the per-call index name against their own
    /// configuration, so write and delete paths must use this name instead
    /// of a hardcoded literal.
    pub fn configured_index_name(&self) -> String {
        match self {
            Self::Local(client) => client.config().index_name.clone(),
            Self::Remote(client) => client.config().index_name.clone(),
        }
    }

    /// Backend-neutral diagnostics snapshot for status endpoints.
    ///
    /// Never fails: probe errors are folded into the snapshot so handlers
    /// stay branch-free.
    pub async fn diagnose_summary(&self) -> FulltextDiagnostics {
        match FulltextStorage::document_count(self).await {
            Ok(count) => FulltextDiagnostics {
                reachable: FulltextStorage::is_enabled(self),
                version: None,
                index_exists: true,
                documents_count: count as u64,
                error: if FulltextStorage::is_enabled(self) {
                    None
                } else {
                    Some("Fulltext backend is disabled".to_string())
                },
            },
            Err(e) => FulltextDiagnostics {
                reachable: false,
                version: None,
                index_exists: false,
                documents_count: 0,
                error: Some(format!("Diagnostic failed: {e}")),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fulltext_store_satisfies_fulltext_contract() {
        cce_storage_common::assert_fulltext_storage::<FulltextStore>();
        let database = cce_config::global::DatabaseConfig::default();
        let store =
            FulltextStore::from_database_config(&database).expect("local branch must build");
        assert_eq!(FulltextStorage::backend_name(&store), "local");
    }

    #[tokio::test]
    async fn fulltext_diagnose_folds_probe_failure() {
        let database = cce_config::global::DatabaseConfig::default();
        let store =
            FulltextStore::from_database_config(&database).expect("local branch must build");
        let diag = store.diagnose_summary().await;
        assert!(!diag.reachable);
        assert!(!diag.index_exists);
        assert_eq!(diag.documents_count, 0);
        assert!(diag.error.is_some());
    }

    #[test]
    fn fulltext_store_remote_branch_builds_when_configured() {
        let mut database = cce_config::global::DatabaseConfig {
            fulltext_backend: cce_config::modules::FulltextBackend::Remote,
            ..cce_config::global::DatabaseConfig::default()
        };
        assert!(FulltextStore::from_database_config(&database).is_err());
        database.fulltext_remote.url = Some("http://localhost:9200".to_string());
        database.fulltext_remote.index_name = Some("code_index".to_string());
        let store = FulltextStore::from_database_config(&database)
            .expect("remote branch must build when configured");
        assert_eq!(FulltextStorage::backend_name(&store), "remote");
    }
}
