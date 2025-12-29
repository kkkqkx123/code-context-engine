/**
 * JSON 查询模式
 * 用于 Tree-sitter JSON 解析的查询规则
 */

const jsonQuery = `
; Objects and arrays - capture the entire structure
(object) @definition.object
(array) @definition.array

; Key-value pairs - capture the entire pair with key and value
(pair
  key: (string) @name.definition.key
  value: (_) @definition.value) @definition.pair

; Basic values
(string) @definition.string
(number) @definition.number
(true) @definition.boolean
(false) @definition.boolean
(null) @definition.null

; Object properties - capture object members
(object
  (pair) @definition.object_member)

; Array elements - capture array items
(array
  (_) @definition.array_element)

; Nested structures - capture nested objects and arrays
(pair
  value: (object) @definition.nested_object) @definition.pair

(pair
  value: (array) @definition.nested_array) @definition.pair

; Key names for capture - handle different key types
(pair
  key: (string) @name.definition) @definition.pair

; Comments (non-standard but common in JSON with comments)
(comment) @definition.comment
`;

export default jsonQuery;