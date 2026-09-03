//! Code plugin host-side conversion.
//!
//! The canonical plugin types (`CodePlugin`, `PluginRegistry`, ...) live in
//! `cce_plugin`; loading implementations live in `cce_plugin_runtime`.
//!
//! [`convert`] provides host-side conversion between [`PluginEntity`] and
//! the pipeline's `EntityGroup` types.

pub mod convert;

/// Host-side resolution of the `LangHeuristics` capability.
pub mod heuristics;

/// Re-export the canonical plugin types for in-crate consumers that route
/// through `crate::plugin::*`.
pub use cce_plugin::{CodePlugin, PluginRegistry};
