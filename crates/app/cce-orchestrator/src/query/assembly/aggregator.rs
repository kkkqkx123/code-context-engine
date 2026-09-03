//! Segment aggregator
//!
//! Aggregates code segments based on position and file coverage.

use super::types::{ExpandedUnit, SPSRGraphConfig};

/// Aggregated segment representing merged units
#[derive(Debug, Clone)]
pub struct AggregatedSegment {
    /// File path
    pub file_path: String,
    /// Start line
    pub start_line: u32,
    /// End line
    pub end_line: u32,
    /// Merged code content
    pub code: String,
    /// Original units that were merged
    pub source_units: Vec<ExpandedUnit>,
    /// Whether this is a whole file
    pub is_whole_file: bool,
}

impl AggregatedSegment {
    /// Create a new aggregated segment
    pub fn new(file_path: String, start_line: u32, end_line: u32, code: String) -> Self {
        Self {
            file_path,
            start_line,
            end_line,
            code,
            source_units: Vec::new(),
            is_whole_file: false,
        }
    }

    /// Create from a single unit
    pub fn from_unit(unit: ExpandedUnit) -> Self {
        Self {
            file_path: unit.file_path.clone(),
            start_line: unit.start_line,
            end_line: unit.end_line,
            code: unit.code.clone(),
            source_units: vec![unit],
            is_whole_file: false,
        }
    }

    /// Get line count
    pub fn line_count(&self) -> u32 {
        self.end_line - self.start_line + 1
    }

    /// Check if this segment contains a unit
    pub fn contains_unit(&self, unit: &ExpandedUnit) -> bool {
        unit.file_path == self.file_path
            && unit.start_line >= self.start_line
            && unit.end_line <= self.end_line
    }
}

/// Segment aggregator
///
/// Aggregates code segments based on:
/// - Adjacent segment merging
/// - File coverage threshold
pub struct SegmentAggregator {
    config: SPSRGraphConfig,
}

impl SegmentAggregator {
    /// Create a new aggregator
    pub fn new(config: SPSRGraphConfig) -> Self {
        Self { config }
    }

    /// Aggregate units by merging adjacent segments
    ///
    /// This function:
    /// 1. Groups units by file (using BTreeMap for sorted order)
    /// 2. Sorts by line number within each file
    /// 3. Merges adjacent segments (gap <= config.segment_merge_gap)
    pub fn aggregate(&self, units: Vec<ExpandedUnit>) -> Vec<AggregatedSegment> {
        if !self.config.enable_segment_merge {
            // Return as-is if merging is disabled
            return units
                .into_iter()
                .map(AggregatedSegment::from_unit)
                .collect();
        }

        // Group by file using BTreeMap to maintain sorted order
        let mut file_groups: std::collections::BTreeMap<String, Vec<ExpandedUnit>> =
            std::collections::BTreeMap::new();
        for unit in units {
            file_groups
                .entry(unit.file_path.clone())
                .or_default()
                .push(unit);
        }

        // Process each file (already sorted by file path due to BTreeMap)
        let mut result = Vec::new();
        for (_file_path, mut file_units) in file_groups {
            // Sort by start line within each file
            file_units.sort_by_key(|u| u.start_line);

            // Merge adjacent segments
            let merged = self.merge_adjacent(file_units);
            result.extend(merged);
        }

        // No need for final sort - BTreeMap ensures file order, and we process sequentially
        result
    }

    /// Merge adjacent segments within a single file
    fn merge_adjacent(&self, units: Vec<ExpandedUnit>) -> Vec<AggregatedSegment> {
        if units.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        let mut current = AggregatedSegment::from_unit(units[0].clone());

        for unit in units.into_iter().skip(1) {
            // Check if this unit is adjacent to current segment
            let gap = if unit.start_line > current.end_line {
                unit.start_line - current.end_line - 1
            } else {
                0
            };

            if gap <= self.config.segment_merge_gap {
                // Merge: extend current segment
                current.end_line = current.end_line.max(unit.end_line);
                current.code = Self::merge_code_efficient(&current.code, &unit.code, gap);

                // Optimization: Clear unit code to save memory as it's now in current.code
                let mut slim_unit = unit;
                slim_unit.code.clear();
                current.source_units.push(slim_unit);
            } else {
                // Not adjacent: push current and start new
                result.push(current);
                current = AggregatedSegment::from_unit(unit);
            }
        }

        // Don't forget the last segment
        result.push(current);

        result
    }

    /// Efficiently merge two code strings with gap handling
    /// Pre-allocates capacity to avoid multiple reallocations
    fn merge_code_efficient(code1: &str, code2: &str, gap: u32) -> String {
        // Pre-calculate total capacity needed
        let total_capacity = code1.len() + code2.len() + (gap as usize) + 1;
        let mut result = String::with_capacity(total_capacity);

        result.push_str(code1);

        // Efficiently add gap newlines using repeat
        if gap > 0 {
            result.push_str(&"\n".repeat(gap as usize));
        }

        result.push('\n');
        result.push_str(code2);

        result
    }

    /// Calculate file coverage for a set of segments
    ///
    /// Returns (covered_lines, total_lines, coverage_ratio)
    pub fn calculate_coverage(
        &self,
        segments: &[AggregatedSegment],
        file_path: &str,
        total_lines: u32,
    ) -> (u32, u32, f32) {
        let file_segments: Vec<&AggregatedSegment> = segments
            .iter()
            .filter(|s| s.file_path == file_path)
            .collect();

        if file_segments.is_empty() || total_lines == 0 {
            return (0, total_lines, 0.0);
        }

        // Calculate covered lines (avoid double-counting overlaps)
        let mut covered_lines = 0u32;
        let mut last_end = 0u32;

        for segment in file_segments {
            if segment.start_line > last_end {
                covered_lines += segment.end_line - segment.start_line + 1;
                last_end = segment.end_line;
            } else if segment.end_line > last_end {
                covered_lines += segment.end_line - last_end;
                last_end = segment.end_line;
            }
        }

        let coverage_ratio = covered_lines as f32 / total_lines as f32;
        (covered_lines, total_lines, coverage_ratio)
    }

    /// Check if file coverage exceeds threshold
    pub fn should_return_whole_file(&self, coverage_ratio: f32) -> bool {
        self.config.enable_file_coverage_threshold
            && coverage_ratio >= self.config.file_coverage_threshold
    }

    /// Get the configuration
    pub fn config(&self) -> &SPSRGraphConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_unit(
        file_path: &str,
        start_line: u32,
        end_line: u32,
        name: &str,
    ) -> ExpandedUnit {
        ExpandedUnit::new(
            format!("fn {}() {{}}", name),
            file_path.to_string(),
            start_line,
            end_line,
            name.to_string(),
        )
    }

    #[test]
    fn test_aggregate_no_merge() {
        let config = SPSRGraphConfig {
            enable_segment_merge: false,
            ..Default::default()
        };
        let aggregator = SegmentAggregator::new(config);

        let units = vec![
            create_test_unit("src/a.rs", 1, 3, "foo"),
            create_test_unit("src/a.rs", 10, 12, "bar"),
        ];

        let result = aggregator.aggregate(units);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_aggregate_adjacent_merge() {
        let config = SPSRGraphConfig {
            enable_segment_merge: true,
            segment_merge_gap: 2,
            ..Default::default()
        };
        let aggregator = SegmentAggregator::new(config);

        // Gap is 2 lines (lines 4-5), should merge
        let units = vec![
            create_test_unit("src/a.rs", 1, 3, "foo"),
            create_test_unit("src/a.rs", 6, 8, "bar"),
        ];

        let result = aggregator.aggregate(units);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start_line, 1);
        assert_eq!(result[0].end_line, 8);
    }

    #[test]
    fn test_aggregate_no_merge_large_gap() {
        let config = SPSRGraphConfig {
            enable_segment_merge: true,
            segment_merge_gap: 2,
            ..Default::default()
        };
        let aggregator = SegmentAggregator::new(config);

        // Gap is 5 lines (lines 4-8), should NOT merge
        let units = vec![
            create_test_unit("src/a.rs", 1, 3, "foo"),
            create_test_unit("src/a.rs", 9, 11, "bar"),
        ];

        let result = aggregator.aggregate(units);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_aggregate_multiple_files() {
        let config = SPSRGraphConfig {
            enable_segment_merge: true,
            segment_merge_gap: 2,
            ..Default::default()
        };
        let aggregator = SegmentAggregator::new(config);

        let units = vec![
            create_test_unit("src/a.rs", 1, 3, "foo"),
            create_test_unit("src/b.rs", 1, 3, "bar"),
            create_test_unit("src/a.rs", 6, 8, "baz"),
        ];

        let result = aggregator.aggregate(units);
        assert_eq!(result.len(), 2); // a.rs merged, b.rs separate

        // Check ordering
        assert_eq!(result[0].file_path, "src/a.rs");
        assert_eq!(result[1].file_path, "src/b.rs");
    }

    #[test]
    fn test_calculate_coverage() {
        let config = SPSRGraphConfig::default();
        let aggregator = SegmentAggregator::new(config);

        let segments = vec![
            AggregatedSegment::new("src/a.rs".to_string(), 1, 10, "code1".to_string()),
            AggregatedSegment::new("src/a.rs".to_string(), 20, 30, "code2".to_string()),
        ];

        let (covered, total, ratio) = aggregator.calculate_coverage(&segments, "src/a.rs", 100);
        assert_eq!(covered, 21); // 10 + 11
        assert_eq!(total, 100);
        assert!((ratio - 0.21).abs() < 0.01);
    }

    #[test]
    fn test_should_return_whole_file() {
        let config = SPSRGraphConfig {
            enable_file_coverage_threshold: true,
            file_coverage_threshold: 0.6,
            ..Default::default()
        };
        let aggregator = SegmentAggregator::new(config);

        assert!(aggregator.should_return_whole_file(0.7));
        assert!(aggregator.should_return_whole_file(0.6));
        assert!(!aggregator.should_return_whole_file(0.5));
    }

    #[test]
    fn test_aggregated_segment_from_unit() {
        let unit = create_test_unit("src/a.rs", 1, 3, "foo");
        let segment = AggregatedSegment::from_unit(unit);

        assert_eq!(segment.file_path, "src/a.rs");
        assert_eq!(segment.start_line, 1);
        assert_eq!(segment.end_line, 3);
        assert_eq!(segment.source_units.len(), 1);
        assert!(!segment.is_whole_file);
    }
}
