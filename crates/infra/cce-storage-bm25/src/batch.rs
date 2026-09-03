//! Batch operations for BM25 index

use std::collections::HashMap;

use crate::Bm25Error;

use crate::manager::IndexManager;
use crate::schema::IndexSchema;

/// Batch add documents to the index
pub fn batch_add_documents(
    manager: &IndexManager,
    schema: &IndexSchema,
    documents: Vec<(String, HashMap<String, String>)>,
) -> Result<usize, Bm25Error> {
    let count = documents.len();
    let mut writer = manager.writer()?;

    for (doc_id, fields) in documents {
        let term = tantivy::Term::from_field_text(schema.document_id, &doc_id);
        writer.delete_term(term);
        let doc = schema.to_document(&doc_id, &fields);
        writer.add_document(doc)?;
    }

    writer.commit()?;
    Ok(count)
}
