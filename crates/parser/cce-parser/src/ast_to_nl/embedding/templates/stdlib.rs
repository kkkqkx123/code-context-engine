use super::group_trait::GroupTemplate;
use crate::ast_to_nl::common::GroupTemplateBase;
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
    fn generate(&self, group: &EntityGroup) -> Vec<String> {
        let category = group
            .header
            .as_ref()
            .and_then(|h| h.stdlib_category)
            .map(|c| c.to_string())
            .unwrap_or_else(|| "standard library".to_string());

        // Output a concise, searchable description that preserves the identifier
        // and category without verbose "entity X from the standard library" phrasing.
        let mut desc = format!("{} {}", category, group.name);
        if let Some(ref header) = group.header {
            if let Some(ref doc) = header.doc_comment {
                // Clean doc text (strip fenced code blocks, markdown noise) the
                // same way regular templates do, so raw examples don't leak into
                // the embedding path.
                let profile = NoiseProfile::for_language(group.language);
                let cleaned = Self::clean_doc_text(doc);
                let cleaned =
                    crate::ast_to_nl::embedding::filter_embedding_noise(&cleaned, profile);
                let cleaned = cleaned.trim();
                if !cleaned.is_empty() {
                    desc.push_str(". ");
                    desc.push_str(cleaned);
                }
            }
        }
        vec![desc]
    }
}
