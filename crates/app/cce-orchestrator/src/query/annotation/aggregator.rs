//! Segment aggregator
//!
//! Merges adjacent unmarked same-file primary segments and reports gaps with
//! omission markers. Relation-marked expansion segments and reference
//! segments never position-merge.

use cce_utils::token_estimation::TokenEstimator;

use super::types::{ExpandedUnit, ExpansionOrigin, RelationAnnotationConfig};
use crate::query::types::content_reference::DowngradeReason;

/// Aggregated segment representing merged units
#[derive(Debug, Clone)]
pub struct AggregatedSegment {
    /// File path
    pub file_path: String,
    /// Start line
    pub start_line: u32,
    /// End line
    pub end_line: u32,
    /// Merged code content (empty for reference segments)
    pub code: String,
    /// Original units that were merged
    pub source_units: Vec<ExpandedUnit>,
    /// Highest score across the source units; single-unit segments inherit
    /// their unit's score so large merged spans are not penalized in budget
    /// selection.
    pub score: f32,
    /// Token estimate of the dropped body; only meaningful on references.
    pub body_tokens: usize,
    /// Set when the segment was downgraded to a path-and-range reference.
    /// Reference segments carry no body text and never merge.
    pub reference: Option<DowngradeReason>,
    /// Optional relation marker rendered above the segment code
    pub marker: Option<String>,
}

impl AggregatedSegment {
    /// Create a new aggregated segment
    pub fn new(file_path: String, start_line: u32, end_line: u32, code: String) -> Self {
        let body_tokens = TokenEstimator::estimate(&code);
        Self {
            file_path,
            start_line,
            end_line,
            code,
            source_units: Vec::new(),
            score: 0.0,
            body_tokens,
            reference: None,
            marker: None,
        }
    }

    /// Create from a single unit, inheriting its score
    pub fn from_unit(unit: ExpandedUnit) -> Self {
        let body_tokens = TokenEstimator::estimate(&unit.code);
        Self {
            file_path: unit.file_path.clone(),
            start_line: unit.start_line,
            end_line: unit.end_line,
            code: unit.code.clone(),
            score: unit.score,
            source_units: vec![unit],
            body_tokens,
            reference: None,
            marker: None,
        }
    }

    /// Whether this segment is a downgraded reference without body text
    pub fn is_reference(&self) -> bool {
        self.reference.is_some()
    }

    /// Whether this segment carries an expansion relation and must keep its
    /// own rendering. Any relation-marked source unit (non-primary origin)
    /// or an explicit marker opts the segment out of position merging.
    pub fn has_relation(&self) -> bool {
        self.marker.is_some()
            || self
                .source_units
                .iter()
                .any(|unit| unit.origin != ExpansionOrigin::Primary)
    }

    /// Downgrade the segment to a path-and-range reference in place,
    /// recording the body magnitude for the reference line.
    pub fn downgrade_to_reference(&mut self, reason: DowngradeReason) {
        self.body_tokens = TokenEstimator::estimate(&self.code);
        self.code.clear();
        self.source_units.clear();
        self.reference = Some(reason);
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
/// Merges adjacent unmarked same-file primary segments.
pub struct SegmentAggregator {
    config: RelationAnnotationConfig,
}

impl SegmentAggregator {
    /// Create a new aggregator
    pub fn new(config: RelationAnnotationConfig) -> Self {
        Self { config }
    }

    /// Aggregate units by merging adjacent segments
    ///
    /// This function:
    /// 1. Groups units by file (using BTreeMap for sorted order)
    /// 2. Sorts by line number within each file
    /// 3. Merges adjacent unmarked primary segments (gap <=
    ///    config.segment_merge_gap), keeping the highest source score and
    ///    reporting gaps with omission markers. Relation-marked expansion
    ///    units and reference segments are never merged.
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

            // Only unmarked primary segments merge; expansion and reference
            // segments always keep their own rendering.
            let mergeable = current.reference.is_none()
                && !current.has_relation()
                && unit.origin == ExpansionOrigin::Primary;
            if mergeable && gap <= self.config.segment_merge_gap {
                // Merge: extend current segment
                let omitted_start = current.end_line + 1;
                let omitted_end = unit.start_line.saturating_sub(1);
                current.end_line = current.end_line.max(unit.end_line);
                current.code = Self::merge_code_with_omission(
                    &current.code,
                    &unit.code,
                    gap,
                    omitted_start,
                    omitted_end,
                );
                current.score = current.score.max(unit.score);
                current.body_tokens = TokenEstimator::estimate(&current.code);

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

    /// Merge two code strings, reporting unknown gap lines with an omission
    /// marker that names the omitted line interval instead of faking
    /// continuity with blank lines.
    fn merge_code_with_omission(
        code1: &str,
        code2: &str,
        gap: u32,
        omitted_start: u32,
        omitted_end: u32,
    ) -> String {
        let marker = if gap > 0 {
            format!("// [omitted] {gap} line(s) [{omitted_start}-{omitted_end}]\n")
        } else {
            String::new()
        };
        let total_capacity = code1.len() + marker.len() + code2.len() + 2;
        let mut result = String::with_capacity(total_capacity);

        result.push_str(code1);
        result.push('\n');
        result.push_str(&marker);
        result.push_str(code2);

        result
    }

    /// Get the configuration
    pub fn config(&self) -> &RelationAnnotationConfig {
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
        .with_score(0.5)
    }

    #[test]
    fn test_aggregate_no_merge() {
        let config = RelationAnnotationConfig {
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
        let config = RelationAnnotationConfig {
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
        // Gap lines are reported with an omission marker, not blank-filled.
        assert!(result[0].code.contains("// [omitted] 2 line(s) [4-5]"));
        assert!(result[0].code.contains("fn foo()"));
        assert!(result[0].code.contains("fn bar()"));
    }

    #[test]
    fn test_merge_keeps_highest_score() {
        let config = RelationAnnotationConfig {
            enable_segment_merge: true,
            segment_merge_gap: 2,
            ..Default::default()
        };
        let aggregator = SegmentAggregator::new(config);

        let low = create_test_unit("src/a.rs", 1, 3, "foo").with_score(0.2);
        let high = create_test_unit("src/a.rs", 6, 8, "bar").with_score(0.9);

        let result = aggregator.aggregate(vec![low, high]);
        assert_eq!(result.len(), 1);
        assert!((result[0].score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_expansion_units_never_merge() {
        let config = RelationAnnotationConfig {
            enable_segment_merge: true,
            segment_merge_gap: 2,
            ..Default::default()
        };
        let aggregator = SegmentAggregator::new(config);

        let primary = create_test_unit("src/a.rs", 1, 3, "foo");
        let expansion = create_test_unit("src/a.rs", 6, 8, "bar")
            .with_expansion(ExpansionOrigin::Forward, "calls");

        let result = aggregator.aggregate(vec![primary, expansion]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_aggregate_no_merge_large_gap() {
        let config = RelationAnnotationConfig {
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
        let config = RelationAnnotationConfig {
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
    fn test_aggregated_segment_from_unit() {
        let unit = create_test_unit("src/a.rs", 1, 3, "foo");
        let segment = AggregatedSegment::from_unit(unit);

        assert_eq!(segment.file_path, "src/a.rs");
        assert_eq!(segment.start_line, 1);
        assert_eq!(segment.end_line, 3);
        assert_eq!(segment.source_units.len(), 1);
        assert!(!segment.is_reference());
        assert!(!segment.has_relation());
        // Single-unit segments inherit the unit score.
        assert!((segment.score - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_reference_segment_never_merges() {
        let mut segment = AggregatedSegment::from_unit(create_test_unit("src/a.rs", 1, 3, "foo"));
        segment.downgrade_to_reference(DowngradeReason::FileMissing);

        assert!(segment.is_reference());
        assert!(segment.code.is_empty());
        assert!(segment.body_tokens > 0);

        // A marked expansion segment also opts out of merging.
        let mut marked = AggregatedSegment::from_unit(create_test_unit("src/a.rs", 6, 8, "bar"));
        marked.marker = Some("// [calls] bar".to_string());
        assert!(marked.has_relation());
    }
}
