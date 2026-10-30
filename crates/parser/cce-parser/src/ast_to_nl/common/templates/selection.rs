//! Template selection shared by the BM25 and Embedding dispatchers.
//!
//! Both dispatchers route a group to one of three templates and both expose
//! that choice as a name. The rule lives here, once, so the name cannot drift
//! away from what `dispatch` actually runs — keeping two copies of the rule is
//! how the label and the executed branch start disagreeing.

use crate::grouper::types::EntityGroup;
use crate::grouper::types::GetterSetterSummary;

/// The template family that handles a group.
#[derive(Debug, Clone, Copy)]
pub enum GroupTemplateKind<'a> {
    /// A getter/setter pattern was detected; the pattern template renders it
    /// and carries the accessor summary.
    GetterSetter(&'a GetterSetterSummary),
    /// The group header is a standard library symbol; the stdlib template
    /// compresses it to a category and a name.
    Stdlib,
    /// Everything else.
    Regular,
}

impl<'a> GroupTemplateKind<'a> {
    /// Route a group the way every dispatcher must route it.
    ///
    /// A detected pattern outranks the stdlib check: a getter/setter class is
    /// still a class, and the pattern template carries the accessor summary
    /// that the stdlib template would drop.
    pub fn select(group: &'a EntityGroup) -> Self {
        if let Some(summary) = group.pattern_info.as_getter_setter() {
            return Self::GetterSetter(summary);
        }
        if group.header.as_ref().is_some_and(|h| h.is_stdlib) {
            return Self::Stdlib;
        }
        Self::Regular
    }

    /// Stable label for logs, metrics and tests.
    ///
    /// These labels name templates, not design patterns: `Stdlib` and
    /// `Regular` are never `PatternInfo` variants. Pattern identity stays on
    /// `PatternInfo::pattern_name`, where `None` is a legal answer.
    pub fn name(&self) -> &'static str {
        match self {
            Self::GetterSetter(_) => "GetterSetter",
            Self::Stdlib => "Stdlib",
            Self::Regular => "Regular",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::types::PatternInfo;

    fn group(pattern: PatternInfo, is_stdlib: bool) -> EntityGroup {
        EntityGroup {
            pattern_info: pattern,
            header: Some(cce_types::GroupedEntity {
                is_stdlib,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn pattern_outranks_stdlib() {
        let group = group(
            PatternInfo::GetterSetter(GetterSetterSummary::empty()),
            true,
        );
        assert_eq!(GroupTemplateKind::select(&group).name(), "GetterSetter");
    }

    #[test]
    fn stdlib_header_beats_regular() {
        let stdlib = group(PatternInfo::None, true);
        assert_eq!(GroupTemplateKind::select(&stdlib).name(), "Stdlib");

        let plain = group(PatternInfo::None, false);
        assert_eq!(GroupTemplateKind::select(&plain).name(), "Regular");
    }

    #[test]
    fn missing_header_is_regular() {
        let group = EntityGroup::default();
        assert_eq!(GroupTemplateKind::select(&group).name(), "Regular");
    }
}
