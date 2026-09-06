//! Implicit hierarchy derivation: Go interface method-set satisfaction.
//!
//! Go has no `implements` keyword: a type satisfies an interface purely by
//! implementing its method set. The extractor therefore never emits an
//! explicit edge, so this post-pass derives `Implementation` edges from the
//! project-global [`TypeMemberIndex`] once every file has been indexed.
//!
//! Rules (conservative by design):
//! - Only Go `Struct`/`TypeAlias` types against Go `Interface` types in the
//!   same directory (a directory is a package, except `_test` packages which
//!   share method sets anyway for this purpose).
//! - The interface method set must be non-empty (the empty interface is
//!   satisfied by everything and would explode the graph) and bounded.
//! - Member comparison is by method name; only members whose defining file
//!   sits in the same directory count, so placeholder-merged foreign methods
//!   cannot cause false positives.
//! - Embedded (promoted) methods are not expanded yet; types that satisfy an
//!   interface only through embedding are a known limitation.

use std::collections::{HashMap, HashSet};

use cce_types::entity::{EntityId, EntityKind};
use cce_types::language::Language;

use crate::symbol_table::TypeMemberIndex;

/// Maximum interface method count considered for derivation.
///
/// Huge interfaces (generated mocks, `any`-like unions) are skipped to keep
/// the derived graph precise; they can still be queried by name.
pub const MAX_INTERFACE_METHODS: usize = 64;

/// One derived `Struct -> Interface` satisfaction edge.
///
/// Entity ids are in the parsed-file-local space carried by the type index;
/// the caller remaps them into the index-global space before inserting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoImplementsEdge {
    /// Implementing struct (or type alias) entity id (type-index space).
    pub struct_id: EntityId,
    /// File the struct is defined in.
    pub struct_file: String,
    /// Struct simple name (for diagnostics and edge labels).
    pub struct_name: String,
    /// Satisfied interface entity id (type-index space).
    pub iface_id: EntityId,
    /// File the interface is defined in.
    pub iface_file: String,
    /// Interface simple name.
    pub iface_name: String,
}

/// Parent directory of a file path (`a/b/c.go` -> `a/b`).
fn parent_dir(file_path: &str) -> &str {
    match file_path.rfind('/') {
        Some(pos) => &file_path[..pos],
        None => "",
    }
}

/// Derive Go interface-satisfaction edges from the global type index.
///
/// See the module documentation for the matching rules. The output is
/// sorted by `(struct_file, struct_name, iface_name)` for determinism.
pub fn derive_go_implements(global: &TypeMemberIndex) -> Vec<GoImplementsEdge> {
    // Method sets keyed by (directory, simple type name), unioned across
    // per-file entries so split definitions still see the full set.
    let mut struct_methods: HashMap<(String, String), HashSet<String>> = HashMap::new();
    // Representative (non-placeholder) entity endpoint per struct.
    let mut struct_repr: HashMap<(String, String), (EntityId, String, String)> = HashMap::new();
    // Interface method sets with their endpoints.
    let mut interfaces: Vec<(String, String, HashSet<String>, EntityId, String, String)> =
        Vec::new();

    for entry in global.all_types() {
        if entry.language != Language::Go {
            continue;
        }
        let dir = parent_dir(&entry.key.file_path).to_string();
        let simple = entry.key.simple.clone();
        match entry.kind {
            EntityKind::Struct | EntityKind::TypeAlias => {
                let methods: HashSet<String> = entry
                    .members
                    .iter()
                    .filter(|(_, members)| members.iter().any(|m| parent_dir(&m.file_path) == dir))
                    .map(|(name, _)| name.clone())
                    .collect();
                if methods.is_empty() {
                    continue;
                }
                let key = (dir, simple.clone());
                struct_methods
                    .entry(key.clone())
                    .or_default()
                    .extend(methods);
                if !entry.is_placeholder {
                    struct_repr.entry(key).or_insert((
                        entry.entity_id,
                        entry.key.file_path.clone(),
                        simple,
                    ));
                }
            }
            EntityKind::Interface => {
                if entry.is_placeholder {
                    continue;
                }
                let methods: HashSet<String> = entry
                    .members
                    .iter()
                    .filter(|(_, members)| members.iter().any(|m| parent_dir(&m.file_path) == dir))
                    .map(|(name, _)| name.clone())
                    .collect();
                if methods.is_empty() || methods.len() > MAX_INTERFACE_METHODS {
                    continue;
                }
                interfaces.push((
                    dir,
                    simple.clone(),
                    methods,
                    entry.entity_id,
                    entry.key.file_path.clone(),
                    simple,
                ));
            }
            _ => {}
        }
    }

    let mut edges = Vec::new();
    for (iface_dir, iface_simple, iface_methods, iface_id, iface_file, iface_name) in &interfaces {
        for ((struct_dir, struct_simple), methods) in &struct_methods {
            if struct_dir != iface_dir || struct_simple == iface_simple {
                continue;
            }
            if !iface_methods.iter().all(|m| methods.contains(m)) {
                continue;
            }
            let Some((struct_id, struct_file, struct_name)) =
                struct_repr.get(&(struct_dir.clone(), struct_simple.clone()))
            else {
                continue;
            };
            if *struct_id == *iface_id {
                continue;
            }
            edges.push(GoImplementsEdge {
                struct_id: *struct_id,
                struct_file: struct_file.clone(),
                struct_name: struct_name.clone(),
                iface_id: *iface_id,
                iface_file: iface_file.clone(),
                iface_name: iface_name.clone(),
            });
        }
    }
    edges.sort_by(|a, b| {
        (&a.struct_file, &a.struct_name, &a.iface_name).cmp(&(
            &b.struct_file,
            &b.struct_name,
            &b.iface_name,
        ))
    });
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::Visibility;
    use crate::symbol_table::type_index::{MemberEntry, TypeEntry, TypeKey};

    use cce_types::Span;

    fn type_key(qualified: &str, simple: &str, file: &str) -> TypeKey {
        TypeKey::new(qualified.to_string(), simple.to_string(), file.to_string())
    }

    fn member(name: &str, id: u64, file: &str) -> MemberEntry {
        MemberEntry {
            entity_id: EntityId(id),
            name: name.to_string(),
            kind: EntityKind::Method,
            visibility: Visibility::Public,
            is_static: false,
            is_associated: false,
            span: Span::default(),
            file_path: file.to_string(),
            module_path: None,
            package: String::new(),
        }
    }

    fn insert_go_type(
        index: &mut TypeMemberIndex,
        kind: EntityKind,
        qualified: &str,
        simple: &str,
        file: &str,
        id: u64,
        methods: &[(&str, u64)],
    ) {
        let key = type_key(qualified, simple, file);
        let entry = TypeEntry::new(
            EntityId(id),
            key.clone(),
            kind,
            Language::Go,
            Visibility::Public,
        );
        index.insert_type(key.clone(), entry);
        for (method_name, method_id) in methods {
            assert!(
                index
                    .insert_member(&key, member(method_name, *method_id, file))
                    .is_none(),
                "test member insert should not duplicate"
            );
        }
    }

    #[test]
    fn test_derive_go_implements_basic() {
        let mut index = TypeMemberIndex::new();
        insert_go_type(
            &mut index,
            EntityKind::Interface,
            "pkg.Stringer",
            "Stringer",
            "pkg/a.go",
            1,
            &[("String", 10)],
        );
        insert_go_type(
            &mut index,
            EntityKind::Struct,
            "pkg.Person",
            "Person",
            "pkg/b.go",
            2,
            &[("String", 20), ("Name", 21)],
        );
        // Different directory: same names must not match.
        insert_go_type(
            &mut index,
            EntityKind::Struct,
            "other.Person",
            "Person",
            "other/c.go",
            3,
            &[("String", 30)],
        );

        let edges = derive_go_implements(&index);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].struct_name, "Person");
        assert_eq!(edges[0].iface_name, "Stringer");
        assert_eq!(edges[0].struct_id, EntityId(2));
        assert_eq!(edges[0].iface_id, EntityId(1));
    }

    #[test]
    fn test_derive_go_implements_skips_empty_interface() {
        let mut index = TypeMemberIndex::new();
        insert_go_type(
            &mut index,
            EntityKind::Interface,
            "pkg.Any",
            "Any",
            "pkg/a.go",
            1,
            &[],
        );
        insert_go_type(
            &mut index,
            EntityKind::Struct,
            "pkg.Person",
            "Person",
            "pkg/b.go",
            2,
            &[("String", 20)],
        );

        assert!(derive_go_implements(&index).is_empty());
    }

    #[test]
    fn test_derive_go_implements_requires_subset() {
        let mut index = TypeMemberIndex::new();
        insert_go_type(
            &mut index,
            EntityKind::Interface,
            "pkg.Named",
            "Named",
            "pkg/a.go",
            1,
            &[("Name", 10), ("Missing", 11)],
        );
        insert_go_type(
            &mut index,
            EntityKind::Struct,
            "pkg.Person",
            "Person",
            "pkg/b.go",
            2,
            &[("Name", 20)],
        );

        assert!(derive_go_implements(&index).is_empty());
    }
}
