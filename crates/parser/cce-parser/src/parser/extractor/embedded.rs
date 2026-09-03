//! Embedded block parser for SFC (Single File Component) files
//!
//! This module provides functionality to:
//! - Extract embedded code blocks (script/style) from Vue/Svelte files
//! - Parse extracted blocks using language-specific queries
//! - Adjust position spans to match original source locations

use crate::parser::ast_parser::AstParser;
use crate::parser::embedded_types::{
    BlockType, CssInJsBlock, CssInJsCollection, CssInJsLibrary, EmbeddedBlock, EmbeddedParseConfig,
};
use crate::parser::extractor::{EntityExtractor, RelationExtractor};
use crate::tree_sitter_query::executor::{QueryExecutor, QueryMatch};
use cce_types::ParseError;
use cce_types::Span;
use cce_types::entity::{Entity, EntityId, RawRelationData};
use cce_types::language::Language;
use std::collections::HashMap;
use std::sync::Arc;
use tree_sitter::Tree;

/// Parser for embedded code blocks in SFC files
pub struct EmbeddedParser {
    query_executor: Arc<QueryExecutor>,
    entity_extractor: EntityExtractor,
    relation_extractor: RelationExtractor,
    ast_parser: AstParser,
    config: EmbeddedParseConfig,
}

impl EmbeddedParser {
    /// Create a new embedded parser with default config
    pub fn new() -> Self {
        Self {
            query_executor: Arc::new(QueryExecutor::new()),
            entity_extractor: EntityExtractor::new(),
            relation_extractor: RelationExtractor::new(),
            ast_parser: AstParser::new(),
            config: EmbeddedParseConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: EmbeddedParseConfig) -> Self {
        Self {
            query_executor: Arc::new(QueryExecutor::new()),
            entity_extractor: EntityExtractor::new(),
            relation_extractor: RelationExtractor::new(),
            ast_parser: AstParser::new(),
            config,
        }
    }

    /// Extract embedded blocks from source code
    ///
    /// # Arguments
    /// * `tree` - Parsed tree-sitter tree of the SFC file
    /// * `source` - Source code string
    /// * `language` - Language (Vue, Svelte, or HTML)
    ///
    /// # Returns
    /// * `Ok(Vec<EmbeddedBlock>)` - List of extracted blocks
    /// * `Err(ParseError)` - If extraction fails
    pub fn extract_blocks(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<Vec<EmbeddedBlock>, ParseError> {
        let mut blocks = Vec::new();

        // Execute embedded block query to find script/style elements
        // Try embedded query first (for Vue/Svelte/HTML), fall back to entity query
        let matches = match self.query_executor.loader().get_embedded_query(language) {
            Ok(query) => self
                .query_executor
                .execute_query(&query, tree, source)
                .map_err(|e| {
                    ParseError::ast_parsing(format!("Embedded query execution failed: {}", e))
                })?,
            Err(_) => {
                // Fall back to entity query for backwards compatibility
                self.query_executor
                    .execute_entity_query(tree, source, language)
                    .map_err(|e| {
                        ParseError::ast_parsing(format!("Entity query execution failed: {}", e))
                    })?
            }
        };

        for mat in &matches {
            if let Some(block) = self.process_block_match(mat, source)? {
                blocks.push(block);
            }
        }

        // Sort blocks by position in source
        blocks.sort_by_key(|b| b.span.start_byte);

        Ok(blocks)
    }

    /// Process a single query match to extract an embedded block
    fn process_block_match(
        &self,
        mat: &QueryMatch,
        _source: &str,
    ) -> Result<Option<EmbeddedBlock>, ParseError> {
        // Find the main block capture
        let block_capture = mat
            .captures
            .iter()
            .find(|c| c.name == "embedded.script" || c.name == "embedded.style");

        let block_type = match block_capture {
            Some(c) if c.name == "embedded.script" => BlockType::Script,
            Some(c) if c.name == "embedded.style" => BlockType::Style,
            _ => return Ok(None),
        };

        // Find content capture (raw_text)
        let content_capture = mat.captures.iter().find(|c| c.name.ends_with(".content"));

        let (content, content_span) = match content_capture {
            Some(c) => {
                let span = Span::new(
                    c.start_byte,
                    c.end_byte,
                    c.start_point.0,
                    c.start_point.1,
                    c.end_point.0,
                    c.end_point.1,
                );
                (c.text.clone(), span)
            }
            None => (String::new(), Span::default()),
        };

        // Find block span
        let block_span = block_capture
            .map(|c| {
                Span::new(
                    c.start_byte,
                    c.end_byte,
                    c.start_point.0,
                    c.start_point.1,
                    c.end_point.0,
                    c.end_point.1,
                )
            })
            .unwrap_or_default();

        // Extract attributes
        let attributes = self.extract_attributes(mat);

        // Create block
        let mut block = EmbeddedBlock::new(block_type, content, block_span, content_span)
            .with_should_parse(match block_type {
                BlockType::Script => self.config.parse_script,
                BlockType::Style => self.config.parse_style,
                _ => false,
            });

        // Add attributes
        for (key, value) in attributes {
            block = block.with_attribute(key, value);
        }

        // Detect language from attributes
        block.detect_language_from_attrs();

        Ok(Some(block))
    }

    /// Extract attributes from a block match
    ///
    /// Note: This is a simplified version for embedded block attributes only.
    /// For detailed entity attribute extraction, use EntityExtractor::extract_attributes.
    fn extract_attributes(&self, mat: &QueryMatch) -> HashMap<String, String> {
        let mut attrs = HashMap::new();

        for capture in &mat.captures {
            if capture.name.contains(".attr") {
                // Try to find corresponding value
                let attr_name = &capture.text;
                let value_capture = mat.captures.iter().find(|c| {
                    c.name == format!("{}.value", attr_name)
                        || c.name.ends_with(&format!(".{}_value", attr_name))
                });

                let value = value_capture.map(|c| c.text.clone()).unwrap_or_default();

                attrs.insert(attr_name.clone(), value);
            }
        }

        attrs
    }

    /// Parse an embedded block and extract entities/relations
    ///
    /// # Arguments
    /// * `block` - The embedded block to parse
    /// * `base_entity_id` - Starting entity ID for entities from this block
    ///
    /// # Returns
    /// * `Ok((Vec<Entity>, Vec<RawRelationData>))` - Extracted entities and relations
    /// * `Err(ParseError)` - If parsing fails
    pub fn parse_block(
        &mut self,
        block: &EmbeddedBlock,
        base_entity_id: u64,
    ) -> Result<(Vec<Entity>, Vec<RawRelationData>), ParseError> {
        if !block.is_parseable() {
            return Ok((Vec::new(), Vec::new()));
        }

        // Check block size limit
        if self.config.max_block_size > 0 && block.content.len() > self.config.max_block_size {
            return Ok((Vec::new(), Vec::new()));
        }

        // Parse the block content with appropriate language
        let (tree, _) = self
            .ast_parser
            .parse_with_tree(&block.content, &block.language)
            .map_err(|e| ParseError::ast_parsing(format!("Failed to parse block: {}", e)))?;

        // Extract entities
        let entities = self
            .entity_extractor
            .extract(&tree, &block.content, &block.language)
            .map_err(|e| ParseError::ast_parsing(format!("Entity extraction failed: {}", e)))?;

        // Adjust entity IDs and spans
        let line_offset = block.content_span.start_position.row;
        let col_offset = block.content_span.start_position.column;
        let adjusted_entities: Vec<Entity> = entities
            .into_iter()
            .enumerate()
            .map(|(idx, mut entity)| {
                entity.id = EntityId(base_entity_id + idx as u64);
                self.adjust_entity_span(
                    &mut entity,
                    block.content_offset(),
                    line_offset,
                    col_offset,
                );
                entity
            })
            .collect();

        // Extract relations
        let relations = self
            .relation_extractor
            .extract(
                &tree,
                &block.content,
                &block.language,
                &adjusted_entities,
                None,
            )
            .map_err(|e| ParseError::ast_parsing(format!("Relation extraction failed: {}", e)))?;

        // Convert to RawRelationData and adjust spans
        let raw_relations: Vec<RawRelationData> = relations
            .into_iter()
            .map(|r| {
                let mut span = r.span;
                self.adjust_span(&mut span, block.content_offset(), line_offset, col_offset);
                RawRelationData {
                    src: cce_types::EntityId(r.caller_id as u64),
                    level: r.caller_level,
                    dst_name: r.dst.name().to_string(),
                    relation_type: r.relation_type,
                    span,
                    stdlib_category: r.stdlib_category,
                }
            })
            .collect();

        Ok((adjusted_entities, raw_relations))
    }

    /// Adjust entity span to match original source position
    fn adjust_entity_span(
        &self,
        entity: &mut Entity,
        offset: usize,
        line_offset: usize,
        col_offset: usize,
    ) {
        self.adjust_span(&mut entity.span, offset, line_offset, col_offset);
    }

    /// Adjust span by adding byte offset and line/column offset
    ///
    /// Byte offsets are shifted by `offset`. Row positions are shifted by
    /// `line_offset`. Column positions are shifted by `col_offset` only when
    /// the entity starts on the first line of the embedded block (row 0),
    /// because subsequent lines are already relative to the original source
    /// line start.
    fn adjust_span(&self, span: &mut Span, offset: usize, line_offset: usize, col_offset: usize) {
        span.start_byte += offset;
        span.end_byte += offset;
        span.start_position.row += line_offset;
        span.end_position.row += line_offset;
        // Column adjustment: only needed for the first line of the embedded block
        // (row == line_offset, i.e. row 0 in the block content). Subsequent lines
        // have columns relative to the line start in the original source.
        if span.start_position.row == line_offset {
            span.start_position.column += col_offset;
        }
        if span.end_position.row == line_offset {
            span.end_position.column += col_offset;
        }
    }

    /// Get the config
    pub fn config(&self) -> &EmbeddedParseConfig {
        &self.config
    }

    /// Set the config
    pub fn set_config(&mut self, config: EmbeddedParseConfig) {
        self.config = config;
    }

    /// Extract CSS-in-JS blocks from JavaScript/TypeScript code
    ///
    /// # Arguments
    /// * `tree` - Parsed tree-sitter tree
    /// * `source` - Source code string
    /// * `language` - Language (JavaScript or TypeScript)
    ///
    /// # Returns
    /// * `Ok(CssInJsCollection)` - Collection of CSS-in-JS blocks
    /// * `Err(ParseError)` - If extraction fails
    pub fn extract_css_in_js(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
    ) -> Result<CssInJsCollection, ParseError> {
        use crate::tree_sitter_query::scheme::javascript;

        let mut collection = CssInJsCollection::new();

        // Get CSS-in-JS query
        let query_str = javascript::css_in_js_query();
        let ts_language = match language {
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            _ => {
                return Err(ParseError::ast_parsing(
                    "CSS-in-JS extraction only supports JavaScript/TypeScript".to_string(),
                ));
            }
        };

        let query = tree_sitter::Query::new(&ts_language, query_str).map_err(|e| {
            ParseError::ast_parsing(format!("Failed to compile CSS-in-JS query: {:?}", e))
        })?;

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

        use streaming_iterator::StreamingIterator;
        while let Some(mat) = matches.next() {
            if let Some(block) = self.process_css_in_js_match(mat, &query, source)? {
                collection.add_block(block);
            }
        }

        Ok(collection)
    }

    /// Process a single CSS-in-JS query match
    fn process_css_in_js_match(
        &self,
        mat: &tree_sitter::QueryMatch,
        query: &tree_sitter::Query,
        source: &str,
    ) -> Result<Option<CssInJsBlock>, ParseError> {
        // Get the pattern index to determine the match type
        let pattern_index = mat.pattern_index;

        // Extract captures
        let mut content: Option<String> = None;
        let mut function_name: Option<String> = None;
        let mut tag_name: Option<String> = None;
        let mut span: Option<Span> = None;

        for capture in mat.captures {
            let node = capture.node;
            let capture_name = query.capture_names()[capture.index as usize];

            // Extract content from template string
            if capture_name.contains(".content") {
                let text = node.utf8_text(source.as_bytes()).map_err(|e| {
                    ParseError::ast_parsing(format!("Failed to extract text: {}", e))
                })?;
                // Extract CSS from template literal (remove backticks)
                content = Some(self.extract_template_content(text));
                span = Some(Span::new(
                    node.start_byte(),
                    node.end_byte(),
                    node.start_position().row,
                    node.start_position().column,
                    node.end_position().row,
                    node.end_position().column,
                ));
            }

            // Extract function name
            if capture_name.contains(".func") || capture_name.ends_with(".name") {
                let text = node.utf8_text(source.as_bytes()).map_err(|e| {
                    ParseError::ast_parsing(format!("Failed to extract text: {}", e))
                })?;
                function_name = Some(text.to_string());
            }

            // Extract tag name for styled-components
            if capture_name.contains(".tag") {
                let text = node.utf8_text(source.as_bytes()).map_err(|e| {
                    ParseError::ast_parsing(format!("Failed to extract text: {}", e))
                })?;
                tag_name = Some(text.to_string());
            }
        }

        // Determine library type based on function name and pattern
        let library = self.detect_css_in_js_library(&function_name, pattern_index);
        let func_name = function_name.unwrap_or_else(|| "unknown".to_string());

        if let (Some(content), Some(span)) = (content, span) {
            let mut block = CssInJsBlock::new(library, content, span, func_name);
            if let Some(tag) = tag_name {
                block = block.with_tag_name(tag);
            }
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }

    /// Extract content from template literal (remove backticks)
    fn extract_template_content(&self, text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.starts_with('`') && trimmed.ends_with('`') {
            // Remove surrounding backticks
            let inner = &trimmed[1..trimmed.len() - 1];
            // Handle basic template literal escaping
            inner.replace("\\`", "`").replace("\\\\", "\\")
        } else {
            text.to_string()
        }
    }

    /// Detect CSS-in-JS library type
    fn detect_css_in_js_library(
        &self,
        function_name: &Option<String>,
        _pattern_index: usize,
    ) -> CssInJsLibrary {
        match function_name.as_deref() {
            Some("styled") => CssInJsLibrary::StyledComponents,
            Some("css") | Some("cx") | Some("injectGlobal") => CssInJsLibrary::Emotion,
            Some("createGlobalStyle") | Some("keyframes") => CssInJsLibrary::StyledComponents,
            _ => CssInJsLibrary::Other,
        }
    }

    /// Parse CSS-in-JS collection and extract entities
    ///
    /// # Arguments
    /// * `collection` - CSS-in-JS collection
    /// * `base_entity_id` - Starting entity ID
    ///
    /// # Returns
    /// * `Ok((Vec<Entity>, Vec<RawRelationData>))` - Extracted entities and relations
    pub fn parse_css_in_js_collection(
        &mut self,
        collection: &CssInJsCollection,
        base_entity_id: u64,
    ) -> Result<(Vec<Entity>, Vec<RawRelationData>), ParseError> {
        if collection.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        // Parse combined CSS as a single CSS file
        let (tree, _) = self
            .ast_parser
            .parse_with_tree(&collection.combined_css, &Language::Css)?;

        // Extract entities using CSS query
        let entities = self
            .entity_extractor
            .extract(&tree, &collection.combined_css, &Language::Css)
            .map_err(|e| ParseError::ast_parsing(format!("CSS entity extraction failed: {}", e)))?;

        // Adjust entity IDs and spans
        let adjusted_entities: Vec<Entity> = entities
            .into_iter()
            .enumerate()
            .map(|(idx, mut entity)| {
                entity.id = EntityId(base_entity_id + idx as u64);
                // Note: Spans would need to be mapped back to original source positions
                // This is complex for CSS-in-JS and requires additional logic
                entity
            })
            .collect();

        Ok((adjusted_entities, Vec::new()))
    }
}

impl Default for EmbeddedParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;

    #[test]
    fn test_embedded_parser_new() {
        let parser = EmbeddedParser::new();
        assert!(parser.config().parse_script);
        assert!(parser.config().parse_style);
    }

    #[test]
    fn test_embedded_parser_with_config() {
        let config = EmbeddedParseConfig::scripts_only();
        let parser = EmbeddedParser::with_config(config);
        assert!(parser.config().parse_script);
        assert!(!parser.config().parse_style);
    }

    #[test]
    fn test_adjust_span() {
        let parser = EmbeddedParser::new();
        let mut span = Span::new(10, 20, 0, 10, 0, 20);
        parser.adjust_span(&mut span, 100, 5, 3);

        assert_eq!(span.start_byte, 110);
        assert_eq!(span.end_byte, 120);
        assert_eq!(span.start_position.row, 5);
        assert_eq!(span.end_position.row, 5);
        assert_eq!(span.start_position.column, 13);
        assert_eq!(span.end_position.column, 23);
    }

    #[test]
    fn test_adjust_span_multiline() {
        let parser = EmbeddedParser::new();
        // Entity on second line of block (row 1 in block = row 6 in original)
        let mut span = Span::new(10, 20, 1, 5, 1, 15);
        parser.adjust_span(&mut span, 100, 5, 3);

        assert_eq!(span.start_byte, 110);
        assert_eq!(span.end_byte, 120);
        assert_eq!(span.start_position.row, 6);
        assert_eq!(span.end_position.row, 6);
        // Column NOT adjusted for non-first lines
        assert_eq!(span.start_position.column, 5);
        assert_eq!(span.end_position.column, 15);
    }

    #[test]
    fn test_css_in_js_collection() {
        let mut collection = CssInJsCollection::new();

        let block = CssInJsBlock::new(
            CssInJsLibrary::StyledComponents,
            ".button { color: red; }".to_string(),
            Span::default(),
            "styled",
        )
        .with_tag_name("button");

        collection.add_block(block);

        assert_eq!(collection.len(), 1);
        assert!(!collection.is_empty());
        assert!(collection.combined_css().contains("color: red"));
    }

    #[test]
    fn test_extract_template_content() {
        let parser = EmbeddedParser::new();

        // Test with backticks
        assert_eq!(
            parser.extract_template_content("`color: red;`"),
            "color: red;"
        );

        // Test without backticks
        assert_eq!(
            parser.extract_template_content("color: red;"),
            "color: red;"
        );
    }

    #[test]
    fn test_detect_css_in_js_library() {
        let parser = EmbeddedParser::new();

        assert_eq!(
            parser.detect_css_in_js_library(&Some("styled".to_string()), 0),
            CssInJsLibrary::StyledComponents
        );

        assert_eq!(
            parser.detect_css_in_js_library(&Some("css".to_string()), 0),
            CssInJsLibrary::Emotion
        );

        assert_eq!(
            parser.detect_css_in_js_library(&Some("unknown".to_string()), 0),
            CssInJsLibrary::Other
        );
    }

    #[test]
    fn test_css_in_js_block_description() {
        let block = CssInJsBlock::new(
            CssInJsLibrary::StyledComponents,
            ".button { color: red; }".to_string(),
            Span::default(),
            "styled",
        )
        .with_tag_name("button");

        assert_eq!(block.description(), "styled.button`...`");

        let block2 = CssInJsBlock::new(
            CssInJsLibrary::Emotion,
            "color: red;".to_string(),
            Span::default(),
            "css",
        );

        assert_eq!(block2.description(), "css`...`");
    }
}
