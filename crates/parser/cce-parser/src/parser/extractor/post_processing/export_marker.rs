use cce_types::entity::Entity;
use cce_types::language::Language;

/// Mark entities explicitly exported via `export` statements.
///
/// Tree-sitter emits `export_statement` wrapper nodes around the declared
/// entity for JS/TS. The entity extractor matches the inner declaration and
/// loses the wrapper context, so without this pass every top-level symbol
/// looks equally public and the export list over-enumerates. This pass
/// re-attaches the wrapper context: any entity whose span is contained in an
/// `export_statement` range gets `is_exported=true` (plus `is_default=true`
/// for `export default`).
pub fn mark_exported_entities(
    entities: &mut [Entity],
    tree: &tree_sitter::Tree,
    source: &str,
    language: &Language,
) {
    if !matches!(
        language,
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx
    ) {
        return;
    }
    let ranges = collect_export_ranges(tree.root_node(), source);
    if ranges.is_empty() {
        return;
    }
    for entity in entities.iter_mut() {
        if entity
            .metadata
            .get("is_exported")
            .is_some_and(|v| v == "true")
        {
            continue;
        }
        let start = entity.span.start_byte;
        let end = entity.span.end_byte;
        for range in &ranges {
            if range.start <= start && end <= range.end {
                entity.set_metadata("is_exported", "true".to_string());
                if range.is_default {
                    entity.set_metadata("is_default", "true".to_string());
                }
                break;
            }
        }
    }
}

struct ExportRange {
    start: usize,
    end: usize,
    is_default: bool,
}

fn collect_export_ranges(root: tree_sitter::Node, source: &str) -> Vec<ExportRange> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "export_statement" {
            let text = source
                .get(node.start_byte()..node.end_byte())
                .unwrap_or_default();
            // `export default ...` / `export default function` forms.
            let is_default = text
                .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                .any(|w| w == "default");
            out.push(ExportRange {
                start: node.start_byte(),
                end: node.end_byte(),
                is_default,
            });
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    out
}

/// Assign C++ member visibility from `access_specifier` sections.
///
/// `class` members default to private and `struct` members to public; each
/// `public:`/`protected:`/`private:` label switches the section until the
/// next label. Members without explicit visibility metadata inherit their
/// enclosing section.
pub fn mark_cpp_access_sections(
    entities: &mut [Entity],
    tree: &tree_sitter::Tree,
    source: &str,
    language: &Language,
) {
    use cce_types::entity::EntityKind;
    if *language != Language::Cpp {
        return;
    }
    let sections = collect_cpp_sections(tree.root_node(), source);
    if sections.is_empty() {
        return;
    }
    for entity in entities.iter_mut() {
        if !matches!(
            entity.kind,
            EntityKind::Method
                | EntityKind::Constructor
                | EntityKind::Destructor
                | EntityKind::Operator
                | EntityKind::Field
                | EntityKind::Property
                | EntityKind::Variable
                | EntityKind::Function
        ) {
            continue;
        }
        if entity.metadata.contains_key("visibility") {
            continue;
        }
        let start = entity.span.start_byte;
        let end = entity.span.end_byte;
        // Innermost enclosing class wins; later sections override earlier.
        let mut best: Option<&CppSection> = None;
        for section in &sections {
            if section.class_start <= start && end <= section.class_end && section.start <= start {
                match best {
                    Some(b) if b.class_start > section.class_start => {}
                    Some(b) if b.class_start == section.class_start && b.start > section.start => {}
                    _ => best = Some(section),
                }
            }
        }
        if let Some(section) = best {
            entity.set_metadata("visibility", section.visibility.clone());
        }
    }
}

struct CppSection {
    class_start: usize,
    class_end: usize,
    start: usize,
    visibility: String,
}

fn collect_cpp_sections(root: tree_sitter::Node, source: &str) -> Vec<CppSection> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let kind = node.kind();
        if kind == "class_specifier" || kind == "struct_specifier" {
            collect_class_sections(node, source, &mut out);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    out
}

fn collect_class_sections(class_node: tree_sitter::Node, source: &str, out: &mut Vec<CppSection>) {
    let is_struct = class_node.kind() == "struct_specifier";
    push_boundaries(
        class_node,
        source,
        is_struct,
        out,
        class_node.start_byte(),
        class_node.end_byte(),
    );
}

fn push_boundaries(
    class_node: tree_sitter::Node,
    source: &str,
    is_struct: bool,
    out: &mut Vec<CppSection>,
    class_start: usize,
    class_end: usize,
) {
    let mut current = if is_struct { "public" } else { "private" }.to_string();
    out.push(CppSection {
        class_start,
        class_end,
        start: class_start,
        visibility: current.clone(),
    });
    // Walk in source order (pre-order with sorted children by start byte).
    let mut nodes = Vec::new();
    let mut stack = vec![class_node];
    while let Some(node) = stack.pop() {
        nodes.push(node);
        let mut cursor = node.walk();
        let mut children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
        children.sort_by_key(|n| std::cmp::Reverse(n.start_byte()));
        for child in children {
            // Do not descend into nested class definitions; they get their
            // own boundaries from the outer collect pass.
            if child.id() != class_node.id()
                && (child.kind() == "class_specifier" || child.kind() == "struct_specifier")
            {
                continue;
            }
            stack.push(child);
        }
    }
    nodes.sort_by_key(|n| n.start_byte());
    for node in nodes {
        if node.kind() == "access_specifier" {
            let text = node
                .utf8_text(source.as_bytes())
                .unwrap_or("")
                .trim()
                .trim_end_matches(':')
                .trim()
                .to_lowercase();
            if text == "public" || text == "protected" || text == "private" {
                current = text;
                out.push(CppSection {
                    class_start,
                    class_end,
                    start: node.end_byte(),
                    visibility: current.clone(),
                });
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ts(code: &str) -> (tree_sitter::Tree, String) {
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).expect("ts");
        let tree = parser.parse(code, None).expect("parse");
        (tree, code.to_string())
    }

    #[test]
    fn marks_exported_and_default() {
        let code =
            "export function foo() {}\nfunction bar() {}\nexport default function baz() {}\n";
        let (tree, source) = parse_ts(code);
        // Derive entity spans from the actual `function_declaration` nodes so
        // the test does not depend on hand-counted byte offsets.
        let mut decls: Vec<(String, usize, usize)> = Vec::new();
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if node.kind() == "function_declaration" {
                if let Some(name) = node.child_by_field_name("name") {
                    let text = source[name.start_byte()..name.end_byte()].to_string();
                    decls.push((text, node.start_byte(), node.end_byte()));
                }
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        decls.sort_by_key(|(_, s, _)| *s);
        assert_eq!(decls.len(), 3);
        let mut entities: Vec<Entity> = decls
            .iter()
            .map(|(name, s, e)| make_entity(name, *s, *e))
            .collect();
        mark_exported_entities(&mut entities, &tree, &source, &Language::TypeScript);
        assert_eq!(
            entities[0].metadata.get("is_exported").map(String::as_str),
            Some("true")
        );
        assert!(!entities[1].metadata.contains_key("is_exported"));
        assert_eq!(
            entities[2].metadata.get("is_exported").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            entities[2].metadata.get("is_default").map(String::as_str),
            Some("true")
        );
    }

    fn make_entity(name: &str, start: usize, end: usize) -> Entity {
        let mut e = Entity {
            name: name.to_string(),
            ..Default::default()
        };
        e.span.start_byte = start;
        e.span.end_byte = end;
        e
    }
}
