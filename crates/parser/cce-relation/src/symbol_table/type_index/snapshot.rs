use std::collections::HashMap;

use cce_types::Span;
use cce_types::entity::{EntityId, EntityKind};
use cce_types::language::Language;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

use crate::symbol::Visibility;

use super::string_pool::StringPoolBuilder;
use super::{MemberEntry, TypeEntry, TypeKey, TypeMemberIndex};

/// rkyv-safe type summary. Strings are replaced by `u32` indices into a
/// shared string pool to eliminate redundant storage.
#[derive(Debug, Clone, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct TypeSummary {
    pub entity_id: EntityId,
    pub qualified_idx: u32,
    pub simple_idx: u32,
    pub kind: EntityKind,
    pub language: Language,
    pub visibility: Visibility,
    pub is_placeholder: bool,
    pub file_path_idx: u32,
    pub member_count: u32,
    pub field_count: u32,
    pub constructor_count: u32,
}

/// rkyv-safe member summary.
#[derive(Debug, Clone, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct MemberSummary {
    pub entity_id: EntityId,
    pub name_idx: u32,
    pub kind: EntityKind,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_associated: bool,
    pub file_path_idx: u32,
}

/// Archived snapshot of `TypeMemberIndex`.
///
/// Uses a flat `Vec<String>` string pool and `u32` indices to minimize
/// serialized size. The pool and indices are rkyv-derivable because all
/// fields are scalars or `Vec` (no `HashMap`).
#[derive(Debug, Clone, Archive, RkyvDeserialize, RkyvSerialize)]
pub struct TypeMemberSnapshot {
    pub string_pool: Vec<String>,
    pub types: Vec<TypeSummary>,
    pub members: Vec<MemberSummary>,
    pub qualified_index: Vec<(u32, Vec<u32>)>,
    pub simple_index: Vec<(u32, Vec<u32>)>,
}

impl TypeMemberIndex {
    /// Export this index as a rkyv-archivable snapshot.
    ///
    /// Builds a string pool to deduplicate `file_path`, `qualified`, `simple`,
    /// and `name` strings, then flattens all types and members into `Vec`-based
    /// structures. O(n) in the number of types + members.
    pub fn to_snapshot(&self) -> TypeMemberSnapshot {
        let mut pool = StringPoolBuilder::new();

        // Track TypeKey -> position in the types Vec for index construction
        let mut key_to_pos: HashMap<TypeKey, u32> = HashMap::new();
        let mut types = Vec::with_capacity(self.types.len());
        let mut members = Vec::new();

        for (key, entry) in &self.types {
            let pos = types.len() as u32;
            key_to_pos.insert(key.clone(), pos);
            types.push(TypeSummary {
                entity_id: entry.entity_id,
                qualified_idx: pool.intern(&key.qualified),
                simple_idx: pool.intern(&key.simple),
                kind: entry.kind,
                language: entry.language,
                visibility: entry.visibility.clone(),
                is_placeholder: entry.is_placeholder,
                file_path_idx: pool.intern(&key.file_path),
                member_count: entry.members.len() as u32,
                field_count: entry.fields.len() as u32,
                constructor_count: entry.constructors.len() as u32,
            });

            for member_list in entry.members.values() {
                for m in member_list {
                    members.push(MemberSummary {
                        entity_id: m.entity_id,
                        name_idx: pool.intern(&m.name),
                        kind: m.kind,
                        visibility: m.visibility.clone(),
                        is_static: m.is_static,
                        is_associated: m.is_associated,
                        file_path_idx: pool.intern(&m.file_path),
                    });
                }
            }
            for field in entry.fields.values() {
                members.push(MemberSummary {
                    entity_id: field.entity_id,
                    name_idx: pool.intern(&field.name),
                    kind: field.kind,
                    visibility: field.visibility.clone(),
                    is_static: field.is_static,
                    is_associated: field.is_associated,
                    file_path_idx: pool.intern(&field.file_path),
                });
            }
            for ctor in &entry.constructors {
                members.push(MemberSummary {
                    entity_id: ctor.entity_id,
                    name_idx: pool.intern(&ctor.name),
                    kind: ctor.kind,
                    visibility: ctor.visibility.clone(),
                    is_static: ctor.is_static,
                    is_associated: ctor.is_associated,
                    file_path_idx: pool.intern(&ctor.file_path),
                });
            }
        }

        // Build qualified_index and simple_index before consuming the pool
        let mut qualified_map: HashMap<String, Vec<u32>> = HashMap::new();
        let mut simple_map: HashMap<String, Vec<u32>> = HashMap::new();
        for key in self.types.keys() {
            let pos = key_to_pos[key];
            qualified_map
                .entry(key.qualified.clone())
                .or_default()
                .push(pos);
            simple_map.entry(key.simple.clone()).or_default().push(pos);
        }

        let string_pool = pool.into_pool();

        let qualified_index: Vec<(u32, Vec<u32>)> = qualified_map
            .into_iter()
            .map(|(name, positions)| {
                let name_idx = string_pool.iter().position(|s| s == &name).unwrap_or(0) as u32;
                (name_idx, positions)
            })
            .collect();

        let simple_index: Vec<(u32, Vec<u32>)> = simple_map
            .into_iter()
            .map(|(name, positions)| {
                let name_idx = string_pool.iter().position(|s| s == &name).unwrap_or(0) as u32;
                (name_idx, positions)
            })
            .collect();

        TypeMemberSnapshot {
            string_pool,
            types,
            members,
            qualified_index,
            simple_index,
        }
    }

    /// Restore a `TypeMemberIndex` from an archived snapshot.
    ///
    /// Rebuilds the `HashMap`-based indexes from the flat snapshot data.
    /// O(n) in the number of types + members.
    pub fn from_snapshot(snapshot: TypeMemberSnapshot) -> Self {
        let get_str = |idx: u32| -> &str {
            snapshot
                .string_pool
                .get(idx as usize)
                .map(|s| s.as_str())
                .unwrap_or("")
        };

        let mut types: HashMap<TypeKey, TypeEntry> = HashMap::new();
        let mut member_to_type: HashMap<EntityId, TypeKey> = HashMap::new();

        for ts in snapshot.types.iter() {
            let qualified = get_str(ts.qualified_idx).to_string();
            let simple = get_str(ts.simple_idx).to_string();
            let file_path = get_str(ts.file_path_idx).to_string();
            let key = TypeKey::new(qualified, simple, file_path);

            let mut entry = TypeEntry::new(
                ts.entity_id,
                key.clone(),
                ts.kind,
                ts.language,
                ts.visibility.clone(),
            );
            entry.is_placeholder = ts.is_placeholder;
            types.insert(key, entry);
        }

        // Populate members by iterating the flat member list.
        // We need to know which type each member belongs to.
        // The snapshot doesn't have explicit owner info per member, so we
        // reconstruct from the type summaries' counts.
        {
            let mut member_offset = 0usize;
            for ts in snapshot.types.iter() {
                let qualified = get_str(ts.qualified_idx).to_string();
                let simple = get_str(ts.simple_idx).to_string();
                let file_path = get_str(ts.file_path_idx).to_string();
                let key = TypeKey::new(qualified, simple, file_path);

                let entry = types.get_mut(&key).unwrap();

                // members
                for ms in snapshot
                    .members
                    .iter()
                    .skip(member_offset)
                    .take(ts.member_count as usize)
                {
                    let member = MemberEntry {
                        entity_id: ms.entity_id,
                        name: get_str(ms.name_idx).to_string(),
                        kind: ms.kind,
                        visibility: ms.visibility.clone(),
                        is_static: ms.is_static,
                        is_associated: ms.is_associated,
                        span: Span::default(),
                        file_path: get_str(ms.file_path_idx).to_string(),
                        module_path: None,
                        package: String::new(),
                    };
                    member_to_type.insert(ms.entity_id, key.clone());
                    entry
                        .members
                        .entry(member.name.clone())
                        .or_default()
                        .push(member);
                }
                member_offset += ts.member_count as usize;

                // fields
                for ms in snapshot
                    .members
                    .iter()
                    .skip(member_offset)
                    .take(ts.field_count as usize)
                {
                    let member = MemberEntry {
                        entity_id: ms.entity_id,
                        name: get_str(ms.name_idx).to_string(),
                        kind: ms.kind,
                        visibility: ms.visibility.clone(),
                        is_static: ms.is_static,
                        is_associated: ms.is_associated,
                        span: Span::default(),
                        file_path: get_str(ms.file_path_idx).to_string(),
                        module_path: None,
                        package: String::new(),
                    };
                    member_to_type.insert(ms.entity_id, key.clone());
                    entry.fields.insert(member.name.clone(), member);
                }
                member_offset += ts.field_count as usize;

                // constructors
                for ms in snapshot
                    .members
                    .iter()
                    .skip(member_offset)
                    .take(ts.constructor_count as usize)
                {
                    let member = MemberEntry {
                        entity_id: ms.entity_id,
                        name: get_str(ms.name_idx).to_string(),
                        kind: ms.kind,
                        visibility: ms.visibility.clone(),
                        is_static: ms.is_static,
                        is_associated: ms.is_associated,
                        span: Span::default(),
                        file_path: get_str(ms.file_path_idx).to_string(),
                        module_path: None,
                        package: String::new(),
                    };
                    member_to_type.insert(ms.entity_id, key.clone());
                    entry.constructors.push(member);
                }
                member_offset += ts.constructor_count as usize;
            }
        }

        // Rebuild qualified_index and simple_index
        let mut qualified_index: HashMap<String, Vec<TypeKey>> = HashMap::new();
        let mut simple_index: HashMap<String, Vec<TypeKey>> = HashMap::new();
        for key in types.keys() {
            qualified_index
                .entry(key.qualified.clone())
                .or_default()
                .push(key.clone());
            simple_index
                .entry(key.simple.clone())
                .or_default()
                .push(key.clone());
        }

        TypeMemberIndex {
            types,
            member_to_type,
            qualified_index,
            simple_index,
        }
    }
}
