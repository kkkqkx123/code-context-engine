use cce_types::ParsedFile;

use crate::ast_to_nl::CodeFormGroup;

impl super::RuleBasedGenerator {
    /// Extract imports from code forms, using the cached import_table.
    pub(crate) fn extract_imports_from_code_forms(
        &self,
        _code_forms: &[CodeFormGroup],
        parsed_file: &ParsedFile,
    ) -> Vec<String> {
        self.extract_imports(parsed_file)
    }

    /// Extract imports from the file
    ///
    /// Delegates to the shared collector (cached `import_table` with AST
    /// fallback) so every summary generator collects the same full import set.
    pub(crate) fn extract_imports(&self, parsed_file: &ParsedFile) -> Vec<String> {
        let mut imports = crate::summary::dependencies::collect_imports(parsed_file);
        imports.truncate(self.config.max_imports);
        imports
    }

    /// Extract exports from the file
    pub(crate) fn extract_exports(&self, parsed_file: &ParsedFile) -> Vec<String> {
        let mut exports = crate::summary::dependencies::collect_exports(parsed_file);
        exports.truncate(self.config.max_imports);
        exports
    }
}
