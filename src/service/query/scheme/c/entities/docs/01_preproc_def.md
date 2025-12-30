```c
#undef MAX
#define MAX
#define MAX(a, b) ((a) > (b) ? (a) : (b))
```

**parser**
```
translation_unit [0, 0] - [3, 41]
preproc_call [0, 0] - [1, 0]
directive: preproc_directive [0, 0] - [0, 6]
argument: preproc_arg [0, 7] - [0, 11]
preproc_def [1, 0] - [2, 0]
name: identifier [1, 8] - [1, 11]
preproc_function_def [3, 0] - [3, 41]
name: identifier [3, 8] - [3, 11]
parameters: preproc_params [3, 11] - [3, 17]
identifier [3, 12] - [3, 13]
identifier [3, 15] - [3, 16]
value: preproc_arg [3, 18] - [3, 41]
```

---

```scm
; 宏定义
(preproc_def
  name: (identifier) @macro.name
) @macro.definition

; 宏函数定义
(preproc_function_def
  name: (identifier) @macro.function.name
  parameters: (preproc_params) @macro.function.params
) @macro.function.definition

; 宏取消定义
; 匹配 #undef 指令，并捕获被取消定义的宏名
(preproc_call
  directive: (preproc_directive) @directive
  argument: (preproc_arg) @macro.name
  (#eq? @directive "#undef")
) @macro.undef
```

**result**

```json
[
    {
        "captureName": "include.system",
        "type": "preproc_include",
        "text": "#include <stdbool.h>\r\n",
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
        "captureName": "include.system.path",
        "type": "system_lib_string",
        "text": "<stdbool.h>",
        "startPosition": {
            "row": 0,
            "column": 9
        },
        "endPosition": {
            "row": 0,
            "column": 20
        }
    },
    {
        "captureName": "include.user",
        "type": "preproc_include",
        "text": "#include \"other_header.h\"",
        "startPosition": {
            "row": 1,
            "column": 0
        },
        "endPosition": {
            "row": 1,
            "column": 25
        }
    },
    {
        "captureName": "include.user.path",
        "type": "string_literal",
        "text": "\"other_header.h\"",
        "startPosition": {
            "row": 1,
            "column": 9
        },
        "endPosition": {
            "row": 1,
            "column": 25
        }
    }
]
```