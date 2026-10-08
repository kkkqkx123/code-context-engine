//! Collection size estimator for Qdrant

use serde::{Deserialize, Serialize};

use crate::config::CollectionPreset;

/// Default average vectors per file
pub const DEFAULT_AVG_VECTORS_PER_FILE: f32 = 10.0;

/// Default average bytes per vector (f32 * 1024 dimensions + metadata overhead)
pub const DEFAULT_BYTES_PER_VECTOR: usize = 4200;

/// Size estimation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSizeEstimate {
    /// Estimated vector count
    pub vector_count: usize,
    /// Estimated collection size in bytes
    pub estimated_bytes: usize,
    /// Estimated memory usage in bytes (with HNSW index)
    pub estimated_memory_bytes: usize,
    /// Recommended preset for this size
    pub recommended_preset: CollectionPreset,
    /// File count used for estimation (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<usize>,
    /// Average vectors per file used
    pub avg_vectors_per_file: f32,
}

impl CollectionSizeEstimate {
    /// Create new size estimate
    pub fn new(vector_count: usize, file_count: Option<usize>, avg_vectors_per_file: f32) -> Self {
        let estimated_bytes = Self::calculate_storage_bytes(vector_count, DEFAULT_BYTES_PER_VECTOR);
        let estimated_memory_bytes =
            Self::calculate_memory_bytes(vector_count, DEFAULT_BYTES_PER_VECTOR);
        let recommended_preset = CollectionPreset::from_vector_count(vector_count);

        Self {
            vector_count,
            estimated_bytes,
            estimated_memory_bytes,
            recommended_preset,
            file_count,
            avg_vectors_per_file,
        }
    }

    /// Create new size estimate with custom bytes per vector
    pub fn new_with_bytes_per_vector(
        vector_count: usize,
        file_count: Option<usize>,
        avg_vectors_per_file: f32,
        bytes_per_vector: usize,
    ) -> Self {
        let estimated_bytes = Self::calculate_storage_bytes(vector_count, bytes_per_vector);
        let estimated_memory_bytes = Self::calculate_memory_bytes(vector_count, bytes_per_vector);
        let recommended_preset = CollectionPreset::from_vector_count(vector_count);

        Self {
            vector_count,
            estimated_bytes,
            estimated_memory_bytes,
            recommended_preset,
            file_count,
            avg_vectors_per_file,
        }
    }

    fn calculate_storage_bytes(vector_count: usize, bytes_per_vector: usize) -> usize {
        vector_count * bytes_per_vector
    }

    fn calculate_memory_bytes(vector_count: usize, bytes_per_vector: usize) -> usize {
        let vector_bytes = vector_count * bytes_per_vector;
        let hnsw_overhead = vector_bytes * 2;
        vector_bytes + hnsw_overhead
    }

    /// Get human-readable size string
    pub fn format_size(bytes: usize) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

        if bytes == 0 {
            return "0 B".to_string();
        }

        let exp = (bytes as f64).log(1024.0).min(UNITS.len() as f64 - 1.0) as usize;
        let value = bytes as f64 / 1024f64.powi(exp as i32);

        if exp == 0 {
            format!("{} {}", bytes, UNITS[0])
        } else {
            format!("{:.2} {}", value, UNITS[exp])
        }
    }

    /// Get formatted storage size
    pub fn storage_size_formatted(&self) -> String {
        Self::format_size(self.estimated_bytes)
    }

    /// Get formatted memory size
    pub fn memory_size_formatted(&self) -> String {
        Self::format_size(self.estimated_memory_bytes)
    }
}

/// Collection size estimator
pub struct CollectionSizeEstimator {
    avg_vectors_per_file: f32,
    bytes_per_vector: usize,
}

impl Default for CollectionSizeEstimator {
    fn default() -> Self {
        Self {
            avg_vectors_per_file: DEFAULT_AVG_VECTORS_PER_FILE,
            bytes_per_vector: DEFAULT_BYTES_PER_VECTOR,
        }
    }
}

impl CollectionSizeEstimator {
    /// Create new estimator with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom average vectors per file
    pub fn with_avg_vectors_per_file(avg: f32) -> Self {
        Self {
            avg_vectors_per_file: avg,
            ..Default::default()
        }
    }

    /// Create with custom bytes per vector
    pub fn with_bytes_per_vector(bytes: usize) -> Self {
        Self {
            bytes_per_vector: bytes,
            ..Default::default()
        }
    }

    /// Estimate size from file count
    pub fn estimate_from_file_count(&self, file_count: usize) -> CollectionSizeEstimate {
        let vector_count = (file_count as f32 * self.avg_vectors_per_file) as usize;

        CollectionSizeEstimate::new_with_bytes_per_vector(
            vector_count,
            Some(file_count),
            self.avg_vectors_per_file,
            self.bytes_per_vector,
        )
    }

    /// Estimate size from vector count directly
    pub fn estimate_from_vector_count(&self, vector_count: usize) -> CollectionSizeEstimate {
        CollectionSizeEstimate::new_with_bytes_per_vector(
            vector_count,
            None,
            self.avg_vectors_per_file,
            self.bytes_per_vector,
        )
    }

    /// Estimate size from lines of code
    pub fn estimate_from_lines_of_code(&self, lines: usize) -> CollectionSizeEstimate {
        let avg_lines_per_vector = 50.0;
        let vector_count = (lines as f32 / avg_lines_per_vector) as usize;

        CollectionSizeEstimate::new_with_bytes_per_vector(
            vector_count,
            None,
            self.avg_vectors_per_file,
            self.bytes_per_vector,
        )
    }

    /// Estimate size from project statistics
    pub fn estimate_from_project_stats(
        &self,
        file_count: usize,
        total_lines: usize,
    ) -> CollectionSizeEstimate {
        let from_files = file_count as f32 * self.avg_vectors_per_file;
        let from_lines = total_lines as f32 / 50.0;

        let vector_count = (from_files.max(from_lines)) as usize;

        CollectionSizeEstimate::new_with_bytes_per_vector(
            vector_count,
            Some(file_count),
            self.avg_vectors_per_file,
            self.bytes_per_vector,
        )
    }

    /// Get recommended preset for file count
    pub fn recommend_preset_for_files(&self, file_count: usize) -> CollectionPreset {
        let estimate = self.estimate_from_file_count(file_count);
        estimate.recommended_preset
    }

    /// Get recommended preset for vector count
    pub fn recommend_preset_for_vectors(&self, vector_count: usize) -> CollectionPreset {
        CollectionPreset::from_vector_count(vector_count)
    }

    /// Check if collection size requires upgrade
    pub fn needs_upgrade(
        &self,
        current_preset: CollectionPreset,
        file_count: usize,
    ) -> Option<CollectionPreset> {
        let recommended = self.recommend_preset_for_files(file_count);

        let preset_order = |p: CollectionPreset| match p {
            CollectionPreset::Tiny => 0,
            CollectionPreset::Small => 1,
            CollectionPreset::Medium => 2,
            CollectionPreset::Large => 3,
        };

        if preset_order(recommended) > preset_order(current_preset) {
            Some(recommended)
        } else {
            None
        }
    }

    /// Calculate size difference between two estimates
    pub fn size_difference(
        &self,
        current_vector_count: usize,
        target_vector_count: usize,
    ) -> SizeDifference {
        let current = self.estimate_from_vector_count(current_vector_count);
        let target = self.estimate_from_vector_count(target_vector_count);

        SizeDifference {
            vector_diff: target.vector_count.saturating_sub(current.vector_count),
            bytes_diff: target
                .estimated_bytes
                .saturating_sub(current.estimated_bytes),
            memory_diff: target
                .estimated_memory_bytes
                .saturating_sub(current.estimated_memory_bytes),
            growth_factor: if current.estimated_bytes > 0 {
                target.estimated_bytes as f64 / current.estimated_bytes as f64
            } else {
                1.0
            },
        }
    }
}

/// Size difference between two estimates
#[derive(Debug, Clone)]
pub struct SizeDifference {
    /// Difference in vector count
    pub vector_diff: usize,
    /// Difference in storage bytes
    pub bytes_diff: usize,
    /// Difference in memory bytes
    pub memory_diff: usize,
    /// Growth factor (target / current)
    pub growth_factor: f64,
}

impl SizeDifference {
    /// Get formatted vector difference
    pub fn vector_diff_formatted(&self) -> String {
        if self.vector_diff >= 1_000_000 {
            format!("{:.2}M", self.vector_diff as f64 / 1_000_000.0)
        } else if self.vector_diff >= 1_000 {
            format!("{:.2}K", self.vector_diff as f64 / 1_000.0)
        } else {
            self.vector_diff.to_string()
        }
    }

    /// Get formatted storage difference
    pub fn storage_diff_formatted(&self) -> String {
        CollectionSizeEstimate::format_size(self.bytes_diff)
    }

    /// Get formatted memory difference
    pub fn memory_diff_formatted(&self) -> String {
        CollectionSizeEstimate::format_size(self.memory_diff)
    }
}

/// Size estimation builder for complex scenarios
#[derive(Debug, Clone)]
pub struct SizeEstimateBuilder {
    file_count: Option<usize>,
    lines_of_code: Option<usize>,
    avg_vectors_per_file: f32,
    existing_vectors: Option<usize>,
}

impl Default for SizeEstimateBuilder {
    fn default() -> Self {
        Self {
            file_count: None,
            lines_of_code: None,
            avg_vectors_per_file: DEFAULT_AVG_VECTORS_PER_FILE,
            existing_vectors: None,
        }
    }
}

impl SizeEstimateBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set file count
    pub fn with_file_count(mut self, count: usize) -> Self {
        self.file_count = Some(count);
        self
    }

    /// Set lines of code
    pub fn with_lines_of_code(mut self, lines: usize) -> Self {
        self.lines_of_code = Some(lines);
        self
    }

    /// Set average vectors per file
    pub fn with_avg_vectors_per_file(mut self, avg: f32) -> Self {
        self.avg_vectors_per_file = avg;
        self
    }

    /// Set existing vector count
    pub fn with_existing_vectors(mut self, count: usize) -> Self {
        self.existing_vectors = Some(count);
        self
    }

    /// Build the estimate
    pub fn build(&self) -> CollectionSizeEstimate {
        let vector_count = if let Some(existing) = self.existing_vectors {
            existing
        } else {
            let from_files = self
                .file_count
                .map(|c| c as f32 * self.avg_vectors_per_file)
                .unwrap_or(0.0);
            let from_lines = self.lines_of_code.map(|l| l as f32 / 50.0).unwrap_or(0.0);

            from_files.max(from_lines) as usize
        };

        CollectionSizeEstimate::new(vector_count, self.file_count, self.avg_vectors_per_file)
    }
}

/// Preset size guidelines
pub struct PresetGuidelines;

impl PresetGuidelines {
    /// Get description for preset
    pub fn description(preset: CollectionPreset) -> &'static str {
        match preset {
            CollectionPreset::Tiny => {
                "Very small codebases (<= 2000 vectors). No HNSW index, uses full scan."
            }
            CollectionPreset::Small => "Small codebases (2000-10000 vectors). Light HNSW index.",
            CollectionPreset::Medium => {
                "Medium codebases (10000-100000 vectors). Moderate HNSW index."
            }
            CollectionPreset::Large => {
                "Large codebases (> 100000 vectors). Heavy HNSW index with quantization."
            }
        }
    }

    /// Get recommended use case for preset
    pub fn use_case(preset: CollectionPreset) -> &'static str {
        match preset {
            CollectionPreset::Tiny => "Personal projects, tutorials, single-file scripts",
            CollectionPreset::Small => "Small libraries, microservices, prototypes",
            CollectionPreset::Medium => "Medium applications, services, frameworks",
            CollectionPreset::Large => "Large monorepos, enterprise codebases, platforms",
        }
    }

    /// Get memory estimate for preset
    pub fn memory_estimate(preset: CollectionPreset) -> &'static str {
        match preset {
            CollectionPreset::Tiny => "< 50 MB",
            CollectionPreset::Small => "50-200 MB",
            CollectionPreset::Medium => "200 MB - 2 GB",
            CollectionPreset::Large => "> 2 GB",
        }
    }

    /// Get all guidelines for preset
    pub fn all_guidelines(preset: CollectionPreset) -> PresetGuideline {
        PresetGuideline {
            preset,
            description: Self::description(preset),
            use_case: Self::use_case(preset),
            memory_estimate: Self::memory_estimate(preset),
        }
    }
}

/// Complete preset guideline
#[derive(Debug, Clone)]
pub struct PresetGuideline {
    /// The preset
    pub preset: CollectionPreset,
    /// Description
    pub description: &'static str,
    /// Recommended use case
    pub use_case: &'static str,
    /// Memory estimate string
    pub memory_estimate: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_from_file_count() {
        let estimator = CollectionSizeEstimator::new();
        let estimate = estimator.estimate_from_file_count(100);

        assert_eq!(estimate.vector_count, 1000);
        assert_eq!(estimate.file_count, Some(100));
        assert_eq!(estimate.avg_vectors_per_file, 10.0);
        assert_eq!(estimate.recommended_preset, CollectionPreset::Tiny);

        let estimate = estimator.estimate_from_file_count(300);
        assert_eq!(estimate.vector_count, 3000);
        assert_eq!(estimate.recommended_preset, CollectionPreset::Small);
    }

    #[test]
    fn test_estimate_from_vector_count() {
        let estimator = CollectionSizeEstimator::new();

        let tiny = estimator.estimate_from_vector_count(1000);
        assert_eq!(tiny.recommended_preset, CollectionPreset::Tiny);

        let small = estimator.estimate_from_vector_count(5000);
        assert_eq!(small.recommended_preset, CollectionPreset::Small);

        let medium = estimator.estimate_from_vector_count(50000);
        assert_eq!(medium.recommended_preset, CollectionPreset::Medium);

        let large = estimator.estimate_from_vector_count(200000);
        assert_eq!(large.recommended_preset, CollectionPreset::Large);
    }

    #[test]
    fn test_format_size() {
        assert_eq!(CollectionSizeEstimate::format_size(0), "0 B");
        assert_eq!(CollectionSizeEstimate::format_size(100), "100 B");
        assert_eq!(CollectionSizeEstimate::format_size(1024), "1.00 KB");
        assert_eq!(CollectionSizeEstimate::format_size(1024 * 1024), "1.00 MB");
        assert_eq!(
            CollectionSizeEstimate::format_size(1024 * 1024 * 1024),
            "1.00 GB"
        );
    }

    #[test]
    fn test_builder() {
        let estimate = SizeEstimateBuilder::new()
            .with_file_count(100)
            .with_lines_of_code(5000)
            .with_avg_vectors_per_file(5.0)
            .build();

        assert_eq!(estimate.vector_count, 500);
        assert_eq!(estimate.file_count, Some(100));
    }

    #[test]
    fn test_builder_with_existing() {
        let estimate = SizeEstimateBuilder::new()
            .with_file_count(100)
            .with_existing_vectors(5000)
            .build();

        assert_eq!(estimate.vector_count, 5000);
    }

    #[test]
    fn test_needs_upgrade() {
        let estimator = CollectionSizeEstimator::new();

        let upgrade = estimator.needs_upgrade(CollectionPreset::Small, 100);
        assert!(upgrade.is_none());

        let upgrade = estimator.needs_upgrade(CollectionPreset::Small, 2000);
        assert_eq!(upgrade, Some(CollectionPreset::Medium));
    }

    #[test]
    fn test_size_difference() {
        let estimator = CollectionSizeEstimator::new();
        let diff = estimator.size_difference(1000, 10000);

        assert_eq!(diff.vector_diff, 9000);
        assert!(diff.growth_factor > 9.0 && diff.growth_factor < 11.0);
    }

    #[test]
    fn test_guidelines() {
        let tiny = PresetGuidelines::all_guidelines(CollectionPreset::Tiny);
        assert!(tiny.description.contains("small"));

        let large = PresetGuidelines::all_guidelines(CollectionPreset::Large);
        assert!(large.description.contains("Large"));
    }
}
