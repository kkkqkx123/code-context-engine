//! HTTP server state management
//!
//! The `AppState` implementation now lives in `cce-server-shared::state`. This
//! module only re-exports it so that `crate::api::state::AppState` keeps
//! resolving for the existing HTTP handlers.

pub use cce_server_shared::state::AppState;
