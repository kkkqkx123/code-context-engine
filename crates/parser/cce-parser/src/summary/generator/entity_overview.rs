//! Entity overview formatting for file summaries
//!
//! Builds a compact, category-first overview from parsed entities so summary
//! text can preserve both the entity names and their semantic kinds.

use std::collections::HashSet;

use cce_types::{Entity, EntityKind};

fn strip_generics(name: &str) -> &str {
    name.split('<').next().unwrap_or(name)
}

fn normalize_summary_entity_name(name: &str) -> Option<String> {
    let cleaned = strip_generics(name).trim();
    if cleaned.is_empty() {
        return None;
    }

    Some(cleaned.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SummarySection {
    Modules,
    Types,
    Implementations,
    Functions,
    Macros,
    Annotations,
    Variables,
    Tests,
    Markup,
    Styles,
    Other,
}

impl SummarySection {
    const ORDER: [Self; 11] = [
        Self::Modules,
        Self::Types,
        Self::Implementations,
        Self::Functions,
        Self::Macros,
        Self::Annotations,
        Self::Variables,
        Self::Tests,
        Self::Markup,
        Self::Styles,
        Self::Other,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Modules => 0,
            Self::Types => 1,
            Self::Implementations => 2,
            Self::Functions => 3,
            Self::Macros => 4,
            Self::Annotations => 5,
            Self::Variables => 6,
            Self::Tests => 7,
            Self::Markup => 8,
            Self::Styles => 9,
            Self::Other => 10,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Modules => "Modules",
            Self::Types => "Types",
            Self::Implementations => "Implementations",
            Self::Functions => "Functions",
            Self::Macros => "Macros",
            Self::Annotations => "Annotations",
            Self::Variables => "Variables",
            Self::Tests => "Tests",
            Self::Markup => "Markup",
            Self::Styles => "Styles",
            Self::Other => "Other",
        }
    }

    const fn from_kind(kind: EntityKind) -> Option<Self> {
        match kind {
            EntityKind::Unknown => None,
            EntityKind::Module
            | EntityKind::Namespace
            | EntityKind::Package
            | EntityKind::Import
            | EntityKind::Export
            | EntityKind::Require
            | EntityKind::Include => Some(Self::Modules),
            EntityKind::Class
            | EntityKind::Struct
            | EntityKind::Enum
            | EntityKind::Interface
            | EntityKind::Trait
            | EntityKind::TypeAlias
            | EntityKind::Union
            | EntityKind::EnumVariant => Some(Self::Types),
            EntityKind::TraitImpl | EntityKind::InherentImpl => Some(Self::Implementations),
            EntityKind::Function
            | EntityKind::Method
            | EntityKind::Constructor
            | EntityKind::Destructor
            | EntityKind::Operator => Some(Self::Functions),
            EntityKind::Macro => Some(Self::Macros),
            EntityKind::Annotation => Some(Self::Annotations),
            EntityKind::Field
            | EntityKind::Property
            | EntityKind::Variable
            | EntityKind::Constant => Some(Self::Variables),
            EntityKind::TestSuite
            | EntityKind::TestCase
            | EntityKind::TestHook
            | EntityKind::Assertion
            | EntityKind::Mock => Some(Self::Tests),
            EntityKind::Element
            | EntityKind::Attribute
            | EntityKind::Expression
            | EntityKind::Component
            | EntityKind::Template
            | EntityKind::Directive
            | EntityKind::ControlFlow
            | EntityKind::Animation
            | EntityKind::Binding
            | EntityKind::Action
            | EntityKind::EventHandler
            | EntityKind::ScriptContent
            | EntityKind::StyleContent
            | EntityKind::EmbeddedBlock => Some(Self::Markup),
            EntityKind::StyleRule
            | EntityKind::StyleSelector
            | EntityKind::StyleProperty
            | EntityKind::Keyframe
            | EntityKind::AtRule => Some(Self::Styles),
        }
    }
}

#[derive(Debug, Default)]
struct SectionItems {
    items: Vec<String>,
    seen: HashSet<String>,
}

impl SectionItems {
    fn push(&mut self, item: String) {
        if self.seen.insert(item.clone()) {
            self.items.push(item);
        }
    }
}

/// Build a category-first entity overview string.
///
/// The output preserves item order within each category and keeps the
/// category order stable so summaries are easier to scan.
pub(crate) fn format_entity_overview(entities: &[Entity]) -> Option<String> {
    let mut sections: [SectionItems; 11] = std::array::from_fn(|_| SectionItems::default());

    for entity in entities {
        let Some(section) = SummarySection::from_kind(entity.kind) else {
            continue;
        };

        let Some(name) = normalize_summary_entity_name(&entity.name) else {
            continue;
        };

        sections[section.index()].push(name);
    }

    let mut parts = Vec::new();
    for section in SummarySection::ORDER {
        let items = &sections[section.index()].items;
        if !items.is_empty() {
            parts.push(format!("{}: {}", section.label(), items.join(", ")));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" | "))
    }
}

/// Build a compact entity-kind statistics line (e.g. "3 types, 12 functions").
///
/// Only non-zero categories are listed, keeping the line short enough for the
/// file-level summary. Returns `None` when no entity kind is recognized.
pub(crate) fn format_entity_stats(entities: &[Entity]) -> Option<String> {
    let mut types = 0usize;
    let mut functions = 0usize;
    let mut modules = 0usize;
    let mut implementations = 0usize;
    let mut macros = 0usize;
    let mut variables = 0usize;
    let mut tests = 0usize;
    let mut other = 0usize;

    for entity in entities {
        match SummarySection::from_kind(entity.kind) {
            Some(SummarySection::Types) => types += 1,
            Some(SummarySection::Functions) => functions += 1,
            Some(SummarySection::Modules) => modules += 1,
            Some(SummarySection::Implementations) => implementations += 1,
            Some(SummarySection::Macros) => macros += 1,
            Some(SummarySection::Variables) => variables += 1,
            Some(SummarySection::Tests) => tests += 1,
            Some(SummarySection::Markup | SummarySection::Styles | SummarySection::Annotations) => {
                other += 1
            }
            Some(SummarySection::Other) => other += 1,
            None => {}
        }
    }

    let mut parts = Vec::new();
    for (label, count) in [
        ("types", types),
        ("functions", functions),
        ("modules", modules),
        ("implementations", implementations),
        ("macros", macros),
        ("variables", variables),
        ("tests", tests),
        ("other entities", other),
    ] {
        if count > 0 {
            parts.push(format!("{count} {label}"));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{Entity, EntityId, Language, ParsedFile};

    fn entity(kind: EntityKind, name: &str) -> Entity {
        let mut file = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), "");
        file.add_entity(Entity::new(
            EntityId(0),
            kind,
            name.to_string(),
            cce_types::Span::from_lines(1, 1),
        ));
        file.entities.remove(0)
    }

    #[test]
    fn test_format_entity_stats_counts_by_category() {
        let entities = vec![
            entity(EntityKind::Struct, "Foo"),
            entity(EntityKind::Class, "Bar"),
            entity(EntityKind::Function, "run"),
            entity(EntityKind::Method, "helper"),
            entity(EntityKind::Module, "core"),
            entity(EntityKind::Import, "use std"),
        ];

        let stats = format_entity_stats(&entities).expect("stats present");

        assert!(stats.contains("2 types"), "got: {stats}");
        assert!(stats.contains("2 functions"), "got: {stats}");
        assert!(stats.contains("2 modules"), "got: {stats}");
        assert!(!stats.contains("tests"), "got: {stats}");
    }

    #[test]
    fn test_format_entity_stats_empty() {
        assert!(format_entity_stats(&[]).is_none());
    }
}
