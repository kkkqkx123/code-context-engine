```c
int max(int a, int b)
int *max(int *a, int *b)
```

**parser**

```
translation_unit [0, 0] - [1, 24]
  declaration [0, 0] - [0, 21]
    type: primitive_type [0, 0] - [0, 3]
    declarator: function_declarator [0, 4] - [0, 21]
      declarator: identifier [0, 4] - [0, 7]
      parameters: parameter_list [0, 7] - [0, 21]
        parameter_declaration [0, 8] - [0, 13]
          type: primitive_type [0, 8] - [0, 11]
          declarator: identifier [0, 12] - [0, 13]
        parameter_declaration [0, 15] - [0, 20]
          type: primitive_type [0, 15] - [0, 18]
          declarator: identifier [0, 19] - [0, 20]
  declaration [1, 0] - [1, 24]
    type: primitive_type [1, 0] - [1, 3]
    declarator: pointer_declarator [1, 4] - [1, 24]
      declarator: function_declarator [1, 5] - [1, 24]
        declarator: identifier [1, 5] - [1, 8]
        parameters: parameter_list [1, 8] - [1, 24]
          parameter_declaration [1, 9] - [1, 15]
            type: primitive_type [1, 9] - [1, 12]
            declarator: pointer_declarator [1, 13] - [1, 15]
              declarator: identifier [1, 14] - [1, 15]
          parameter_declaration [1, 17] - [1, 23]
            type: primitive_type [1, 17] - [1, 20]
            declarator: pointer_declarator [1, 21] - [1, 23]
              declarator: identifier [1, 22] - [1, 23]
```

---

```scm

; 函数声明（非定义）
(declaration
  declarator: (function_declarator
    declarator: (identifier) @function.declaration.name
  )
) @function.declaration

; 函数指针声明
(declaration
  declarator: (pointer_declarator
    declarator: (function_declarator
      declarator: (identifier) @function.pointer.declaration.name
    )
  )
) @function.pointer.declaration
```

---

**result**

```json
[
  {
    "captureName": "function.declaration",
    "type": "declaration",
    "text": "int max(int a, int b)",
    "startPosition": {
      "row": 0,
      "column": 0
    },
    "endPosition": {
      "row": 0,
      "column": 21
    }
  },
  {
    "captureName": "function.declaration.name",
    "type": "identifier",
    "text": "max",
    "startPosition": {
      "row": 0,
      "column": 4
    },
    "endPosition": {
      "row": 0,
      "column": 7
    }
  },
  {
    "captureName": "function.pointer.declaration",
    "type": "declaration",
    "text": "int *max(int *a, int *b)",
    "startPosition": {
      "row": 1,
      "column": 0
    },
    "endPosition": {
      "row": 1,
      "column": 24
    }
  },
  {
    "captureName": "function.pointer.declaration.name",
    "type": "identifier",
    "text": "max",
    "startPosition": {
      "row": 1,
      "column": 5
    },
    "endPosition": {
      "row": 1,
      "column": 8
    }
  }
]
```
