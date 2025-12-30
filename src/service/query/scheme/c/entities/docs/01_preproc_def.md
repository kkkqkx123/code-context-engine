```c
#undef MAX
#define MAX
#define MAX(a, b) ((a) > (b) ? (a) : (b))
```

**parser**

```
translation_unit [0, 0] - [2, 41]
  preproc_call [0, 0] - [1, 0]
    directive: preproc_directive [0, 0] - [0, 6]
    argument: preproc_arg [0, 7] - [0, 11]
  preproc_def [1, 0] - [2, 0]
    name: identifier [1, 8] - [1, 11]
  preproc_function_def [2, 0] - [2, 41]
    name: identifier [2, 8] - [2, 11]
    parameters: preproc_params [2, 11] - [2, 17]
      identifier [2, 12] - [2, 13]
      identifier [2, 15] - [2, 16]
    value: preproc_arg [2, 18] - [2, 41]
```

---

```scm
; 宏定义
(preproc_def
  name: (identifier) @entity.macro.name
) @entity.macro.definition

; 宏函数定义
(preproc_function_def
  name: (identifier) @entity.macro.function.name
  parameters: (preproc_params) @entity.macro.function.params
) @entity.macro.function.definition

; 宏取消定义
; 匹配 #undef 指令，并捕获被取消定义的宏名
(preproc_call
  directive: (preproc_directive) @directive
  argument: (preproc_arg) @entity.macro.name
  (#eq? @directive "#undef")
) @entity.macro.undef
```

---

**result**

```json
[
  {
    "captureName": "entity.macro.undef",
    "type": "preproc_call",
    "text": "#undef MAX\r\n",
    "startPosition": {
      "row": 0,
      "column": 0
    },
    "endPosition": {
      "row": 1,
      "column": 0
    }
  },
  {
    "captureName": "directive",
    "type": "preproc_directive",
    "text": "#undef",
    "startPosition": {
      "row": 0,
      "column": 0
    },
    "endPosition": {
      "row": 0,
      "column": 6
    }
  },
  {
    "captureName": "entity.macro.name",
    "type": "preproc_arg",
    "text": "MAX\r",
    "startPosition": {
      "row": 0,
      "column": 7
    },
    "endPosition": {
      "row": 0,
      "column": 11
    }
  },
  {
    "captureName": "entity.macro.definition",
    "type": "preproc_def",
    "text": "#define MAX\r\n",
    "startPosition": {
      "row": 1,
      "column": 0
    },
    "endPosition": {
      "row": 2,
      "column": 0
    }
  },
  {
    "captureName": "entity.macro.name",
    "type": "identifier",
    "text": "MAX",
    "startPosition": {
      "row": 1,
      "column": 8
    },
    "endPosition": {
      "row": 1,
      "column": 11
    }
  },
  {
    "captureName": "entity.macro.function.definition",
    "type": "preproc_function_def",
    "text": "#define MAX(a, b) ((a) > (b) ? (a) : (b))",
    "startPosition": {
      "row": 2,
      "column": 0
    },
    "endPosition": {
      "row": 2,
      "column": 41
    }
  },
  {
    "captureName": "entity.macro.function.name",
    "type": "identifier",
    "text": "MAX",
    "startPosition": {
      "row": 2,
      "column": 8
    },
    "endPosition": {
      "row": 2,
      "column": 11
    }
  },
  {
    "captureName": "entity.macro.function.params",
    "type": "preproc_params",
    "text": "(a, b)",
    "startPosition": {
      "row": 2,
      "column": 11
    },
    "endPosition": {
      "row": 2,
      "column": 17
    }
  }
]
```
