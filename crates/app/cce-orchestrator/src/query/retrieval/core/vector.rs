//! Vector retrieval types
//!
//! Provides filter options and result types for vector search operations.
//! The actual retrieval logic is delegated to `QdrantRetrieval::search_dense`
//! in the infrastructure layer.

/// Filter options for vector search
#[derive(Debug, Clone, Default)]
pub struct FilterOptions {
    /// Directory prefix filter
    pub directory_prefix: Option<String>,
    /// Content types to exclude (e.g., test files, generated code)
    pub exclude_content_types: Vec<crate::query::types::ExcludableContentType>,
    /// Include only specific categories
    pub include_categories: Vec<cce_types::FileCategory>,
    /// Exclude specific categories
    pub exclude_categories: Vec<cce_types::FileCategory>,
}

impl FilterOptions {
    /// Create new filter options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set directory prefix filter
    pub fn with_directory_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.directory_prefix = Some(prefix.into());
        self
    }

    /// Set content types to exclude
    pub fn with_exclude_content_types(
        mut self,
        types: Vec<crate::query::types::ExcludableContentType>,
    ) -> Self {
        self.exclude_content_types = types;
        self
    }

    /// Set include categories
    pub fn with_include_categories(mut self, categories: Vec<cce_types::FileCategory>) -> Self {
        self.include_categories = categories;
        self
    }

    /// Set exclude categories
    pub fn with_exclude_categories(mut self, categories: Vec<cce_types::FileCategory>) -> Self {
        self.exclude_categories = categories;
        self
    }

    /// Check if any filter is active
    pub fn is_empty(&self) -> bool {
        self.directory_prefix.is_none()
            && self.exclude_content_types.is_empty()
            && self.include_categories.is_empty()
            && self.exclude_categories.is_empty()
    }
}
