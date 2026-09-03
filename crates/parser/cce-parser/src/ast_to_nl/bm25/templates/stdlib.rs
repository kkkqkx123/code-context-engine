use super::group_trait::GroupTemplate;
use crate::ast_to_nl::common::GroupTemplateBase;
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
                result.push(' ');
                result.push_str(doc);
            }
        }
        result
    }
}
