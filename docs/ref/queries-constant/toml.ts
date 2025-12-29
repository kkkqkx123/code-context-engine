// Query patterns for TOML syntax elements
export const tomlQuery = `
; Tables - capture the entire table node
(table) @definition.table

; Array tables - capture the entire array table node
(table_array_element) @definition.table_array

; Key-value pairs - capture the entire pair
(pair
  key: [
    (bare_key)
    (quoted_key)
  ] @name.definition.key
  value: (_) @definition.value) @definition.pair

; Dotted keys - capture dotted key-value pairs
(dotted_key
  (bare_key) @name.definition.dotted_segment
  (quoted_key) @name.definition.dotted_segment) @name.definition.dotted_key

; Arrays and inline tables
(array) @definition.array
(inline_table) @definition.inline_table

; Basic values
(string) @definition.string
(integer) @definition.integer
(float) @definition.float
(boolean) @definition.boolean
(offset_date_time) @definition.datetime
(local_date) @definition.date
(local_time) @definition.time

; Key names for capture
(pair
  key: [
    (bare_key)
    (quoted_key)
  ] @name.definition) @definition.pair

; Array values
(array
  (_) @definition.array_element)

; Inline table key-value pairs
(inline_table
  (pair
    key: [
      (bare_key)
      (quoted_key)
    ] @name.definition.inline_key
    value: (_) @definition.inline_value)) @definition.inline_pair

; Comments
(comment) @definition.comment
`