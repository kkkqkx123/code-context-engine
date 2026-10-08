use cce_types::entity::EntityKind;

/// Strip generic parameters from a type name.
pub fn strip_generics(text: &str) -> &str {
    if let Some(pos) = text.find('<') {
        &text[..pos]
    } else {
        text
    }
}

/// Extract simple name from qualified path.
pub fn simple_name(text: &str) -> &str {
    let without_generics = strip_generics(text);
    without_generics
        .rsplit([':', '.'])
        .next()
        .unwrap_or(without_generics)
        .trim()
}

/// Whether a kind represents a type definition that can own members.
pub fn is_type_definition_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Class
            | EntityKind::Struct
            | EntityKind::Enum
            | EntityKind::Interface
            | EntityKind::Trait
            | EntityKind::TypeAlias
            | EntityKind::Union
    )
}

/// Whether a kind represents a member-like entity (method/function/field etc).
pub fn is_member_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Method
            | EntityKind::Function
            | EntityKind::Constructor
            | EntityKind::Destructor
            | EntityKind::Operator
            | EntityKind::Field
            | EntityKind::Property
            | EntityKind::Variable
            | EntityKind::Constant
            | EntityKind::EnumVariant
    )
}

/// Check if parameters contain a self-like receiver.
pub fn has_self_param(params: &[(String, Option<String>)]) -> bool {
    params.iter().any(|(name, _)| {
        let n = name
            .trim()
            .trim_start_matches('&')
            .trim_start_matches("mut ")
            .trim();
        n == "self" || n == "Self" || n.starts_with("self:") || n.starts_with("Self:")
    }) || params.iter().any(|(name, typ)| {
        if name == "self" || name == "&self" || name == "&mut self" {
            return true;
        }
        if let Some(t) = typ {
            let tl = t.to_ascii_lowercase();
            tl.contains("self")
        } else {
            false
        }
    })
}

/// Determine if entity modifiers mark it as static.
pub fn is_static_modifiers(modifiers: &[String]) -> bool {
    modifiers.iter().any(|m| m.eq_ignore_ascii_case("static"))
}

/// Normalize a qualified type part for index lookup.
pub fn normalize_qualified(s: &str) -> String {
    let t = strip_generics(s).trim();
    let without_crate = t.strip_prefix("crate::").unwrap_or(t);
    without_crate.trim().to_string()
}
