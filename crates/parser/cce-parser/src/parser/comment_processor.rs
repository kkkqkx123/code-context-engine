//! Comment processor for extracting and associating comments
//!
//! This module provides functionality for:
//! - Extracting comments from source code using tree-sitter queries
//! - Classifying comments by marker shape (doc line, doc block, docstring, plain)
//! - Dispatching comments into separate channels:
//!   - Documentation comments only fill entity `doc_comment` slots
//!   - Plain comments become behavior-sidecar fragments of their context entity
//!
//! # Design Principles
//!
//! - **Marker-based classification**: a comment's channel is decided solely by
//!   its shape (`///`, `//!`, `/* */`, `"""`, `#!`, `//`, `#`), never by
//!   indentation or column position.
//! - **Channel separation**: doc comments never enter behavior fragments and
//!   plain comments never enter doc slots, so the two pipelines cannot
//!   cross-contaminate.
//! - **Position-based association**: inner docs (and docstrings) attach
//!   backward to the smallest containing entity; outer docs attach forward
//!   only when the gap contains nothing but blank/attribute lines.

use crate::tree_sitter_query::error::Result;
use crate::tree_sitter_query::executor::QueryExecutor;
use cce_types::language::Language;
use cce_types::{
    BehaviorFact, BehaviorFactKind, BehaviorStore, Entity, FILE_DOC_SENTINEL_ID, Span,
};
use cce_utils::comment_cleaner::strip_comment_markers;
use std::sync::Arc;
use tree_sitter::Tree;

mod associator;
mod classifier;
mod license_detector;

use associator::{
    attach_doc_comment, forward_adjacent_entity, merge_plain_comment_blocks,
    smallest_containing_entity,
};
use classifier::{
    CommentClass, classify_comment, dedup_same_row_comments, merge_top_level_line_comments,
};
use license_detector::find_license_block_end;

/// Comment entry with text and position
#[derive(Debug, Clone)]
pub struct Comment {
    /// Comment text
    pub text: String,
    /// Source span
    pub span: Span,
    /// Tree-sitter capture name (e.g. "comment.docstring", "comment", "doc_comment")
    pub capture_name: String,
}

/// File-level documentation retained with its source location.
#[derive(Debug, Clone)]
pub struct FileDocComment {
    /// Cleaned documentation text.
    pub text: String,
    /// Original source range of the comment.
    pub span: Span,
}

/// Comment processor for extracting and filtering doc comments
pub struct CommentProcessor {
    /// Query executor
    query_executor: Arc<QueryExecutor>,
}

impl CommentProcessor {
    /// Create a new comment processor
    pub fn new() -> Self {
        Self {
            query_executor: Arc::new(QueryExecutor::new()),
        }
    }

    /// Create with custom query executor
    pub fn with_executor(executor: Arc<QueryExecutor>) -> Self {
        Self {
            query_executor: executor,
        }
    }

    /// Extract and classify comments from source code.
    ///
    /// All comments are collected regardless of language; classification by
    /// marker shape happens downstream so no comment is silently dropped.
    pub fn extract_comments(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<Comment>> {
        let matches = self
            .query_executor
            .execute_comment_query(tree, source, language)?;
        let mut comments = Vec::new();

        for mat in matches {
            for capture in &mat.captures {
                comments.push(Comment {
                    text: capture.text.clone(),
                    span: Span::new(
                        capture.start_byte,
                        capture.end_byte,
                        capture.start_point.0,
                        capture.start_point.1,
                        capture.end_point.0,
                        capture.end_point.1,
                    ),
                    capture_name: capture.name.clone(),
                });
            }
        }

        // Sort by start position for deterministic processing
        comments.sort_by_key(|c| c.span.start_byte);

        if matches!(language, Language::Rust | Language::Java) {
            let comments = dedup_same_row_comments(comments);
            let comments = merge_top_level_line_comments(comments);
            Ok(comments)
        } else {
            Ok(comments)
        }
    }

    /// Process comments for a file - extract and associate to entities.
    ///
    /// Behavior-sidecar fragments are computed into a throwaway store; use
    /// `process_with_span` when the real behavior store is available.
    ///
    /// # Arguments
    ///
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Programming language
    /// * `entities` - Mutable slice of entities to receive doc comments
    ///
    /// # Returns
    ///
    /// * `Ok(Option<String>)` - File-level doc comment if found
    /// * `Err(QueryError)` - If query execution fails
    pub fn process(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &mut [Entity],
    ) -> Result<Option<String>> {
        let mut behavior = BehaviorStore::default();
        self.process_with_span(tree, source, language, entities, &mut behavior)
            .map(|doc| doc.map(|value| value.text))
    }

    /// Process comments and retain the source location of file-level documentation.
    ///
    /// Dispatch order: classify → license → file-level doc → plain comment
    /// fragments (file header / up-adjacent / span-contained) → doc association.
    ///
    /// Plain comments are attached to their context entity as behavior facts:
    /// file-header comments go to `FILE_DOC_SENTINEL_ID` so they render with
    /// the FileDocumentation group; comments adjacent to (or contained in) an
    /// entity attach to that entity; orphan comments are dropped.
    ///
    /// Documentation comments fill entity `doc_comment` slots only: inner docs
    /// (`//!`, `#!`, `/*!`, docstrings) attach backward to the smallest
    /// containing entity (module docstrings without a container become
    /// file-level docs); outer docs (`///`, block comments) attach forward
    /// only when the gap is blank/attribute-only; ownerless docs are dropped.
    pub fn process_with_span(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &mut [Entity],
        behavior: &mut BehaviorStore,
    ) -> Result<Option<FileDocComment>> {
        let comments = self.extract_comments(tree, source, language)?;

        let first_entity_start = entities.iter().map(|e| e.span.start_byte).min();
        let license_end = find_license_block_end(&comments, entities);

        // Channel 1: plain comments become behavior fragments.
        for block in merge_plain_comment_blocks(&comments) {
            if block.span.end_byte <= license_end {
                continue;
            }

            let is_file_header =
                first_entity_start.is_none_or(|start| block.span.end_byte <= start);
            let target = if is_file_header {
                Some(FILE_DOC_SENTINEL_ID)
            } else {
                forward_adjacent_entity(source, &block, entities)
                    .or_else(|| smallest_containing_entity(&block, entities))
                    .map(|idx| entities[idx].id)
            };

            if let Some(entity_id) = target {
                behavior.push_fact(
                    entity_id,
                    BehaviorFact::new(
                        BehaviorFactKind::Comment,
                        strip_comment_markers(&block.text, true),
                        block.span.start_byte,
                        block.span.end_byte,
                    ),
                );
            }
        }

        // Channel 2: documentation comments fill doc slots only.
        let mut file_doc: Option<FileDocComment> = None;
        for comment in &comments {
            if comment.span.end_byte <= license_end {
                continue;
            }

            match classify_comment(comment) {
                CommentClass::Plain => {}
                CommentClass::InnerDoc => {
                    if let Some(idx) = smallest_containing_entity(comment, entities) {
                        attach_doc_comment(&mut entities[idx], comment);
                    } else if file_doc.is_none() {
                        let before_first =
                            first_entity_start.is_none_or(|start| comment.span.end_byte <= start);
                        if before_first {
                            file_doc = Some(FileDocComment {
                                text: clean_doc_comment_impl(&comment.text, true),
                                span: comment.span,
                            });
                        }
                    }
                }
                CommentClass::Docstring => {
                    if let Some(idx) = smallest_containing_entity(comment, entities) {
                        attach_doc_comment(&mut entities[idx], comment);
                    } else if file_doc.is_none() {
                        // Module docstring: no containing entity -> file-level.
                        file_doc = Some(FileDocComment {
                            text: clean_doc_comment_impl(&comment.text, true),
                            span: comment.span,
                        });
                    }
                }
                CommentClass::OuterDoc | CommentClass::DocBlock => {
                    if let Some(idx) = forward_adjacent_entity(source, comment, entities) {
                        attach_doc_comment(&mut entities[idx], comment);
                    }
                }
            }
        }

        Ok(file_doc)
    }
}

impl Default for CommentProcessor {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn clean_doc_comment_impl(text: &str, preserve_newlines: bool) -> String {
    strip_comment_markers(text, preserve_newlines)
}

#[cfg(test)]
mod tests;
