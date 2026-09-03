//! Single entity conversion methods for AST to Natural Language conversion

use crate::ast_to_nl::ConversionRequest;
use cce_types::{ConversionResult, GroupedEntity, OutputMode};

impl super::AstToNlConverter {
    /// Convert a GroupedEntity to natural language based on output mode
    pub fn convert_grouped(
        &self,
        entity: &GroupedEntity,
        file_path: &str,
        request: Option<&ConversionRequest>,
    ) -> ConversionResult {
        match self.resolve_mode(request) {
            OutputMode::Bm25 => self.convert_bm25_grouped(entity, file_path),
            OutputMode::Embedding => self.convert_embedding_grouped(entity, file_path),
            OutputMode::Both => self.convert_both_grouped(entity, file_path),
        }
    }

    fn convert_bm25_grouped(&self, entity: &GroupedEntity, file_path: &str) -> ConversionResult {
        let bm25_text = self.bm25_generator.generate(entity);
        let keywords = self.bm25_generator.extract_keywords(entity);

        ConversionResult::bm25_only(
            entity.id,
            entity.kind,
            entity.name.clone(),
            file_path.to_string(),
            bm25_text,
            keywords,
        )
    }

    fn convert_embedding_grouped(
        &self,
        entity: &GroupedEntity,
        file_path: &str,
    ) -> ConversionResult {
        let embedding_text = self.embedding_generator.generate(entity);

        ConversionResult::embedding_only(
            entity.id,
            entity.kind,
            entity.name.clone(),
            file_path.to_string(),
            embedding_text,
        )
    }

    fn convert_both_grouped(&self, entity: &GroupedEntity, file_path: &str) -> ConversionResult {
        let bm25_text = self.bm25_generator.generate(entity);
        let embedding_text = self.embedding_generator.generate(entity);
        let keywords = self.bm25_generator.extract_keywords(entity);

        ConversionResult::new(
            entity.id,
            entity.kind,
            entity.name.clone(),
            file_path.to_string(),
            bm25_text,
            embedding_text,
            keywords,
        )
    }
}
