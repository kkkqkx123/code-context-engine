//! Document operations for BM25 index

use std::collections::HashMap;

use crate::Bm25Error;

use crate::manager::IndexManager;
use crate::schema::IndexSchema;

/// Add a document to the index
pub fn add_document(
    manager: &IndexManager,
    schema: &IndexSchema,
    document_id: &str,
    fields: &HashMap<String, String>,
) -> Result<(), Bm25Error> {
    let mut writer = manager.writer()?;
    let doc = schema.to_document(document_id, fields);
    writer.add_document(doc)?;
    writer.commit()?;
    manager.reload_reader()?;
    Ok(())
}

/// Update a document in the index
pub fn update_document(
    manager: &IndexManager,
    schema: &IndexSchema,
    document_id: &str,
    fields: &HashMap<String, String>,
) -> Result<(), Bm25Error> {
    let mut writer = manager.writer()?;

    let term = tantivy::Term::from_field_text(schema.document_id, document_id);
    writer.delete_term(term);

    let doc = schema.to_document(document_id, fields);
    writer.add_document(doc)?;
    writer.commit()?;
    manager.reload_reader()?;
    Ok(())
}

/// Get a document from the index
pub fn get_document(
    manager: &IndexManager,
    schema: &IndexSchema,
    document_id: &str,
) -> Result<Option<tantivy::schema::TantivyDocument>, Bm25Error> {
    let reader = manager.reader()?;
    let searcher = reader.searcher();

    let term = tantivy::Term::from_field_text(schema.document_id, document_id);
    let query = tantivy::query::TermQuery::new(term, tantivy::schema::IndexRecordOption::Basic);
    let top_docs = tantivy::collector::TopDocs::with_limit(1).order_by_score();
    let results: Vec<(f32, tantivy::DocAddress)> = searcher.search(&query, &top_docs)?;

    if results.is_empty() {
        Ok(None)
    } else {
        let (_, doc_address) = &results[0];
        Ok(Some(searcher.doc(*doc_address)?))
    }
}
