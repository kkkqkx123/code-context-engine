//! BM25 client implementation using embedded Tantivy index

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tantivy::collector::DocSetCollector;
use tantivy::query::AllQuery;
use tantivy::schema::Value;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::metrics::Bm25Metrics;
use crate::{Bm25Config, Bm25Document, Bm25Error};
use crate::{
    IndexManager, IndexSchema, batch_add_documents, delete_document,
    delete_documents_by_file_path_and_project, delete_documents_by_file_path_project_epoch,
    delete_documents_by_project, delete_documents_by_project_epoch,
};

/// BM25 storage client
pub struct Bm25Client {
    config: Bm25Config,
    index_manager: Option<Arc<RwLock<IndexManager>>>,
    schema: IndexSchema,
    metrics: Option<Arc<dyn Bm25Metrics>>,
}

impl Bm25Client {
    /// Create a new BM25 client
    pub fn new(config: Bm25Config) -> Self {
        Self {
            config,
            index_manager: None,
            schema: IndexSchema::new(),
            metrics: None,
        }
    }

    /// Create a new BM25 client with default config
    pub fn default_client() -> Self {
        Self::new(Bm25Config::default())
    }

    /// Set metrics collector for this client
    pub fn with_metrics(mut self, metrics: Arc<dyn Bm25Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Get the metrics collector
    pub fn metrics(&self) -> Option<&Arc<dyn Bm25Metrics>> {
        self.metrics.as_ref()
    }

    /// Initialize the BM25 index
    pub async fn connect(&mut self) -> Result<(), Bm25Error> {
        if !self.config.enabled {
            debug!("BM25 is disabled, skipping connection");
            return Ok(());
        }

        debug!("Connecting to BM25 index");

        let index_root = match &self.config.index_path {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from("./data/bm25"),
        };
        let index_path = IndexManager::versioned_path(&index_root);

        let manager_config = self.config.index_manager.clone();
        let algorithm = self.config.algorithm.clone();

        let is_existing_index = index_path.join("meta.json").exists();
        let is_compatible = is_existing_index && IndexManager::is_compatible(&index_path);

        let manager = if is_compatible {
            debug!(path = ?index_path, "Opening existing BM25 index");
            IndexManager::open_with_config(&index_path, manager_config)?
        } else {
            if is_existing_index {
                info!(
                    path = ?index_path,
                    "BM25 index format version mismatch, rebuilding index"
                );
                std::fs::remove_dir_all(&index_path)?;
            } else {
                debug!(path = ?index_path, "Creating new BM25 index");
            }
            IndexManager::create_with_config(&index_path, manager_config, algorithm)?
        };

        self.index_manager = Some(Arc::new(RwLock::new(manager)));

        info!(
            path = ?index_path,
            config = ?self.config.index_manager,
            "BM25 index initialized"
        );
        Ok(())
    }

    /// Check if BM25 is enabled and connected
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.index_manager.is_some()
    }

    /// Check if client is connected
    pub fn is_connected(&self) -> bool {
        self.index_manager.is_some()
    }

    /// Get the config
    pub fn config(&self) -> &Bm25Config {
        &self.config
    }

    /// Get the index manager for retrieval operations
    pub fn index_manager(&self) -> Option<&Arc<RwLock<IndexManager>>> {
        self.index_manager.as_ref()
    }

    /// Get the index schema
    pub fn schema(&self) -> &IndexSchema {
        &self.schema
    }

    /// Validate index name matches configuration
    fn validate_index_name(&self, index_name: &str) -> Result<(), Bm25Error> {
        if index_name != self.config.index_name {
            return Err(Bm25Error::Index(format!(
                "Index name '{}' does not match configured name '{}'",
                index_name, self.config.index_name
            )));
        }
        Ok(())
    }

    /// Batch index documents
    pub async fn batch_index(
        &mut self,
        index_name: &str,
        documents: &[Bm25Document],
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        if documents.is_empty() {
            return Ok(0);
        }

        let start_time = Instant::now();
        let result = self.batch_index_inner(index_name, documents).await;
        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;

        if let Some(metrics) = &self.metrics {
            let count = result.as_ref().map_or(0, |&c| c);
            metrics.record_index(elapsed, count, result.is_ok());
        }

        result
    }

    async fn batch_index_inner(
        &mut self,
        _index_name: &str,
        documents: &[Bm25Document],
    ) -> Result<usize, Bm25Error> {
        if documents.is_empty() {
            return Ok(0);
        }

        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;

        let manager_guard = manager.read().await;
        let schema = manager_guard.schema();

        let docs: Vec<(String, HashMap<String, String>)> = documents
            .iter()
            .map(|d| (d.document_id.clone(), d.fields.clone()))
            .collect();

        let count = batch_add_documents(&manager_guard, schema, docs)?;
        manager_guard.reload_reader()?;

        Ok(count)
    }

    /// Delete a document by ID
    pub async fn delete(&mut self, index_name: &str, document_id: &str) -> Result<(), Bm25Error> {
        self.validate_index_name(index_name)?;
        let start_time = Instant::now();
        let result = self.delete_inner(index_name, document_id).await;
        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;

        if let Some(metrics) = &self.metrics {
            metrics.record_delete(elapsed, 1, result.is_ok());
        }

        result
    }

    async fn delete_inner(
        &mut self,
        _index_name: &str,
        document_id: &str,
    ) -> Result<(), Bm25Error> {
        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;

        let manager_guard = manager.read().await;
        let schema = manager_guard.schema();

        delete_document(&manager_guard, schema, document_id)?;

        Ok(())
    }

    /// Delete documents matching both a file path AND a project ID from the index.
    pub async fn delete_by_file_path_scoped(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        let start_time = Instant::now();

        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;
        let manager_guard = manager.read().await;
        let schema = manager_guard.schema();
        let count = delete_documents_by_file_path_and_project(
            &manager_guard,
            schema,
            file_path,
            project_id,
        )?;

        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_delete(elapsed, count, true);
        }

        Ok(count)
    }

    /// Delete documents for one file in one data epoch.
    pub async fn delete_by_file_path_scoped_epoch(
        &mut self,
        index_name: &str,
        file_path: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        let start_time = Instant::now();
        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;
        let manager_guard = manager.read().await;
        let result = delete_documents_by_file_path_project_epoch(
            &manager_guard,
            &self.schema,
            file_path,
            project_id,
            epoch,
        );
        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            let count = result.as_ref().map_or(0, |&c| c);
            metrics.record_delete(elapsed, count, result.is_ok());
        }
        result
    }

    /// Delete all documents for one project and data epoch.
    pub async fn delete_by_project_epoch(
        &mut self,
        index_name: &str,
        project_id: i64,
        epoch: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        let start_time = Instant::now();
        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;
        let manager_guard = manager.read().await;
        let result =
            delete_documents_by_project_epoch(&manager_guard, &self.schema, project_id, epoch);
        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            let count = result.as_ref().map_or(0, |&c| c);
            metrics.record_delete(elapsed, count, result.is_ok());
        }
        result
    }

    /// Read the stored fields needed to copy a published epoch into a
    /// candidate generation.
    pub async fn snapshot_documents(
        &self,
        project_id: i64,
        epoch: i64,
    ) -> Result<Vec<Bm25Document>, Bm25Error> {
        let start_time = Instant::now();
        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;
        let manager_guard = manager.read().await;
        let reader = manager_guard.reader()?;
        let searcher = reader.searcher();
        let query = tantivy::query::BooleanQuery::new(vec![
            (tantivy::query::Occur::Must, Box::new(AllQuery)),
            (
                tantivy::query::Occur::Must,
                Box::new(tantivy::query::TermQuery::new(
                    tantivy::Term::from_field_text(self.schema.project_id, &project_id.to_string()),
                    tantivy::schema::IndexRecordOption::Basic,
                )),
            ),
            (
                tantivy::query::Occur::Must,
                Box::new(tantivy::query::TermQuery::new(
                    tantivy::Term::from_field_i64(self.schema.epoch, epoch),
                    tantivy::schema::IndexRecordOption::Basic,
                )),
            ),
        ]);
        let addresses = searcher.search(&query, &DocSetCollector)?;
        let mut documents = Vec::with_capacity(addresses.len());
        for address in addresses {
            let document: tantivy::schema::TantivyDocument = searcher.doc(address)?;
            let Some(document_id) = document
                .get_first(self.schema.document_id)
                .and_then(|value| value.as_str().map(ToString::to_string))
            else {
                continue;
            };
            let mut result = Bm25Document::new(document_id.to_string());
            for (field, target) in [
                (self.schema.title, "title"),
                (self.schema.chunk_id, "chunk_id"),
                (self.schema.file_path, "file_path"),
                (self.schema.segment_id, "segment_id"),
                (self.schema.test, "test"),
                (self.schema.category, "category"),
            ] {
                if let Some(value) = document
                    .get_first(field)
                    .and_then(|value| value.as_str().map(ToString::to_string))
                {
                    result = result.with_field(target, value);
                }
            }
            let entity_ids: Vec<String> = document
                .get_all(self.schema.entity_id)
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect();
            if !entity_ids.is_empty() {
                result = result.with_field("entity_id", entity_ids.join(","));
            }
            result = result
                .with_field("project_id", project_id.to_string())
                .with_field("epoch", epoch.to_string());
            documents.push(result);
        }
        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_index(elapsed, documents.len(), true);
        }
        Ok(documents)
    }

    /// Delete all documents for a project from the index.
    pub async fn delete_all_project_docs(
        &mut self,
        index_name: &str,
        project_id: i64,
    ) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        let start_time = Instant::now();

        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;
        let manager_guard = manager.read().await;
        let schema = manager_guard.schema();
        let count = delete_documents_by_project(&manager_guard, schema, project_id)?;

        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_delete(elapsed, count, true);
        }

        tracing::debug!(project_id, count, "Deleted all BM25 documents for project");
        Ok(count)
    }

    /// Get the number of documents in the index
    pub async fn document_count(&self) -> Result<usize, Bm25Error> {
        let start_time = Instant::now();
        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;
        let manager_guard = manager.read().await;
        let reader = manager_guard.reader()?;
        let searcher = reader.searcher();
        let count = searcher.num_docs() as usize;
        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_index(elapsed, count, true);
        }
        Ok(count)
    }

    /// Get the number of documents belonging to a specific project
    pub async fn document_count_by_project(&self, project_id: i64) -> Result<usize, Bm25Error> {
        use tantivy::collector::Count;
        use tantivy::query::TermQuery;
        use tantivy::schema::IndexRecordOption;

        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;
        let manager_guard = manager.read().await;
        let reader = manager_guard.reader()?;
        let searcher = reader.searcher();

        let term = tantivy::Term::from_field_text(self.schema.project_id, &project_id.to_string());
        let query = TermQuery::new(term, IndexRecordOption::Basic);
        let count = searcher.search(&query, &Count)?;
        Ok(count)
    }

    /// Return all data epochs currently present for a project.
    pub async fn epochs_by_project(&self, project_id: i64) -> Result<Vec<i64>, Bm25Error> {
        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;
        let manager_guard = manager.read().await;
        let reader = manager_guard.reader()?;
        let searcher = reader.searcher();
        let query = tantivy::query::TermQuery::new(
            tantivy::Term::from_field_text(self.schema.project_id, &project_id.to_string()),
            tantivy::schema::IndexRecordOption::Basic,
        );
        let addresses = searcher.search(&query, &DocSetCollector)?;
        let mut epochs = std::collections::BTreeSet::new();
        for address in addresses {
            let document: tantivy::schema::TantivyDocument = searcher.doc(address)?;
            if let Some(epoch) = document
                .get_first(self.schema.epoch)
                .and_then(|value| value.as_i64())
            {
                epochs.insert(epoch);
            }
        }
        Ok(epochs.into_iter().collect())
    }

    /// Clear an index by recreating it from scratch
    pub async fn clear_index(&mut self, index_name: &str) -> Result<usize, Bm25Error> {
        self.validate_index_name(index_name)?;
        let start_time = Instant::now();
        let manager = self.index_manager.as_ref().ok_or(Bm25Error::Disabled)?;

        let index_path = match &self.config.index_path {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from("./data/bm25"),
        };

        if index_path.exists() {
            std::fs::remove_dir_all(&index_path)?;
        }

        let new_manager = IndexManager::create_with_config(
            &index_path,
            self.config.index_manager.clone(),
            self.config.algorithm.clone(),
        )?;

        let mut manager_guard = manager.write().await;
        *manager_guard = new_manager;

        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_delete(elapsed, 0, true);
        }

        tracing::info!("Cleared BM25 index");
        Ok(0)
    }

    /// Get the BM25 index directory path
    fn index_dir(&self) -> PathBuf {
        self.config
            .index_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./data/bm25"))
    }

    /// Start a background task that periodically samples BM25 index disk usage.
    pub fn start_disk_sampling(&self, interval_secs: u64) -> Option<tokio::task::JoinHandle<()>> {
        let metrics = self.metrics.clone()?;
        if !self.config.enabled {
            return None;
        }
        let index_dir = self.index_dir();
        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                let size = Self::dir_size(&index_dir);
                metrics.record_disk_usage(size);
            }
        }))
    }

    fn dir_size(path: &std::path::Path) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        total += meta.len();
                    } else if meta.is_dir() {
                        total += Self::dir_size(&entry.path());
                    }
                }
            }
        }
        total
    }
}

impl Clone for Bm25Client {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            index_manager: self.index_manager.clone(),
            schema: self.schema.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl Drop for Bm25Client {
    fn drop(&mut self) {
        if self.index_manager.is_some() {
            tracing::debug!("Bm25Client is being dropped with active index");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = Bm25Config::default();
        let client = Bm25Client::new(config);
        assert!(!client.is_enabled());
    }

    #[test]
    fn test_client_clone() {
        let config = Bm25Config::default().enabled();
        let client = Bm25Client::new(config);
        let cloned = client.clone();

        assert!(cloned.config().enabled);
    }
}
