//! Application services shared by the HTTP and MCP surfaces.
//!
//! Transports keep wire validation and response rendering; every shared
//! behavior (query assembly, execution, conversion) lives in the modules
//! below exactly once.

pub mod config;
pub mod graph;
pub mod index;
pub mod project;
pub mod relation;
pub mod search;
pub mod tools;
pub mod watch;

pub use index::build_index_options;
pub use project::{
    CreatedProject, ProjectCreateError, ProjectServiceError, create_project, get_project,
    list_projects, project_config_from_record, reload_project_after_profile_change,
};
pub use relation::{
    CallChainPage, CallDirection, CallEdge, CallEdges, CallPath, ChainNode, ClassHierarchy,
    ClassImplementations, ClassificationCategory, ClassificationStats, EntityRef, FunctionDetail,
    RelationFilterSpec, RelationServiceError, RelationStamp, SymbolCandidate,
};
pub use watch::{
    WatchStartError, WatchStatusError, WatchStopError, watch_start, watch_status, watch_stop,
};
