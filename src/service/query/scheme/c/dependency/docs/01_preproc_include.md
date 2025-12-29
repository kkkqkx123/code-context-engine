```c
#include <stdbool.h>
#include "other_header.h"
```

**parser**

```
translation_unit [0, 0] - [1, 25]
preproc_include [0, 0] - [1, 0]
path: system_lib_string [0, 9] - [0, 20]
preproc_include [1, 0] - [1, 25]
path: string_literal [1, 9] - [1, 25]
string_content [1, 10] - [1, 24]
```

---

```scm
; 头文件包含（同项目文件）
(preproc_include
  path: (string_literal) @include.user.path
) @include.user

; 系统头文件包含（标准库）
(preproc_include
  path: (system_lib_string) @include.system.path
) @include.system
```

---

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