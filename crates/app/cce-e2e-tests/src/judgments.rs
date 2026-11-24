//! Relevance judgment definitions and benchmark evaluation infrastructure.
//!
//! Sub-modules:
//! - `flask`, `once_cell`, `ripgrep`: Ground-truth judgment definitions
//! - `evaluate`: Shared evaluation logic (scoring, ranking, file-doc metrics)
//! - `report`: Shared report formatting (CSV, Markdown, chunk dumping)

pub mod evaluate;
pub mod express;
pub mod flask;
pub mod gin;
pub mod jackson_core;
pub mod media_server;
pub mod mediatr;
pub mod once_cell;
pub mod report;
pub mod ripgrep;
pub mod spring_boot;

pub use express::express_relevance_judgments;
pub use flask::flask_relevance_judgments;
pub use gin::gin_relevance_judgments;
pub use jackson_core::jackson_core_relevance_judgments;
pub use media_server::media_server_relevance_judgments;
pub use mediatr::mediatr_relevance_judgments;
pub use once_cell::once_cell_relevance_judgments;
pub use ripgrep::ripgrep_relevance_judgments;
pub use spring_boot::spring_boot_relevance_judgments;
