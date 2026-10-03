//! EntityGroup and GroupedEntity conversions between Rust and Lua tables.

use mlua::{Lua, Table, Value};

use cce_types::Language;
use cce_types::entity::{EntityId, GroupedEntity};
use cce_types::grouper::{EntityGroup, GroupType};

use super::table_accessors::{get_string, get_string_array, get_string_map};

/// Convert an EntityGroup to a Lua table
///
/// This function creates a Lua table representation of the EntityGroup that can be
/// passed to Lua plugins. It includes all relevant fields including metadata.
pub fn entity_group_to_lua_table(lua: &Lua, group: &EntityGroup) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;

    // Basic group info
    table.set("group_id", group.group_id.as_str())?;
    table.set("group_type", format!("{:?}", group.group_type))?;
    table.set("name", group.name.as_str())?;
    table.set("kind", format!("{:?}", group.kind))?;
    table.set("language", format!("{:?}", group.language))?;

    // Header entity
    if let Some(ref header) = group.header {
        let header_table = grouped_entity_to_lua_table(lua, header)?;
        table.set("header", header_table)?;
    } else {
        table.set("header", mlua::Nil)?;
    }

    // Member entities
    let members_table = lua.create_table()?;
    for (idx, member) in group.members.iter().enumerate() {
        let member_table = grouped_entity_to_lua_table(lua, member)?;
        members_table.set(idx + 1, member_table)?;
    }
    table.set("members", members_table)?;

    // Pattern info
    table.set("pattern_info", format!("{:?}", group.pattern_info))?;

    // Metadata - entity-specific information
    let metadata_table = lua.create_table()?;
    for (key, value) in &group.metadata {
        metadata_table.set(key.as_str(), value.as_str())?;
    }
    table.set("metadata", metadata_table)?;

    // Nesting info
    table.set("nesting_level", group.nesting_level)?;
    table.set("has_significant_nested", group.has_significant_nested)?;
    if let Some(ref parent_id) = group.parent_group_id {
        table.set("parent_group_id", parent_id.as_str())?;
    } else {
        table.set("parent_group_id", mlua::Nil)?;
    }

    // Header reference
    if let Some(ref header_id) = group.header_id {
        let id_table = lua.create_table()?;
        id_table.set("id", header_id.0)?;
        table.set("header_id", id_table)?;
    } else {
        table.set("header_id", mlua::Nil)?;
    }

    // Member reference IDs
    let member_ids_table = lua.create_table()?;
    for (idx, member_id) in group.member_ids.iter().enumerate() {
        member_ids_table.set(idx + 1, member_id.0)?;
    }
    table.set("member_ids", member_ids_table)?;

    // Member roles
    let member_roles_table = lua.create_table()?;
    for (idx, (entity_id, role)) in group.member_roles.iter().enumerate() {
        let role_table = lua.create_table()?;
        role_table.set("entity_id", entity_id.0)?;
        role_table.set("role", role.to_string())?;
        member_roles_table.set(idx + 1, role_table)?;
    }
    table.set("member_roles", member_roles_table)?;

    // Source span
    let span_table = lua.create_table()?;
    span_table.set("start_byte", group.span.start_byte)?;
    span_table.set("end_byte", group.span.end_byte)?;
    let start_pos = lua.create_table()?;
    start_pos.set("row", group.span.start_position.row)?;
    start_pos.set("column", group.span.start_position.column)?;
    span_table.set("start_position", start_pos)?;
    let end_pos = lua.create_table()?;
    end_pos.set("row", group.span.end_position.row)?;
    end_pos.set("column", group.span.end_position.column)?;
    span_table.set("end_position", end_pos)?;
    table.set("span", span_table)?;

    // Nested groups (recursive conversion)
    let nested_table = lua.create_table()?;
    for (idx, nested) in group.nested_groups.iter().enumerate() {
        let nested_group_table = entity_group_to_lua_table(lua, nested)?;
        nested_table.set(idx + 1, nested_group_table)?;
    }
    table.set("nested_groups", nested_table)?;

    Ok(table)
}

/// Convert a GroupedEntity to a Lua table
///
/// Creates a Lua table representation of a single entity with all its properties
/// including entity-specific metadata.
pub fn grouped_entity_to_lua_table(
    lua: &Lua,
    entity: &GroupedEntity,
) -> Result<Table, mlua::Error> {
    let table = lua.create_table()?;

    // Basic entity info
    table.set("id", entity.id.0)?;
    table.set("name", entity.name.as_str())?;
    table.set("kind", format!("{:?}", entity.kind))?;
    table.set("signature", entity.signature.as_str())?;

    // Parameters
    let params_table = lua.create_table()?;
    for (idx, (name, ty)) in entity.parameters.iter().enumerate() {
        let param_table = lua.create_table()?;
        param_table.set("name", name.as_str())?;
        if let Some(type_str) = ty {
            param_table.set("type", type_str.as_str())?;
        } else {
            param_table.set("type", mlua::Nil)?;
        }
        params_table.set(idx + 1, param_table)?;
    }
    table.set("parameters", params_table)?;

    // Return type
    if let Some(ref ret_type) = entity.return_type {
        table.set("return_type", ret_type.as_str())?;
    } else {
        table.set("return_type", mlua::Nil)?;
    }

    // Doc comment
    if let Some(ref doc) = entity.doc_comment {
        table.set("doc_comment", doc.as_str())?;
    } else {
        table.set("doc_comment", mlua::Nil)?;
    }

    // Stdlib info
    table.set("is_stdlib", entity.is_stdlib)?;
    if let Some(ref category) = entity.stdlib_category {
        table.set("stdlib_category", format!("{:?}", category))?;
    } else {
        table.set("stdlib_category", mlua::Nil)?;
    }

    // Metadata - important for NL generation
    let metadata_table = lua.create_table()?;
    for (key, value) in &entity.metadata {
        metadata_table.set(key.as_str(), value.as_str())?;
    }
    table.set("metadata", metadata_table)?;

    Ok(table)
}

/// Patch an [`EntityGroup`] from a Lua table, using `fallback` for any field
/// the plugin did not provide.
pub fn lua_table_to_entity_group(
    table: &Table,
    fallback: EntityGroup,
) -> Result<EntityGroup, mlua::Error> {
    let mut group = fallback;

    if let Some(s) = get_string(table, "group_id") {
        group.group_id = compact_str::CompactString::from(s);
    }
    if let Some(s) = get_string(table, "group_type") {
        if let Ok(gt) = serde_json::from_value::<GroupType>(serde_json::Value::String(s)) {
            group.group_type = gt;
        }
    }
    if let Some(s) = get_string(table, "name") {
        group.name = compact_str::CompactString::from(s);
    }
    if let Some(s) = get_string(table, "kind") {
        if let Some(kind) = entity_kind_from_string(&s) {
            group.kind = kind;
        }
    }
    if let Some(s) = get_string(table, "language") {
        if let Ok(lang) = serde_json::from_value::<Language>(serde_json::Value::String(s)) {
            group.language = lang;
        }
    }
    if let Some(s) = get_string(table, "pattern_info") {
        if let Ok(pi) = serde_json::from_str(&s) {
            group.pattern_info = pi;
        }
    }
    group.metadata = get_string_map(table, "metadata");
    if let Some(s) = get_string(table, "parent_group_id") {
        group.parent_group_id = Some(compact_str::CompactString::from(s));
    }
    if let Some(n) = table.get::<Option<u64>>("nesting_level")? {
        group.nesting_level = n as usize;
    }
    if let Some(header_table) = table.get::<Option<Table>>("header")? {
        let current = group.header.take();
        if let Some(header) = lua_table_to_grouped_entity(&header_table, current)? {
            group.header = Some(header);
        }
    }
    if let Some(members_table) = table.get::<Option<Table>>("members")? {
        let mut members = Vec::new();
        let mut member_ids = Vec::new();
        for pair in members_table.pairs::<Value, Value>() {
            let (_, value) = pair?;
            if let Value::Table(member) = value {
                if let Ok(Some(ge)) = lua_table_to_grouped_entity(&member, None) {
                    member_ids.push(ge.id);
                    members.push(ge);
                }
            }
        }
        group.members = members.into();
        group.member_ids = member_ids.into();
    }
    Ok(group)
}

/// Patch a [`GroupedEntity`] from a Lua table, using `fallback` for missing fields.
pub fn lua_table_to_grouped_entity(
    table: &Table,
    fallback: Option<GroupedEntity>,
) -> Result<Option<GroupedEntity>, mlua::Error> {
    if table.is_empty() {
        return Ok(fallback);
    }
    let mut entity = fallback.unwrap_or_default();

    if let Some(id) = table.get::<Option<u64>>("id")? {
        entity.id = EntityId(id);
    }
    if let Some(s) = get_string(table, "name") {
        entity.name = s;
    }
    if let Some(s) = get_string(table, "kind") {
        if let Some(kind) = entity_kind_from_string(&s) {
            entity.kind = kind;
        }
    }
    if let Some(s) = get_string(table, "signature") {
        entity.signature = s;
    }
    if let Some(s) = get_string(table, "doc_comment") {
        entity.doc_comment = Some(s);
    }
    entity.metadata = get_string_map(table, "metadata");
    if let Some(mods) = get_string_array(table, "modifiers") {
        entity.modifiers = mods;
    }
    if let Some(s) = get_string(table, "subtype") {
        entity.subtype = Some(s);
    }
    if let Some(b) = table.get::<Option<bool>>("is_stdlib")? {
        entity.is_stdlib = b;
    }
    Ok(Some(entity))
}

/// Best-effort map from a string (Debug or serde snake_case form) to [`EntityKind`].
fn entity_kind_from_string(s: &str) -> Option<cce_types::EntityKind> {
    use cce_types::EntityKind;
    // Try the Debug form first (what `grouped_entity_to_lua_table` emits).
    if let Ok(kind) = serde_json::from_value::<EntityKind>(serde_json::Value::String(s.to_string()))
    {
        return Some(kind);
    }
    // Fall back to snake_case conversion.
    let snake = snake_case(s);
    serde_json::from_value::<EntityKind>(serde_json::Value::String(snake)).ok()
}

/// Convert a camel/PascalCase identifier to snake_case (ASCII).
fn snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::CompactString;
    use smallvec::SmallVec;
    use std::collections::HashMap;

    use cce_types::Span;
    use cce_types::grouper::{GroupType, PatternInfo};

    #[test]
    fn test_grouped_entity_to_lua_table() {
        let lua = Lua::new();
        let entity = GroupedEntity {
            id: cce_types::entity::EntityId(1),
            kind: cce_types::entity::EntityKind::Function,
            name: "test_function".to_string(),
            signature: "fn test_function() -> i32".to_string(),
            parameters: SmallVec::new(),
            return_type: Some("i32".to_string()),
            doc_comment: Some("A test function".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("endpoint".to_string(), "/api/test".to_string());
                m.insert("methods".to_string(), "GET,POST".to_string());
                m
            },
        };

        let table = grouped_entity_to_lua_table(&lua, &entity).expect("Failed to convert");

        // Verify basic fields
        let name: String = table.get("name").expect("Missing name");
        assert_eq!(name, "test_function");

        let kind: String = table.get("kind").expect("Missing kind");
        assert!(kind.contains("Function"));

        // Verify metadata
        let metadata: Table = table.get("metadata").expect("Missing metadata");
        let endpoint: String = metadata.get("endpoint").expect("Missing endpoint");
        assert_eq!(endpoint, "/api/test");

        let methods: String = metadata.get("methods").expect("Missing methods");
        assert_eq!(methods, "GET,POST");
    }

    #[test]
    fn test_entity_group_to_lua_table_with_metadata() {
        let lua = Lua::new();
        let group = EntityGroup {
            group_id: CompactString::from("test_group"),
            group_type: GroupType::ClassWithMethods,
            header: None,
            header_id: None,
            members: SmallVec::new(),
            member_ids: SmallVec::new(),
            entity_spans: HashMap::new(),
            combined_source: None,
            combined_source_lazy: std::sync::OnceLock::new(),
            span: Span::default(),
            kind: cce_types::entity::EntityKind::Class,
            name: CompactString::from("TestClass"),
            language: cce_types::language::Language::Python,
            pattern_info: PatternInfo::None,
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: {
                let mut m = HashMap::new();
                m.insert("route_pattern".to_string(), "/api/users".to_string());
                m
            },
            test_info: cce_types::TestInfo::unknown(),
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");

        // Verify group info
        let name: String = table.get("name").expect("Missing name");
        assert_eq!(name, "TestClass");

        let group_type: String = table.get("group_type").expect("Missing group_type");
        assert!(group_type.contains("ClassWithMethods"));

        // Verify metadata
        let metadata: Table = table.get("metadata").expect("Missing metadata");
        let route: String = metadata
            .get("route_pattern")
            .expect("Missing route_pattern");
        assert_eq!(route, "/api/users");

        // Verify new nesting fields
        let nesting_level: usize = table.get("nesting_level").expect("Missing nesting_level");
        assert_eq!(nesting_level, 0);

        let has_significant_nested: bool = table
            .get("has_significant_nested")
            .expect("Missing has_significant_nested");
        assert!(!has_significant_nested);

        let parent_group_id: mlua::Value = table
            .get("parent_group_id")
            .expect("Missing parent_group_id");
        assert!(matches!(parent_group_id, mlua::Value::Nil));

        // Verify header_id is nil when None
        let header_id: mlua::Value = table.get("header_id").expect("Missing header_id");
        assert!(matches!(header_id, mlua::Value::Nil));

        // Verify empty member_ids
        let member_ids: Table = table.get("member_ids").expect("Missing member_ids");
        let member_ids_len: i64 = member_ids.len().expect("Failed to get length");
        assert_eq!(member_ids_len, 0);

        // Verify empty member_roles
        let member_roles: Table = table.get("member_roles").expect("Missing member_roles");
        let member_roles_len: i64 = member_roles.len().expect("Failed to get length");
        assert_eq!(member_roles_len, 0);

        // Verify span
        let span: Table = table.get("span").expect("Missing span");
        let start_byte: usize = span.get("start_byte").expect("Missing start_byte");
        assert_eq!(start_byte, 0);

        // Verify nested_groups empty
        let nested_groups: Table = table.get("nested_groups").expect("Missing nested_groups");
        let nested_len: i64 = nested_groups.len().expect("Failed to get length");
        assert_eq!(nested_len, 0);
    }

    #[test]
    fn test_grouped_entity_with_none_optionals() {
        let lua = Lua::new();
        let entity = GroupedEntity {
            id: cce_types::entity::EntityId(2),
            kind: cce_types::entity::EntityKind::Function,
            name: "minimal".to_string(),
            signature: "fn minimal()".to_string(),
            parameters: SmallVec::new(),
            return_type: None,
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: HashMap::new(),
        };

        let table = grouped_entity_to_lua_table(&lua, &entity).expect("Failed to convert");

        let return_type: mlua::Value = table.get("return_type").expect("Missing return_type");
        assert!(matches!(return_type, mlua::Value::Nil));

        let doc_comment: mlua::Value = table.get("doc_comment").expect("Missing doc_comment");
        assert!(matches!(doc_comment, mlua::Value::Nil));
    }

    #[test]
    fn test_grouped_entity_with_parameters() {
        let lua = Lua::new();
        let entity = GroupedEntity {
            id: cce_types::entity::EntityId(3),
            kind: cce_types::entity::EntityKind::Function,
            name: "with_params".to_string(),
            signature: "fn(x: i32, y: String)".to_string(),
            parameters: {
                let mut params = SmallVec::new();
                params.push((CompactString::from("x"), Some(CompactString::from("i32"))));
                params.push((
                    CompactString::from("y"),
                    Some(CompactString::from("String")),
                ));
                params
            },
            return_type: Some("bool".to_string()),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: HashMap::new(),
        };

        let table = grouped_entity_to_lua_table(&lua, &entity).expect("Failed to convert");
        let params: Table = table.get("parameters").expect("Missing parameters");
        let len: i64 = params.len().expect("Failed to get length");
        assert_eq!(len, 2);

        let p1: Table = params.get(1).expect("Missing param 1");
        let name: String = p1.get("name").expect("Missing name");
        assert_eq!(name, "x");
    }

    #[test]
    fn test_entity_group_with_header_and_members() {
        let lua = Lua::new();
        let header = GroupedEntity::new(
            cce_types::entity::EntityId(10),
            cce_types::entity::EntityKind::Class,
            "MyClass".to_string(),
            "class MyClass".to_string(),
        );
        let member = GroupedEntity::new(
            cce_types::entity::EntityId(11),
            cce_types::entity::EntityKind::Method,
            "my_method".to_string(),
            "fn my_method()".to_string(),
        );

        let group = EntityGroup {
            group_id: CompactString::from("g1"),
            group_type: GroupType::ClassWithMethods,
            header: Some(header),
            header_id: Some(cce_types::entity::EntityId(10)),
            members: smallvec::smallvec![member],
            member_ids: smallvec::smallvec![cce_types::entity::EntityId(11)],
            name: CompactString::from("MyClass"),
            kind: cce_types::entity::EntityKind::Class,
            language: cce_types::language::Language::Python,
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");

        // Verify header
        let header_table: Table = table.get("header").expect("Missing header");
        let header_name: String = header_table.get("name").expect("Missing header name");
        assert_eq!(header_name, "MyClass");

        // Verify members
        let members_table: Table = table.get("members").expect("Missing members");
        let member_len: i64 = members_table.len().expect("Failed to get length");
        assert_eq!(member_len, 1);

        // Verify header_id
        let header_id_table: Table = table.get("header_id").expect("Missing header_id");
        let header_id_val: u64 = header_id_table.get("id").expect("Missing id");
        assert_eq!(header_id_val, 10);
    }

    #[test]
    fn test_entity_group_with_nested_groups() {
        let lua = Lua::new();
        let nested = EntityGroup {
            group_id: CompactString::from("nested1"),
            name: CompactString::from("NestedClass"),
            nesting_level: 1,
            parent_group_id: Some(CompactString::from("parent")),
            ..Default::default()
        };

        let group = EntityGroup {
            group_id: CompactString::from("parent"),
            name: CompactString::from("ParentClass"),
            nested_groups: Box::new([nested]),
            nesting_level: 0,
            has_significant_nested: true,
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");

        let nested_tables: Table = table.get("nested_groups").expect("Missing nested_groups");
        let len: i64 = nested_tables.len().expect("Failed to get length");
        assert_eq!(len, 1);

        let nested_t: Table = nested_tables.get(1).expect("Missing nested table");
        let nested_name: String = nested_t.get("name").expect("Missing name");
        assert_eq!(nested_name, "NestedClass");

        let is_nested: bool = table.get("has_significant_nested").expect("Missing flag");
        assert!(is_nested);
    }

    #[test]
    fn test_entity_group_with_member_roles() {
        let lua = Lua::new();
        use cce_types::grouper::MemberRole;

        let mut roles = SmallVec::new();
        roles.push((
            cce_types::entity::EntityId(1),
            MemberRole::BoilerplateMethod,
        ));
        roles.push((
            cce_types::entity::EntityId(2),
            MemberRole::BoilerplateMethod,
        ));

        let group = EntityGroup {
            group_id: CompactString::from("roles_group"),
            name: CompactString::from("RolesGroup"),
            member_roles: roles,
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");
        let roles_table: Table = table.get("member_roles").expect("Missing member_roles");
        let len: i64 = roles_table.len().expect("Failed to get length");
        assert_eq!(len, 2);

        let role1: Table = roles_table.get(1).expect("Missing role 1");
        let role_str: String = role1.get("role").expect("Missing role");
        assert_eq!(role_str, "boilerplate_method");
    }

    #[test]
    fn test_entity_group_with_non_default_span() {
        let lua = Lua::new();
        use cce_types::{Position, Span};

        let span = Span {
            start_byte: 100,
            end_byte: 500,
            start_position: Position { row: 5, column: 0 },
            end_position: Position {
                row: 20,
                column: 10,
            },
        };

        let group = EntityGroup {
            group_id: CompactString::from("span_group"),
            name: CompactString::from("SpanGroup"),
            span,
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");
        let span_table: Table = table.get("span").expect("Missing span");

        let start_byte: usize = span_table.get("start_byte").expect("Missing start_byte");
        assert_eq!(start_byte, 100);

        let end_byte: usize = span_table.get("end_byte").expect("Missing end_byte");
        assert_eq!(end_byte, 500);

        let start_pos: Table = span_table.get("start_position").expect("Missing start_pos");
        let row: usize = start_pos.get("row").expect("Missing row");
        assert_eq!(row, 5);
    }

    #[test]
    fn test_entity_group_with_member_ids() {
        let lua = Lua::new();
        use cce_types::entity::EntityId;

        let group = EntityGroup {
            group_id: CompactString::from("id_group"),
            name: CompactString::from("IdGroup"),
            member_ids: smallvec::smallvec![EntityId(101), EntityId(102), EntityId(103)],
            ..Default::default()
        };

        let table = entity_group_to_lua_table(&lua, &group).expect("Failed to convert");
        let member_ids: Table = table.get("member_ids").expect("Missing member_ids");
        let len: i64 = member_ids.len().expect("Failed to get length");
        assert_eq!(len, 3);

        let id1: u64 = member_ids.get(1).expect("Missing id 1");
        assert_eq!(id1, 101);
    }
}
