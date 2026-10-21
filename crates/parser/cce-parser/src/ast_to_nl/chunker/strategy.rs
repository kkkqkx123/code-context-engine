use crate::grouper::GroupType;

/// Split strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitStrategy {
    /// Split at sentence boundaries
    BySentences,
    /// Split at paragraph boundaries
    ByParagraphs,
    /// Split at line boundaries
    ByLines,
    /// Force token-level split
    ByTokens,
    /// Split at nested group boundaries
    ByNestedGroups,
    /// Split at entity boundaries using pre-computed NL-text-relative offsets.
    ///
    /// The offsets come from the conversion layer, which knows which text
    /// each entity contributed. Without them the level has no data and the
    /// recursion descends to a coarser source.
    ByNlEntityBoundaries,
}

impl SplitStrategy {
    /// Split strategy for the BM25 path.
    ///
    /// Declaration boundaries first; oversized declarations fall back to
    /// line boundaries before the hard token cut so method signatures stay
    /// intact. Sentence and paragraph fallbacks would slice module text
    /// into topic-less fragments and dilute rare terms.
    pub fn for_bm25_group_type(group_type: GroupType) -> Self {
        match group_type {
            GroupType::ClassWithNestedClasses => SplitStrategy::ByNestedGroups,
            GroupType::StructWithNestedStructs => SplitStrategy::ByNestedGroups,
            _ => SplitStrategy::ByNlEntityBoundaries,
        }
    }

    /// Get default strategy for group type
    pub fn for_group_type(group_type: GroupType) -> Self {
        match group_type {
            GroupType::ClassWithMethods => SplitStrategy::ByNlEntityBoundaries,
            GroupType::RelatedFunctions => SplitStrategy::ByNlEntityBoundaries,
            GroupType::Standalone => SplitStrategy::ByParagraphs,
            GroupType::InterfaceWithImpls => SplitStrategy::ByNlEntityBoundaries,
            GroupType::TraitWithImpls => SplitStrategy::ByNlEntityBoundaries,
            GroupType::ModuleWithContents => SplitStrategy::ByNlEntityBoundaries,
            GroupType::TestSuiteWithCases => SplitStrategy::ByNlEntityBoundaries,
            GroupType::CompositePattern => SplitStrategy::ByNlEntityBoundaries,
            GroupType::ClassWithNestedClasses => SplitStrategy::ByNestedGroups,
            GroupType::StructWithNestedStructs => SplitStrategy::ByNestedGroups,
            GroupType::FunctionWithLogicalBlocks => SplitStrategy::BySentences,
            GroupType::FunctionWithMembers => SplitStrategy::ByNlEntityBoundaries,
            GroupType::MergedFragments => SplitStrategy::ByParagraphs,
            GroupType::FileDocumentation => SplitStrategy::ByParagraphs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_for_group_type() {
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::ClassWithMethods),
            SplitStrategy::ByNlEntityBoundaries
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::Standalone),
            SplitStrategy::ByParagraphs
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::RelatedFunctions),
            SplitStrategy::ByNlEntityBoundaries
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::InterfaceWithImpls),
            SplitStrategy::ByNlEntityBoundaries
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::ClassWithNestedClasses),
            SplitStrategy::ByNestedGroups
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::StructWithNestedStructs),
            SplitStrategy::ByNestedGroups
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::FunctionWithLogicalBlocks),
            SplitStrategy::BySentences
        );
        assert_eq!(
            SplitStrategy::for_bm25_group_type(GroupType::FunctionWithLogicalBlocks),
            SplitStrategy::ByNlEntityBoundaries
        );
        assert_eq!(
            SplitStrategy::for_bm25_group_type(GroupType::Standalone),
            SplitStrategy::ByNlEntityBoundaries
        );
        assert_eq!(
            SplitStrategy::for_bm25_group_type(GroupType::MergedFragments),
            SplitStrategy::ByNlEntityBoundaries
        );
        assert_eq!(
            SplitStrategy::for_bm25_group_type(GroupType::ClassWithNestedClasses),
            SplitStrategy::ByNestedGroups
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::MergedFragments),
            SplitStrategy::ByParagraphs
        );
    }
}
