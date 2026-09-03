use crate::grouper::GroupType;

/// Split strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitStrategy {
    /// Split at member boundaries (uses group.entity_spans — source code offsets)
    ByMembers,
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
    ByNlEntityBoundaries,
}

impl SplitStrategy {
    /// Get default strategy for group type
    pub fn for_group_type(group_type: GroupType) -> Self {
        match group_type {
            GroupType::ClassWithMethods => SplitStrategy::ByMembers,
            GroupType::RelatedFunctions => SplitStrategy::ByMembers,
            GroupType::Standalone => SplitStrategy::ByParagraphs,
            GroupType::InterfaceWithImpls => SplitStrategy::ByMembers,
            GroupType::TraitWithImpls => SplitStrategy::ByMembers,
            GroupType::ModuleWithContents => SplitStrategy::ByMembers,
            GroupType::TestSuiteWithCases => SplitStrategy::ByMembers,
            GroupType::CompositePattern => SplitStrategy::ByMembers,
            GroupType::ClassWithNestedClasses => SplitStrategy::ByNestedGroups,
            GroupType::StructWithNestedStructs => SplitStrategy::ByNestedGroups,
            GroupType::FunctionWithLogicalBlocks => SplitStrategy::BySentences,
            GroupType::FunctionWithMembers => SplitStrategy::ByMembers,
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
            SplitStrategy::ByMembers
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::Standalone),
            SplitStrategy::ByParagraphs
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::RelatedFunctions),
            SplitStrategy::ByMembers
        );
        assert_eq!(
            SplitStrategy::for_group_type(GroupType::InterfaceWithImpls),
            SplitStrategy::ByMembers
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
            SplitStrategy::for_group_type(GroupType::MergedFragments),
            SplitStrategy::ByParagraphs
        );
    }
}
