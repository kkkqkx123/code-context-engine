//! Output serializer for different formats
//!
//! Provides serialization for test results in JSON and Markdown formats.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// JSON format
    #[default]
    Json,
    /// Markdown format (human-readable)
    Markdown,
}

impl OutputFormat {
    /// Get file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Json => "json",
            OutputFormat::Markdown => "md",
        }
    }
}

/// Serializable index result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableIndexResult {
    /// Test name
    pub test_name: String,
    /// Timestamp
    pub timestamp: String,
    /// Total files processed
    pub total_files: usize,
    /// Files successfully indexed
    pub indexed_files: usize,
    /// Files that failed
    pub failed_files: usize,
    /// Total entities extracted
    pub total_entities: usize,
    /// Total relations extracted
    pub total_relations: usize,
    /// Total vectors stored
    pub total_vectors: usize,
    /// Total tokens used
    pub total_tokens: u64,
    /// Errors encountered
    pub errors: Vec<String>,
    /// Elapsed time in milliseconds
    pub elapsed_ms: u64,
    /// Success rate
    pub success_rate: f32,
    /// Additional metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl SerializableIndexResult {
    /// Create from IndexResult
    pub fn from_index_result(test_name: &str, result: &cce_orchestrator::IndexResult) -> Self {
        Self {
            test_name: test_name.to_string(),
            timestamp: chrono::Local::now().to_rfc3339(),
            total_files: result.total_files,
            indexed_files: result.indexed_files,
            failed_files: result.failed_files,
            total_entities: result.total_entities,
            total_relations: result.total_relations,
            total_vectors: result.total_vectors,
            total_tokens: result.total_tokens,
            errors: result.errors().to_vec(),
            elapsed_ms: result.elapsed_ms,
            success_rate: result.success_rate(),
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Output serializer
pub struct OutputSerializer {
    /// Output format
    format: OutputFormat,
}

impl OutputSerializer {
    /// Create a new serializer
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    /// Create JSON serializer
    pub fn json() -> Self {
        Self::new(OutputFormat::Json)
    }

    /// Create Markdown serializer
    pub fn markdown() -> Self {
        Self::new(OutputFormat::Markdown)
    }

    /// Get the current format
    pub fn format(&self) -> OutputFormat {
        self.format
    }

    /// Serialize index result
    pub fn serialize_index_result(&self, result: &SerializableIndexResult) -> Result<String> {
        match self.format {
            OutputFormat::Json => self.serialize_json(result),
            OutputFormat::Markdown => self.serialize_index_result_markdown(result),
        }
    }

    /// Serialize to JSON
    fn serialize_json<T: Serialize>(&self, value: &T) -> Result<String> {
        serde_json::to_string_pretty(value)
            .map_err(|e| anyhow::anyhow!("JSON serialization failed: {}", e))
    }

    /// Serialize index result to Markdown
    fn serialize_index_result_markdown(&self, result: &SerializableIndexResult) -> Result<String> {
        let mut output = String::new();

        output.push_str(&format!("# Index Result: {}\n\n", result.test_name));
        output.push_str(&format!("**Timestamp:** {}\n\n", result.timestamp));

        output.push_str("## Summary\n\n");
        output.push_str(&format!("- **Total Files:** {}\n", result.total_files));
        output.push_str(&format!("- **Indexed Files:** {}\n", result.indexed_files));
        output.push_str(&format!("- **Failed Files:** {}\n", result.failed_files));
        output.push_str(&format!(
            "- **Success Rate:** {:.1}%\n",
            result.success_rate
        ));
        output.push_str(&format!("- **Elapsed Time:** {}ms\n\n", result.elapsed_ms));

        output.push_str("## Statistics\n\n");
        output.push_str(&format!(
            "- **Total Entities:** {}\n",
            result.total_entities
        ));
        output.push_str(&format!(
            "- **Total Relations:** {}\n",
            result.total_relations
        ));
        output.push_str(&format!("- **Total Vectors:** {}\n", result.total_vectors));
        output.push_str(&format!("- **Total Tokens:** {}\n\n", result.total_tokens));

        if !result.errors.is_empty() {
            output.push_str("## Errors\n\n");
            for (i, error) in result.errors.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, error));
            }
            output.push('\n');
        }

        if !result.metadata.is_empty() {
            output.push_str("## Metadata\n\n");
            for (key, value) in &result.metadata {
                output.push_str(&format!("- **{}:** {}\n", key, value));
            }
            output.push('\n');
        }

        Ok(output)
    }
}

impl Default for OutputSerializer {
    fn default() -> Self {
        Self::json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_extension() {
        assert_eq!(OutputFormat::Json.extension(), "json");
        assert_eq!(OutputFormat::Markdown.extension(), "md");
    }

    #[test]
    fn test_serialize_index_result_json() {
        let result = SerializableIndexResult {
            test_name: "test_index".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            total_files: 10,
            indexed_files: 9,
            failed_files: 1,
            total_entities: 100,
            total_relations: 50,
            total_vectors: 100,
            total_tokens: 1000,
            errors: vec!["error1".to_string()],
            elapsed_ms: 100,
            success_rate: 90.0,
            metadata: std::collections::HashMap::new(),
        };

        let serializer = OutputSerializer::json();
        let output = serializer
            .serialize_index_result(&result)
            .expect("Failed to serialize");

        assert!(output.contains("\"test_name\": \"test_index\""));
        assert!(output.contains("\"total_files\": 10"));
    }

    #[test]
    fn test_serialize_index_result_markdown() {
        let result = SerializableIndexResult {
            test_name: "test_index".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            total_files: 10,
            indexed_files: 9,
            failed_files: 1,
            total_entities: 100,
            total_relations: 50,
            total_vectors: 100,
            total_tokens: 1000,
            errors: vec!["error1".to_string()],
            elapsed_ms: 100,
            success_rate: 90.0,
            metadata: std::collections::HashMap::new(),
        };

        let serializer = OutputSerializer::markdown();
        let output = serializer
            .serialize_index_result(&result)
            .expect("Failed to serialize");

        assert!(output.contains("# Index Result: test_index"));
        assert!(output.contains("**Total Files:** 10"));
    }
}
