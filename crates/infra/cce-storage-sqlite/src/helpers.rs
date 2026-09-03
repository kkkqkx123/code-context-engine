//! Helper functions for SQLite repository operations.
//!
//! This module provides common helper functions to reduce code duplication
//! across different repository implementations.

use rusqlite::OptionalExtension;
use rusqlite::{Connection, Row, ToSql, Transaction};

use cce_types::StorageError;

/// Unified trait for row-to-record mapping.
///
/// Each record type implements this trait to provide a standard way
/// of mapping database rows to Rust structs.
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error>;
}

/// Execute a query and collect results into a vector.
pub fn execute_query<T>(
    conn: &Connection,
    sql: &str,
    params: &[&dyn ToSql],
    mapper: fn(&Row) -> Result<T, rusqlite::Error>,
) -> Result<Vec<T>, StorageError> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| StorageError::query(format!("Failed to prepare statement: {}", e)))?;
    let results = stmt
        .query_map(params, mapper)
        .map_err(|e| StorageError::query(format!("Failed to execute query: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| StorageError::query(format!("Failed to collect results: {}", e)))?;
    drop(stmt);
    Ok(results)
}

/// Execute a query and return a single optional result.
pub fn execute_query_optional<T>(
    conn: &Connection,
    sql: &str,
    params: &[&dyn ToSql],
    mapper: fn(&Row) -> Result<T, rusqlite::Error>,
) -> Result<Option<T>, StorageError> {
    conn.query_row(sql, params, mapper)
        .optional()
        .map_err(|e| StorageError::query(format!("Failed to execute query: {}", e)))
}

/// Execute an INSERT statement and return the last inserted row ID.
pub fn execute_insert(
    tx: &Transaction,
    sql: &str,
    params: &[&dyn ToSql],
    entity_name: &str,
) -> Result<i64, StorageError> {
    tx.execute(sql, params)
        .map_err(|e| StorageError::insert(format!("Failed to insert {}: {}", entity_name, e)))?;
    Ok(tx.last_insert_rowid())
}

/// Execute a batch INSERT statement.
pub fn execute_insert_batch(
    tx: &Transaction,
    sql: &str,
    param_list: &[Vec<&dyn ToSql>],
    entity_name: &str,
) -> Result<Vec<i64>, StorageError> {
    if param_list.is_empty() {
        return Ok(Vec::new());
    }

    let mut stmt = tx
        .prepare(sql)
        .map_err(|e| StorageError::insert(format!("Failed to prepare statement: {}", e)))?;

    let mut ids = Vec::with_capacity(param_list.len());
    for params in param_list {
        stmt.execute(params.as_slice()).map_err(|e| {
            StorageError::insert(format!("Failed to insert {}: {}", entity_name, e))
        })?;
        ids.push(tx.last_insert_rowid());
    }

    Ok(ids)
}

/// Execute a DELETE or UPDATE statement.
pub fn execute_update(
    tx: &Transaction,
    sql: &str,
    params: &[&dyn ToSql],
    operation_name: &str,
) -> Result<(), StorageError> {
    tx.execute(sql, params)
        .map_err(|e| StorageError::delete(format!("Failed to {}: {}", operation_name, e)))?;
    Ok(())
}

/// Execute a COUNT query.
pub fn execute_count(
    conn: &Connection,
    sql: &str,
    params: &[&dyn ToSql],
    table_name: &str,
) -> Result<i64, StorageError> {
    conn.query_row(sql, params, |row| row.get::<_, i64>(0))
        .map_err(|e| StorageError::query(format!("Failed to count {}: {}", table_name, e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_execute_query() {
        let conn = rusqlite::Connection::open_in_memory().expect("Failed to open database");

        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)", [])
            .expect("Failed to create table");

        conn.execute("INSERT INTO test (name) VALUES ('test1')", [])
            .expect("Failed to insert");

        fn mapper(row: &Row) -> Result<String, rusqlite::Error> {
            row.get(1)
        }

        let results = execute_query(
            &conn,
            "SELECT id, name FROM test WHERE name = ?1",
            &[&"test1"],
            mapper,
        )
        .expect("Failed to execute query");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "test1");
    }

    #[test]
    fn test_execute_query_optional() {
        let conn = rusqlite::Connection::open_in_memory().expect("Failed to open database");

        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)", [])
            .expect("Failed to create table");

        conn.execute("INSERT INTO test (name) VALUES ('test1')", [])
            .expect("Failed to insert");

        fn mapper(row: &Row) -> Result<String, rusqlite::Error> {
            row.get(1)
        }

        let result = execute_query_optional(
            &conn,
            "SELECT id, name FROM test WHERE id = ?1",
            &[&1i64],
            mapper,
        )
        .expect("Failed to execute query");

        assert_eq!(result, Some("test1".to_string()));
    }

    #[test]
    fn test_execute_insert() {
        let conn = rusqlite::Connection::open_in_memory().expect("Failed to open database");
        let tx = conn
            .unchecked_transaction()
            .expect("Failed to start transaction");

        tx.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)", [])
            .expect("Failed to create table");

        let id = execute_insert(
            &tx,
            "INSERT INTO test (name) VALUES (?1)",
            &[&"test1"],
            "test",
        )
        .expect("Failed to insert");

        assert_eq!(id, 1);
        tx.commit().expect("Failed to commit");
    }

    #[test]
    fn test_execute_count() {
        let conn = rusqlite::Connection::open_in_memory().expect("Failed to open database");

        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)", [])
            .expect("Failed to create table");

        conn.execute("INSERT INTO test (name) VALUES ('test1')", [])
            .expect("Failed to insert");

        conn.execute("INSERT INTO test (name) VALUES ('test2')", [])
            .expect("Failed to insert");

        let count = execute_count(&conn, "SELECT COUNT(*) FROM test", &[], "test")
            .expect("Failed to count");

        assert_eq!(count, 2);
    }
}
