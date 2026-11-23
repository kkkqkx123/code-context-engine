use cce_types::StorageError;

pub fn classify_pg(error: tokio_postgres::Error) -> StorageError {
    if error.is_closed() {
        return StorageError::connection(format!("pg connection closed: {error}"));
    }
    if let Some(db) = error.as_db_error() {
        let code = db.code().code();
        if code.starts_with("08") {
            return StorageError::connection(format!("pg connection failed: {error}"));
        }
        if code.starts_with("40") || code == "55P03" {
            return StorageError::transaction(format!("pg transient failure: {error}"));
        }
        if code.starts_with("23") {
            return StorageError::validation(format!("pg constraint violated: {error}"));
        }
        return StorageError::table(format!("pg error: {error}"));
    }
    StorageError::connection(format!("pg transport failed: {error}"))
}

/// Project registration maps SQLSTATE 23505 (unique_violation) to the same
/// structured variant the local branch emits. Callers classify conflicts by
/// `StorageError::is_unique_violation`, never by backend error text.
pub fn classify_project_insert(error: tokio_postgres::Error) -> StorageError {
    if let Some(db) = error.as_db_error()
        && db.code().code() == "23505"
    {
        return StorageError::unique_violation("project name or root path already exists");
    }
    classify_pg(error)
}
