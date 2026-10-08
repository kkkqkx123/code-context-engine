//! Chunk and entity-detail mapping record types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRecord {
    pub chunk_id: String,
    pub file_path: String,
    pub content: String,
    pub start_line: i64,
    pub end_line: i64,
    pub entity_ids: String,
    pub entity_names: String,
    pub chunk_type: String,
    pub test_status: u8,
    pub test_source: u8,
    pub created_at: i64,
    pub updated_at: i64,
    pub project_id: Option<i64>,
    pub epoch: i64,
    pub batch_id: i64,
    pub path: String,
    pub bm25_keywords: String,
    pub segment_id: String,
    pub truncated: u8,
}

impl ChunkRecord {
    pub fn new(
        chunk_id: String,
        file_path: String,
        content: String,
        start_line: i64,
        end_line: i64,
    ) -> Self {
        use chrono::Utc;
        let now = Utc::now().timestamp();
        Self {
            chunk_id,
            file_path,
            content,
            start_line,
            end_line,
            entity_ids: "[]".to_string(),
            entity_names: "[]".to_string(),
            chunk_type: "unknown".to_string(),
            test_status: 0,
            test_source: 0,
            created_at: now,
            updated_at: now,
            project_id: None,
            epoch: 0,
            batch_id: 0,
            path: "emb".to_string(),
            bm25_keywords: String::new(),
            segment_id: String::new(),
            truncated: 0,
        }
    }

    pub fn with_bm25_keywords(mut self, keywords: impl Into<String>) -> Self {
        self.bm25_keywords = keywords.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_truncated(mut self, truncated: bool) -> Self {
        self.truncated = truncated as u8;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_entity_ids(mut self, entity_ids: &[i64]) -> Self {
        self.entity_ids = serde_json::to_string(entity_ids).unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Failed to serialize entity ids; storing empty list");
            "[]".to_string()
        });
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_entity_ids_json(mut self, entity_ids_json: impl Into<String>) -> Self {
        self.entity_ids = entity_ids_json.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_entity_names(mut self, entity_names: &[String]) -> Self {
        self.entity_names = serde_json::to_string(entity_names).unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Failed to serialize entity names; storing empty list");
            "[]".to_string()
        });
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_chunk_type(mut self, chunk_type: String) -> Self {
        self.chunk_type = chunk_type;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_test_status(mut self, test_status: u8) -> Self {
        self.test_status = test_status;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_test_source(mut self, test_source: u8) -> Self {
        self.test_source = test_source;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_project_id(mut self, project_id: i64) -> Self {
        self.project_id = Some(project_id);
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_epoch(mut self, epoch: i64) -> Self {
        self.epoch = epoch;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_batch_id(mut self, batch_id: i64) -> Self {
        self.batch_id = batch_id;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_segment_id(mut self, segment_id: impl Into<String>) -> Self {
        self.segment_id = segment_id.into();
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn get_entity_ids(&self) -> Vec<i64> {
        match serde_json::from_str(&self.entity_ids) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(error = %error, "Corrupt entity ids; returning empty list");
                Vec::new()
            }
        }
    }

    pub fn try_get_entity_ids(&self) -> Result<Vec<i64>, serde_json::Error> {
        serde_json::from_str(&self.entity_ids)
    }

    pub fn get_entity_names(&self) -> Vec<String> {
        match serde_json::from_str(&self.entity_names) {
            Ok(names) => names,
            Err(error) => {
                tracing::warn!(error = %error, "Corrupt entity names; returning empty list");
                Vec::new()
            }
        }
    }

    pub fn try_get_entity_names(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.entity_names)
    }

    pub fn has_entity_id(&self, entity_id: i64) -> bool {
        self.get_entity_ids().contains(&entity_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDetailMapping {
    pub id: i64,
    pub entity_id: i64,
    pub project_id: Option<i64>,
    pub epoch: i64,
    pub qdrant_point_ids: String,
    pub bm25_doc_ids: String,
    pub chunk_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl EntityDetailMapping {
    pub fn new(entity_id: i64) -> Self {
        use chrono::Utc;
        let now = Utc::now().timestamp();
        Self {
            id: 0,
            entity_id,
            project_id: None,
            epoch: 0,
            qdrant_point_ids: "[]".to_string(),
            bm25_doc_ids: "[]".to_string(),
            chunk_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_project_id(mut self, project_id: i64) -> Self {
        self.project_id = Some(project_id);
        self
    }

    pub fn with_epoch(mut self, epoch: i64) -> Self {
        self.epoch = epoch;
        self
    }

    pub fn with_qdrant_point_ids(mut self, point_ids: &[String]) -> Self {
        self.qdrant_point_ids = serde_json::to_string(point_ids).unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Failed to serialize qdrant point ids; storing empty list");
            "[]".to_string()
        });
        self.chunk_count = point_ids.len() as i64;
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn with_bm25_doc_ids(mut self, doc_ids: &[String]) -> Self {
        self.bm25_doc_ids = serde_json::to_string(doc_ids).unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Failed to serialize bm25 doc ids; storing empty list");
            "[]".to_string()
        });
        self.updated_at = chrono::Utc::now().timestamp();
        self
    }

    pub fn get_qdrant_point_ids(&self) -> Vec<String> {
        match serde_json::from_str(&self.qdrant_point_ids) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(error = %error, "Corrupt qdrant point ids; returning empty list");
                Vec::new()
            }
        }
    }

    pub fn try_get_qdrant_point_ids(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.qdrant_point_ids)
    }

    pub fn get_bm25_doc_ids(&self) -> Vec<String> {
        match serde_json::from_str(&self.bm25_doc_ids) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(error = %error, "Corrupt bm25 doc ids; returning empty list");
                Vec::new()
            }
        }
    }

    pub fn try_get_bm25_doc_ids(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.bm25_doc_ids)
    }
}
