//! Application services shared by the HTTP and MCP surfaces.
//!
//! Transports keep wire validation and response rendering; every shared
//! behavior (query assembly, execution, conversion) lives in the modules
//! below exactly once.

pub mod index;
pub mod project;
pub mod search;
pub mod watch;

pub use index::build_index_options;
pub use project::{CreatedProject, ProjectCreateError, create_project};
pub use watch::{
    WatchStartError, WatchStatusError, WatchStopError, watch_start, watch_status, watch_stop,
};
