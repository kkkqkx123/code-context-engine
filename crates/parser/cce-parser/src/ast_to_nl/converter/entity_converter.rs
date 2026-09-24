//! Single entity conversion methods for AST to Natural Language conversion

use std::path::Path;

use crate::ast_to_nl::ConversionRequest;
use crate::grouper::EntityGroup;
use cce_types::{ConversionResult, EntityKind, GroupedEntity, OutputMode};

/// Prefix a module-level `function <name>` head with the file's module stem.
///
/// Matches the head even when a signature follows (`function make_response (`),
/// and never matches a longer identifier that merely starts with `name`.
pub(crate) fn qualify_function_text(name: &str, file_path: &str, text: &str) -> String {
    let Some(stem) = Path::new(file_path).file_stem() else {
        return text.to_string();
    };
    let module = stem.to_string_lossy();
    if module.is_empty() {
        return text.to_string();
    }
    // Needle search only matches an unqualified head: an already-qualified
    // `function logging.foo` does not contain `function foo` after `function `.
    let needle = format!("function {}", name);
    let mut search_from = 0usize;
    while let Some(rel) = text[search_from..].find(&needle) {
        let pos = search_from + rel;
        let after = pos + needle.len();
        let boundary = text[after..].chars().next();
        if boundary.is_none_or(|c| !c.is_alphanumeric() && c != '_') {
            let mut out = String::with_capacity(text.len() + module.len() + 2);
            out.push_str(&text[..pos]);
            out.push_str(&format!("function {}.{}", module, name));
            out.push_str(&text[after..]);
            return out;
        }
        search_from = after;
    }
    text.to_string()
}

/// Qualify a member's NL head with the owning group name (`Flask.make_response`).
///
/// Matches the member name only inside the first line (the kind/signature head)
/// and only as a standalone token not already prefixed by `group.`, so prose
/// and body mentions are left alone.
pub(crate) fn qualify_member_head(group_name: &str, member_name: &str, text: &str) -> String {
    if group_name.is_empty() || group_name == member_name || member_name.is_empty() {
        return text.to_string();
    }
    let first_nl = text.find('\n').unwrap_or(text.len());
    let head = &text[..first_nl];
    let mut search_from = 0usize;
    while let Some(rel) = head[search_from..].find(member_name) {
        let pos = search_from + rel;
        let after = pos + member_name.len();
        let before = pos.checked_sub(1).map(|i| head.as_bytes()[i]);
        let next = head.as_bytes().get(after).copied();
        let boundary_before = before.is_none_or(|b| b != b'.' && !is_ident_byte(b));
        let boundary_after = next.is_none_or(|b| !is_ident_byte(b));
        if boundary_before && boundary_after {
            let mut out = String::with_capacity(text.len() + group_name.len() + 1);
            out.push_str(&text[..pos]);
            out.push_str(group_name);
            out.push('.');
            out.push_str(&text[pos..]);
            return out;
        }
        search_from = after;
    }
    text.to_string()
}

/// Qualify every module-level `function <name>` head in joined group text.
///
/// Walks the header, members, and nested groups so heads absorbed into a
/// parent group's descriptions (function-with-members, skipped members,
/// nested function groups) receive the file's module stem — not only the
/// group header itself. Needles are entity names, so prose such as
/// `function callable` is left alone unless a real function carries that name.
pub(crate) fn qualify_group_function_heads(
    group: &EntityGroup,
    file_path: &str,
    text: &str,
) -> String {
    let mut out = text.to_string();
    if let Some(ref header) = group.header {
        if header.kind == EntityKind::Function {
            out = qualify_function_text(&header.name, file_path, &out);
        }
    }
    for member in &group.members {
        if member.kind == EntityKind::Function {
            out = qualify_function_text(&member.name, file_path, &out);
        }
    }
    for nested in group.nested_groups.iter() {
        out = qualify_group_function_heads(nested, file_path, &out);
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

impl super::AstToNlConverter {
    /// Convert a GroupedEntity to natural language based on output mode
    pub fn convert_grouped(
        &self,
        entity: &GroupedEntity,
        file_path: &str,
        request: Option<&ConversionRequest>,
    ) -> ConversionResult {
        match self.resolve_mode(request) {
            OutputMode::Bm25 => self.convert_bm25_grouped(entity, file_path),
            OutputMode::Embedding => self.convert_embedding_grouped(entity, file_path),
            OutputMode::Both => self.convert_both_grouped(entity, file_path),
        }
    }

    /// Module-level functions carry no class owner in their NL text; qualify
    /// them with the file's module stem so same-named methods stay distinct
    /// (`helpers.make_response` vs `Flask.make_response`).
    fn qualify_module_level(entity: &GroupedEntity, file_path: &str, text: &str) -> String {
        if entity.kind != EntityKind::Function {
            return text.to_string();
        }
        qualify_function_text(&entity.name, file_path, text)
    }

    fn convert_bm25_grouped(&self, entity: &GroupedEntity, file_path: &str) -> ConversionResult {
        let bm25_text = self.bm25_generator.generate(entity);
        let keywords = self.bm25_generator.extract_keywords(entity);

        ConversionResult::bm25_only(
            entity.id,
            entity.kind,
            entity.name.clone(),
            file_path.to_string(),
            bm25_text,
            keywords,
        )
    }

    fn convert_embedding_grouped(
        &self,
        entity: &GroupedEntity,
        file_path: &str,
    ) -> ConversionResult {
        let embedding_text = self.embedding_generator.generate(entity);
        let embedding_text = Self::qualify_module_level(entity, file_path, &embedding_text);

        ConversionResult::embedding_only(
            entity.id,
            entity.kind,
            entity.name.clone(),
            file_path.to_string(),
            embedding_text,
        )
    }

    fn convert_both_grouped(&self, entity: &GroupedEntity, file_path: &str) -> ConversionResult {
        let bm25_text = self.bm25_generator.generate(entity);
        let embedding_text = self.embedding_generator.generate(entity);
        let embedding_text = Self::qualify_module_level(entity, file_path, &embedding_text);
        let keywords = self.bm25_generator.extract_keywords(entity);

        ConversionResult::new(
            entity.id,
            entity.kind,
            entity.name.clone(),
            file_path.to_string(),
            bm25_text,
            embedding_text,
            keywords,
        )
    }
}
