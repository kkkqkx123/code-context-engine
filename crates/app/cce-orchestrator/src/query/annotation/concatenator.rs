//! Structure-aware concatenator
//!
//! Concatenates code units with relation markers and structure-aware formatting.
//! Uses unit-level boundaries rather than text pattern matching.

use std::path::Path;

use cce_utils::token_estimation::TokenEstimator;

use super::aggregator::{AggregatedSegment, SegmentAggregator};
use super::types::{ExpandedUnit, ExpansionOrigin, FileInfo, RelationAnnotationConfig};
use crate::query::types::content_reference::{DowngradeReason, reference_content};

/// Escape a string for use inside a double-quoted XML attribute value.
/// Only the five XML-mandatory characters are escaped; code bodies are never
/// attribute values and stay verbatim.
fn xml_escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Structure-aware concatenator
///
/// Concatenates code units into an XML-wrapped annotation document:
/// `<annotation>` root with self-locating `<unit>` fragments. The primary
/// carries its `name`, expansions carry their `rel` direction, and every
/// unit carries its file and line range in `location`, with code placed
/// verbatim between the open and close tags.
pub struct StructureConcatenator {
    config: RelationAnnotationConfig,
}

impl StructureConcatenator {
    /// Create a new concatenator
    pub fn new(config: RelationAnnotationConfig) -> Self {
        Self { config }
    }

    /// Concatenate units into a single string
    ///
    /// Pipeline: aggregate primary segments, attach expansion units with
    /// relation markers, downgrade missing files to references, downgrade
    /// oversized segments to references, select by score with the primary
    /// pinned, then render the survivors in structural (file, line) order.
    ///
    /// # Arguments
    ///
    /// * `primary` - The primary result unit
    /// * `forward` - Forward expansion units (callees)
    /// * `backward` - Backward expansion units (callers)
    ///
    /// # Returns
    ///
    /// A tuple of (annotated_content, involved_files)
    pub async fn concatenate(
        &self,
        primary: &ExpandedUnit,
        forward: &[ExpandedUnit],
        backward: &[ExpandedUnit],
    ) -> (String, Vec<FileInfo>) {
        // Aggregate position-mergeable primary segments.
        let aggregator = SegmentAggregator::new(self.config.clone());
        let mut segments = aggregator.aggregate(vec![primary.clone()]);
        let primary_count = segments.len();

        // Expansion units keep explicit relation ordering; position-based
        // merging must not fold them into primary segments. The relation is
        // rendered as the `rel` attribute of the fragment tag, so no marker
        // line is needed here.
        for unit in forward.iter().chain(backward.iter()) {
            let segment = AggregatedSegment::from_unit(unit.clone());
            segments.push(segment);
        }

        // Files that vanished under the workspace root become references.
        self.apply_existence_check(&mut segments).await;

        // Oversized bodies become references without spending others' budget.
        self.apply_segment_budget(&mut segments);

        // Score selection (primary pinned) then structural-order rendering.
        self.select_and_render(segments, primary_count)
    }

    /// Direction label for an expansion unit, rendered as the `rel` attribute
    /// of its fragment tag (`calls` / `called by` / `constructed by`). Only
    /// the coarse direction is rendered: the fine-grained relation type is
    /// visible in the evidence body itself, so repeating it would tax every
    /// unit for no additional signal.
    fn relation_key(unit: &ExpandedUnit) -> Option<String> {
        let default_label = match unit.origin {
            ExpansionOrigin::Primary => return None,
            ExpansionOrigin::Forward => "calls",
            ExpansionOrigin::Backward => "called by",
        };
        Some(if unit.edge_label.is_empty() {
            default_label.to_string()
        } else {
            unit.edge_label.clone()
        })
    }

    /// Downgrade segments whose source file no longer exists to references.
    ///
    /// Relative paths resolve against the configured workspace root, absolute
    /// paths are checked directly. Without a workspace root the check is
    /// skipped entirely and nothing is asserted about freshness. The
    /// self-evident gate reuses this exact path so it always mirrors what the
    /// renderer will do: it must never second-guess rendering.
    async fn apply_existence_check(&self, segments: &mut [AggregatedSegment]) {
        let Some(root) = self.config.workspace_root.clone() else {
            return;
        };
        for segment in segments.iter_mut() {
            if segment.is_reference() {
                continue;
            }
            let path = Path::new(&segment.file_path);
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else {
                root.join(path)
            };
            if tokio::fs::metadata(&resolved).await.is_err() {
                segment.downgrade_to_reference(DowngradeReason::FileMissing);
            }
        }
    }

    /// Downgrade oversized segment bodies to references.
    ///
    /// A body larger than the single per-result quota degrades alone to a
    /// reference instead of crowding out the other spans.
    fn apply_segment_budget(&self, segments: &mut [AggregatedSegment]) {
        for segment in segments.iter_mut() {
            if segment.is_reference() {
                continue;
            }
            if self.standalone_cost(segment) > self.config.get_max_length() {
                segment.downgrade_to_reference(DowngradeReason::OverLimit);
            }
        }
    }

    /// Whether the primary segment will render with its full body in the
    /// final output: not omitted by configuration, present on disk, and
    /// small enough to survive the budget check. Callers use this to decide
    /// whether forward (callee) relations are self-evident from the
    /// primary's own rendered code.
    pub async fn primary_renders_fully(&self, primary: &ExpandedUnit) -> bool {
        if self.config.omit_primary_body {
            return false;
        }
        let mut segment = AggregatedSegment::from_unit(primary.clone());
        self.apply_existence_check(std::slice::from_mut(&mut segment))
            .await;
        self.apply_segment_budget(std::slice::from_mut(&mut segment));
        !segment.is_reference()
    }

    /// Select segments by score with the primary pinned, then render the
    /// survivors in structural order: primary first, remaining expansions
    /// grouped by file and line for readability. With primary-body omission
    /// enabled the pinned primary still reserves nothing and renders nothing;
    /// only expansion segments reach the output.
    fn select_and_render(
        &self,
        segments: Vec<AggregatedSegment>,
        primary_count: usize,
    ) -> (String, Vec<FileInfo>) {
        let max_length = self.config.get_max_length();
        let primary_count = primary_count.min(segments.len());
        let render_primary = !self.config.omit_primary_body;

        // The primary is always kept; an oversized primary already shrank to
        // a small reference above, so pinning cannot blow the budget. An
        // omitted primary neither reserves budget nor renders.
        let mut total: usize = if render_primary {
            segments[..primary_count]
                .iter()
                .map(|segment| self.selection_cost(segment))
                .sum()
        } else {
            0
        };
        let mut picked = vec![false; segments.len()];
        for slot in picked.iter_mut().take(primary_count) {
            *slot = true;
        }

        // Remaining expansions enter by score, highest first (stable: ties
        // keep caller order). Whole units are kept or dropped; a unit is
        // never split to fit.
        let mut rest: Vec<usize> = (primary_count..segments.len()).collect();
        rest.sort_by(|a, b| segments[*b].score.total_cmp(&segments[*a].score));
        let mut omitted_count = 0;
        let mut omitted_size = 0;
        for index in rest {
            let cost = self.selection_cost(&segments[index]);
            if total + cost <= max_length {
                picked[index] = true;
                total += cost;
            } else {
                omitted_count += 1;
                omitted_size += cost;
            }
        }

        let mut order: Vec<usize> = if render_primary {
            (0..primary_count).collect()
        } else {
            Vec::new()
        };
        let mut picked_rest: Vec<usize> = (primary_count..segments.len())
            .filter(|index| picked[*index])
            .collect();
        picked_rest.sort_by(|a, b| {
            segments[*a]
                .file_path
                .cmp(&segments[*b].file_path)
                .then(segments[*a].start_line.cmp(&segments[*b].start_line))
        });
        order.extend(picked_rest);

        let mut result = String::new();
        let mut file_info_map: std::collections::HashMap<String, FileInfo> =
            std::collections::HashMap::new();
        result.push_str("<annotation>\n");
        for index in order {
            self.render_segment(&mut result, &mut file_info_map, &segments[index]);
        }
        result.push_str("</annotation>");

        // Add informative truncation marker if we stopped early
        if omitted_count > 0 {
            result.push_str("\n\n");
            result.push_str(&format!(
                "<omitted units=\"{}\" tokens=\"~{}\"/>",
                omitted_count, omitted_size
            ));
        }

        let involved_files: Vec<FileInfo> = file_info_map.into_values().collect();
        (result, involved_files)
    }

    /// Render one segment as a self-locating `<unit>` fragment: the primary
    /// carries its `name`, expansions carry their `rel` direction, and every
    /// unit carries its file and line range in `location`, with the code
    /// placed verbatim between the open and close tags. Reference segments
    /// render as self-closing `<reference>` tags instead. No file-grouping
    /// layer is emitted: repeating the path per unit keeps every fragment
    /// interpretable on its own, which is what grep-style consumers expect.
    fn render_segment(
        &self,
        result: &mut String,
        file_info_map: &mut std::collections::HashMap<String, FileInfo>,
        segment: &AggregatedSegment,
    ) {
        file_info_map
            .entry(segment.file_path.clone())
            .or_insert_with(|| FileInfo::new(segment.file_path.clone()));

        if let Some(reason) = segment.reference {
            result.push_str(&reference_content(
                &segment.file_path,
                segment.start_line,
                segment.end_line,
                segment.body_tokens,
                reason,
            ));
            result.push('\n');
        } else {
            result.push_str(&Self::open_tag(segment));
            result.push('\n');
            result.push_str(&segment.code);
            result.push('\n');
            result.push_str("</unit>\n");
        }

        if let Some(file_info) = file_info_map.get_mut(&segment.file_path) {
            file_info.unit_count += segment.source_units.len();
            file_info.total_lines += segment.end_line - segment.start_line + 1;
        }
    }

    /// Open tag for one body segment, shared by rendering and cost
    /// estimation so the two can never drift apart.
    fn open_tag(segment: &AggregatedSegment) -> String {
        let first = segment.source_units.first();
        let location = format!(
            "{}:{}-{}",
            segment.file_path, segment.start_line, segment.end_line
        );
        let excerpt_attr = if first.map(|unit| unit.is_excerpt).unwrap_or(false) {
            " excerpt=\"true\""
        } else {
            ""
        };
        match first.map(|unit| unit.origin) {
            // The primary is the answer headline: it keeps its name.
            Some(ExpansionOrigin::Primary) | None => {
                let name = first.map(|unit| unit.name.as_str()).unwrap_or("");
                format!(
                    "<unit name=\"{}\" location=\"{}\"{excerpt_attr}>",
                    xml_escape_attr(name),
                    xml_escape_attr(&location),
                )
            }
            // Expansions are edge evidence: direction plus location is the
            // whole pointer. The neighbour name already shows in (or
            // resolves unambiguously from) the evidence body.
            _ => {
                let rel = first.and_then(Self::relation_key).unwrap_or_default();
                format!(
                    "<unit rel=\"{}\" location=\"{}\"{excerpt_attr}>",
                    xml_escape_attr(&rel),
                    xml_escape_attr(&location),
                )
            }
        }
    }

    /// Standalone token cost of a segment body: fragment tags plus code.
    fn standalone_cost(&self, segment: &AggregatedSegment) -> usize {
        self.xml_segment_cost(segment)
    }

    /// Token cost of a segment: the exact open tag (shared with rendering),
    /// code, and close tag, so selection measures what rendering emits.
    fn xml_segment_cost(&self, segment: &AggregatedSegment) -> usize {
        if let Some(reason) = segment.reference {
            let tag = reference_content(
                &segment.file_path,
                segment.start_line,
                segment.end_line,
                segment.body_tokens,
                reason,
            );
            return TokenEstimator::estimate(&tag) + 1;
        }
        let open = Self::open_tag(segment);
        let close = "</unit>";
        TokenEstimator::estimate(&segment.code)
            + 1
            + TokenEstimator::estimate(&open)
            + 1
            + TokenEstimator::estimate(close)
            + 1
    }

    /// Token cost used for budget selection: same estimate as standalone.
    fn selection_cost(&self, segment: &AggregatedSegment) -> usize {
        self.xml_segment_cost(segment)
    }

    /// Get the configuration
    pub fn config(&self) -> &RelationAnnotationConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concatenate_basic() {
        let config = RelationAnnotationConfig::default();
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            "fn multiply(a: i32, b: i32) -> i32 {\n    compute(a, b) + compute(a, b)\n}"
                .to_string(),
            "src/calc.rs".to_string(),
            10,
            12,
            "multiply".to_string(),
        );

        let forward = vec![
            ExpandedUnit::new(
                "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}".to_string(),
                "src/math.rs".to_string(),
                1,
                3,
                "add".to_string(),
            )
            .with_expansion(ExpansionOrigin::Forward, "calls"),
        ];

        let (result, files) = concat.concatenate(&primary, &forward, &[]).await;

        // Forward expansion is attached with its direction and location
        assert!(result.contains("multiply"));
        assert!(result.contains("fn add"));
        assert!(result.contains("<unit name=\"multiply\" location=\"src/calc.rs:10-12\">"));
        assert!(result.contains("<unit rel=\"calls\" location=\"src/math.rs:1-3\">"));
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_concatenate_respects_unit_boundaries() {
        let config = RelationAnnotationConfig {
            max_annotated_length: 100, // Very small to trigger truncation
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

        // The pinned primary is always kept whole (never split mid-function).
        assert!(result.contains("fn large_function"));
        assert!(result.contains("let x = 1"));
    }

    #[tokio::test]
    async fn test_priority_based_truncation() {
        let config = RelationAnnotationConfig {
            max_annotated_length: 200,
            ..Default::default()
        };
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            "fn main() {}".to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );

        let (result, _) = concat.concatenate(&primary, &[], &[]).await;

        // Primary should always be included
        assert!(result.contains("main"));
    }

    #[tokio::test]
    async fn test_informative_truncation_markers() {
        let primary_code = "fn large_function() {\n    let x = 1;\n    let y = 2;\n    x + y\n}";
        let budget = standalone_cost(primary_code, "large_function", "", "src/a.rs") + 1;
        let config = RelationAnnotationConfig {
            max_annotated_length: budget,
            ..Default::default()
        };
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            primary_code.to_string(),
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

        // The primary fits the quota and is kept whole; the expansion unit
        // does not fit and must be reported through the omission marker
        assert!(result.contains("large_function"));
        assert!(result.contains("omitted"));
    }

    #[test]
    fn test_character_counting_vs_byte_counting() {
        let _config = RelationAnnotationConfig::default();
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

    fn expansion_unit(name: &str, path: &str, code: &str, score: f32) -> ExpandedUnit {
        ExpandedUnit::new(code.to_string(), path.to_string(), 1, 3, name.to_string())
            .with_expansion(ExpansionOrigin::Forward, "calls")
            .with_score(score)
    }

    /// Standalone selection cost mirror: fragment tags + code, matching the
    /// render shapes (an empty `rel` marks the primary shape, which keeps
    /// `name`; expansions keep `rel` with `location`).
    fn standalone_cost(code: &str, name: &str, rel: &str, path: &str) -> usize {
        let open = if rel.is_empty() {
            format!("<unit name=\"{name}\" location=\"{path}:1-3\">")
        } else {
            format!("<unit rel=\"{rel}\" location=\"{path}:1-3\">")
        };
        TokenEstimator::estimate(code)
            + 1
            + TokenEstimator::estimate(&open)
            + 1
            + TokenEstimator::estimate("</unit>")
            + 1
    }

    #[tokio::test]
    async fn test_missing_file_becomes_reference() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = RelationAnnotationConfig::new()
            .enable(true)
            .with_workspace_root(dir.path());
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            "fn ghost() {}".to_string(),
            "src/missing.rs".to_string(),
            1,
            3,
            "ghost".to_string(),
        );

        let (result, files) = concat.concatenate(&primary, &[], &[]).await;

        // The reference tag carries path and range as attributes; no body.
        assert!(
            result.contains(
                "<reference path=\"src/missing.rs\" lines=\"1-3\" reason=\"file_missing\""
            )
        );
        assert!(!result.contains("fn ghost"));
        assert_eq!(files.len(), 1);
    }

    #[tokio::test]
    async fn test_no_workspace_root_skips_existence_check() {
        let config = RelationAnnotationConfig::new().enable(true);
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            "fn ghost() {}".to_string(),
            "src/missing.rs".to_string(),
            1,
            3,
            "ghost".to_string(),
        );

        let (result, _) = concat.concatenate(&primary, &[], &[]).await;

        assert!(result.contains("fn ghost"));
        assert!(!result.contains("<reference"));
    }

    #[tokio::test]
    async fn test_oversized_segment_becomes_reference() {
        // The quota fits the tiny expansion plus the primary's downgraded
        // reference tag; the huge primary body must exceed it and degrade.
        let huge_code = "let value = compute();\n".repeat(200);
        let tiny_code = "fn tiny() {}";

        let tiny_cost = standalone_cost(tiny_code, "tiny", "calls", "src/tiny.rs");
        let primary_reference = reference_content(
            "src/huge.rs",
            1,
            200,
            TokenEstimator::estimate(&huge_code),
            DowngradeReason::OverLimit,
        );
        // Reference segments render no file tag in the single-file estimate;
        // selection counts the tag only.
        let primary_reference_cost = TokenEstimator::estimate(&primary_reference) + 1;
        let limit = tiny_cost + primary_reference_cost + 2;

        let config = RelationAnnotationConfig {
            max_annotated_length: limit,
            ..RelationAnnotationConfig::new().enable(true)
        };
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            huge_code,
            "src/huge.rs".to_string(),
            1,
            200,
            "huge".to_string(),
        );
        let tiny = expansion_unit("tiny", "src/tiny.rs", tiny_code, 0.9);

        let (result, _) = concat.concatenate(&primary, &[tiny], &[]).await;

        // The pinned primary degrades to a reference instead of crowding out
        // the small expansion.
        assert!(result.contains("<reference path=\"src/huge.rs\" lines=\"1-200\""));
        assert!(result.contains("reason=\"over_limit\""));
        assert!(!result.contains("let value = compute();"));
        assert!(result.contains("fn tiny() {}"));
    }

    #[tokio::test]
    async fn test_score_selection_prefers_high_score() {
        let primary_code = "fn main() {}";
        let high_code = "fn high() {\n    work();\n}";
        let low_code = "fn low() {\n    rest();\n}";

        let budget = standalone_cost(primary_code, "main", "", "src/main.rs")
            + standalone_cost(high_code, "high", "calls", "src/high.rs")
            + 5;
        let config = RelationAnnotationConfig {
            max_annotated_length: budget,
            ..RelationAnnotationConfig::new().enable(true)
        };
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            primary_code.to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );
        let high = expansion_unit("high", "src/high.rs", high_code, 0.9);
        let low = expansion_unit("low", "src/low.rs", low_code, 0.1);

        let (result, _) = concat.concatenate(&primary, &[low, high], &[]).await;

        // Arrival order is low-then-high; selection must follow score.
        assert!(result.contains("fn main"));
        assert!(result.contains("fn high"));
        assert!(!result.contains("fn low"));
        assert!(result.contains("omitted"));
    }

    #[tokio::test]
    async fn test_omitted_size_counts_markers() {
        let low_code = "fn low() {}";
        let budget = standalone_cost("fn main() {}", "main", "", "src/main.rs") + 1;
        let config = RelationAnnotationConfig {
            max_annotated_length: budget,
            ..RelationAnnotationConfig::new().enable(true)
        };
        let concat = StructureConcatenator::new(config);

        let primary = ExpandedUnit::new(
            "fn main() {}".to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );
        let low = expansion_unit("low", "src/low.rs", low_code, 0.1);

        let (result, _) = concat.concatenate(&primary, &[low], &[]).await;

        assert!(result.contains("omitted"));
        let start = result.find("tokens=\"~").expect("omitted magnitude");
        let tail = &result[start + "tokens=\"~".len()..];
        let end = tail.find('"').expect("magnitude end");
        let omitted: usize = tail[..end].trim().parse().expect("magnitude number");
        // The reported magnitude covers code plus fragment tags.
        assert!(omitted > TokenEstimator::estimate(low_code));
    }

    #[tokio::test]
    async fn test_relation_key_keeps_direction_only() {
        let concat = StructureConcatenator::new(RelationAnnotationConfig::new().enable(true));
        let primary = ExpandedUnit::new(
            "fn main() {}".to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );
        let typed = expansion_unit("new", "src/lib.rs", "pub const fn new() {}", 0.9)
            .with_relation_type(cce_types::RelationType::ConstructorCall);

        let (result, _) = concat.concatenate(&primary, &[typed], &[]).await;

        // Only the coarse direction renders; the fine-grained relation type
        // already shows in the evidence body.
        assert!(result.contains("<unit rel=\"calls\" location=\"src/lib.rs:1-3\">"));
        assert!(!result.contains("call.constructor"));
    }

    #[tokio::test]
    async fn test_omit_primary_body_keeps_expansions_only() {
        let config = RelationAnnotationConfig::new()
            .enable(true)
            .omit_primary_body(true);
        let concat = StructureConcatenator::new(config);
        let primary = ExpandedUnit::new(
            "fn main() {}".to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );
        let high = expansion_unit("high", "src/high.rs", "fn high() {}", 0.9);

        let (result, files) = concat.concatenate(&primary, &[high], &[]).await;

        assert!(!result.contains("fn main"));
        assert!(result.contains("fn high"));
        assert!(result.contains("<unit rel=\"calls\" location=\"src/high.rs:1-3\">"));
        assert_eq!(files.len(), 1);
    }

    #[tokio::test]
    async fn test_xml_style_flat_self_located_units() {
        let config = RelationAnnotationConfig::new().enable(true);
        let concat = StructureConcatenator::new(config);
        let primary = ExpandedUnit::new(
            "fn main() {}".to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );
        let high = expansion_unit("high", "src/high.rs", "fn high() {}", 0.9);

        let (result, _) = concat.concatenate(&primary, &[high], &[]).await;

        // Root layer wraps everything; every fragment carries its own
        // file and range in `location`, so no file-grouping layer exists.
        // Expansion fragments carry the edge direction in `rel`.
        assert!(result.starts_with("<annotation>\n"));
        assert!(!result.contains("<file"));
        assert!(result.contains("<unit name=\"main\" location=\"src/main.rs:1-1\">"));
        assert!(result.contains("<unit rel=\"calls\" location=\"src/high.rs:1-3\">"));
        assert!(result.contains("</unit>"));
        assert!(result.trim_end().ends_with("</annotation>"));
    }

    #[tokio::test]
    async fn test_xml_style_no_file_layer() {
        let config = RelationAnnotationConfig::new().enable(true);
        let concat = StructureConcatenator::new(config);
        let primary = ExpandedUnit::new(
            "fn main() {}".to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );

        let (result, _) = concat.concatenate(&primary, &[], &[]).await;

        // No file-grouping layer exists anymore: units always carry their
        // own location, whether or not files are shared.
        assert!(!result.contains("<file"));
        assert!(result.contains("<unit name=\"main\" location=\"src/main.rs:1-1\">"));
        assert!(result.contains("fn main() {}"));
    }

    #[tokio::test]
    async fn test_xml_style_reference_is_self_closing_tag() {
        // A workspace root enables the existence check so the vanished file
        // downgrades to a reference.
        let dir = tempfile::tempdir().expect("tempdir");
        let config = RelationAnnotationConfig::new()
            .enable(true)
            .with_workspace_root(dir.path());
        let concat = StructureConcatenator::new(config);
        let primary = ExpandedUnit::new(
            "fn ghost() {}".to_string(),
            "src/missing.rs".to_string(),
            1,
            3,
            "ghost".to_string(),
        );

        let (result, _) = concat.concatenate(&primary, &[], &[]).await;

        // Downgraded references encode location and reason as attributes
        // only; no comment-style reference line remains. The token magnitude
        // of the dropped body is non-zero here, so the tokens attribute
        // renders too.
        assert!(result.contains(
            "<reference path=\"src/missing.rs\" lines=\"1-3\" reason=\"file_missing\" tokens=\""
        ));
        assert!(result.contains("/>"));
        assert!(!result.contains("[reference]"));
        assert!(!result.contains("fn ghost"));
    }

    #[tokio::test]
    async fn test_xml_style_omitted_tag() {
        let primary_code = "fn main() {}";
        // Mirror of the XML cost keeps the budget tight so the
        // expansion is dropped and reported through the omission tag.
        let budget = TokenEstimator::estimate(primary_code)
            + 1
            + TokenEstimator::estimate("<unit name=\"main\" location=\"src/main.rs:1-1\">")
            + 1
            + TokenEstimator::estimate("</unit>")
            + 1;
        let config = RelationAnnotationConfig {
            max_annotated_length: budget,
            ..RelationAnnotationConfig::new().enable(true)
        };
        let concat = StructureConcatenator::new(config);
        let primary = ExpandedUnit::new(
            primary_code.to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );
        let low = expansion_unit("low", "src/low.rs", "fn low() {}", 0.1);

        let (result, _) = concat.concatenate(&primary, &[low], &[]).await;

        assert!(result.contains("<omitted units=\"1\""));
        assert!(!result.contains("// [omitted]"));
    }

    #[tokio::test]
    async fn test_xml_style_excerpt_attribute() {
        let config = RelationAnnotationConfig::new().enable(true);
        let concat = StructureConcatenator::new(config);
        let primary = ExpandedUnit::new(
            "fn main() {}".to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );
        let snippet =
            expansion_unit("snippet", "src/snip.rs", "fn snippet();", 0.9).with_excerpt(true);

        let (result, _) = concat.concatenate(&primary, &[snippet], &[]).await;

        // Windowed excerpts carry the excerpt state attribute; full bodies do not.
        assert!(result.contains("excerpt=\"true\""));
        assert!(
            !result.contains("<unit name=\"main\" location=\"src/main.rs:1-1\" excerpt=\"true\"")
        );
    }

    #[tokio::test]
    async fn test_xml_style_window_omits_excerpt_attribute() {
        let config = RelationAnnotationConfig::new().enable(true);
        let concat = StructureConcatenator::new(config);
        let primary = ExpandedUnit::new(
            "fn main() {}".to_string(),
            "src/main.rs".to_string(),
            1,
            1,
            "main".to_string(),
        );
        let window =
            expansion_unit("callee", "src/main.rs", "    callee();", 0.9).with_window(true);

        let (result, _) = concat.concatenate(&primary, &[window], &[]).await;

        // Callsite windows are slices by construction; the narrow location
        // and the edge direction identify them, so no excerpt attribute and
        // no neighbour name are emitted.
        assert!(!result.contains("excerpt="));
        assert!(result.contains("<unit rel=\"calls\" location=\"src/main.rs:1-3\">"));
    }
}
