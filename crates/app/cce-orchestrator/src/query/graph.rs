//! Independent graph retrieval path.
//!
//! The graph path answers graph-shaped questions (neighborhoods, paths,
//! components, exports) over a relation snapshot. It shares no scoring
//! logic with semantic search; semantic results never embed graph data,
//! and graph queries never rank text.

pub mod model;
pub mod service;

pub use model::{Confidence, GraphEdge, GraphNode, SubGraph, confidence_of};
pub use service::{GraphDirection, GraphService};
