//! Data types and shared helpers for structured output.
//!
//! Contains serializable entry types, language-aware visibility resolution,
//! relation filtering, span formatting, and markdown escape utilities used by
//! all rendering modules.

use std::collections::{BTreeMap, HashSet};

use cce_codegraph::RelationIndex;
use cce_codegraph::index::EntityIndexOps;
use cce_codegraph::policy::{cpp, csharp, dart, go, java, javascript, python, rust};
use cce_codegraph::symbol::Visibility;
use cce_types::{Entity, EntityId, EntityKind, ParsedFile};

// ---------------------------------------------------------------------------
// Public data structures (for JSON / programmatic use)
// ---------------------------------------------------------------------------

/// Serializable symbol entry for structured output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub file_path: String,
    pub visibility: String,
    pub span: String,
    pub parent: Option<String>,
    pub return_type: Option<String>,
    pub parameters: Vec<(String, Option<String>)>,
    pub doc_present: bool,
}

/// Serializable relation entry for structured output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelationEntry {
    pub caller: String,
    pub caller_file: String,
    pub callee: String,
    pub callee_file: Option<String>,
    pub relation_type: String,
    pub span: String,
    pub is_external: bool,
}

/// Project summary for structured output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectSummary {
    pub project_name: String,
    pub file_count: usize,
    pub entity_count: usize,
    pub relation_count: usize,
    pub type_relation_count: usize,
    pub call_count: usize,
    pub dependency_count: usize,
    pub languages: Vec<String>,
    pub entities_by_kind: BTreeMap<String, usize>,
}

// ---------------------------------------------------------------------------
// Markdown helpers
// ---------------------------------------------------------------------------

pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

pub fn escape_md(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ").replace('\r', "")
}

/// Normalize a nullable type spelling for report display.
///
/// The same nullable meaning surfaces in several spellings
/// (`Optional[str]` vs `str | None`, `String?` vs `String | null`), which
/// splits the `Shape` column and the distinct-shape count. This maps the
/// two-member nullable union to the language's canonical display form and
/// leaves everything else untouched. Display-only: raw bindings keep their
/// source spelling.
pub fn normalize_nullable_display(language: cce_types::language::Language, value: &str) -> String {
    use cce_types::language::Language;
    let trimmed = value.trim();
    // Split a top-level `|` union into members.
    let members: Vec<String> = trimmed
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if members.len() != 2 {
        return value.to_string();
    }
    let null_names: &[&str] = match language {
        Language::Python => &["None", "NoneType"],
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
            &["null", "undefined"]
        }
        Language::Rust => &["None"],
        Language::Go => &["nil"],
        Language::CSharp | Language::Kotlin | Language::Java => &["null"],
        Language::Dart => &["Null", "null"],
        Language::Scala => &["None", "Null", "null"],
        _ => return value.to_string(),
    };
    let null_pos = members
        .iter()
        .position(|m| null_names.iter().any(|n| *m == **n));
    let Some(null_idx) = null_pos else {
        return value.to_string();
    };
    let inner = members[1 - null_idx].clone();
    if inner.is_empty() {
        return value.to_string();
    }
    match language {
        Language::Python | Language::Scala => format!("Optional[{inner}]"),
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
            value.to_string()
        }
        _ => {
            if inner.contains([' ', '|', '&', '<', '>', '[', ']', '(', ')', ',']) {
                value.to_string()
            } else {
                format!("{inner}?")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Span helpers
// ---------------------------------------------------------------------------

pub fn span_str(entity: &Entity) -> String {
    if let Some((s, e)) = entity.span.line_range_opt() {
        if s == e {
            format!("L{s}")
        } else {
            format!("L{s}-L{e}")
        }
    } else {
        "n/a".to_string()
    }
}

pub fn span_str_from_span(span: &cce_types::Span) -> String {
    if let Some((s, e)) = span.line_range_opt() {
        if s == e {
            format!("L{s}")
        } else {
            format!("L{s}-L{e}")
        }
    } else {
        "n/a".to_string()
    }
}

// ---------------------------------------------------------------------------
// Visibility resolution
// ---------------------------------------------------------------------------

pub fn visibility_display(vis: &Visibility) -> String {
    match vis {
        Visibility::Public => "public",
        Visibility::Package | Visibility::Module | Visibility::Super => "package",
        Visibility::Private => "private",
        Visibility::Restricted { .. } => "restricted",
        Visibility::Protected => "protected",
        Visibility::Internal => "internal",
        Visibility::ProtectedInternal => "protected internal",
        Visibility::PrivateProtected => "private protected",
        Visibility::Friend { .. } => "friend",
    }
    .to_string()
}

fn fallback_signal_to_visibility(signal: &str) -> Option<Visibility> {
    let t = signal.trim();
    if t == "pub" || t == "public" || t == "export" || t == "exported" {
        Some(Visibility::Public)
    } else if t == "pub(crate)" || t == "crate" || t == "internal" || t == "package" {
        Some(Visibility::Package)
    } else if t == "protected" || t == "protected internal" || t == "private protected" {
        Some(Visibility::Protected)
    } else if t == "private" {
        Some(Visibility::Private)
    } else {
        None
    }
}

fn policy_signal_to_visibility(signal: &str, lang: &str) -> Option<Visibility> {
    match lang {
        "rust" => rust::visibility_from_signal(signal),
        "go" => go::visibility_from_signal(signal),
        "python" | "py" => python::visibility_from_signal(signal),
        "dart" => dart::visibility_from_signal(signal),
        "java" | "kotlin" | "scala" => java::visibility_from_signal(signal),
        "c#" | "csharp" => csharp::visibility_from_signal(signal),
        "c++" | "cpp" | "c" | "h" => cpp::visibility_from_signal(signal),
        "javascript" | "js" | "jsx" | "typescript" | "ts" | "tsx" => {
            javascript::visibility_from_signal(signal)
        }
        _ => fallback_signal_to_visibility(signal),
    }
}

/// Language-aware visibility for display.
///
/// Dispatch mirrors `cce-parser::relation_helpers::detect_entity_visibility`
/// and the authoritative `cce-relation::policy` modules: explicit modifier
/// signals first, then the `metadata.visibility` signal, then Python
/// `__all__` membership, then naming rules, finally the language default.
pub fn visibility_of(entity: &Entity, language: &str) -> String {
    let lang = language.to_lowercase();
    let lang = lang.as_str();
    for modifier in &entity.modifiers {
        if let Some(vis) = policy_signal_to_visibility(&modifier.to_lowercase(), lang) {
            return visibility_display(&vis);
        }
    }
    if let Some(signal) = entity.metadata.get("visibility") {
        if let Some(vis) = policy_signal_to_visibility(&signal.to_lowercase(), lang) {
            return visibility_display(&vis);
        }
    }
    if matches!(lang, "python" | "py") {
        if let Some(flag) = entity.metadata.get("is_exported_by_all") {
            if flag == "true" {
                return "public".to_string();
            } else if flag == "false" {
                return "private".to_string();
            }
        }
    }
    let name_vis = match lang {
        "go" => go::visibility_from_name(&entity.name),
        "python" | "py" => python::visibility_from_name(&entity.name),
        "dart" => dart::visibility_from_name(&entity.name),
        "javascript" | "js" | "jsx" | "typescript" | "ts" | "tsx" => {
            javascript::visibility_from_name(&entity.name)
        }
        _ => None,
    };
    if let Some(vis) = name_vis {
        return visibility_display(&vis);
    }
    let default = match lang {
        "rust" => rust::default_visibility(),
        "go" => go::default_visibility(&entity.name),
        "python" | "py" => python::default_visibility(&entity.name),
        "dart" => dart::default_visibility(&entity.name),
        "java" | "kotlin" | "scala" => java::default_visibility(),
        "c#" | "csharp" => csharp::default_visibility(),
        "c" | "h" | "php" | "ruby" => Visibility::Public,
        "c++" | "cpp" => {
            if entity.parent.is_some() {
                Visibility::Private
            } else {
                Visibility::Public
            }
        }
        "javascript" | "js" | "jsx" | "typescript" | "ts" | "tsx" => {
            javascript::default_visibility()
        }
        _ => Visibility::Public,
    };
    visibility_display(&default)
}

// ---------------------------------------------------------------------------
// Relation filtering helpers
// ---------------------------------------------------------------------------

/// String literals (`"Alice"`, `'go'`) are call arguments, not modules.
/// They leak into dependency/reference edges when argument extraction
/// mistakes a literal for a symbol reference; drop them at render time.
pub fn is_string_literal_ref(name: &str) -> bool {
    let t = name.trim();
    t.len() >= 2
        && ((t.starts_with('"') && t.ends_with('"'))
            || (t.starts_with('\'') && t.ends_with('\''))
            || (t.starts_with('`') && t.ends_with('`')))
}

/// Collect local value names (variables, parameters, receivers) for a file.
pub fn local_value_names(entities: &[(EntityId, Entity)]) -> HashSet<String> {
    let mut names = HashSet::new();
    for (_, e) in entities {
        match e.kind {
            EntityKind::Variable | EntityKind::Field | EntityKind::Property => {
                names.insert(e.name.clone());
                if let Some(base) = e.name.split(['.', ':']).next() {
                    names.insert(base.to_string());
                }
            }
            EntityKind::Function | EntityKind::Method => {
                for (pname, _) in &e.parameters {
                    names.insert(pname.clone());
                    if let Some(base) = pname.split(['.', ':']).next() {
                        names.insert(base.to_string());
                    }
                }
                if let Some(recv) = e.metadata.get("receiver_type") {
                    let base = recv
                        .trim_start_matches('*')
                        .split(['.', '<', '['])
                        .next()
                        .unwrap_or(recv)
                        .trim();
                    if !base.is_empty() {
                        names.insert(base.to_string());
                    }
                }
            }
            _ => {}
        }
        if let Some(recv) = e.metadata.get("receiver_type") {
            let base = recv
                .trim_start_matches('*')
                .split(['.', '<', '['])
                .next()
                .unwrap_or(recv)
                .trim();
            if !base.is_empty() {
                names.insert(base.to_string());
            }
        }
    }
    names
}

/// Whether a `dependency.module` target is noise: a bare local identifier
/// (or a `local.member` path rooted at a local) rather than a real module.
pub fn is_noise_module_target(callee: &str, kind: &str, local_names: &HashSet<String>) -> bool {
    if kind != "dependency.module" {
        return false;
    }
    let t = callee
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if t.is_empty() {
        return true;
    }
    let base = t.split(['.', '/', ':', '\\']).next().unwrap_or(t);
    if !t.contains(['.', '/', ':']) {
        return local_names.contains(t);
    }
    local_names.contains(base)
}

// ---------------------------------------------------------------------------
// Index helpers
// ---------------------------------------------------------------------------

/// Resolve the language string for a file in the index.
pub fn file_language(index: &RelationIndex, file_path: &str) -> String {
    index
        .file_records()
        .read()
        .get(file_path)
        .map(|r| r.info.language.clone())
        .unwrap_or_default()
}

/// Return all files ordered alphabetically with their entities sorted by span.
pub fn ordered_files(index: &RelationIndex) -> BTreeMap<String, Vec<(EntityId, Entity)>> {
    let mut map: BTreeMap<String, Vec<(EntityId, Entity)>> = BTreeMap::new();
    for entry in index.function_index().iter() {
        let id = *entry.key();
        let entity = entry.value().clone();
        let file = index
            .get_file_path_by_entity(id)
            .unwrap_or_else(|| "unknown".to_string());
        map.entry(file).or_default().push((id, entity));
    }
    for vec in map.values_mut() {
        vec.sort_by_key(|(_, e)| e.span.start_position.row);
    }
    map
}

/// Resolve the display name of an inferred return binding.
///
/// Return bindings produced by [`infer_project_contexts`](crate::type_inference_assert::infer_project_contexts)
/// are keyed by parsed-file-local [`EntityId`]s, while per-file reports are
/// rendered against index-space entity tables whose numeric IDs live in a
/// different space. Looking a local ID up in the index table first
/// misattributes names (same number, different entity), so the parsed file
/// — which shares the binding's ID space — is authoritative and consulted
/// first. The index tables are only a fallback for bindings whose entity is
/// absent locally (e.g. no parsed file was retained for the report).
pub fn resolve_inferred_return_name(
    eid: &EntityId,
    parsed_file: Option<&ParsedFile>,
    index_entities: &[(EntityId, Entity)],
    global_id_to_entity: &BTreeMap<EntityId, Entity>,
) -> String {
    if let Some(pf) = parsed_file
        && let Some(entity) = pf.entities.iter().find(|e| e.id == *eid)
    {
        return entity.name.clone();
    }
    if let Some((_, entity)) = index_entities.iter().find(|(id, _)| id == eid) {
        return entity.name.clone();
    }
    if let Some(entity) = global_id_to_entity.get(eid) {
        return entity.name.clone();
    }
    format!("EntityId({})", eid.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityKind, Language, Span};

    fn entity(id: u64, name: &str) -> Entity {
        Entity::new(
            EntityId(id),
            EntityKind::Function,
            name.to_string(),
            Span::default(),
        )
    }

    fn parsed_file_with(names: &[&str]) -> ParsedFile {
        let mut file = ParsedFile::new(Language::Kotlin, "Overloads.kt".to_string(), String::new());
        for (idx, name) in names.iter().enumerate() {
            file.add_entity(entity(idx as u64 + 1, name));
        }
        file
    }

    /// Regression test for the per-file/aggregated report mismatch: local
    /// ID `1` means `combine` in the parsed file but `ints` in the index
    /// space. The parsed file must win so both reports agree.
    #[test]
    fn test_return_name_prefers_parsed_file_id_space() {
        let pf = parsed_file_with(&["combine", "run"]);
        let index_entities = vec![
            (EntityId(1), entity(1, "ints")),
            (EntityId(2), entity(2, "mixed")),
        ];
        let mut global = BTreeMap::new();
        global.insert(EntityId(1), entity(1, "ints"));
        global.insert(EntityId(2), entity(2, "mixed"));

        assert_eq!(
            resolve_inferred_return_name(&EntityId(1), Some(&pf), &index_entities, &global),
            "combine"
        );
        assert_eq!(
            resolve_inferred_return_name(&EntityId(2), Some(&pf), &index_entities, &global),
            "run"
        );
    }

    #[test]
    fn test_return_name_falls_back_without_parsed_file() {
        let index_entities = vec![(EntityId(7), entity(7, "helper"))];
        let global = BTreeMap::new();
        assert_eq!(
            resolve_inferred_return_name(&EntityId(7), None, &index_entities, &global),
            "helper"
        );
        assert_eq!(
            resolve_inferred_return_name(&EntityId(9), None, &index_entities, &global),
            "EntityId(9)"
        );
    }

    #[test]
    fn test_nullable_display_unifies_spellings() {
        assert_eq!(
            normalize_nullable_display(Language::Python, "str | None"),
            "Optional[str]"
        );
        assert_eq!(
            normalize_nullable_display(Language::Kotlin, "String | null"),
            "String?"
        );
        assert_eq!(
            normalize_nullable_display(Language::Dart, "String | Null"),
            "String?"
        );
        // Non-nullable unions and other languages keep their spelling.
        assert_eq!(
            normalize_nullable_display(Language::Python, "str | int"),
            "str | int"
        );
        assert_eq!(
            normalize_nullable_display(Language::TypeScript, "string | null"),
            "string | null"
        );
    }
}
