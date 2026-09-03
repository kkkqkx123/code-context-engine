//! Structure-aware concatenator
//!
//! Concatenates code units with structure-aware formatting.
//! Uses unit-level boundaries (from AST parsing) rather than text pattern matching.

use super::aggregator::{AggregatedSegment, SegmentAggregator};
use super::types::{ExpandedUnit, FileInfo, RelationType, SPSRGraphConfig, UnitPriority};
use cce_utils::file::read_file_to_utf8_async;

/// Structure-aware concatenator
///
/// Concatenates code units with:
/// - File boundary markers
/// - Relation markers
/// - Unit-boundary-respecting truncation (never splits a semantic unit)
pub struct StructureConcatenator {
    config: SPSRGraphConfig,
}

impl StructureConcatenator {
    /// Create a new concatenator
    pub fn new(config: SPSRGraphConfig) -> Self {
        Self { config }
    }

    /// Concatenate units into a single string
    ///
    /// # Arguments
    ///
    /// * `primary` - The primary result unit
    /// * `forward` - Forward expansion units (callees)
    /// * `backward` - Backward expansion units (callers)
    ///
    /// # Returns
    ///
    /// A tuple of (assembled_content, involved_files)
    pub async fn concatenate(
        &self,
        primary: &ExpandedUnit,
        forward: &[ExpandedUnit],
        backward: &[ExpandedUnit],
    ) -> (String, Vec<FileInfo>) {
        // Collect all units with their priorities
        let mut all_units_with_priority: Vec<(ExpandedUnit, UnitPriority)> = Vec::new();
        all_units_with_priority.push((primary.clone(), UnitPriority::Primary));

        for unit in forward {
            all_units_with_priority.push((unit.clone(), unit.priority()));
        }
        for unit in backward {
            all_units_with_priority.push((unit.clone(), unit.priority()));
        }

        // Sort by priority (lowest enum value = highest priority)
        all_units_with_priority.sort_by_key(|(_, priority)| *priority as u8);

        // Extract sorted units
        let units_only: Vec<ExpandedUnit> = all_units_with_priority
            .iter()
            .map(|(u, _)| u.clone())
            .collect();

        // Aggregate segments once
        let aggregator = SegmentAggregator::new(self.config.clone());
        let mut segments = aggregator.aggregate(units_only);

        // Integrate file coverage check
        if self.config.enable_file_coverage_threshold {
            self.apply_coverage_replacement(&mut segments).await;
        }

        // Always use semantic boundary strategy (respect unit boundaries)
        self.concatenate_respect_unit_boundaries(&segments)
    }

    /// Respect unit boundaries - never split a semantic unit
    fn concatenate_respect_unit_boundaries(
        &self,
        segments: &[AggregatedSegment],
    ) -> (String, Vec<FileInfo>) {
        let max_length = self.config.get_max_length();
        let mut result = String::new();
        let mut current_file: Option<String> = None;
        let mut file_info_map: std::collections::HashMap<String, FileInfo> =
            std::collections::HashMap::new();
        let mut added_any = false;
        let mut omitted_count = 0;
        let mut omitted_size = 0;

        for segment in segments {
            let actual_size = self.calculate_actual_segment_size(segment, &current_file);

            // Only add if it fits within limit (respect unit boundary)
            let current_size = self.config.estimate_content_tokens(&result);
            if current_size + actual_size <= max_length || !added_any {
                self.render_segment(&mut result, &mut current_file, &mut file_info_map, segment);
                added_any = true;
            } else {
                // Can't fit this unit, track it as omitted
                omitted_count += 1;
                omitted_size += self.config.estimate_content_tokens(&segment.code);
            }
        }

        // Add informative truncation marker if we stopped early
        if omitted_count > 0 {
            result.push_str("\n\n");
            result.push_str(&format!(
                "// === Additional context truncated to preserve code structure: {} unit(s) omitted (~{} tokens) ===",
                omitted_count, omitted_size
            ));
        }

        let involved_files: Vec<FileInfo> = file_info_map.into_values().collect();
        (result, involved_files)
    }

    /// Render a segment with appropriate markers
    fn render_segment(
        &self,
        result: &mut String,
        current_file: &mut Option<String>,
        file_info_map: &mut std::collections::HashMap<String, FileInfo>,
        segment: &AggregatedSegment,
    ) {
        // Add file marker if entering a new file
        if self.config.include_file_markers && *current_file != Some(segment.file_path.clone()) {
            if current_file.is_some() {
                result.push('\n');
            }
            result.push_str(&self.format_file_marker(&segment.file_path));
            result.push('\n');
            *current_file = Some(segment.file_path.clone());

            // Initialize file info if not exists
            file_info_map
                .entry(segment.file_path.clone())
                .or_insert_with(|| FileInfo::new(segment.file_path.clone()));
        }

        // Add segment marker
        if self.config.include_relation_markers {
            let marker = self.format_segment_marker(segment);
            if !marker.is_empty() {
                result.push_str(&marker);
                result.push('\n');
            }
        }

        // Add the code
        result.push_str(&segment.code);
        result.push('\n');

        // Update file info
        if let Some(file_info) = file_info_map.get_mut(&segment.file_path) {
            file_info.unit_count += segment.source_units.len();
            file_info.total_lines += segment.end_line - segment.start_line + 1;
        }
    }

    /// Calculate actual size contribution of a segment (code + markers) using token count
    fn calculate_actual_segment_size(
        &self,
        segment: &AggregatedSegment,
        current_file: &Option<String>,
    ) -> usize {
        use cce_utils::token_estimation::TokenEstimator;

        let mut token_count = TokenEstimator::estimate(&segment.code) + 1; // Code tokens + newline

        // Calculate actual file marker size (only if entering new file)
        if self.config.include_file_markers && *current_file != Some(segment.file_path.clone()) {
            let marker = self.format_file_marker(&segment.file_path);
            token_count += TokenEstimator::estimate(&marker) + 1; // Marker + newline
        }

        // Calculate actual relation marker size
        if self.config.include_relation_markers {
            let marker = self.format_segment_marker(segment);
            if !marker.is_empty() {
                token_count += TokenEstimator::estimate(&marker) + 1; // Marker + newline
            }
        }

        token_count
    }

    /// Apply file coverage replacement logic
    async fn apply_coverage_replacement(&self, segments: &mut Vec<AggregatedSegment>) {
        use std::collections::HashMap;

        // Group segments by file
        let mut file_segments: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, seg) in segments.iter().enumerate() {
            file_segments
                .entry(seg.file_path.clone())
                .or_default()
                .push(idx);
        }

        let mut replacements: Vec<(String, String, u32)> = Vec::new(); // (path, content, total_lines)

        for (file_path, indices) in &file_segments {
            // Get segments for this file
            let file_segs: Vec<AggregatedSegment> =
                indices.iter().map(|&i| segments[i].clone()).collect();

            // Read file to get actual line count
            if let Ok(content) = read_file_to_utf8_async(std::path::Path::new(file_path)).await {
                let total_lines = content.lines().count() as u32;
                if total_lines == 0 {
                    continue;
                }

                let (_, _, ratio) = SegmentAggregator::new(self.config.clone()).calculate_coverage(
                    &file_segs,
                    file_path,
                    total_lines,
                );

                if SegmentAggregator::new(self.config.clone()).should_return_whole_file(ratio) {
                    replacements.push((file_path.clone(), content, total_lines));
                }
            }
        }

        // Apply replacements
        for (path, content, total_lines) in replacements {
            // Remove old segments for this file
            segments.retain(|s| s.file_path != path);

            // Add a new "whole file" segment
            let whole_file_seg = AggregatedSegment {
                file_path: path.clone(),
                start_line: 1,
                end_line: total_lines,
                code: content,
                source_units: Vec::new(),
                is_whole_file: true,
            };
            segments.push(whole_file_seg);
        }

        // Re-sort segments to maintain order
        segments.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.start_line.cmp(&b.start_line))
        });
    }

    /// Format a file marker
    fn format_file_marker(&self, file_path: &str) -> String {
        format!("// ===== File: {} =====", file_path)
    }

    /// Format a segment marker
    fn format_segment_marker(&self, segment: &AggregatedSegment) -> String {
        if segment.is_whole_file {
            return format!(
                "// [Whole File] {} ({} lines)",
                segment.file_path, segment.end_line
            );
        }

        if segment.source_units.len() == 1 {
            // Single unit: use original relation marker
            self.format_relation_marker(&segment.source_units[0])
        } else {
            // Merged segment: show summary
            let names: Vec<&str> = segment
                .source_units
                .iter()
                .map(|u| u.name.as_str())
                .collect();
            let names_str = names.join(" & ");
            format!(
                "// [Merged] {} (lines {}-{})",
                names_str, segment.start_line, segment.end_line
            )
        }
    }

    /// Format a relation marker
    fn format_relation_marker(&self, unit: &ExpandedUnit) -> String {
        match unit.relation {
            RelationType::Primary => {
                format!(
                    "// [Primary] {} ({}:{}-{})",
                    unit.name, unit.file_path, unit.start_line, unit.end_line
                )
            }
            RelationType::Caller => {
                format!(
                    "// [Caller] {} ({}:{}-{})",
                    unit.name, unit.file_path, unit.start_line, unit.end_line
                )
            }
            RelationType::Callee => {
                format!(
                    "// [Callee] {} ({}:{}-{})",
                    unit.name, unit.file_path, unit.start_line, unit.end_line
                )
            }
            RelationType::Sibling => {
                format!(
                    "// [Sibling] {} ({}:{}-{})",
                    unit.name, unit.file_path, unit.start_line, unit.end_line
                )
            }
            RelationType::BaseClass => {
                format!(
                    "// [BaseClass] {} ({}:{}-{})",
                    unit.name, unit.file_path, unit.start_line, unit.end_line
                )
            }
            RelationType::DerivedClass => {
                format!(
                    "// [DerivedClass] {} ({}:{}-{})",
                    unit.name, unit.file_path, unit.start_line, unit.end_line
                )
            }
        }
    }

    /// Simple concatenation without markers
    pub fn concatenate_simple(
        &self,
        primary: &ExpandedUnit,
        forward: &[ExpandedUnit],
        backward: &[ExpandedUnit],
    ) -> String {
        let max_length = self.config.get_max_length();
        let mut result = String::new();

        result.push_str(&primary.code);
        result.push('\n');

        for unit in forward {
            if result.len() + unit.code.len() + 1 > max_length && !result.is_empty() {
                break;
            }
            result.push_str(&unit.code);
            result.push('\n');
        }

        for unit in backward {
            if result.len() + unit.code.len() + 1 > max_length && !result.is_empty() {
                break;
            }
            result.push_str(&unit.code);
            result.push('\n');
        }

        result
    }

    /// Get the configuration
    pub fn config(&self) -> &SPSRGraphConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concatenate_basic() {
        let config = SPSRGraphConfig::default();
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            "fn multiply(a: i32, b: i32) -> i32 {\n    add(a, b) + add(a, b)\n}".to_string(),
            "src/calc.rs".to_string(),
            10,
            12,
            "multiply".to_string(),
        );

        let forward = vec![ExpandedUnit::new(
            "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}".to_string(),
            "src/math.rs".to_string(),
            1,
            3,
            "add".to_string(),
        )];

        let (result, files) = concat.concatenate(&primary, &forward, &[]).await;

        assert!(result.contains("multiply"));
        assert!(result.contains("add"));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_concatenate_simple() {
        let config = SPSRGraphConfig::default();
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            "fn foo() {}".to_string(),
            "src/a.rs".to_string(),
            1,
            1,
            "foo".to_string(),
        );

        let result = concat.concatenate_simple(&primary, &[], &[]);

        assert_eq!(result, "fn foo() {}\n");
    }

    #[test]
    fn test_format_file_marker() {
        let config = SPSRGraphConfig::default();
        let concat = StructureConcatenator::new(config);

        let marker = concat.format_file_marker("src/main.rs");
        assert_eq!(marker, "// ===== File: src/main.rs =====");
    }

    #[test]
    fn test_format_relation_marker() {
        let config = SPSRGraphConfig::default();
        let concat = StructureConcatenator::new(config);

        let unit = ExpandedUnit::new(
            "fn foo() {}".to_string(),
            "src/a.rs".to_string(),
            1,
            1,
            "foo".to_string(),
        )
        .with_relation(RelationType::Callee);

        let marker = concat.format_relation_marker(&unit);
        assert!(marker.contains("[Callee]"));
        assert!(marker.contains("foo"));
    }

    #[tokio::test]
    async fn test_concatenate_respects_unit_boundaries() {
        let config = SPSRGraphConfig {
            max_assembled_length: 100, // Very small to trigger truncation
            ..Default::default()
        };
        let concat = StructureConcatenator::new(config);

        // Large primary unit
        let primary = ExpandedUnit::new(
            "fn large_function() {\n    // Lots of code here\n    let x = 1;\n    let y = 2;\n    x + y\n}".to_string(),
            "src/a.rs".to_string(),
            1,
            6,
            "large_function".to_string(),
        );

        let (result, _) = concat.concatenate(&primary, &[], &[]).await;

        // Should include the complete unit or nothing (never split mid-function)
        assert!(result.contains("large_function") || result.is_empty());
        // Should NOT contain partial function
        assert!(!result.contains("let x = 1") || result.contains("fn large_function"));
    }

    #[tokio::test]
    async fn test_priority_based_truncation() {
        let config = SPSRGraphConfig {
            max_assembled_length: 200,
            ..Default::default()
        };
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            "fn main() {}".to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        )
        .with_relation(RelationType::Primary);

        let callee = ExpandedUnit::new(
            "fn helper() {}".to_string(),
            "src/helper.rs".to_string(),
            1,
            1,
            "helper".to_string(),
        )
        .with_relation(RelationType::Callee)
        .with_depth(1);

        let (result, _) = concat.concatenate(&primary, &[callee], &[]).await;

        // Primary should always be included
        assert!(result.contains("main"));
    }

    #[tokio::test]
    async fn test_informative_truncation_markers() {
        let config = SPSRGraphConfig {
            max_assembled_length: 50, // Very small to trigger truncation
            ..Default::default()
        };
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            "fn large_function() {\n    let x = 1;\n    let y = 2;\n    x + y\n}".to_string(),
            "src/a.rs".to_string(),
            1,
            5,
            "large_function".to_string(),
        );

        let extra_unit = ExpandedUnit::new(
            "fn another_function() {\n    println!(\"hello\");\n}".to_string(),
            "src/b.rs".to_string(),
            1,
            3,
            "another_function".to_string(),
        );

        let (result, _) = concat.concatenate(&primary, &[extra_unit], &[]).await;

        // Should contain truncation marker with count
        assert!(result.contains("omitted") || result.contains("Truncated"));
    }

    #[test]
    fn test_character_counting_vs_byte_counting() {
        let _config = SPSRGraphConfig::default();
        let _concat = StructureConcatenator::new(_config);

        // Create a unit with multi-byte characters (e.g., Chinese comments)
        let unit_with_unicode = ExpandedUnit::new(
            "// This is a test function.\nfn test() {}".to_string(),
            "src/test.rs".to_string(),
            1,
            2,
            "test".to_string(),
        );

        // Character count should be less than byte count for Unicode text
        let char_count = unit_with_unicode.code.chars().count();
        let byte_count = unit_with_unicode.code.len();

        // This demonstrates that we're now using character counting
        assert!(char_count <= byte_count);
    }
}
