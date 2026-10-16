//! Static parsing pipeline execution.
//!
//! Executes the fixed parsing pipeline in sequence without dynamic dispatch.

use crate::parser::components::Components;
use crate::parser::context::ParseContext;
use crate::parser::cpp_interop_guard;
use crate::parser::extractor::MacroBodyExtractor;
use crate::parser::stages;
use cce_types::ParseError;
use std::borrow::Cow;

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
    let language_info = components.language_detector.detect(context.file_path())?;
    context.language_info = Some(language_info);
    Ok(())
}

fn ast_parsing(context: &mut ParseContext, components: &mut Components) -> Result<(), ParseError> {
    let language = *context
        .language()
        .ok_or_else(|| ParseError::unsupported_language("language not detected"))?;

    if !language.is_supported_for_ast() {
        return Err(ParseError::unsupported_language(language.to_string()));
    }

    // The C/C++ grammars mis-nest the preprocessor conditionals that wrap
    // `extern "C" { ... }`, so parse a shadow copy with those two brace
    // lines blanked. The shadow is byte-length preserving, so all spans
    // taken from its tree index into the original source unchanged.
    let shadow = cpp_interop_guard::shadow_for_parse(context.source(), &language);
    let had_guard_pair = shadow.is_some();
    let parse_source = match shadow {
        Some(shadow) => shadow,
        None => Cow::Borrowed(context.source()),
    };

    let (tree, _) = components
        .ast_parser
        .parse_with_tree(&parse_source, &language)?;

    // Syntax errors produce a partial tree that is still consumed downstream;
    // entities inside ERROR regions are silently lost, so surface the fact
    // that this file's index result is degraded.
    let has_syntax_errors = tree.root_node().has_error();
    if has_syntax_errors {
        context.has_syntax_errors = true;
        tracing::warn!(
            file = %context.file_path(),
            language = %language,
            "Parsed with syntax errors; extracted entities may be incomplete"
        );
    }

    // Read-only diagnostics for tooling; never used to repair the AST or
    // to suppress the degraded status computed above.
    context.syntax_diagnostics = crate::parser::diagnostics::collect_syntax_diagnostics(&tree);
    context.syntax_diagnostic_tag = crate::parser::diagnostics::coarse_tag_for_c_cpp(
        context.source(),
        had_guard_pair,
        has_syntax_errors,
    );

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
        .extract(tree, context.source(), &language)
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Entity extraction failed for file '{}': {}",
                context.file_path(),
                e
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

    let source = context.source().to_string();
    let entities = context.entities.clone();

    components
        .control_flow_extractor
        .extract(
            tree,
            &source,
            &language,
            &entities,
            &mut context.control_flow,
        )
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Control-flow extraction failed for file '{}': {}",
                context.file_path(),
                e
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

    let source = context.source().to_string();
    let entities = context.entities.clone();

    components
        .behavior_extractor
        .extract(tree, &source, &language, &entities, &mut context.behavior)
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Behavior extraction failed for file '{}': {}",
                context.file_path(),
                e
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

    let source = context.source().to_string();
    let entities = context.entities.clone();

    components
        .macro_body_extractor
        .extract(tree, &source, &language, &entities, &mut context.behavior)
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

    let source = context.source().to_string();
    let mut entities = context.entities.clone();
    let mut behavior = context.behavior.clone();

    let file_doc = components
        .comment_processor
        .process_with_span(tree, &source, &language, &mut entities, &mut behavior)
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Comment processing failed for file '{}': {}",
                context.file_path(),
                e
            ))
        })?;

    context.entities = entities;
    context.behavior = behavior;
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

    let source = context.source().to_string();
    let entities = context.entities.clone();

    let relations = components
        .relation_extractor
        .extract(tree, &source, language, &entities, None)
        .map_err(|e| {
            ParseError::ast_parsing(format!(
                "Relation extraction failed for file '{}': {}",
                context.file_path(),
                e
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
