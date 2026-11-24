//! Workflow test entry point.
//!
//! Smoke subset of the e2e suite: alignment, plugin, rerank, retrieval
//! method alignment, hot update and index workflows only. This entry stays
//! intentionally small for fast feedback. The full suite lives in
//! `tests/e2e.rs`; new workflow modules must be mounted there, not here.

#[path = "e2e/alignment_workflow.rs"]
pub mod alignment_workflow;
#[path = "e2e/helper.rs"]
pub mod helper;
#[path = "e2e/plugin_workflow.rs"]
pub mod plugin_workflow;
#[path = "e2e/rerank_workflow.rs"]
pub mod rerank_workflow;

#[path = "e2e/retrieval_method_alignment.rs"]
pub mod retrieval_method_alignment;

// The hot-update suite (change detection, processor chain, resume/recovery)
// runs under the workflow test binary.
#[path = "e2e/hot_update.rs"]
pub mod hot_update;
// The index workflow helper used by the hot-update suite.
#[path = "e2e/index.rs"]
pub mod index;
