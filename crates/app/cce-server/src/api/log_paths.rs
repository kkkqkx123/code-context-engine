//! Console log routes.
//!
//! The HTTP log endpoints are part of the shared contract now: their payloads
//! live in `cce_api::models::logs`, so the paths come from `cce_api::paths`
//! and the router registers the same templates the client expands. Only the
//! WebSocket stream stays outside the contract, because its frames are not
//! HTTP payloads.

pub use cce_api::paths::{
    LOGS, LOGS_CONFIG, LOGS_FILE_DOWNLOAD_TEMPLATE, LOGS_FILE_TEMPLATE, LOGS_FILES, LOGS_ROTATE,
    LOGS_WS,
};
