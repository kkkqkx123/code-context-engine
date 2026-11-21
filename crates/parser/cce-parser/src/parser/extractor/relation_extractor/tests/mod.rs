// Relation extraction tests grouped by language and concern.
//! Tests for relation extraction.
//!
//! Covers call extraction, callee naming, dependency edges and
//! import edge cases, plus entity binding regressions exercised through
//! the extractor harness.

use super::*;

use crate::parser::ast_parser::AstParser;

use crate::parser::extractor::entity_extractor::EntityExtractor;

use crate::tree_sitter_query::executor::{Capture, QueryMatch};

use cce_types::language::Language;

use cce_types::{RelationType, StdlibCategory};

mod calls;
mod entities;
mod imports;
mod javascript;
mod python;
mod rust;
