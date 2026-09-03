use std::collections::{HashMap, HashSet};

use cce_types::Span;
use cce_types::entity::{EntityId, EntityKind};
use cce_types::language::Language;
use serde::{Deserialize, Serialize};

use crate::symbol::{ScopeContext, Visibility};

pub mod snapshot;
pub mod string_pool;

pub use snapshot::{MemberSummary, TypeMemberSnapshot, TypeSummary};
pub use string_pool::StringPoolBuilder;

/// Stable key identifying a type definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeKey {
    pub qualified: String,
    pub simple: String,
    pub file_path: String,
}

impl TypeKey {
    pub fn new(qualified: String, simple: String, file_path: String) -> Self {
        Self {
            qualified,
            simple,
            file_path,
        }
    }
}

/// Entry for a type and its members.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeEntry {
    pub entity_id: EntityId,
    pub key: TypeKey,
    pub kind: EntityKind,
    pub language: Language,
    pub visibility: Visibility,
    pub members: HashMap<String, Vec<MemberEntry>>,
    pub fields: HashMap<String, MemberEntry>,
    pub constructors: Vec<MemberEntry>,
    pub is_placeholder: bool,
}

impl TypeEntry {
    pub fn new(
        entity_id: EntityId,
        key: TypeKey,
        kind: EntityKind,
        language: Language,
        visibility: Visibility,
    ) -> Self {
        Self {
            entity_id,
            key,
            kind,
            language,
            visibility,
            members: HashMap::new(),
            fields: HashMap::new(),
            constructors: Vec::new(),
            is_placeholder: false,
        }
    }

    pub fn placeholder(key: TypeKey, language: Language) -> Self {
        Self {
            entity_id: EntityId(0),
            key,
            kind: EntityKind::Struct,
            language,
            visibility: Visibility::Public,
            members: HashMap::new(),
            fields: HashMap::new(),
            constructors: Vec::new(),
            is_placeholder: true,
        }
    }
}

/// Returned by `insert_member` / `merge_from` when a duplicate member is detected.
pub struct DuplicateEvent {
    pub type_key: TypeKey,
    pub member_name: String,
    pub duplicate_entity_id: EntityId,
}

/// A single member belonging to a type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberEntry {
    pub entity_id: EntityId,
    pub name: String,
    pub kind: EntityKind,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_associated: bool,
    pub span: Span,
    pub file_path: String,
    pub module_path: Option<String>,
    pub package: String,
}

impl MemberEntry {
    pub fn defined_scope(&self) -> ScopeContext {
        match &self.module_path {
            Some(mp) => ScopeContext::with_module(&self.file_path, &self.package, mp),
            None => ScopeContext::new(&self.file_path, &self.package),
        }
    }
}

/// Index maintaining type -> members relationships.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeMemberIndex {
    types: HashMap<TypeKey, TypeEntry>,
    member_to_type: HashMap<EntityId, TypeKey>,
    qualified_index: HashMap<String, Vec<TypeKey>>,
    simple_index: HashMap<String, Vec<TypeKey>>,
}

impl TypeMemberIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len_types(&self) -> usize {
        self.types.len()
    }

    pub fn len_members(&self) -> usize {
        self.member_to_type.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    pub fn clear(&mut self) {
        self.types.clear();
        self.member_to_type.clear();
        self.qualified_index.clear();
        self.simple_index.clear();
    }

    pub fn insert_type(&mut self, key: TypeKey, mut entry: TypeEntry) {
        entry.key = key.clone();
        if self.types.contains_key(&key) {
            return;
        }
        self.qualified_index
            .entry(key.qualified.clone())
            .or_default()
            .push(key.clone());
        self.simple_index
            .entry(key.simple.clone())
            .or_default()
            .push(key.clone());
        self.types.insert(key, entry);
    }

    pub fn upsert_type_placeholder(&mut self, key: TypeKey, language: Language) -> &mut TypeEntry {
        if !self.types.contains_key(&key) {
            let placeholder = TypeEntry::placeholder(key.clone(), language);
            self.qualified_index
                .entry(key.qualified.clone())
                .or_default()
                .push(key.clone());
            self.simple_index
                .entry(key.simple.clone())
                .or_default()
                .push(key.clone());
            self.types.insert(key.clone(), placeholder);
        }
        self.types.get_mut(&key).expect("just inserted")
    }

    pub fn complete_placeholder(
        &mut self,
        key: &TypeKey,
        entity_id: EntityId,
        kind: EntityKind,
        visibility: Visibility,
    ) {
        if let Some(entry) = self.types.get_mut(key) {
            if entry.is_placeholder {
                entry.entity_id = entity_id;
                entry.kind = kind;
                entry.visibility = visibility;
                entry.is_placeholder = false;
            }
        }
    }

    pub fn get_type_by_key(&self, key: &TypeKey) -> Option<&TypeEntry> {
        self.types.get(key)
    }

    pub fn get_type(&self, qualified: &str) -> Option<&TypeEntry> {
        let keys = self.qualified_index.get(qualified)?;
        for k in keys {
            if let Some(e) = self.types.get(k) {
                if !e.is_placeholder {
                    return Some(e);
                }
            }
        }
        for k in keys {
            if let Some(e) = self.types.get(k) {
                return Some(e);
            }
        }
        None
    }

    pub fn get_type_by_simple(&self, simple: &str) -> Vec<&TypeEntry> {
        let mut out = Vec::new();
        if let Some(keys) = self.simple_index.get(simple) {
            for k in keys {
                if let Some(e) = self.types.get(k) {
                    out.push(e);
                }
            }
        }
        out
    }

    pub fn get_members(&self, qualified: &str, name: &str) -> Option<&[MemberEntry]> {
        let entry = self.get_type(qualified)?;
        entry.members.get(name).map(|v| v.as_slice())
    }

    pub fn insert_member(
        &mut self,
        type_key: &TypeKey,
        member: MemberEntry,
    ) -> Option<DuplicateEvent> {
        let is_ctor = member.kind == EntityKind::Constructor;
        let is_field = matches!(member.kind, EntityKind::Field | EntityKind::Property);
        if let Some(entry) = self.types.get_mut(type_key) {
            self.member_to_type
                .insert(member.entity_id, type_key.clone());
            if is_ctor {
                let is_dup = entry
                    .constructors
                    .iter()
                    .any(|m| m.entity_id == member.entity_id);
                if !is_dup {
                    entry.constructors.push(member);
                    None
                } else {
                    Some(DuplicateEvent {
                        type_key: type_key.clone(),
                        member_name: "constructor".into(),
                        duplicate_entity_id: member.entity_id,
                    })
                }
            } else if is_field {
                let is_dup = entry.fields.contains_key(&member.name);
                let name = member.name.clone();
                let entity_id = member.entity_id;
                entry.fields.entry(name.clone()).or_insert(member);
                if is_dup {
                    Some(DuplicateEvent {
                        type_key: type_key.clone(),
                        member_name: name,
                        duplicate_entity_id: entity_id,
                    })
                } else {
                    None
                }
            } else {
                let list = entry.members.entry(member.name.clone()).or_default();
                let is_dup = list.iter().any(|m| m.entity_id == member.entity_id);
                if !is_dup {
                    list.push(member);
                    None
                } else {
                    Some(DuplicateEvent {
                        type_key: type_key.clone(),
                        member_name: member.name,
                        duplicate_entity_id: member.entity_id,
                    })
                }
            }
        } else {
            None
        }
    }

    pub fn owner_of(&self, member_id: EntityId) -> Option<&TypeKey> {
        self.member_to_type.get(&member_id)
    }

    pub fn type_keys(&self) -> Vec<&TypeKey> {
        self.types.keys().collect()
    }

    pub fn all_types(&self) -> Vec<&TypeEntry> {
        self.types.values().collect()
    }

    pub fn file_contribution_keys(&self, file_path: &str) -> HashSet<TypeKey> {
        self.types
            .keys()
            .filter(|k| k.file_path == file_path)
            .cloned()
            .collect()
    }

    pub fn remove_file_contribution(&mut self, file_path: &str) {
        let keys: Vec<TypeKey> = self
            .types
            .keys()
            .filter(|k| k.file_path == file_path)
            .cloned()
            .collect();
        for k in keys {
            if let Some(entry) = self.types.remove(&k) {
                for member in entry.members.values().flat_map(|v| v.iter()) {
                    self.member_to_type.remove(&member.entity_id);
                }
                for member in entry.fields.values() {
                    self.member_to_type.remove(&member.entity_id);
                }
                for member in &entry.constructors {
                    self.member_to_type.remove(&member.entity_id);
                }
                if let Some(vec) = self.qualified_index.get_mut(&k.qualified) {
                    vec.retain(|x| x != &k);
                    if vec.is_empty() {
                        self.qualified_index.remove(&k.qualified);
                    }
                }
                if let Some(vec) = self.simple_index.get_mut(&k.simple) {
                    vec.retain(|x| x != &k);
                    if vec.is_empty() {
                        self.simple_index.remove(&k.simple);
                    }
                }
            }
        }
        let member_keys: Vec<EntityId> = self
            .member_to_type
            .iter()
            .filter_map(|(mid, tk)| {
                if tk.file_path == file_path {
                    Some(*mid)
                } else {
                    None
                }
            })
            .collect();
        for mid in member_keys {
            self.member_to_type.remove(&mid);
        }
        // Also need to remove members whose type lives in another file but member defined in this file.
        // Those members were inserted via placeholder types; they remain under placeholder entry's file_path?
        // Placeholder key file_path is where member defined? Actually placeholder key's file_path is where first member seen.
        // So removal of file where placeholder created will remove entire placeholder type, which is fine.
        // For scattered Go methods: type defined in file A, methods in file B appended to same global entry but per-module indexes keep separation.
        // For per-module removal, we only remove keys matching file_path, so other file's type remains.
        for entry in self.types.values_mut() {
            for members in entry.members.values_mut() {
                members.retain(|m| m.file_path != file_path);
            }
            entry.members.retain(|_, v| !v.is_empty());
            entry.fields.retain(|_, m| m.file_path != file_path);
            entry.constructors.retain(|m| m.file_path != file_path);
        }
        // second pass to clean member_to_type for orphaned members removed above where type not removed
        // We already retained only members not matching file_path, but member_to_type still has entries for removed members.
        // To clean, iterate member_to_type and check if member still exists in types.
        let still_present: HashSet<EntityId> = self
            .types
            .values()
            .flat_map(|e| {
                e.members
                    .values()
                    .flat_map(|v| v.iter().map(|m| m.entity_id))
                    .chain(e.fields.values().map(|m| m.entity_id))
                    .chain(e.constructors.iter().map(|m| m.entity_id))
            })
            .collect();
        self.member_to_type
            .retain(|mid, _| still_present.contains(mid));
    }

    pub fn merge_from(&mut self, other: &TypeMemberIndex) -> Vec<DuplicateEvent> {
        let mut dups = Vec::new();
        for (key, entry) in &other.types {
            if let Some(existing) = self.types.get_mut(key) {
                // merge members
                for (name, members) in &entry.members {
                    let list = existing.members.entry(name.clone()).or_default();
                    for m in members {
                        if !list
                            .iter()
                            .any(|e| e.entity_id == m.entity_id && e.file_path == m.file_path)
                        {
                            list.push(m.clone());
                            self.member_to_type.insert(m.entity_id, key.clone());
                        } else {
                            dups.push(DuplicateEvent {
                                type_key: key.clone(),
                                member_name: m.name.clone(),
                                duplicate_entity_id: m.entity_id,
                            });
                        }
                    }
                }
                for (name, field) in &entry.fields {
                    let is_dup = existing.fields.contains_key(name);
                    existing
                        .fields
                        .entry(name.clone())
                        .or_insert_with(|| field.clone());
                    self.member_to_type.insert(field.entity_id, key.clone());
                    if is_dup {
                        dups.push(DuplicateEvent {
                            type_key: key.clone(),
                            member_name: name.clone(),
                            duplicate_entity_id: field.entity_id,
                        });
                    }
                }
                for ctor in &entry.constructors {
                    if !existing
                        .constructors
                        .iter()
                        .any(|m| m.entity_id == ctor.entity_id)
                    {
                        existing.constructors.push(ctor.clone());
                        self.member_to_type.insert(ctor.entity_id, key.clone());
                    } else {
                        dups.push(DuplicateEvent {
                            type_key: key.clone(),
                            member_name: ctor.name.clone(),
                            duplicate_entity_id: ctor.entity_id,
                        });
                    }
                }
                if existing.is_placeholder && !entry.is_placeholder {
                    existing.entity_id = entry.entity_id;
                    existing.kind = entry.kind;
                    existing.visibility = entry.visibility.clone();
                    existing.is_placeholder = false;
                }
            } else {
                // insert new type; need to update indexes
                self.qualified_index
                    .entry(key.qualified.clone())
                    .or_default()
                    .push(key.clone());
                self.simple_index
                    .entry(key.simple.clone())
                    .or_default()
                    .push(key.clone());
                for members in entry.members.values() {
                    for m in members {
                        self.member_to_type.insert(m.entity_id, key.clone());
                    }
                }
                for field in entry.fields.values() {
                    self.member_to_type.insert(field.entity_id, key.clone());
                }
                for ctor in &entry.constructors {
                    self.member_to_type.insert(ctor.entity_id, key.clone());
                }
                self.types.insert(key.clone(), entry.clone());
            }
        }
        dups
    }

    /// Complete placeholder types by merging them into real types with the
    /// same simple name. When a placeholder (e.g. `b::Foo` created by an
    /// impl in file b for a struct defined in file a) has members that
    /// belong to a real type (`a::Foo`), this method moves those members
    /// into the real type and removes the placeholder.
    pub fn complete_placeholders_from(&mut self, source: &TypeMemberIndex) {
        // Collect placeholder keys from source that have a matching real type in self
        let mut to_merge: Vec<(TypeKey, TypeKey)> = Vec::new(); // (placeholder_key, real_key)
        for (src_key, src_entry) in &source.types {
            if !src_entry.is_placeholder {
                continue;
            }
            // Find a real (non-placeholder) type in self with the same simple name
            if let Some(real_key) = self
                .simple_index
                .get(&src_entry.key.simple)
                .and_then(|keys| {
                    keys.iter().find(|k| {
                        self.types.get(k).is_some_and(|e| {
                            !e.is_placeholder && e.key.simple == src_entry.key.simple
                        })
                    })
                })
            {
                to_merge.push((src_key.clone(), real_key.clone()));
            }
        }
        for (placeholder_key, real_key) in to_merge {
            if let Some(placeholder_entry) = source.types.get(&placeholder_key) {
                // Merge members from placeholder into real type
                if let Some(real_entry) = self.types.get_mut(&real_key) {
                    // Derive the correct module path from the real type's qualified name
                    let correct_module_path = real_key
                        .qualified
                        .rsplit_once("::")
                        .map(|(m, _)| m.to_string());
                    for (name, members) in &placeholder_entry.members {
                        let list = real_entry.members.entry(name.clone()).or_default();
                        for m in members {
                            if !list
                                .iter()
                                .any(|e| e.entity_id == m.entity_id && e.file_path == m.file_path)
                            {
                                self.member_to_type.insert(m.entity_id, real_key.clone());
                                let mut fixed = m.clone();
                                fixed.module_path = correct_module_path.clone();
                                list.push(fixed);
                            }
                        }
                    }
                    for (name, field) in &placeholder_entry.fields {
                        real_entry
                            .fields
                            .entry(name.clone())
                            .or_insert_with(|| field.clone());
                        self.member_to_type
                            .insert(field.entity_id, real_key.clone());
                    }
                    for ctor in &placeholder_entry.constructors {
                        if !real_entry
                            .constructors
                            .iter()
                            .any(|m| m.entity_id == ctor.entity_id)
                        {
                            self.member_to_type.insert(ctor.entity_id, real_key.clone());
                            real_entry.constructors.push(ctor.clone());
                        }
                    }
                }
                // Remove the specific placeholder type from all indexes
                self.remove_type(&placeholder_key);
            }
        }
    }

    /// Remove a specific type entry and clean up all secondary indexes.
    fn remove_type(&mut self, key: &TypeKey) {
        if let Some(entry) = self.types.remove(key) {
            // Remove member_to_type entries
            for member in entry.members.values().flat_map(|v| v.iter()) {
                self.member_to_type.remove(&member.entity_id);
            }
            for member in entry.fields.values() {
                self.member_to_type.remove(&member.entity_id);
            }
            for member in &entry.constructors {
                self.member_to_type.remove(&member.entity_id);
            }
            // Clean qualified_index
            if let Some(vec) = self.qualified_index.get_mut(&key.qualified) {
                vec.retain(|x| x != key);
                if vec.is_empty() {
                    self.qualified_index.remove(&key.qualified);
                }
            }
            // Clean simple_index
            if let Some(vec) = self.simple_index.get_mut(&key.simple) {
                vec.retain(|x| x != key);
                if vec.is_empty() {
                    self.simple_index.remove(&key.simple);
                }
            }
        }
    }

    pub fn strip_generics(s: &str) -> &str {
        if let Some(pos) = s.find('<') {
            &s[..pos]
        } else {
            s
        }
    }

    pub fn normalize_type_part(s: &str) -> String {
        let stripped = Self::strip_generics(s).trim();
        let crate_stripped = stripped.strip_prefix("crate::").unwrap_or(stripped);
        crate_stripped.trim().to_string()
    }

    pub fn resolve_qualified(
        &self,
        type_part: &str,
        member_part: &str,
        from_scope: &ScopeContext,
        language: Language,
    ) -> Option<MemberEntry> {
        let type_norm = Self::normalize_type_part(type_part);
        let member_norm = Self::normalize_type_part(member_part);
        if type_norm.is_empty() || member_norm.is_empty() {
            return None;
        }
        // skip this/self/Self receivers
        let lower = type_norm.to_ascii_lowercase();
        if lower == "self" || lower == "this" || lower == "super" {
            return None;
        }

        let candidates = self.candidates_for_type_and_member(&type_norm, &member_norm);
        if candidates.is_empty() {
            // try simple fallback
            let simple_type = type_norm.rsplit([':', '.']).next().unwrap_or(&type_norm);
            let simple_norm = Self::normalize_type_part(simple_type);
            if simple_norm != type_norm {
                let fallback = self.candidates_for_type_and_member(&simple_norm, &member_norm);
                return self.pick_visible(fallback, from_scope, language);
            }
            return None;
        }
        self.pick_visible(candidates, from_scope, language)
    }

    fn candidates_for_type_and_member(
        &self,
        type_part: &str,
        member_part: &str,
    ) -> Vec<MemberEntry> {
        let mut out = Vec::new();
        // try qualified exact first
        if let Some(keys) = self.qualified_index.get(type_part) {
            for key in keys {
                if let Some(entry) = self.types.get(key) {
                    if let Some(members) = entry.members.get(member_part) {
                        out.extend(members.clone());
                    }
                    if let Some(field) = entry.fields.get(member_part) {
                        out.push(field.clone());
                    }
                    for ctor in &entry.constructors {
                        if ctor.name == member_part {
                            out.push(ctor.clone());
                        }
                    }
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
        // fallback to simple type name matching (when type_part is qualified but stored qualified differs)
        // search all types where simple == type_part or qualified ends with type_part
        if let Some(keys) = self.simple_index.get(type_part) {
            for key in keys {
                if let Some(entry) = self.types.get(key) {
                    if let Some(members) = entry.members.get(member_part) {
                        out.extend(members.clone());
                    }
                    if let Some(field) = entry.fields.get(member_part) {
                        out.push(field.clone());
                    }
                    for ctor in &entry.constructors {
                        if ctor.name == member_part {
                            out.push(ctor.clone());
                        }
                    }
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
        // also try suffix match for qualified (e.g., a::b::Type stored as Type with module a::b)
        for (key, entry) in &self.types {
            if key.qualified == type_part
                || key.qualified.ends_with(&format!("::{}", type_part))
                || key.qualified.ends_with(&format!(".{}", type_part))
            {
                if let Some(members) = entry.members.get(member_part) {
                    out.extend(members.clone());
                }
                if let Some(field) = entry.fields.get(member_part) {
                    out.push(field.clone());
                }
                for ctor in &entry.constructors {
                    if ctor.name == member_part {
                        out.push(ctor.clone());
                    }
                }
            }
        }
        out
    }

    fn pick_visible(
        &self,
        mut candidates: Vec<MemberEntry>,
        from_scope: &ScopeContext,
        language: Language,
    ) -> Option<MemberEntry> {
        if candidates.is_empty() {
            return None;
        }
        // filter by visibility
        candidates.retain(|m| {
            let defined = m.defined_scope();
            m.visibility.is_visible_from(from_scope, &defined, language)
        });
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            return Some(candidates.remove(0));
        }
        // deterministic sort: file_path + module_path
        candidates.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.module_path.cmp(&b.module_path))
                .then_with(|| a.entity_id.0.cmp(&b.entity_id.0))
        });
        Some(candidates.remove(0))
    }
}

#[cfg(test)]
mod tests;
