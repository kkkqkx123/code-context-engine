use crate::parser::components::Components;
use crate::parser::context::ParseContext;
use crate::parser::extractor::MacroBodyExtractor;
use crate::parser::helpers;
use cce_types::ParseError;

/// Post-processing: symbol table, imports/exports, embedded blocks.
pub(crate) fn run(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    let language = context
        .language()
        .cloned()
        .ok_or_else(|| ParseError::ast_parsing("Language not detected".to_string()))?;

    let tree = context
        .tree
        .as_ref()
        .ok_or_else(|| ParseError::ast_parsing("AST tree not available".to_string()))?;

    // Parse embedded blocks if applicable
    if language.has_embedded_blocks() {
        let embedded_result = helpers::parse_embedded_blocks(
            tree,
            &context.source,
            &language,
            &context.entities,
            &mut components.embedded_parser,
        )?;

        context.embedded_blocks = embedded_result.blocks;
        context.block_entities = embedded_result.entities;
        context.block_relations = embedded_result.relations;
    }

    // Build symbol table (after all entities are collected)
    let mut all_entities = context.entities.clone();
    all_entities.extend(context.block_entities.clone());

    context.local_symbols = helpers::build_symbol_table(&all_entities);

    // Extract macro body facts (Rust only, runs after entity extraction)
    if MacroBodyExtractor::supports_language(&language) {
        components
            .macro_body_extractor
            .extract(
                tree,
                &context.source,
                &language,
                &all_entities,
                &mut context.behavior,
            )
            .map_err(|e| ParseError::ast_parsing(format!("Macro body extraction failed: {e}")))?;
    }

    Ok(())
}
