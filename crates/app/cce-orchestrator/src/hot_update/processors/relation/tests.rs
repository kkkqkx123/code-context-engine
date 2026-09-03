//! Relation index update processor
//!
//! This module handles updates to the relation index during hot updates.
//! It also persists relation data to SQLite for fast cold start recovery.
//!
//! # Phase 3: Dependency Propagation
//!
//! This processor implements dependency propagation for hot updates:
//! 1. When a file changes, find all files that depend on it
//! 2. Collect all affected files (changed + dependents)
//! 3. Process files in topological order (dependencies first)
