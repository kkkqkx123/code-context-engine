use cce_types::ParsedFile;

use crate::grouper::{GroupType, ProcessingResult};

impl super::RuleBasedGenerator {
    /// Generate tags using group information
    pub(crate) fn generate_tags_with_groups(
        &self,
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
    ) -> Vec<String> {
        let mut tags = self.generate_tags(parsed_file);

        // Add tags based on group types
        let has_class_with_methods = processing_result
            .groups
            .iter()
            .any(|g| g.group_type == GroupType::ClassWithMethods);
        if has_class_with_methods && !tags.contains(&"class".to_string()) {
            tags.push("class".to_string());
        }

        let has_trait = processing_result
            .groups
            .iter()
            .any(|g| g.group_type == GroupType::TraitWithImpls);
        if has_trait && !tags.contains(&"trait".to_string()) {
            tags.push("trait".to_string());
        }

        let has_interface = processing_result
            .groups
            .iter()
            .any(|g| g.group_type == GroupType::InterfaceWithImpls);
        if has_interface && !tags.contains(&"interface".to_string()) {
            tags.push("interface".to_string());
        }

        // Check for merged calls
        if processing_result.stats.merged_calls > 0 {
            tags.push("repetitive-patterns".to_string());
        }

        tags.sort();
        tags.dedup();

        tags
    }

    /// Generate tags based on file analysis
    ///
    /// Only reliable signals are used: path rules (test/config/documentation)
    /// and AST metadata (`is_async`). Name/substring heuristics (web,
    /// controller, database) were removed as they misclassify unrelated files.
    pub(crate) fn generate_tags(&self, parsed_file: &ParsedFile) -> Vec<String> {
        use crate::summary::strategy::categorization::{
            is_config_file, is_documentation, is_test_file,
        };

        let mut tags = Vec::new();

        // Check for test files
        if is_test_file(&parsed_file.path) {
            tags.push("test".to_string());
        }

        // Check for configuration files
        if is_config_file(&parsed_file.path) {
            tags.push("config".to_string());
        }

        // Check for documentation
        if is_documentation(&parsed_file.path) {
            tags.push("documentation".to_string());
        }

        // Check for async code in signature
        let has_async = parsed_file.entities.iter().any(|e| {
            e.signature.contains("async ")
                || e.metadata
                    .get("is_async")
                    .map(|v| v == "true")
                    .unwrap_or(false)
        });
        if has_async {
            tags.push("async".to_string());
        }

        tags.sort();
        tags.dedup();

        tags
    }
}
