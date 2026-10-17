//! Delete operations for BM25 index

use cce_types::path::normalize_project_path;

use crate::Bm25Error;
use tantivy::Term;
use tantivy::collector::DocSetCollector;
use tantivy::query::Occur;
use tantivy::schema::Value;

use crate::manager::{IndexManager, commit_or_rollback};
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
    commit_or_rollback(&mut writer)?;
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
    commit_or_rollback(&mut writer)?;
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
    commit_or_rollback(&mut writer)?;
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
    commit_or_rollback(&mut writer)?;
    manager.reload_reader()?;

    Ok(doc_ids.len())
}

/// Delete only summary documents of a file within a project.
///
/// Chunk documents share the same `file_path`/`project_id` fields as summary
/// documents, so the generic per-file delete would clear both. Summary
/// documents are identified by the `::summary::` segment in their document
/// ID (`{project}::{epoch}::summary::{path}`); only those IDs are removed.
pub fn delete_summary_documents_by_file_path_and_project(
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
            let id = value.as_str().unwrap_or_default().to_string();
            if id.contains("::summary::") {
                doc_ids.push(id);
            }
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
    commit_or_rollback(&mut writer)?;
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
    delete_documents_by_file_paths_project_epoch(
        manager,
        schema,
        std::slice::from_ref(&file_path.to_string()),
        project_id,
        epoch,
    )
}

/// Delete documents matching any of the given file paths within one project
/// and data epoch.
///
/// The batch shares one search, one writer, and one commit+reload: the
/// per-file form repeats that whole cycle per path, and the commit (an fsync
/// plus a segment merge) dominates the cost when files are deleted in bulk.
/// The result equals the sum of the per-file deletes.
pub fn delete_documents_by_file_paths_project_epoch(
    manager: &IndexManager,
    schema: &IndexSchema,
    file_paths: &[String],
    project_id: i64,
    epoch: i64,
) -> Result<usize, Bm25Error> {
    if file_paths.is_empty() {
        return Ok(0);
    }
    let reader = manager.reader()?;
    let searcher = reader.searcher();
    let mut clauses = vec![
        (
            Occur::Must,
            Box::new(tantivy::query::TermQuery::new(
                Term::from_field_text(schema.project_id, &project_id.to_string()),
                tantivy::schema::IndexRecordOption::Basic,
            )) as Box<dyn tantivy::query::Query>,
        ),
        (
            Occur::Must,
            Box::new(tantivy::query::TermQuery::new(
                Term::from_field_i64(schema.epoch, epoch),
                tantivy::schema::IndexRecordOption::Basic,
            )),
        ),
    ];
    // Match any of the paths: a Should group over the path terms, required
    // as a whole by the surrounding Must clauses.
    let path_terms: Vec<Box<dyn tantivy::query::Query>> = file_paths
        .iter()
        .map(|path| {
            Box::new(tantivy::query::TermQuery::new(
                Term::from_field_text(schema.file_path, &normalize_project_path(path)),
                tantivy::schema::IndexRecordOption::Basic,
            )) as Box<dyn tantivy::query::Query>
        })
        .collect();
    clauses.push((
        Occur::Must,
        Box::new(tantivy::query::BooleanQuery::new(
            path_terms
                .into_iter()
                .map(|term| (Occur::Should, term))
                .collect(),
        )),
    ));
    let query = tantivy::query::BooleanQuery::new(clauses);
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
    commit_or_rollback(&mut writer)?;
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
    commit_or_rollback(&mut writer)?;
    manager.reload_reader()?;
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch_add_documents;
    use std::collections::HashMap;

    const PROJECT_ID: i64 = 1;
    const EPOCH: i64 = 1;

    fn seed(manager: &IndexManager, schema: &IndexSchema, files: usize) {
        let docs: Vec<(String, HashMap<String, String>)> = (0..files)
            .map(|i| {
                let mut fields = HashMap::new();
                fields.insert("title".to_string(), format!("doc {i}"));
                fields.insert("content".to_string(), format!("body {i}"));
                fields.insert("file_path".to_string(), format!("src/mod_{i:06}.rs"));
                fields.insert("project_id".to_string(), PROJECT_ID.to_string());
                fields.insert("epoch".to_string(), EPOCH.to_string());
                (format!("doc_{i}"), fields)
            })
            .collect();
        batch_add_documents(manager, schema, docs).expect("seed");
        manager.reload_reader().expect("reload");
    }

    fn remaining(manager: &IndexManager, schema: &IndexSchema) -> usize {
        let reader = manager.reader().expect("reader");
        let searcher = reader.searcher();
        let query = tantivy::query::TermQuery::new(
            Term::from_field_text(schema.project_id, &PROJECT_ID.to_string()),
            tantivy::schema::IndexRecordOption::Basic,
        );
        searcher
            .search(&query, &DocSetCollector)
            .expect("count")
            .len()
    }

    /// The batched delete must remove exactly the same documents as the
    /// per-file loop it replaces.
    #[test]
    fn batch_delete_matches_per_file_delete() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = IndexManager::create(dir.path()).expect("create");
        let schema = manager.schema().clone();
        seed(&manager, &schema, 10);

        let paths: Vec<String> = (0..5).map(|i| format!("src/mod_{i:06}.rs")).collect();
        let deleted = delete_documents_by_file_paths_project_epoch(
            &manager, &schema, &paths, PROJECT_ID, EPOCH,
        )
        .expect("batch delete");
        assert_eq!(deleted, 5);
        assert_eq!(remaining(&manager, &schema), 5);
    }

    /// A batch that covers every file must empty the project's epoch.
    #[test]
    fn batch_delete_all_files_clears_epoch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = IndexManager::create(dir.path()).expect("create");
        let schema = manager.schema().clone();
        seed(&manager, &schema, 8);

        let paths: Vec<String> = (0..8).map(|i| format!("src/mod_{i:06}.rs")).collect();
        let deleted = delete_documents_by_file_paths_project_epoch(
            &manager, &schema, &paths, PROJECT_ID, EPOCH,
        )
        .expect("batch delete");
        assert_eq!(deleted, 8);
        assert_eq!(remaining(&manager, &schema), 0);
    }

    /// An empty batch is a no-op, and paths outside the epoch/project are
    /// never touched.
    #[test]
    fn batch_delete_empty_and_foreign_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = IndexManager::create(dir.path()).expect("create");
        let schema = manager.schema().clone();
        seed(&manager, &schema, 4);

        assert_eq!(
            delete_documents_by_file_paths_project_epoch(&manager, &schema, &[], PROJECT_ID, EPOCH)
                .expect("empty batch"),
            0
        );
        assert_eq!(remaining(&manager, &schema), 4);

        // Same path, wrong epoch: nothing matches.
        assert_eq!(
            delete_documents_by_file_paths_project_epoch(
                &manager,
                &schema,
                &["src/mod_000000.rs".to_string()],
                PROJECT_ID,
                999,
            )
            .expect("wrong epoch"),
            0
        );
        assert_eq!(remaining(&manager, &schema), 4);
    }
}
