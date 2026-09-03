//! Index schema definition for BM25 search

use std::collections::HashMap;
use tantivy::schema::{
    Field, IndexRecordOption, NumericOptions, STORED, STRING, Schema, TantivyDocument,
    TextFieldIndexing, TextOptions,
};

use crate::Bm25Error;

/// Index schema for BM25 documents
#[derive(Debug, Clone)]
pub struct IndexSchema {
    pub document_id: Field,
    pub title: Field,
    pub content: Field,
    pub keywords: Field,
    pub chunk_id: Field,
    pub file_path: Field,
    pub project_id: Field,
    pub epoch: Field,
    pub entity_id: Field,
    /// Segment ID for hybrid fusion alignment of document/plain-text chunks.
    /// Two chunks from the same logical segment share the same segment_id,
    /// enabling BM25 ↔ vector matching when no entity is available.
    pub segment_id: Field,
    pub test: Field,
    pub category: Field,
    schema: Schema,
}

impl IndexSchema {
    // 13-field canonical schema: keep the flat tuple (mirrors the index)
    #[allow(clippy::type_complexity)]
    fn build_canonical() -> (
        Schema,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
        Field,
    ) {
        let mut schema_builder = Schema::builder();
        let document_id = schema_builder.add_text_field("document_id", STRING | STORED);
        let title_text_indexing = TextFieldIndexing::default()
            .set_tokenizer("mixed")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let title = schema_builder.add_text_field(
            "title",
            TextOptions::default()
                .set_indexing_options(title_text_indexing)
                .set_stored(),
        );
        let content_text_indexing = TextFieldIndexing::default()
            .set_tokenizer("mixed")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let content = schema_builder.add_text_field(
            "content",
            TextOptions::default().set_indexing_options(content_text_indexing),
        );
        let keywords_text_indexing = TextFieldIndexing::default()
            .set_tokenizer("mixed")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let keywords = schema_builder.add_text_field(
            "keywords",
            TextOptions::default().set_indexing_options(keywords_text_indexing),
        );
        let chunk_id = schema_builder.add_text_field("chunk_id", STRING | STORED);
        let file_path = schema_builder.add_text_field("file_path", STRING | STORED);
        let project_id = schema_builder.add_text_field("project_id", STRING | STORED);
        let epoch = schema_builder.add_i64_field(
            "epoch",
            NumericOptions::default().set_stored().set_indexed(),
        );
        let entity_id =
            schema_builder.add_text_field("entity_id", TextOptions::default().set_stored());
        let segment_id =
            schema_builder.add_text_field("segment_id", TextOptions::default().set_stored());
        let test = schema_builder.add_u64_field(
            "test",
            NumericOptions::default()
                .set_stored()
                .set_indexed()
                .set_fast(),
        );
        let category = schema_builder.add_u64_field(
            "category",
            NumericOptions::default()
                .set_stored()
                .set_indexed()
                .set_fast(),
        );
        let schema = schema_builder.build();

        (
            schema,
            document_id,
            title,
            content,
            keywords,
            chunk_id,
            file_path,
            project_id,
            epoch,
            entity_id,
            segment_id,
            test,
            category,
        )
    }

    fn resolve_field(tantivy_schema: &Schema, name: &str) -> Result<Field, Bm25Error> {
        tantivy_schema
            .fields()
            .find(|(f, _)| tantivy_schema.get_field_name(*f) == name)
            .map(|(f, _)| f)
            .ok_or_else(|| {
                Bm25Error::Schema(format!(
                    "Field '{}' not found in BM25 index schema. The index may be from an older, incompatible version.",
                    name
                ))
            })
    }

    pub fn new() -> Self {
        let (
            schema,
            document_id,
            title,
            content,
            keywords,
            chunk_id,
            file_path,
            project_id,
            epoch,
            entity_id,
            segment_id,
            test,
            category,
        ) = Self::build_canonical();

        IndexSchema {
            document_id,
            title,
            content,
            keywords,
            chunk_id,
            file_path,
            project_id,
            epoch,
            entity_id,
            segment_id,
            test,
            category,
            schema,
        }
    }

    pub fn from_tantivy_schema(tantivy_schema: &Schema) -> Result<Self, Bm25Error> {
        let document_id = Self::resolve_field(tantivy_schema, "document_id")?;
        let title = Self::resolve_field(tantivy_schema, "title")?;
        let content = Self::resolve_field(tantivy_schema, "content")?;
        let keywords = Self::resolve_field(tantivy_schema, "keywords")?;
        let chunk_id = Self::resolve_field(tantivy_schema, "chunk_id")?;
        let file_path = Self::resolve_field(tantivy_schema, "file_path")?;
        let project_id = Self::resolve_field(tantivy_schema, "project_id")?;
        let epoch = Self::resolve_field(tantivy_schema, "epoch")?;
        let entity_id = Self::resolve_field(tantivy_schema, "entity_id")?;
        let segment_id = Self::resolve_field(tantivy_schema, "segment_id")?;
        let test = Self::resolve_field(tantivy_schema, "test")?;
        let category = Self::resolve_field(tantivy_schema, "category")?;

        Ok(IndexSchema {
            document_id,
            title,
            content,
            keywords,
            chunk_id,
            file_path,
            project_id,
            epoch,
            entity_id,
            segment_id,
            test,
            category,
            schema: tantivy_schema.clone(),
        })
    }

    /// Get the full schema
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Convert document data to Tantivy document
    pub fn to_document(
        &self,
        document_id: &str,
        fields: &HashMap<String, String>,
    ) -> TantivyDocument {
        let mut doc = TantivyDocument::new();

        doc.add_text(self.document_id, document_id);

        for (key, value) in fields {
            match key.as_str() {
                "title" => doc.add_text(self.title, value),
                "content" => doc.add_text(self.content, value),
                "keywords" => doc.add_text(self.keywords, value),
                "chunk_id" => doc.add_text(self.chunk_id, value),
                "file_path" => doc.add_text(self.file_path, value),
                "project_id" => {
                    doc.add_text(self.project_id, value);
                }
                "epoch" => {
                    if let Ok(v) = value.parse::<i64>() {
                        doc.add_i64(self.epoch, v);
                    }
                }
                "entity_id" => {
                    for id in value.split(',') {
                        let id = id.trim();
                        if !id.is_empty() {
                            doc.add_text(self.entity_id, id);
                        }
                    }
                }
                "segment_id" => {
                    doc.add_text(self.segment_id, value);
                }
                "test" => {
                    if let Ok(v) = value.parse::<u64>() {
                        doc.add_u64(self.test, v);
                    }
                }
                "category" => {
                    if let Ok(v) = value.parse::<u64>() {
                        doc.add_u64(self.category, v);
                    }
                }
                "entity_kind" | "batch_id" => {
                    tracing::trace!("Ignoring legacy BM25 field '{}', use SQLite instead", key);
                }
                _ => {
                    tracing::warn!("Unknown BM25 field '{}', ignoring", key);
                }
            }
        }

        doc
    }
}

impl Default for IndexSchema {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tantivy::schema::Value;

    #[test]
    fn test_to_document_stores_segment_id_and_entity_id() {
        let schema = IndexSchema::new();
        let mut fields = HashMap::new();
        fields.insert("document_id".to_string(), "d1".to_string());
        fields.insert("entity_id".to_string(), "10,20".to_string());
        fields.insert("segment_id".to_string(), "doc_group_1".to_string());
        fields.insert("test".to_string(), "1".to_string());
        fields.insert("category".to_string(), "2".to_string());

        let doc = schema.to_document("d1", &fields);

        let entity_ids: Vec<String> = doc
            .get_all(schema.entity_id)
            .filter_map(|v| v.as_str().map(ToString::to_string))
            .collect();
        let segment_id = doc
            .get_first(schema.segment_id)
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let test = doc.get_first(schema.test).and_then(|v| v.as_u64());

        assert_eq!(entity_ids, vec!["10".to_string(), "20".to_string()]);
        assert_eq!(segment_id.as_deref(), Some("doc_group_1"));
        assert_eq!(test, Some(1));
    }

    #[test]
    fn test_alignment_fields_are_stored_but_not_indexed() {
        use tantivy::schema::FieldType;

        let schema = IndexSchema::new();
        let entries: Vec<(Field, &str)> = vec![
            (schema.entity_id, "entity_id"),
            (schema.segment_id, "segment_id"),
        ];
        for (field, name) in entries {
            match schema.schema().get_field_entry(field).field_type() {
                FieldType::Str(options) => {
                    assert!(options.is_stored(), "{name} must be stored");
                    assert!(
                        options.get_indexing_options().is_none(),
                        "{name} must not be indexed"
                    );
                }
                other => panic!("{name} must be a text field, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_keywords_are_indexed_but_not_stored() {
        use tantivy::schema::FieldType;

        let schema = IndexSchema::new();
        match schema
            .schema()
            .get_field_entry(schema.keywords)
            .field_type()
        {
            FieldType::Str(options) => {
                assert!(!options.is_stored(), "keywords must not be stored");
                assert!(
                    options.get_indexing_options().is_some(),
                    "keywords must stay indexed for scoring"
                );
            }
            other => panic!("keywords must be a text field, got {other:?}"),
        }
    }

    #[test]
    fn test_schema_exposes_segment_id_field() {
        let schema = IndexSchema::new();
        assert_eq!(
            schema.schema().get_field_name(schema.segment_id),
            "segment_id"
        );
    }

    #[test]
    fn test_unknown_field_is_ignored() {
        let schema = IndexSchema::new();
        let mut fields = HashMap::new();
        fields.insert("document_id".to_string(), "d1".to_string());
        fields.insert("entity_kind".to_string(), "function".to_string());

        let doc = schema.to_document("d1", &fields);
        assert_eq!(doc.len(), 1);
    }
}
