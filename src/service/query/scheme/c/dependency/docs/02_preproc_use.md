```c
MAX(1,2)
```

**parser**

```
translation_unit [0, 0] - [0, 8]
  expression_statement [0, 0] - [0, 8]
    call_expression [0, 0] - [0, 8]
      function: identifier [0, 0] - [0, 3]
      arguments: argument_list [0, 3] - [0, 8]
        number_literal [0, 4] - [0, 5]
        number_literal [0, 6] - [0, 7]
```

---

```scm
(call_expression
  function: (identifier) @macro.usage.name
  (#match? @macro.usage.name "^[A-Z][A-Z0-9_]*$")
) @macro.usage

; 宏条件使用（在预处理指令中）
(preproc_if
  condition: (identifier) @macro.condition.name
) @macro.condition.usage

(preproc_ifdef
  name: (identifier) @macro.condition.name
) @macro.condition.usage
```

---

**result**

```json
[
  {
    "captureName": "macro.usage",
    "type": "call_expression",
    "text": "MAX(1,2)",
    "startPosition": {
      "row": 0,
      "column": 0
    },
    "endPosition": {
      "row": 0,
      "column": 8
    }
  },
  {
    "captureName": "macro.usage.name",
    "type": "identifier",
    "text": "MAX",
    "startPosition": {
      "row": 0,
      "column": 0
    },
    "endPosition": {
      "row": 0,
      "column": 3
    }
  }
]
```
