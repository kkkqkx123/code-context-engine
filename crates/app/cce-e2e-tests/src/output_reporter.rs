//! Output reporter for generating human-readable reports
//!
//! Provides high-level API for generating and writing test result reports.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;

use super::output_manager::OutputManager;
use super::output_serializer::{OutputSerializer, SerializableIndexResult};

/// Report metadata
#[derive(Debug, Clone, Default)]
pub struct ReportMetadata {
    /// Test description
    pub description: Option<String>,
    /// Test category
    pub category: Option<String>,
    /// Test tags
    pub tags: Vec<String>,
    /// Additional key-value pairs
    pub extra: HashMap<String, String>,
}

impl ReportMetadata {
    /// Create new metadata
    pub fn new() -> Self {
        Self::default()
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set category
    pub fn category(mut self, cat: impl Into<String>) -> Self {
        self.category = Some(cat.into());
        self
    }

    /// Add tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add extra metadata
    pub fn extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

/// Output reporter for generating test result reports
///
/// Provides a high-level API for generating and writing test result
/// reports to files for human review.
pub struct OutputReporter {
    /// Output manager
    pub manager: OutputManager,
    /// Serializer
    serializer: OutputSerializer,
    /// Report metadata
    metadata: ReportMetadata,
}

impl OutputReporter {
    /// Create a new reporter
    pub fn new(manager: OutputManager, serializer: OutputSerializer) -> Self {
        Self {
            manager,
            serializer,
            metadata: ReportMetadata::default(),
        }
    }

    /// Set report metadata
    pub fn with_metadata(mut self, metadata: ReportMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Report index result
    ///
    /// Writes the index result to a file and returns the file path.
    pub fn report_index_result(
        &self,
        test_name: &str,
        result: &cce_orchestrator::IndexResult,
    ) -> Result<PathBuf> {
        let mut serializable = SerializableIndexResult::from_index_result(test_name, result);

        if let Some(desc) = &self.metadata.description {
            serializable = serializable.with_metadata("description", desc);
        }
        if let Some(cat) = &self.metadata.category {
            serializable = serializable.with_metadata("category", cat);
        }
        for tag in &self.metadata.tags {
            serializable = serializable.with_metadata("tag", tag);
        }
        for (key, value) in &self.metadata.extra {
            serializable = serializable.with_metadata(key, value);
        }

        let content = self.serializer.serialize_index_result(&serializable)?;
        let filename = format!("result.{}", self.serializer.format().extension());

        self.manager
            .write(&filename, &content)
            .map_err(|e| anyhow::anyhow!("Failed to write: {}", e))
    }

    /// Write a custom report
    ///
    /// Writes arbitrary content to a file.
    pub fn write_custom(&self, filename: &str, content: &str) -> Result<PathBuf> {
        self.manager
            .write(filename, content)
            .map_err(|e| anyhow::anyhow!("Failed to write: {}", e))
    }
}

impl Default for OutputReporter {
    fn default() -> Self {
        Self::new(OutputManager::new(), OutputSerializer::json())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_metadata_builder() {
        let metadata = ReportMetadata::new()
            .description("Test description")
            .category("index")
            .tag("rust")
            .tag("e2e")
            .extra("key", "value");

        assert_eq!(metadata.description, Some("Test description".to_string()));
        assert_eq!(metadata.category, Some("index".to_string()));
        assert_eq!(metadata.tags, vec!["rust", "e2e"]);
        assert_eq!(metadata.extra.get("key"), Some(&"value".to_string()));
    }
}
