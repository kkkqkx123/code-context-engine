//! Delete operations for BM25 index

use cce_types::path::normalize_project_path;

use crate::Bm25Error;
use tantivy::Term;
use tantivy::collector::DocSetCollector;
use tantivy::query::Occur;
use tantivy::schema::Value;

use crate::manager::IndexManager;
use crate::schema::IndexSchema;

/// Delete a document from the index
pub fn delete_document(
    manager: &IndexManager,
    schema: &IndexSchema,
    document_id: &str,
) -> Result<(), Bm25Error> {
    let mut writer = manager.writer()?;
    let term = Term::from_field_text(schema.document_id, document_id);
    writer.delete_term(term);
    writer.commit()?;
    manager.reload_reader()?;
    Ok(())
}

/// Delete all documents matching a file path from the index
pub fn delete_documents_by_file_path(
    manager: &IndexManager,
    schema: &IndexSchema,
    file_path: &str,
) -> Result<usize, Bm25Error> {
    let normalized = normalize_project_path(file_path);
    let mut writer = manager.writer()?;
    let term = Term::from_field_text(schema.file_path, &normalized);
    let count = writer.delete_term(term) as usize;
    writer.commit()?;
    manager.reload_reader()?;
    Ok(count)
}

/// Delete all documents for a given project from the index.
pub fn delete_documents_by_project(
    manager: &IndexManager,
    schema: &IndexSchema,
    project_id: i64,
) -> Result<usize, Bm25Error> {
    let reader = manager.reader()?;
    let searcher = reader.searcher();

    let project_id_term = Term::from_field_text(schema.project_id, &project_id.to_string());

    let query =
        tantivy::query::TermQuery::new(project_id_term, tantivy::schema::IndexRecordOption::Basic);

    let top_docs = searcher.search(&query, &DocSetCollector)?;

    if top_docs.is_empty() {
        tracing::debug!(project_id, "No BM25 documents found for project");
        return Ok(0);
    }

    let mut doc_ids = Vec::new();
    for doc_address in &top_docs {
        let doc: tantivy::schema::TantivyDocument = searcher.doc(*doc_address)?;
        if let Some(value) = doc.get_first(schema.document_id) {
            doc_ids.push(value.as_str().unwrap_or_default().to_string());
        }
    }

    if doc_ids.is_empty() {
        return Ok(0);
    }

    let mut writer = manager.writer()?;
    for doc_id in &doc_ids {
        let term = Term::from_field_text(schema.document_id, doc_id);
        writer.delete_term(term);
    }
    writer.commit()?;
    manager.reload_reader()?;

    tracing::debug!(
        project_id,
        count = doc_ids.len(),
        "Deleted all BM25 documents for project"
    );

    Ok(doc_ids.len())
}

/// Delete documents matching both a file path AND a project ID.
pub fn delete_documents_by_file_path_and_project(
    manager: &IndexManager,
    schema: &IndexSchema,
    file_path: &str,
    project_id: i64,
) -> Result<usize, Bm25Error> {
    let reader = manager.reader()?;
    let searcher = reader.searcher();

    let normalized = normalize_project_path(file_path);
    let file_path_term = Term::from_field_text(schema.file_path, &normalized);
    let project_id_term = Term::from_field_text(schema.project_id, &project_id.to_string());

    let query = tantivy::query::BooleanQuery::new(vec![
        (
            Occur::Must,
            Box::new(tantivy::query::TermQuery::new(
                file_path_term,
                tantivy::schema::IndexRecordOption::Basic,
            )),
        ),
        (
            Occur::Must,
            Box::new(tantivy::query::TermQuery::new(
                project_id_term,
                tantivy::schema::IndexRecordOption::Basic,
            )),
        ),
    ]);

    let top_docs = searcher.search(&query, &DocSetCollector)?;

    if top_docs.is_empty() {
        return Ok(0);
    }

    let mut doc_ids = Vec::new();
    for doc_address in &top_docs {
        let doc: tantivy::schema::TantivyDocument = searcher.doc(*doc_address)?;
        if let Some(value) = doc.get_first(schema.document_id) {
            doc_ids.push(value.as_str().unwrap_or_default().to_string());
        }
    }

    if doc_ids.is_empty() {
        return Ok(0);
    }

    let mut writer = manager.writer()?;
    for doc_id in &doc_ids {
        let term = Term::from_field_text(schema.document_id, doc_id);
        writer.delete_term(term);
    }
    writer.commit()?;
    manager.reload_reader()?;

    Ok(doc_ids.len())
}

/// Delete documents matching a file path, project and data epoch.
pub fn delete_documents_by_file_path_project_epoch(
    manager: &IndexManager,
    schema: &IndexSchema,
    file_path: &str,
    project_id: i64,
    epoch: i64,
) -> Result<usize, Bm25Error> {
    let reader = manager.reader()?;
    let searcher = reader.searcher();
    let query = tantivy::query::BooleanQuery::new(vec![
        (
            Occur::Must,
            Box::new(tantivy::query::TermQuery::new(
                Term::from_field_text(schema.file_path, &normalize_project_path(file_path)),
                tantivy::schema::IndexRecordOption::Basic,
            )),
        ),
        (
            Occur::Must,
            Box::new(tantivy::query::TermQuery::new(
                Term::from_field_text(schema.project_id, &project_id.to_string()),
                tantivy::schema::IndexRecordOption::Basic,
            )),
        ),
        (
            Occur::Must,
            Box::new(tantivy::query::TermQuery::new(
                Term::from_field_i64(schema.epoch, epoch),
                tantivy::schema::IndexRecordOption::Basic,
            )),
        ),
    ]);
    let docs = searcher.search(&query, &DocSetCollector)?;
    let mut ids = Vec::with_capacity(docs.len());
    for address in docs {
        let document: tantivy::schema::TantivyDocument = searcher.doc(address)?;
        if let Some(value) = document.get_first(schema.document_id)
            && let Some(document_id) = value.as_str()
        {
            ids.push(document_id.to_string());
        }
    }
    if ids.is_empty() {
        return Ok(0);
    }
    let mut writer = manager.writer()?;
    for document_id in &ids {
        writer.delete_term(Term::from_field_text(schema.document_id, document_id));
    }
    writer.commit()?;
    manager.reload_reader()?;
    Ok(ids.len())
}

/// Delete all documents for one project and data epoch.
pub fn delete_documents_by_project_epoch(
    manager: &IndexManager,
    schema: &IndexSchema,
    project_id: i64,
    epoch: i64,
) -> Result<usize, Bm25Error> {
    let reader = manager.reader()?;
    let searcher = reader.searcher();
    let query = tantivy::query::BooleanQuery::new(vec![
        (
            Occur::Must,
            Box::new(tantivy::query::TermQuery::new(
                Term::from_field_text(schema.project_id, &project_id.to_string()),
                tantivy::schema::IndexRecordOption::Basic,
            )),
        ),
        (
            Occur::Must,
            Box::new(tantivy::query::TermQuery::new(
                Term::from_field_i64(schema.epoch, epoch),
                tantivy::schema::IndexRecordOption::Basic,
            )),
        ),
    ]);
    let docs = searcher.search(&query, &DocSetCollector)?;
    let mut ids = Vec::with_capacity(docs.len());
    for address in docs {
        let document: tantivy::schema::TantivyDocument = searcher.doc(address)?;
        if let Some(value) = document.get_first(schema.document_id)
            && let Some(document_id) = value.as_str()
        {
            ids.push(document_id.to_string());
        }
    }
    if ids.is_empty() {
        return Ok(0);
    }
    let mut writer = manager.writer()?;
    for document_id in &ids {
        writer.delete_term(Term::from_field_text(schema.document_id, document_id));
    }
    writer.commit()?;
    manager.reload_reader()?;
    Ok(ids.len())
}
