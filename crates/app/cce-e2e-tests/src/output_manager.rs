//! Output manager for E2E test results
//!
//! Manages output directory structure and file creation.

use std::io;
use std::path::PathBuf;

/// Output category - determines subdirectory structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputCategory {
    /// Index workflow results
    Index,
    /// Query workflow results
    Query,
    /// Scenario test results
    Scenarios,
    /// Hot update workflow results
    HotUpdate,
    /// Standalone tool outputs
    Tools,
}

impl OutputCategory {
    /// Get directory name for this category
    pub fn dir_name(&self) -> &'static str {
        match self {
            OutputCategory::Index => "index",
            OutputCategory::Query => "query",
            OutputCategory::Scenarios => "scenarios",
            OutputCategory::HotUpdate => "hot_update",
            OutputCategory::Tools => "tools",
        }
    }
}

/// Output configuration
#[derive(Debug, Clone)]
pub struct OutputConfig {
    /// Base output directory (default: crate root `outputs/`)
    pub base_dir: PathBuf,
    /// Whether to create timestamped subdirectories
    pub use_timestamp: bool,
    /// Whether to overwrite existing files
    pub overwrite: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("outputs"),
            use_timestamp: false,
            overwrite: true,
        }
    }
}

/// Output manager for test results
///
/// Manages the output directory structure and provides methods for
/// creating output files in a organized manner.
pub struct OutputManager {
    /// Configuration
    config: OutputConfig,
    /// Current language scope
    language: Option<String>,
    /// Current test scenario name
    scenario: Option<String>,
    /// Current category
    category: Option<OutputCategory>,
}

impl OutputManager {
    /// Create a new output manager with default configuration
    pub fn new() -> Self {
        Self {
            config: OutputConfig::default(),
            language: None,
            scenario: None,
            category: None,
        }
    }

    /// Set the current language scope
    pub fn language(mut self, name: impl Into<String>) -> Self {
        self.language = Some(name.into());
        self
    }

    /// Set the current test scenario
    pub fn scenario(mut self, name: impl Into<String>) -> Self {
        self.scenario = Some(name.into());
        self
    }

    /// Set the output category
    pub fn category(mut self, category: OutputCategory) -> Self {
        self.category = Some(category);
        self
    }

    /// Get the output directory for current context
    ///
    /// Returns path: base_dir/category[/language]/scenario[/timestamp]
    pub fn output_dir(&self) -> PathBuf {
        let mut dir = self.config.base_dir.clone();

        if let Some(category) = &self.category {
            dir = dir.join(category.dir_name());
        }

        if let Some(language) = &self.language {
            dir = dir.join(language);
        }

        if let Some(scenario) = &self.scenario {
            dir = dir.join(scenario);
        }

        if self.config.use_timestamp {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            dir = dir.join(timestamp.to_string());
        }

        dir
    }

    /// Ensure output directory exists
    pub fn ensure_output_dir(&self) -> io::Result<PathBuf> {
        let dir = self.output_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    /// Get output file path for a given filename
    pub fn output_path(&self, filename: &str) -> PathBuf {
        self.output_dir().join(filename)
    }

    /// Write content to an output file
    ///
    /// Creates the output directory (including any nested parents of
    /// `filename`, e.g. `entity/x.md`) if it doesn't exist.
    pub fn write(&self, filename: &str, content: &str) -> io::Result<PathBuf> {
        self.ensure_output_dir()?;
        let path = self.output_path(filename);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if path.exists() && !self.config.overwrite {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("File already exists: {:?}", path),
            ));
        }

        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Create a builder for a specific test output
    pub fn builder() -> OutputBuilder {
        OutputBuilder::new()
    }

    /// Clean all outputs in the current context
    pub fn clean(&self) -> io::Result<()> {
        let dir = self.output_dir();
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
}

impl Default for OutputManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating output context
pub struct OutputBuilder {
    config: OutputConfig,
    language: Option<String>,
    category: Option<OutputCategory>,
    scenario: Option<String>,
}

impl Default for OutputBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: OutputConfig::default(),
            language: None,
            category: None,
            scenario: None,
        }
    }

    /// Set language scope
    pub fn language(mut self, name: impl Into<String>) -> Self {
        self.language = Some(name.into());
        self
    }

    /// Set output category
    pub fn category(mut self, category: OutputCategory) -> Self {
        self.category = Some(category);
        self
    }

    /// Set scenario name
    pub fn scenario(mut self, name: impl Into<String>) -> Self {
        self.scenario = Some(name.into());
        self
    }

    /// Build the output manager
    pub fn build(self) -> OutputManager {
        OutputManager {
            config: self.config,
            language: self.language,
            category: self.category,
            scenario: self.scenario,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_category_dir_name() {
        assert_eq!(OutputCategory::Index.dir_name(), "index");
        assert_eq!(OutputCategory::Query.dir_name(), "query");
        assert_eq!(OutputCategory::Scenarios.dir_name(), "scenarios");
        assert_eq!(OutputCategory::HotUpdate.dir_name(), "hot_update");
        assert_eq!(OutputCategory::Tools.dir_name(), "tools");
    }

    #[test]
    fn test_output_config_default() {
        let config = OutputConfig::default();
        assert!(config.base_dir.ends_with("outputs"));
        assert!(!config.use_timestamp);
        assert!(config.overwrite);
    }

    #[test]
    fn test_output_manager_output_dir() {
        let manager = OutputManager::builder()
            .category(OutputCategory::Query)
            .scenario("bm25_search")
            .build();

        let dir = manager.output_dir();
        assert!(dir.ends_with("outputs/query/bm25_search"));
    }

    #[test]
    fn test_output_manager_output_dir_with_language() {
        let manager = OutputManager::builder()
            .category(OutputCategory::Scenarios)
            .language("rust")
            .scenario("presentation/once_cell")
            .build();

        let dir = manager.output_dir();
        assert!(dir.ends_with("outputs/scenarios/rust/presentation/once_cell"));
    }

    #[test]
    fn test_output_manager_output_dir_with_scenario() {
        let manager = OutputManager::builder()
            .category(OutputCategory::Tools)
            .scenario("file-fold")
            .build();

        let dir = manager.output_dir();
        assert!(dir.ends_with("outputs/tools/file-fold"));
    }

    #[test]
    fn test_output_manager_write() {
        let manager = OutputManager::builder()
            .category(OutputCategory::Index)
            .scenario("test_write")
            .build();

        // Write a test file
        let result = manager.write("test.txt", "test content");
        assert!(result.is_ok());

        let path = result.expect("Failed to write");
        assert!(path.exists());

        // Clean up
        let _ = manager.clean();
    }
}
