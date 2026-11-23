//! Batch operations for BM25 index

use std::collections::HashMap;
use std::time::Instant;

use crate::Bm25Error;

use crate::manager::{IndexManager, commit_or_rollback, rollback_writer};
use crate::schema::IndexSchema;

/// Timing detail for one committed document batch.
#[derive(Debug, Clone, Copy, Default)]
pub struct BatchWriteStats {
    pub documents: usize,
    pub document_nanos: u64,
    pub commit_nanos: u64,
}

/// Add documents to the shared writer without publishing them.
///
/// Idempotent per document id (`delete_term` then add), so replaying a staged
/// batch after a transient failure yields the same index state. Callers that
/// stage across orchestration batches must commit before publishing a
/// generation.
pub fn stage_documents(
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
        if let Err(error) = writer.add_document(doc) {
            rollback_writer(&mut writer);
            return Err(Bm25Error::from(error));
        }
    }

    Ok(count)
}

/// Commit the shared writer and report how long Tantivy spent committing.
pub fn commit_documents(manager: &IndexManager) -> Result<BatchWriteStats, Bm25Error> {
    let mut writer = manager.writer()?;
    let started = Instant::now();
    commit_or_rollback(&mut writer)?;
    Ok(BatchWriteStats {
        documents: 0,
        document_nanos: 0,
        commit_nanos: started.elapsed().as_nanos() as u64,
    })
}

/// Batch add documents to the index.
///
/// Idempotent per document id (`delete_term` then add), so replaying the same
/// batch after a transient failure yields the same index state. The batch
/// shares the manager's long-lived writer and commits once at the end.
pub fn batch_add_documents(
    manager: &IndexManager,
    schema: &IndexSchema,
    documents: Vec<(String, HashMap<String, String>)>,
) -> Result<BatchWriteStats, Bm25Error> {
    let started = Instant::now();
    let count = stage_documents(manager, schema, documents)?;
    let mut stats = commit_documents(manager)?;
    stats.documents = count;
    stats.document_nanos = started.elapsed().as_nanos() as u64;
    Ok(stats)
}
