//! Generic document node trait for document processing
//!
//! This trait provides a common interface for different document node types
//! (DocNode, JsonNode, XmlNode, TomlNode, YamlNode) to reduce code duplication.

use cce_types::Span;

pub trait DocumentNode: Clone {
    fn span(&self) -> &Span;

    fn depth(&self) -> usize;
}
