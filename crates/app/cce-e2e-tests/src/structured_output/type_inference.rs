//! Type inference rendering for structured output.
//!
//! Provides `render_type_inference` and `render_type_inference_with_index`
//! for aggregated markdown type inference reports.

use std::collections::HashSet;
use std::fmt::Write as _;

use cce_codegraph::RelationIndex;
use cce_types::ParsedFile;
use cce_types::language::Language;

use super::types::{escape_md, normalize_nullable_display, span_str_from_span};

/// Normalize Scala generic brackets for report display.
///
/// Shapes already render with angle brackets (`Map<K, V>`), while raw
/// inferred names may keep Scala square brackets (`Map[K, V]`). Convert
/// balanced square-bracket generics to angle brackets for Scala only so
/// both columns agree; other languages keep their own spelling.
fn normalize_generic_brackets(language: Language, value: &str) -> String {
    if language != Language::Scala {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut depth = 0usize;
    for ch in value.chars() {
        match ch {
            '[' => {
                depth += 1;
                out.push('<');
            }
            ']' if depth > 0 => {
                depth -= 1;
                out.push('>');
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Display form of an inferred type or shape for reports.
///
/// Applies Scala bracket normalization first, then the shared nullable
/// spelling normalization, so `str | None` and `Optional[str]` (or
/// `String | null` and `String?`) render identically.
fn display_type_name(language: Language, value: &str) -> String {
    normalize_nullable_display(language, &normalize_generic_brackets(language, value))
}

/// Render type inference results from parsed files.
///
/// For each file the full per-file `TypeInferenceEngine` is re-run so the
/// output reflects the same inference used by the relation resolver.
pub fn render_type_inference(files: &[ParsedFile], project_name: &str) -> String {
    render_type_inference_with_index(files, None, project_name)
}

/// Render type inference with an optional relation index for member index.
pub fn render_type_inference_with_index(
    files: &[ParsedFile],
    _index: Option<&RelationIndex>,
    project_name: &str,
) -> String {
    let mut out = String::new();
    writeln!(out, "# Type Inference for {project_name}").expect("write");
    writeln!(out).expect("write");
    writeln!(
        out,
        "_Inferred per-file using the same engine as the resolver_"
    )
    .expect("write");
    writeln!(out).expect("write");

    if files.is_empty() {
        writeln!(out, "(no files)").expect("write");
        return out;
    }

    let mut total_bindings = 0usize;
    let mut total_narrowed = 0usize;
    let mut total_shapes: HashSet<String> = HashSet::new();

    let project_contexts = crate::type_inference_assert::infer_project_contexts(files);

    for (file, merged) in files.iter().zip(project_contexts.iter()) {
        writeln!(out, "## File: {}", file.path).expect("write");
        writeln!(out).expect("write");
        writeln!(out, "- Language: {}", file.language).expect("write");
        writeln!(out, "- Entities: {}", file.entities.len()).expect("write");
        writeln!(
            out,
            "- Control-flow facts: {}",
            if file.control_flow.is_empty() { 0 } else { 1 }
        )
        .expect("write");
        writeln!(out).expect("write");

        if merged.is_empty() {
            writeln!(out, "_No inferred types_").expect("write");
            writeln!(out).expect("write");
            continue;
        }

        // Variables
        writeln!(out, "### Variables").expect("write");
        writeln!(out).expect("write");
        writeln!(
            out,
            "| Variable | Inferred Type | Origin | Priority | Shape | Span |"
        )
        .expect("write");
        writeln!(
            out,
            "|----------|---------------|--------|----------|-------|------|"
        )
        .expect("write");
        let mut var_count = 0usize;
        for frame in merged.frames_iter() {
            for (name, binding) in &frame.bindings {
                let origin = binding
                    .origin
                    .map(|o| format!("{:?}", o))
                    .unwrap_or_else(|| "-".to_string());
                let priority = binding
                    .origin
                    .map(|o| {
                        cce_codegraph::type_inference::types::origin_priority(Some(o)).to_string()
                    })
                    .unwrap_or_else(|| "0".to_string());
                let shape = binding
                    .shape
                    .as_ref()
                    .map(|s| display_type_name(file.language, &s.to_type_string()))
                    .unwrap_or_else(|| "-".to_string());
                total_shapes.insert(shape.clone());
                let display_type = display_type_name(file.language, &binding.type_name);
                writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {} |",
                    escape_md(name),
                    escape_md(&display_type),
                    escape_md(&origin),
                    priority,
                    escape_md(&shape),
                    span_str_from_span(&binding.span)
                )
                .expect("write");
                var_count += 1;
            }
        }
        if var_count == 0 {
            writeln!(out, "| - | - | - | - | - | - |").expect("write");
            // Distinguish "nothing to bind" (no variable entities, e.g.
            // all values are annotated parameters) from inference failure.
            if !file
                .entities
                .iter()
                .any(|e| matches!(e.kind, cce_types::EntityKind::Variable))
            {
                writeln!(
                    out,
                    "_No variable entities: all values are annotated parameters or returns._"
                )
                .expect("write");
            }
        }
        total_bindings += var_count;
        writeln!(out).expect("write");

        // Return types
        writeln!(out, "### Function Returns").expect("write");
        writeln!(out).expect("write");
        writeln!(
            out,
            "| Function (EntityId) | Return Type | Origin | Shape |"
        )
        .expect("write");
        writeln!(out, "|-----|-------------|--------|-------|").expect("write");
        let mut ret_count = 0usize;
        for (eid, binding) in merged.return_types_iter() {
            let shape = binding
                .shape
                .as_ref()
                .map(|s| display_type_name(file.language, &s.to_type_string()))
                .unwrap_or_else(|| "-".to_string());
            total_shapes.insert(shape.clone());
            let origin = binding
                .origin
                .map(|o| format!("{:?}", o))
                .unwrap_or_else(|| "-".to_string());
            let func_name = super::types::resolve_inferred_return_name(
                eid,
                Some(file),
                &[],
                &std::collections::BTreeMap::new(),
            );
            let display_return = display_type_name(file.language, &binding.type_name);
            writeln!(
                out,
                "| {} ({}) | {} | {} | {} |",
                escape_md(&func_name),
                eid.0,
                escape_md(&display_return),
                escape_md(&origin),
                escape_md(&shape)
            )
            .expect("write");
            ret_count += 1;
        }
        if ret_count == 0 {
            writeln!(out, "| - | - | - | - |").expect("write");
        }
        writeln!(out).expect("write");

        // Control-flow narrowing
        writeln!(out, "### Control-Flow Narrowing").expect("write");
        writeln!(out).expect("write");
        writeln!(out, "| Variable | Narrowed Type | Origin | Span |").expect("write");
        writeln!(out, "|----------|---------------|--------|------|").expect("write");
        let mut narrow_count = 0usize;
        for frame in merged.frames_iter() {
            for (name, list) in &frame.narrowed {
                for binding in list {
                    let origin = binding
                        .origin
                        .map(|o| format!("{:?}", o))
                        .unwrap_or_else(|| "-".to_string());
                    let display_narrowed = display_type_name(file.language, &binding.type_name);
                    if let Some(shape) = binding
                        .shape
                        .as_ref()
                        .map(|s| display_type_name(file.language, &s.to_type_string()))
                    {
                        total_shapes.insert(shape);
                    }
                    writeln!(
                        out,
                        "| {} | {} | {} | {} |",
                        escape_md(name),
                        escape_md(&display_narrowed),
                        escape_md(&origin),
                        span_str_from_span(&binding.span)
                    )
                    .expect("write");
                    narrow_count += 1;
                }
            }
        }
        if narrow_count == 0 {
            writeln!(out, "| - | - | - | - |").expect("write");
            // Distinguish "nothing to narrow" (no control-flow facts) from
            // conservative no-guess narrowing.
            if file.control_flow.is_empty() {
                writeln!(out, "_No control-flow facts._").expect("write");
            } else {
                writeln!(
                    out,
                    "_No narrowable conditions: guards carry no supported type tests._"
                )
                .expect("write");
            }
        }
        total_narrowed += narrow_count;
        writeln!(out).expect("write");

        // Type shapes summary per file
        writeln!(out, "### Type Shapes (distinct)").expect("write");
        writeln!(out).expect("write");
        let mut shapes: Vec<String> = merged
            .frames_iter()
            .flat_map(|f| {
                f.bindings.values().filter_map(|b| {
                    b.shape
                        .as_ref()
                        .map(|s| display_type_name(file.language, &s.to_type_string()))
                })
            })
            .collect();
        shapes.extend(merged.return_types_iter().filter_map(|(_, b)| {
            b.shape
                .as_ref()
                .map(|s| display_type_name(file.language, &s.to_type_string()))
        }));
        shapes.extend(merged.frames_iter().flat_map(|f| {
            f.narrowed.values().flatten().filter_map(|b| {
                b.shape
                    .as_ref()
                    .map(|s| display_type_name(file.language, &s.to_type_string()))
            })
        }));
        shapes.sort();
        shapes.dedup();
        if shapes.is_empty() {
            writeln!(out, "_No complex shapes_").expect("write");
            writeln!(out).expect("write");
        } else {
            writeln!(out, "| Shape |").expect("write");
            writeln!(out, "|-------|").expect("write");
            for s in shapes {
                writeln!(out, "| {} |", escape_md(&s)).expect("write");
            }
            writeln!(out).expect("write");
        }
    }

    writeln!(out, "## Aggregate").expect("write");
    writeln!(out).expect("write");
    writeln!(out, "- Files with inference: {}", files.len()).expect("write");
    writeln!(out, "- Total variable bindings: {total_bindings}").expect("write");
    writeln!(out, "- Total narrowed bindings: {total_narrowed}").expect("write");
    writeln!(out, "- Distinct shapes: {}", total_shapes.len()).expect("write");
    writeln!(out).expect("write");

    out
}
