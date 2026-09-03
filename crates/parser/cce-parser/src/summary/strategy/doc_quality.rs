//! Documentation comment quality assessment for summary generation
//!
//! Evaluates the quality and completeness of documentation comments
//! to adjust summary generation strategies accordingly.

use cce_utils::{clean_comment_markers, estimate_tokens};

/// Quality assessment result for a documentation comment
#[derive(Debug, Clone, Default)]
pub struct DocCommentQuality {
    /// Whether the doc is sufficiently detailed (token count > threshold)
    pub is_detailed: bool,
    /// Whether the doc contains parameter documentation
    pub has_param_docs: bool,
    /// Whether the doc contains return value documentation
    pub has_return_doc: bool,
    /// Whether the doc contains TODO/FIXME markers (indicates incomplete docs)
    pub has_todo_fixme: bool,
    /// Overall quality score (0.0 - 1.0) - incorporates examples, length, and structure
    pub quality_score: f32,
}

impl DocCommentQuality {
    /// Minimum token count to be considered "detailed"
    /// Using tokens instead of characters for language-agnostic evaluation
    const DETAILED_TOKEN_THRESHOLD: usize = 25;

    /// Evaluate the quality of a documentation comment
    ///
    /// This method:
    /// 1. Cleans comment markers from the raw doc string
    /// 2. Estimates token count for language-agnostic length evaluation
    /// 3. Checks for structured documentation patterns
    /// 4. Applies bonuses for valuable content (examples, params) and penalties for TODO/FIXME
    pub fn evaluate(doc: &str) -> Self {
        if doc.is_empty() {
            return Self::default();
        }

        // Step 1: Clean comment markers for accurate analysis
        let cleaned_doc = clean_comment_markers(doc);

        if cleaned_doc.is_empty() {
            return Self::default();
        }

        // Step 2: Use token estimation for language-agnostic length evaluation
        let length = estimate_tokens(&cleaned_doc);
        let is_detailed = length >= Self::DETAILED_TOKEN_THRESHOLD;

        // Check for parameter documentation
        let has_param_docs = cleaned_doc.contains("@param")
            || cleaned_doc.contains("@argument")
            || doc.contains("# Arguments")
            || cleaned_doc.contains("Args:")
            || cleaned_doc.contains("Parameters:")
            || doc.contains("# Parameters");

        // Check for return value documentation
        let has_return_doc = cleaned_doc.contains("@return")
            || cleaned_doc.contains("@returns")
            || doc.contains("# Returns")
            || cleaned_doc.contains("Returns:")
            || cleaned_doc.contains("-> ");

        // Check for examples (on cleaned text)
        let has_examples = cleaned_doc.contains("# Example")
            || cleaned_doc.contains("# Examples")
            || cleaned_doc.contains("```")
            || cleaned_doc.contains("@example");

        // Check for TODO/FIXME markers (indicates incomplete or needs improvement)
        let has_todo_fixme = cleaned_doc.contains("TODO")
            || cleaned_doc.contains("FIXME")
            || cleaned_doc.contains("todo")
            || cleaned_doc.contains("fixme")
            || cleaned_doc.contains("HACK")
            || cleaned_doc.contains("XXX");

        // Calculate quality score
        let mut score: f32 = 0.0;

        // Base score from token count (simplified: only one threshold needed)
        if is_detailed {
            score += 0.3; // Detailed enough to be useful
        } else if length > 5 {
            score += 0.1; // Very short but not empty
        }

        // Bonus for structured documentation (high value content)
        if has_param_docs {
            score += 0.25; // Parameters are very important for understanding
        }
        if has_return_doc {
            score += 0.2; // Return docs help understand behavior
        }
        if has_examples {
            score += 0.25; // Examples are extremely valuable for comprehension
        }

        // Penalty for TODO/FIXME (indicates incomplete documentation)
        if has_todo_fixme {
            score *= 0.7; // Reduce score by 30% if marked as incomplete
        }

        // Cap at 1.0
        let quality_score = score.min(1.0);

        Self {
            is_detailed,
            has_param_docs,
            has_return_doc,
            has_todo_fixme,
            quality_score,
        }
    }

    /// Check if the documentation is comprehensive enough to be used as primary source
    pub fn is_comprehensive(&self) -> bool {
        // Must be detailed and have structural docs (params or returns)
        // AND have decent quality score
        // BUT TODO/FIXME markers disqualify it from being comprehensive
        self.is_detailed
            && (self.has_param_docs || self.has_return_doc)
            && self.quality_score >= 0.5
            && !self.has_todo_fixme
    }
}

/// Calculate aggregate documentation quality for multiple entities
pub fn calculate_aggregate_quality(qualities: &[DocCommentQuality]) -> AggregateQuality {
    if qualities.is_empty() {
        return AggregateQuality::default();
    }

    let total = qualities.len();
    let well_documented = qualities.iter().filter(|q| q.quality_score >= 0.5).count();
    let comprehensive = qualities.iter().filter(|q| q.is_comprehensive()).count();

    let avg_score = qualities.iter().map(|q| q.quality_score).sum::<f32>() / total as f32;

    AggregateQuality {
        total_entities: total,
        well_documented_count: well_documented,
        comprehensive_count: comprehensive,
        average_score: avg_score,
        documentation_ratio: well_documented as f32 / total as f32,
    }
}

/// Aggregate quality statistics for a collection of documentation
#[derive(Debug, Clone, Default)]
pub struct AggregateQuality {
    /// Total number of public entities evaluated (used for logging and ratio calculation)
    pub total_entities: usize,
    /// Number of entities with quality score >= 0.5 (used for coverage calculation)
    pub well_documented_count: usize,
    /// Number of entities with comprehensive documentation (used to confirm high quality coverage)
    pub comprehensive_count: usize,
    /// Average quality score across all documented entities (used in enhancement decision)
    pub average_score: f32,
    /// Ratio of well-documented entities to total public entities (0.0 - 1.0) (used in enhancement decision)
    pub documentation_ratio: f32,
}

impl AggregateQuality {
    /// Check if documentation coverage is high enough to skip model enhancement
    pub fn should_skip_model_enhancement(&self) -> bool {
        // High coverage with reasonable score
        (self.documentation_ratio >= 0.6 && self.average_score >= 0.4)
            // Or every well-documented entity is comprehensive (detailed + structural docs)
            || (self.comprehensive_count > 0
                && self.comprehensive_count == self.well_documented_count
                && self.average_score >= 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_empty() {
        let quality = DocCommentQuality::evaluate("");
        assert_eq!(quality.quality_score, 0.0);
        assert!(!quality.is_detailed);
    }

    #[test]
    fn test_evaluate_simple() {
        let doc = "Returns the sum of two numbers.";
        let quality = DocCommentQuality::evaluate(doc);
        assert!(!quality.is_detailed);
        assert!(quality.quality_score > 0.0);
    }

    #[test]
    fn test_evaluate_detailed() {
        let doc = r#"Calculates the sum of two numbers with overflow checking.

This function performs addition while checking for potential overflow
conditions. If overflow would occur, it returns None instead.

# Arguments
* `a` - The first addend
* `b` - The second addend

# Returns
Some(a + b) if no overflow, None otherwise

# Example
```
let result = checked_add(5, 3);
assert_eq!(result, Some(8));
```
"#;
        let quality = DocCommentQuality::evaluate(doc);
        assert!(quality.is_detailed);
        assert!(quality.has_param_docs);
        assert!(quality.has_return_doc);
        // has_examples was removed - it's still used for score calculation but not stored
        assert!(!quality.has_todo_fixme);
        assert!(quality.quality_score >= 0.7); // High score with examples bonus
        assert!(quality.is_comprehensive());
    }

    #[test]
    fn test_evaluate_with_todo_fixme() {
        let doc = r#"Adds two numbers together.

TODO: Add overflow checking
FIXME: Handle edge cases

# Arguments
* `a` - First number
* `b` - Second number
"#;
        let quality = DocCommentQuality::evaluate(doc);
        assert!(quality.is_detailed);
        assert!(quality.has_param_docs);
        assert!(quality.has_todo_fixme);
        // Score should be penalized due to TODO/FIXME
        assert!(quality.quality_score < 0.6);
        // Should not be comprehensive due to TODO/FIXME
        assert!(!quality.is_comprehensive());
    }

    #[test]
    fn test_evaluate_with_example_only() {
        let doc = r#"Example usage:
```
let x = my_function(42);
```
"#;
        let quality = DocCommentQuality::evaluate(doc);
        assert!(!quality.is_detailed); // Too short
        // has_examples assertion removed - examples still used for score calculation
        // Examples give good bonus even for short docs
        assert!(quality.quality_score > 0.15);
    }

    #[test]
    fn test_calculate_aggregate_quality() {
        let qualities = vec![
            DocCommentQuality {
                quality_score: 0.8,
                ..Default::default()
            },
            DocCommentQuality {
                quality_score: 0.6,
                ..Default::default()
            },
            DocCommentQuality {
                quality_score: 0.3,
                ..Default::default()
            },
            DocCommentQuality {
                quality_score: 0.7,
                ..Default::default()
            },
        ];

        let aggregate = calculate_aggregate_quality(&qualities);
        // total_entities should reflect the number of documented entities passed in
        assert_eq!(aggregate.total_entities, 4);
        // well_documented_count counts entities with score >= 0.5
        assert_eq!(aggregate.well_documented_count, 3);
        assert!((aggregate.average_score - 0.6).abs() < 0.01);
        // When calculated directly from documented entities, ratio is well_documented/total
        assert!((aggregate.documentation_ratio - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_aggregate_should_skip_model() {
        let qualities = vec![
            DocCommentQuality {
                quality_score: 0.7,
                ..Default::default()
            },
            DocCommentQuality {
                quality_score: 0.6,
                ..Default::default()
            },
            DocCommentQuality {
                quality_score: 0.5,
                ..Default::default()
            },
        ];

        let aggregate = calculate_aggregate_quality(&qualities);
        assert!(aggregate.should_skip_model_enhancement());
    }
}
