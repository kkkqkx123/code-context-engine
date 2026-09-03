//! Embedded block types for SFC (Single File Component) parsing
//!
//! This module provides types for handling embedded code blocks in Vue/Svelte files:
//! - `<script>` / `<script setup>` / `<script lang="ts">`
//! - `<style>` / `<style scoped>` / `<style lang="scss">`

use std::collections::HashMap;

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

use crate::types::Span;
use crate::types::entity::EntityId;
use crate::types::language::Language;

/// Block type for embedded code in SFC files
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
)]
pub enum BlockType {
    /// Template block (HTML/template syntax)
    #[serde(rename = "template")]
    Template,
    /// Script block (JavaScript/TypeScript)
    #[serde(rename = "script")]
    Script,
    /// Style block (CSS/SCSS/LESS)
    #[serde(rename = "style")]
    Style,
}

impl BlockType {
    /// Get default language for this block type
    pub fn default_language(&self) -> Language {
        match self {
            BlockType::Template => Language::Html,
            BlockType::Script => Language::JavaScript,
            BlockType::Style => Language::Css,
        }
    }

    /// Check if this block can have a `lang` attribute
    pub fn supports_lang_attribute(&self) -> bool {
        matches!(self, BlockType::Script | BlockType::Style)
    }
}

impl std::fmt::Display for BlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockType::Template => write!(f, "template"),
            BlockType::Script => write!(f, "script"),
            BlockType::Style => write!(f, "style"),
        }
    }
}

/// An embedded code block within an SFC file
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct EmbeddedBlock {
    /// Block type (template/script/style)
    pub block_type: BlockType,

    /// Detected language for this block
    pub language: Language,

    /// The extracted code content
    pub content: String,

    /// Position of this block in the original source file
    pub span: Span,

    /// Content span (position of raw_text inside the block)
    pub content_span: Span,

    /// HTML attributes from the block tag (e.g., lang="ts", scoped, setup)
    pub attributes: HashMap<String, String>,

    /// Whether this block should be parsed for entities
    pub should_parse: bool,
}

/// rkyv-safe snapshot of [`EmbeddedBlock`] with `HashMap` fields replaced by
/// `Vec` tuples to satisfy rkyv 0.8 trait bounds.
#[derive(Debug, Clone, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct EmbeddedBlockSnapshot {
    pub block_type: BlockType,
    pub language: Language,
    pub content: String,
    pub span: Span,
    pub content_span: Span,
    pub attributes: Vec<(String, String)>,
    pub should_parse: bool,
}

impl From<&EmbeddedBlock> for EmbeddedBlockSnapshot {
    fn from(b: &EmbeddedBlock) -> Self {
        Self {
            block_type: b.block_type,
            language: b.language,
            content: b.content.clone(),
            span: b.span,
            content_span: b.content_span,
            attributes: b
                .attributes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            should_parse: b.should_parse,
        }
    }
}

impl From<EmbeddedBlockSnapshot> for EmbeddedBlock {
    fn from(s: EmbeddedBlockSnapshot) -> Self {
        Self {
            block_type: s.block_type,
            language: s.language,
            content: s.content,
            span: s.span,
            content_span: s.content_span,
            attributes: s.attributes.into_iter().collect(),
            should_parse: s.should_parse,
        }
    }
}

impl EmbeddedBlock {
    /// Create a new embedded block
    pub fn new(block_type: BlockType, content: String, span: Span, content_span: Span) -> Self {
        let language = block_type.default_language();
        Self {
            block_type,
            language,
            content,
            span,
            content_span,
            attributes: HashMap::new(),
            should_parse: true,
        }
    }

    /// Set the detected language
    pub fn with_language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }

    /// Add an attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Set whether this block should be parsed
    pub fn with_should_parse(mut self, should_parse: bool) -> Self {
        self.should_parse = should_parse;
        self
    }

    /// Detect language from attributes
    pub fn detect_language_from_attrs(&mut self) {
        self.language = match self.block_type {
            BlockType::Script => {
                if let Some(type_attr) = self.attributes.get("type") {
                    match type_attr.as_str() {
                        "module" => Language::JavaScript,
                        "text/javascript" => Language::JavaScript,
                        "application/javascript" => Language::JavaScript,
                        "application/ecmascript" => Language::JavaScript,
                        "text/typescript" => Language::TypeScript,
                        "application/typescript" => Language::TypeScript,
                        "text/babel" => Language::JavaScript,
                        "text/jsx" => Language::Jsx,
                        "text/tsx" => Language::Tsx,
                        _ => Language::JavaScript,
                    }
                } else if let Some(lang) = self.attributes.get("lang") {
                    match lang.as_str() {
                        "ts" | "typescript" => Language::TypeScript,
                        "js" | "javascript" => Language::JavaScript,
                        "jsx" => Language::Jsx,
                        "tsx" => Language::Tsx,
                        _ => Language::JavaScript,
                    }
                } else {
                    Language::JavaScript
                }
            }
            BlockType::Style => {
                if let Some(type_attr) = self.attributes.get("type") {
                    match type_attr.as_str() {
                        "text/css" => Language::Css,
                        "text/scss" | "text/x-scss" => Language::Scss,
                        "text/less" | "text/x-less" => Language::Less,
                        _ => Language::Css,
                    }
                } else if let Some(lang) = self.attributes.get("lang") {
                    match lang.as_str() {
                        "scss" | "sass" => Language::Scss,
                        "less" => Language::Less,
                        "css" => Language::Css,
                        "stylus" => Language::Css,
                        _ => Language::Css,
                    }
                } else {
                    Language::Css
                }
            }
            BlockType::Template => Language::Html,
        };
    }

    /// Check if this is a script block with `setup` attribute (Vue 3)
    pub fn is_setup_script(&self) -> bool {
        self.block_type == BlockType::Script && self.attributes.contains_key("setup")
    }

    /// Check if this is a scoped style block (Vue)
    pub fn is_scoped_style(&self) -> bool {
        self.block_type == BlockType::Style && self.attributes.contains_key("scoped")
    }

    /// Check if this is a module style block (Vue)
    pub fn is_module_style(&self) -> bool {
        self.block_type == BlockType::Style && self.attributes.contains_key("module")
    }

    /// Get the content offset relative to the original file
    pub fn content_offset(&self) -> usize {
        self.content_span.start_byte
    }

    /// Check if this block type should be parsed for entities
    pub fn is_parseable(&self) -> bool {
        self.should_parse
            && matches!(
                self.language,
                Language::JavaScript
                    | Language::TypeScript
                    | Language::Jsx
                    | Language::Tsx
                    | Language::Css
                    | Language::Scss
                    | Language::Less
            )
    }

    /// Check if a span is contained within this block's span
    pub fn contains_span(&self, span: &Span) -> bool {
        self.content_span.start_byte <= span.start_byte
            && self.content_span.end_byte >= span.end_byte
    }
}

/// Relation between entities in different blocks
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct BlockRelation {
    /// Source entity ID (e.g., template element)
    pub source_id: EntityId,

    /// Target entity ID (e.g., script function)
    pub target_id: EntityId,

    /// Type of relation
    pub relation_type: BlockRelationType,

    /// Description of the relation
    pub description: String,
}

/// Types of relations between blocks
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
)]
pub enum BlockRelationType {
    /// Template references a script variable/function
    #[serde(rename = "template_to_script")]
    TemplateToScript,

    /// Template event handler binds to script function
    #[serde(rename = "event_handler")]
    EventHandler,

    /// Template uses a script-defined component
    #[serde(rename = "component_usage")]
    ComponentUsage,

    /// Template element uses a style selector (by class/id)
    #[serde(rename = "template_to_style")]
    TemplateToStyle,

    /// Style selector matches template element (by class/id)
    #[serde(rename = "style_to_template")]
    StyleToTemplate,

    /// Script imports a style module
    #[serde(rename = "script_to_style")]
    ScriptToStyle,

    /// Prop binding between components
    #[serde(rename = "prop_binding")]
    PropBinding,

    /// Ref binding (template ref to script variable)
    #[serde(rename = "ref_binding")]
    RefBinding,
}

impl std::fmt::Display for BlockRelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockRelationType::TemplateToScript => write!(f, "template_to_script"),
            BlockRelationType::EventHandler => write!(f, "event_handler"),
            BlockRelationType::ComponentUsage => write!(f, "component_usage"),
            BlockRelationType::TemplateToStyle => write!(f, "template_to_style"),
            BlockRelationType::StyleToTemplate => write!(f, "style_to_template"),
            BlockRelationType::ScriptToStyle => write!(f, "script_to_style"),
            BlockRelationType::PropBinding => write!(f, "prop_binding"),
            BlockRelationType::RefBinding => write!(f, "ref_binding"),
        }
    }
}
