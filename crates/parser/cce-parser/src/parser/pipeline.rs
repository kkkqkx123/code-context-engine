//! Static parsing pipeline execution.
//!
//! Executes the fixed parsing pipeline in sequence without dynamic dispatch.

use crate::parser::components::Components;
use crate::parser::context::ParseContext;
use crate::parser::extractor::MacroBodyExtractor;
use crate::parser::stages;
use cce_types::ParseError;

/// Execute the full parsing pipeline on the given context.
///
/// The pipeline is fixed and does not use dynamic dispatch — each step is a
/// direct function call.
pub(crate) fn execute_full(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    language_detection(context, components)?;
    ast_parsing(context, components)?;
    entity_extraction(context, components)?;
    control_flow_extraction(context, components)?;
    behavior_extraction(context, components)?;
    macro_body_extraction(context, components)?;
    doc_comment_processing(context, components)?;
    relation_extraction(context, components)?;
    structural_extraction(context, components)?;
    post_processing(context, components)?;
    Ok(())
}

/// Execute the parsing pipeline skipping language detection (language is
/// already known).
pub(crate) fn execute_skip_language_detection(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    ast_parsing(context, components)?;
    entity_extraction(context, components)?;
    control_flow_extraction(context, components)?;
    behavior_extraction(context, components)?;
    macro_body_extraction(context, components)?;
    doc_comment_processing(context, components)?;
    relation_extraction(context, components)?;
    structural_extraction(context, components)?;
    post_processing(context, components)?;
    Ok(())
}

fn language_detection(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    let language_info = components.language_detector.detect(&context.file_path)?;
    context.language_info = Some(language_info);
    Ok(())
}

fn ast_parsing(context: &mut ParseContext, components: &mut Components) -> Result<(), ParseError> {
    let language = *context
        .language()
        .ok_or_else(|| ParseError::ast_parsing("Language not detected".to_string()))?;

    if !language.is_supported_for_ast() {
        return Err(ParseError::ast_parsing(format!(
            "Language not supported for AST parsing: {}",
            language
        )));
    }

    let (tree, _) = components
        .ast_parser
        .parse_with_tree(&context.source, &language)?;

    context.tree = Some(tree);
    Ok(())
}

fn entity_extraction(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    let language = *context
        .language()
        .ok_or_else(|| ParseError::ast_parsing("Language not detected".to_string()))?;

    let tree = context
        .tree
        .as_ref()
        .ok_or_else(|| ParseError::ast_parsing("AST tree not available".to_string()))?;

    context.entities = components
        .entity_extractor
        .extract(tree, &context.source, &language)
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Entity extraction failed for file '{}': {}",
                context.file_path, e
            ))
        })?;
    Ok(())
}

fn control_flow_extraction(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    let language = *context
        .language()
        .ok_or_else(|| ParseError::ast_parsing("Language not detected".to_string()))?;

    let tree = context
        .tree
        .as_ref()
        .ok_or_else(|| ParseError::ast_parsing("AST tree not available".to_string()))?;

    components
        .control_flow_extractor
        .extract(
            tree,
            &context.source,
            &language,
            &context.entities,
            &mut context.control_flow,
        )
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Control-flow extraction failed for file '{}': {}",
                context.file_path, e
            ))
        })?;

    Ok(())
}

fn behavior_extraction(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    let language = *context
        .language()
        .ok_or_else(|| ParseError::ast_parsing("Language not detected".to_string()))?;

    let tree = context
        .tree
        .as_ref()
        .ok_or_else(|| ParseError::ast_parsing("AST tree not available".to_string()))?;

    components
        .behavior_extractor
        .extract(
            tree,
            &context.source,
            &language,
            &context.entities,
            &mut context.behavior,
        )
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Behavior extraction failed for file '{}': {}",
                context.file_path, e
            ))
        })?;

    Ok(())
}

fn macro_body_extraction(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    let language = *context
        .language()
        .ok_or_else(|| ParseError::ast_parsing("Language not detected".to_string()))?;

    if !MacroBodyExtractor::supports_language(&language) {
        return Ok(());
    }

    let tree = context
        .tree
        .as_ref()
        .ok_or_else(|| ParseError::ast_parsing("AST tree not available".to_string()))?;

    components
        .macro_body_extractor
        .extract(
            tree,
            &context.source,
            &language,
            &context.entities,
            &mut context.behavior,
        )
        .map_err(|e| ParseError::ast_parsing(format!("Macro body extraction failed: {e}")))?;

    Ok(())
}

fn doc_comment_processing(
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

    let file_doc = components
        .comment_processor
        .process_with_span(
            tree,
            &context.source,
            &language,
            &mut context.entities,
            &mut context.behavior,
        )
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Comment processing failed for file '{}': {}",
                context.file_path, e
            ))
        })?;

    context.file_doc_comment = file_doc.as_ref().map(|doc| doc.text.clone());
    context.file_doc_span = file_doc.map(|doc| doc.span);
    Ok(())
}

fn relation_extraction(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    let language = context
        .language()
        .ok_or_else(|| ParseError::ast_parsing("Language not detected".to_string()))?;

    let tree = context
        .tree
        .as_ref()
        .ok_or_else(|| ParseError::ast_parsing("AST tree not available".to_string()))?;

    let relations = components
        .relation_extractor
        .extract(tree, &context.source, language, &context.entities, None)
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Relation extraction failed for file '{}': {}",
                context.file_path, e
            ))
        })?;

    context.relations = relations;
    Ok(())
}

fn structural_extraction(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    stages::structural::run(context, components)
}

fn post_processing(
    context: &mut ParseContext,
    components: &mut Components,
) -> Result<(), ParseError> {
    stages::post_processing::run(context, components)
}
