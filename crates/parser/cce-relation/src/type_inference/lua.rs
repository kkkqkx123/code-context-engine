//! Lua-specific type inference.
//!
//! Lua has no annotations; variables bind from literal initializers
//! (`local max_retries = 3` yields `number`, `local app = "demo"`
//! yields `string`) and simple call targets. Control-flow narrowing
//! is unsupported.

use cce_types::entity::{Entity, EntityKind};

use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::ScopedTypeContext;

/// Lua type inference implementation.
pub struct LuaTypeInferer;

impl LanguageTypeInferer for LuaTypeInferer {
    fn infer_declarations(&self, entities: &[Entity], ctx: &mut ScopedTypeContext) {
        for entity in entities {
            match entity.kind {
                EntityKind::Function | EntityKind::Method => {
                    extract_function_types(entity, ctx);
                }
                EntityKind::Variable => {
                    extract_variable_type(entity, ctx);
                }
                EntityKind::Field | EntityKind::Property => {
                    extract_field_type(entity, ctx);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::EntityId;
    use cce_types::{Language, Span};

    fn variable(id: u64, name: &str, literal_type: &str) -> Entity {
        Entity::new(
            EntityId(id),
            EntityKind::Variable,
            name.to_string(),
            Span::default(),
        )
        .with_metadata("literal_type", literal_type)
    }

    #[test]
    fn test_lua_literal_assignments_bind() {
        let inferer = LuaTypeInferer;
        let mut ctx = ScopedTypeContext::new(Language::Lua);
        inferer.infer_declarations(
            &[
                variable(1, "app_name", "string"),
                variable(2, "max_retries", "number"),
                variable(3, "verbose", "boolean"),
            ],
            &mut ctx,
        );
        assert_eq!(
            ctx.get_variable_type("app_name").unwrap().type_name,
            "string"
        );
        assert_eq!(
            ctx.get_variable_type("max_retries").unwrap().type_name,
            "number"
        );
        assert_eq!(
            ctx.get_variable_type("verbose").unwrap().type_name,
            "boolean"
        );
    }
}
