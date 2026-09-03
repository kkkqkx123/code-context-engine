use serde::{Deserialize, Serialize};

/// Entity metadata for Post-Processor association
///
/// This is a lightweight summary extracted from `Entity` for use in query results.
/// It avoids duplicating the full entity data while providing essential information
/// for display and filtering.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityMetadata {
    pub entity_names: Vec<String>,
    pub entity_kinds: Vec<String>,
    pub is_combined: bool,
    pub parent_entity: Option<String>,
}

impl EntityMetadata {
    pub fn single(name: String, kind: String) -> Self {
        Self {
            entity_names: vec![name],
            entity_kinds: vec![kind],
            is_combined: false,
            parent_entity: None,
        }
    }

    pub fn combined(class_name: String, method_names: Vec<String>) -> Self {
        let total_count = method_names.len() + 1;
        let mut names = vec![class_name];
        names.extend(method_names);

        Self {
            entity_names: names,
            entity_kinds: vec!["Class".to_string(); total_count],
            is_combined: true,
            parent_entity: None,
        }
    }

    pub fn with_parent(mut self, parent: String) -> Self {
        self.parent_entity = Some(parent);
        self
    }

    pub fn primary_name(&self) -> Option<&str> {
        self.entity_names.first().map(|s| s.as_str())
    }

    pub fn primary_kind(&self) -> Option<&str> {
        self.entity_kinds.first().map(|s| s.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.entity_names.is_empty()
    }

    pub fn entity_count(&self) -> usize {
        self.entity_names.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_entity() {
        let meta = EntityMetadata::single("my_function".to_string(), "Function".to_string());
        assert!(!meta.is_combined);
        assert_eq!(meta.primary_name(), Some("my_function"));
        assert_eq!(meta.primary_kind(), Some("Function"));
        assert_eq!(meta.entity_count(), 1);
    }

    #[test]
    fn test_combined_entity() {
        let meta = EntityMetadata::combined(
            "MyClass".to_string(),
            vec!["method1".to_string(), "method2".to_string()],
        );
        assert!(meta.is_combined);
        assert_eq!(meta.entity_count(), 3);
    }

    #[test]
    fn test_with_parent() {
        let meta = EntityMetadata::single("method".to_string(), "Method".to_string())
            .with_parent("MyClass".to_string());
        assert_eq!(meta.parent_entity, Some("MyClass".to_string()));
    }

    #[test]
    fn test_is_empty() {
        let meta = EntityMetadata::default();
        assert!(meta.is_empty());
    }

    #[test]
    fn test_primary() {
        let meta = EntityMetadata::single("foo".to_string(), "Function".to_string());
        assert_eq!(meta.primary_name(), Some("foo"));
        assert_eq!(meta.primary_kind(), Some("Function"));
    }

    #[test]
    fn test_empty_primary() {
        let meta = EntityMetadata::default();
        assert_eq!(meta.primary_name(), None);
        assert_eq!(meta.primary_kind(), None);
    }
}
