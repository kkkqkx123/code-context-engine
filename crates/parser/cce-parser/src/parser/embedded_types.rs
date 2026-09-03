//! Embedded block types for SFC (Single File Component) parsing
//!
//! This module provides types for handling embedded code blocks in Vue/Svelte files:
//! - `<script>` / `<script setup>` / `<script lang="ts">`
//! - `<style>` / `<style scoped>` / `<style lang="scss">`
//!
//! # Design Goals
//!
//! - Enable deep parsing of embedded code blocks
//! - Maintain accurate position mapping to original source
//! - Support cross-block relationship extraction

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use cce_types::Span;

// Re-export core embedded block types from cce_core.
// These were moved to break the circular dependency between cce_core and cce_parser.
pub use cce_types::entity::{BlockRelation, BlockRelationType, BlockType, EmbeddedBlock};

// ============================================================================
// CSS-in-JS Types
// ============================================================================

/// CSS-in-JS library type
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub enum CssInJsLibrary {
    /// styled-components
    #[serde(rename = "styled_components")]
    StyledComponents,
    /// Emotion
    #[serde(rename = "emotion")]
    Emotion,
    /// Other CSS-in-JS libraries
    #[serde(rename = "other")]
    Other,
}

impl std::fmt::Display for CssInJsLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CssInJsLibrary::StyledComponents => write!(f, "styled-components"),
            CssInJsLibrary::Emotion => write!(f, "emotion"),
            CssInJsLibrary::Other => write!(f, "other"),
        }
    }
}

/// A CSS-in-JS block found in JavaScript/TypeScript code
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub struct CssInJsBlock {
    /// The CSS-in-JS library used
    pub library: CssInJsLibrary,
    /// The CSS content (extracted from template literal)
    pub content: String,
    /// Position in the original source file
    pub span: Span,
    /// The component or variable name (if available)
    pub name: Option<String>,
    /// The tag name for styled-components (e.g., "div", "button")
    pub tag_name: Option<String>,
    /// The CSS-in-JS function/method name (e.g., "styled", "css", "keyframes")
    pub function_name: String,
}

impl CssInJsBlock {
    /// Create a new CSS-in-JS block
    pub fn new(
        library: CssInJsLibrary,
        content: String,
        span: Span,
        function_name: impl Into<String>,
    ) -> Self {
        Self {
            library,
            content,
            span,
            name: None,
            tag_name: None,
            function_name: function_name.into(),
        }
    }

    /// Set the component/variable name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the tag name (for styled-components)
    pub fn with_tag_name(mut self, tag_name: impl Into<String>) -> Self {
        self.tag_name = Some(tag_name.into());
        self
    }

    /// Check if this block has valid CSS content
    pub fn has_content(&self) -> bool {
        !self.content.is_empty()
    }

    /// Get a description of this block
    pub fn description(&self) -> String {
        match self.library {
            CssInJsLibrary::StyledComponents => {
                if let Some(tag) = &self.tag_name {
                    format!("styled.{}`...`", tag)
                } else if let Some(name) = &self.name {
                    format!("styled({})`...`", name)
                } else {
                    "styled`...`".to_string()
                }
            }
            CssInJsLibrary::Emotion => {
                format!("{}`...`", self.function_name)
            }
            CssInJsLibrary::Other => {
                format!("{}`...`", self.function_name)
            }
        }
    }
}

/// Collection of CSS-in-JS blocks from a file
#[derive(
    Debug, Clone, Default, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize,
)]
pub struct CssInJsCollection {
    /// All CSS-in-JS blocks found
    pub blocks: Vec<CssInJsBlock>,
    /// Total CSS content (concatenated for parsing)
    pub combined_css: String,
    /// Block offsets for mapping back to source
    pub block_offsets: Vec<usize>,
}

impl CssInJsCollection {
    /// Create a new empty collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a block to the collection
    pub fn add_block(&mut self, block: CssInJsBlock) {
        self.block_offsets.push(self.combined_css.len());
        if !self.combined_css.is_empty() {
            self.combined_css.push('\n');
        }
        self.combined_css.push_str(&block.content);
        self.blocks.push(block);
    }

    /// Check if collection is empty
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Get number of blocks
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Get combined CSS for parsing
    pub fn combined_css(&self) -> &str {
        &self.combined_css
    }
}

/// Parser configuration for embedded blocks
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub struct EmbeddedParseConfig {
    /// Whether to parse script blocks
    pub parse_script: bool,
    /// Whether to parse style blocks
    pub parse_style: bool,
    /// Whether to extract cross-block relations
    pub extract_relations: bool,
    /// Maximum size of a block to parse (in bytes, 0 = unlimited)
    pub max_block_size: usize,
}

impl Default for EmbeddedParseConfig {
    fn default() -> Self {
        Self {
            parse_script: true,
            parse_style: true,
            extract_relations: true,
            max_block_size: 0,
        }
    }
}

impl EmbeddedParseConfig {
    /// Create a config that parses only scripts
    pub fn scripts_only() -> Self {
        Self {
            parse_script: true,
            parse_style: false,
            extract_relations: true,
            max_block_size: 0,
        }
    }

    /// Create a config with all features disabled
    pub fn disabled() -> Self {
        Self {
            parse_script: false,
            parse_style: false,
            extract_relations: false,
            max_block_size: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_parse_config_default() {
        let config = EmbeddedParseConfig::default();
        assert!(config.parse_script);
        assert!(config.parse_style);
        assert!(config.extract_relations);
        assert_eq!(config.max_block_size, 0);
    }

    #[test]
    fn test_embedded_parse_config_scripts_only() {
        let config = EmbeddedParseConfig::scripts_only();
        assert!(config.parse_script);
        assert!(!config.parse_style);
    }
}
