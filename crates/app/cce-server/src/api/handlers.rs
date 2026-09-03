//! API handlers
//!
//! This module provides HTTP handlers for all API endpoints.
//! AppState and router are defined in the parent module (state.rs, router.rs).
//!
//! ## Module Structure
//!
//! - `entity/` - Entity queries (immediate return, read-only)
//! - `index/` - Index operations (long-running, resource-intensive)
//! - `project/` - Project management (configuration management)
//! - `tools/` - Tool APIs for programming tasks (compression, diagnosis, symbol lookup)
//! - `metrics` - Metrics export (Prometheus, JSON)
//! - `search` - Search queries (immediate return, read-only)
//! - `storage` - Storage management (data lifecycle management)
//! - `summary` - File summary generation
//! - `watch` - Hot reload (background task management)

pub mod config;
pub mod entity;
pub mod entity_search;
pub mod health;
pub mod index;
pub mod metrics;
pub mod project;
pub mod qdrant_admin;
pub mod search;
pub mod storage;
pub mod summary;
pub mod tools;
pub mod watch;
