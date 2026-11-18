//! Relation canvas built on the `compograph` library set.

mod canvas;
mod model;

pub use canvas::{CanvasEvent, GraphCanvas};
pub use model::{DEFAULT_MAX_NODES, LayoutKind, Topology};
