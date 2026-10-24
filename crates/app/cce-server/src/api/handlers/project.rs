//! Project management handlers
//!
//! This module provides handlers for project operations organized by type:
//! - Query: Read-only operations (list, get)
//! - Management: Write operations (create, update, delete)
//! - Indexing: Index operations
//! - Config: Configuration management

pub mod config;
pub mod indexing;
pub mod management;
pub mod query;

pub use config::{
    handle_get_project_config, handle_get_project_llm_overview, handle_reload_project_config,
    handle_test_project_llm, handle_update_project_config, handle_update_project_llm_config,
};
pub use indexing::{
    handle_dead_letter_acknowledge, handle_dead_letter_files_retry, handle_dead_letter_list,
    handle_project_index,
};
pub use management::{handle_create_project, handle_delete_project, handle_update_project};
pub use query::{handle_get_project, handle_list_projects};
