use super::group_trait::GroupTemplate;
use crate::ast_to_nl::common::GroupTemplateBase;
use crate::ast_to_nl::common::filter_nl_noise;
use crate::ast_to_nl::noise::NoiseProfile;
use crate::grouper::types::EntityGroup;

pub struct StdlibTemplate;

impl StdlibTemplate {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StdlibTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupTemplateBase for StdlibTemplate {}

impl GroupTemplate for StdlibTemplate {
    fn generate(&self, group: &EntityGroup) -> String {
        let category = group
            .header
            .as_ref()
            .and_then(|h| h.stdlib_category)
            .map(|c| c.to_string())
            .unwrap_or_else(|| "stdlib".to_string());

        // Concise output: category + name only, avoiding verbose "standard library"
        // phrasing that dilutes BM25 keyword density.
        let mut result = format!("{} {}", category, group.name);
        if let Some(ref header) = group.header {
            if let Some(ref doc) = header.doc_comment {
                // Language-gated doc filtering mirrors the embedding path and
                // the regular BM25 template: generic empty markers always drop,
                // per-language boilerplate drops only for its language.
                let profile = NoiseProfile::for_language(group.language);
                let cleaned = Self::clean_doc_text(doc);
                let filtered = filter_nl_noise(&cleaned, profile);
                if !filtered.trim().is_empty() {
                    result.push(' ');
                    result.push_str(filtered.trim());
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    #[test]
    fn test_stdlib_filters_empty_markers_per_profile() {
        let group = EntityGroup {
            name: "Cell".into(),
            kind: cce_types::entity::EntityKind::Struct,
            language: Language::Rust,
            header: Some(cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "Cell".to_string(),
                kind: cce_types::entity::EntityKind::Struct,
                doc_comment: Some("# Example.\nSets the contents.".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let text = StdlibTemplate::new().generate(&group);
        assert!(!text.contains("# Example"), "got: {text}");
        assert!(text.contains("Sets the contents"), "got: {text}");
    }

    #[test]
    fn test_stdlib_keeps_safety_boilerplate_for_non_rust() {
        let group = EntityGroup {
            name: "Cell".into(),
            kind: cce_types::entity::EntityKind::Class,
            language: Language::Python,
            header: Some(cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "Cell".to_string(),
                kind: cce_types::entity::EntityKind::Class,
                doc_comment: Some("Safe due to batching.\nSets the contents.".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let text = StdlibTemplate::new().generate(&group);
        assert!(text.contains("Safe due to"), "got: {text}");
    }
}
