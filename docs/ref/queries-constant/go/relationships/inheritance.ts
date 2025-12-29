/*
Go Inheritance Relationships-specific Tree-Sitter Query Patterns
用于识别和分析代码中的继承关系
只包含基于抽象语法树的真实查询，不使用字符匹配
*/
export default `
; ===== 接口实现关系 =====

; 基本接口实现关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.implementing.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @inheritance.implemented.interface))))) @inheritance.relationship.interface.implementation

; 多接口实现关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.multiple.implementing.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @inheritance.implemented.interface.1)
        (field_declaration
          type: (type_identifier) @inheritance.implemented.interface.2)*)))) @inheritance.relationship.multiple.interface.implementation

; 接口方法实现关系
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.receiver.name
      type: (type_identifier) @inheritance.receiver.type))
  name: (field_identifier) @inheritance.implemented.method
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.param.name
      type: (_) @inheritance.param.type)*)?
  result: (_)? @inheritance.method.return.type) @inheritance.relationship.method.implementation

; ===== 结构体嵌入继承关系 =====

; 单一结构体嵌入关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.derived.struct
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @inheritance.embedded.struct))))) @inheritance.relationship.struct.embedding

; 多重结构体嵌入关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.multiple.derived.struct
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @inheritance.embedded.struct.1)
        (field_declaration
          type: (type_identifier) @inheritance.embedded.struct.2)*)))) @inheritance.relationship.multiple.struct.embedding

; 指针类型嵌入关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.pointer.derived.struct
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (pointer_type
            element: (type_identifier) @inheritance.embedded.pointer.struct)))))) @inheritance.relationship.pointer.embedding

; ===== 接口嵌入关系 =====

; 基本接口嵌入关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.derived.interface
    type: (interface_type
      (method_spec_list
        (method_spec
          type: (type_identifier) @inheritance.embedded.interface)*)))) @inheritance.relationship.interface.embedding

; ===== 方法集继承关系 =====

; 值接收者方法集
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.value.receiver
      type: (type_identifier) @inheritance.value.receiver.type))
  name: (field_identifier) @inheritance.value.method) @inheritance.relationship.value.method.set

; 指针接收者方法集
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.pointer.receiver
      type: (pointer_type
        element: (type_identifier) @inheritance.pointer.receiver.type)))
  name: (field_identifier) @inheritance.pointer.method) @inheritance.relationship.pointer.method.set

; 嵌入方法访问关系
(call_expression
  function: (selector_expression
    operand: (selector_expression
      operand: (identifier) @inheritance.embedding.object
      field: (field_identifier) @inheritance.embedded.field)
    field: (field_identifier) @inheritance.accessed.method)
  arguments: (argument_list
    (identifier) @inheritance.method.arg)*) @inheritance.relationship.embedded.method.access

; 嵌入字段访问关系
(selector_expression
  operand: (selector_expression
    operand: (identifier) @inheritance.embedding.object
    field: (field_identifier) @inheritance.embedded.field)
  field: (field_identifier) @inheritance.accessed.field) @inheritance.relationship.embedded.field.access

; ===== 接口组合继承关系 =====

; 基本接口组合关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.composite.interface
    type: (interface_type
      (method_spec_list
        (method_spec
          type: (type_identifier) @inheritance.base.interface.1)
        (method_spec
          type: (type_identifier) @inheritance.base.interface.2)*)))) @inheritance.relationship.interface.composition

; ===== 泛型继承关系 =====

; 泛型接口实现关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.generic.implementing.type
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @inheritance.generic.type.parameter))
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (generic_type
            name: (type_identifier) @inheritance.generic.implemented.interface
            type_arguments: (type_argument_list
              (type_identifier) @inheritance.generic.type.argument)*)))))) @inheritance.relationship.generic.interface.implementation

; 泛型结构体嵌入关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.generic.derived.struct
    type_parameters: (type_parameter_list
      (type_parameter
        name: (identifier) @inheritance.generic.type.parameter))
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (generic_type
            name: (type_identifier) @inheritance.generic.embedded.struct
            type_arguments: (type_argument_list
              (type_identifier) @inheritance.generic.type.argument)*)))))) @inheritance.relationship.generic.struct.embedding

; ===== 类型断言继承关系 =====

; 接口断言关系
(type_assertion_expression
  expression: (identifier) @inheritance.asserted.value
  type: (type_identifier) @inheritance.asserted.type) @inheritance.relationship.interface.assertion

; 指针类型断言关系
(type_assertion_expression
  expression: (identifier) @inheritance.asserted.value
  type: (pointer_type
    element: (type_identifier) @inheritance.asserted.pointer.type)) @inheritance.relationship.pointer.assertion

; 类型切换关系
(type_switch_statement
  (expression_statement
    (type_assertion_expression
      expression: (identifier) @inheritance.switch.value
      type: (type_identifier) @inheritance.switch.type)))
  (type_switch_statement
    body: (block
      (type_case
        type: (type_identifier) @inheritance.case.type
        (block) @inheritance.case.body)*
      (default_case
        (block) @inheritance.default.body)*)) @inheritance.relationship.type.switch

; ===== 标准接口继承关系 =====

; Stringer接口实现关系
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.stringer.receiver
      type: (type_identifier) @inheritance.stringer.type))
  name: (field_identifier) @inheritance.stringer.method
  result: (type_identifier) @inheritance.stringer.return.type
  (#eq? @inheritance.stringer.method "String")
  (#eq? @inheritance.stringer.return.type "string")) @inheritance.relationship.stringer.implementation

; Error接口实现关系
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.error.receiver
      type: (type_identifier) @inheritance.error.type))
  name: (field_identifier) @inheritance.error.method
  result: (type_identifier) @inheritance.error.return.type
  (#eq? @inheritance.error.method "Error")
  (#eq? @inheritance.error.return.type "string")) @inheritance.relationship.error.implementation

; Reader接口实现关系
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.reader.receiver
      type: (type_identifier) @inheritance.reader.type))
  name: (field_identifier) @inheritance.reader.method
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.reader.param
      type: (slice_type
        element: (type_identifier) @inheritance.reader.slice.element)))
  result: (type_identifier) @inheritance.reader.return.type
  (#eq? @inheritance.reader.method "Read")) @inheritance.relationship.reader.implementation

; Writer接口实现关系
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.writer.receiver
      type: (type_identifier) @inheritance.writer.type))
  name: (field_identifier) @inheritance.writer.method
  parameters: (parameter_list
    (parameter_declaration
      name: (identifier) @inheritance.writer.param
      type: (slice_type
        element: (type_identifier) @inheritance.writer.slice.element)))
  result: (type_identifier) @inheritance.writer.return.type
  (#eq? @inheritance.writer.method "Write")) @inheritance.relationship.writer.implementation

; ===== 继承链关系 =====

; 接口继承链关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.chain.derived.interface
    type: (interface_type
      (method_spec_list
        (method_spec
          type: (type_identifier) @inheritance.chain.base.interface.1)
        (method_spec
          type: (type_identifier) @inheritance.chain.base.interface.2)*)))) @inheritance.relationship.interface.chain

; 结构体嵌入链关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.chain.derived.struct
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @inheritance.chain.embedded.struct.1)
        (field_declaration
          type: (type_identifier) @inheritance.chain.embedded.struct.2)*)))) @inheritance.relationship.struct.chain

; 混合继承链关系
(type_declaration
  (type_spec
    name: (type_identifier) @inheritance.chain.mixed.type
    type: (struct_type
      (field_declaration_list
        (field_declaration
          type: (type_identifier) @inheritance.chain.interface.1)
        (field_declaration
          type: (type_identifier) @inheritance.chain.struct.1)
        (field_declaration
          type: (type_identifier) @inheritance.chain.interface.2)*)))) @inheritance.relationship.mixed.chain
`;