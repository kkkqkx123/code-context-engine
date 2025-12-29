// Query patterns for YAML syntax elements
export const yamlQuery = `
; Documents - capture the entire document node
(document) @definition.document

; Block mappings - capture the entire mapping node
(block_mapping) @definition.mapping

; Flow mappings - capture flow style mappings {key: value}
(flow_mapping) @definition.flow_mapping

; Block sequences - capture block style sequences
(block_sequence) @definition.sequence

; Flow sequences - capture flow style sequences [item1, item2]
(flow_sequence) @definition.flow_sequence

; Block mapping pairs - capture key-value pairs in block style
(block_mapping_pair
  key: (flow_node) @name.definition.key
  value: (flow_node) @definition.value) @definition.pair

; Flow mapping pairs - capture key-value pairs in flow style
(flow_mapping_pair
  key: (flow_node) @name.definition.key
  value: (flow_node) @definition.value) @definition.pair

; Sequence items - capture individual sequence items
(block_sequence_item) @definition.sequence_item
(flow_sequence_item) @definition.flow_sequence_item

; Basic scalar values
(boolean_scalar) @definition.boolean
(null_scalar) @definition.null
(integer_scalar) @definition.integer
(float_scalar) @definition.float

; String scalars - all string types
(double_quote_scalar) @definition.string
(single_quote_scalar) @definition.string
(block_scalar) @definition.string
(string_scalar) @definition.string

; Plain scalars - unquoted values that can be strings, numbers, etc.
(plain_scalar) @definition.plain_scalar

; Anchors and aliases - for YAML references
(anchor_name) @definition.anchor
(alias_name) @definition.alias

; Tags - for type information
(tag) @definition.tag

; Directives - YAML version and tag directives
(yaml_directive) @definition.directive
(tag_directive) @definition.directive
(reserved_directive) @definition.directive

; Document boundaries
(document_start) @definition.document_start
(document_end) @definition.document_end

; Comments
(comment) @definition.comment

; Flow node keys for capture - handle different key types
(block_mapping_pair
  key: (flow_node
    [
      (double_quote_scalar)
      (single_quote_scalar)
      (plain_scalar)
    ] @name.definition)) @definition.pair

(flow_mapping_pair
  key: (flow_node
    [
      (double_quote_scalar)
      (single_quote_scalar)
      (plain_scalar)
    ] @name.definition)) @definition.pair

; Nested structures - capture nested mappings and sequences
(block_mapping
  (block_mapping_pair) @definition.nested_pair)

(flow_mapping
  (flow_mapping_pair) @definition.nested_pair)

(block_sequence
  (block_sequence_item) @definition.nested_item)

(flow_sequence
  (flow_sequence_item) @definition.nested_item)

; Complex values - capture mappings and sequences as values
(block_mapping_pair
  value: (flow_node
    (block_mapping) @definition.mapping_value)) @definition.pair

(block_mapping_pair
  value: (flow_node
    (flow_mapping) @definition.flow_mapping_value)) @definition.pair

(block_mapping_pair
  value: (flow_node
    (block_sequence) @definition.sequence_value)) @definition.pair

(block_mapping_pair
  value: (flow_node
    (flow_sequence) @definition.flow_sequence_value)) @definition.pair
`