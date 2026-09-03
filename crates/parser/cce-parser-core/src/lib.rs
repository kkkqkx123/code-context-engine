//! Core parser types shared between cce-parser and cce-relation.
//!
//! This crate provides foundational types for code parsing that are
//! needed by multiple crates in the workspace.

pub mod ast;
pub mod ast_accessor;
pub mod capture_rules;
pub mod default_rules;
pub mod extraction;
pub mod local_call_resolver;

pub use ast::{AstNode, AstParser, set_language_resolver};
pub use capture_rules::{
    CaptureRule, CapturedItem, LanguageRules, apply_capture_rule, find_capture_by_suffix,
    try_capture_rules,
};
pub use default_rules::default_language_rules;
pub use extraction::{ExtractionConfig, ExtractionContext, determine_module_path};
pub use local_call_resolver::{
    LocalCall, LocalCallResolver, LocalCallResolverConfig, count_call_arguments_from_node,
};
