//! Hot update coordinator (facade)
//!
//! The implementation is split across submodules:
//! - `coordinator_core` – core struct and basic lifecycle
//! - `temp_db` – temporary database creation
//! - `change_merger` – change merging and operation handling

pub mod change_merger;
pub mod coordinator_core;
pub mod temp_db;

#[cfg(test)]
mod tests;

pub(crate) use change_merger::coalesce_pending_changes;
pub use coordinator_core::HotUpdateCoordinator;
