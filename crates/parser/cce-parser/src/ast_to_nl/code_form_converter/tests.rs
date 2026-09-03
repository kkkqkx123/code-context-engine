//! Tests for CodeFormConverter

#[cfg(test)]
mod code_form_converter_tests {
    use cce_types::EntityKind;

    #[test]
    fn test_code_form_entity_is_type_definition() {
        use crate::ast_to_nl::code_form_converter::CodeFormEntity;
        use cce_types::EntityId;

        let entity = CodeFormEntity {
            id: EntityId(1),
            name: "MyClass".to_string(),
            kind: EntityKind::Class,
            modifiers: vec![],
            type_annotation: None,
            doc_comment: None,
            summary_hint: String::new(),
            signature: None,
            parameters: vec![],
            depth: 0,
        };

        assert!(entity.is_type_definition());
    }

    #[test]
    fn test_code_form_entity_is_function_like() {
        use crate::ast_to_nl::code_form_converter::CodeFormEntity;
        use cce_types::EntityId;

        let entity = CodeFormEntity {
            id: EntityId(1),
            name: "my_function".to_string(),
            kind: EntityKind::Function,
            modifiers: vec![],
            type_annotation: None,
            doc_comment: None,
            summary_hint: String::new(),
            signature: None,
            parameters: vec![],
            depth: 0,
        };

        assert!(entity.is_function_like());
    }

    #[test]
    fn test_code_form_group_entity_count() {
        use crate::ast_to_nl::code_form_converter::{CodeFormEntity, CodeFormGroup};
        use cce_types::EntityId;

        let header = CodeFormEntity {
            id: EntityId(1),
            name: "MyClass".to_string(),
            kind: EntityKind::Class,
            modifiers: vec![],
            type_annotation: None,
            doc_comment: None,
            summary_hint: String::new(),
            signature: None,
            parameters: vec![],
            depth: 0,
        };

        let member1 = CodeFormEntity {
            id: EntityId(2),
            name: "method1".to_string(),
            kind: EntityKind::Method,
            modifiers: vec![],
            type_annotation: None,
            doc_comment: None,
            summary_hint: String::new(),
            signature: None,
            parameters: vec![],
            depth: 1,
        };

        let member2 = CodeFormEntity {
            id: EntityId(3),
            name: "method2".to_string(),
            kind: EntityKind::Method,
            modifiers: vec![],
            type_annotation: None,
            doc_comment: None,
            summary_hint: String::new(),
            signature: None,
            parameters: vec![],
            depth: 1,
        };

        let group = CodeFormGroup {
            header,
            members: vec![member1, member2],
            nested_groups: vec![],
            group_type: "ClassWithMethods".to_string(),
        };

        // header + 2 members = 3
        assert_eq!(group.entity_count(), 3);
    }
}
