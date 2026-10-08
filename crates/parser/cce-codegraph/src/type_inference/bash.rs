//! Bash-specific type inference.
//!
//! Shell has no type annotations, so variables bind from literal
//! initializers (`APP_NAME="demo"` yields `string`, `MAX_RETRIES=3`
//! yields `int`) and simple call targets. Control-flow narrowing is
//! unsupported: conditions are untyped string tests.

use cce_types::entity::{Entity, EntityKind};

use super::extractors::{extract_field_type, extract_function_types, extract_variable_type};
use super::traits::LanguageTypeInferer;
use super::types::ScopedTypeContext;

/// Bash type inference implementation.
pub struct BashTypeInferer;

impl LanguageTypeInferer for BashTypeInferer {
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
    fn test_bash_literal_assignments_bind() {
        let inferer = BashTypeInferer;
        let mut ctx = ScopedTypeContext::new(Language::Bash);
        inferer.infer_declarations(
            &[
                variable(1, "APP_NAME", "string"),
                variable(2, "MAX_RETRIES", "int"),
            ],
            &mut ctx,
        );
        assert_eq!(
            ctx.get_variable_type("APP_NAME").unwrap().type_name,
            "string"
        );
        assert_eq!(
            ctx.get_variable_type("MAX_RETRIES").unwrap().type_name,
            "int"
        );
    }
}
