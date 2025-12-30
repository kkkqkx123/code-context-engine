```c
#if TEST
#define TEST
#else 
#define DEV
#endif 

#ifdef TEST
#define TEST
#endif

#ifdef TEST

```

**parser**

```
translation_unit [0, 0] - [11, 0]
  preproc_if [0, 0] - [4, 6]
    condition: identifier [0, 4] - [0, 8]
    preproc_def [1, 0] - [2, 0]
      name: identifier [1, 8] - [1, 12]
    alternative: preproc_else [2, 0] - [4, 0]
      preproc_def [3, 0] - [4, 0]
        name: identifier [3, 8] - [3, 11]
  preproc_ifdef [6, 0] - [8, 6]
    name: identifier [6, 7] - [6, 11]
    preproc_def [7, 0] - [8, 0]
      name: identifier [7, 8] - [7, 12]
  preproc_ifdef [10, 0] - [10, 11]
    name: identifier [10, 7] - [10, 11]
```

---

```scm
(preproc_if
  condition: (identifier) @entity.condition_name
  (preproc_def
    name: (identifier) @entity.def_name
    value: (_)? @entity.def_value) @entity.def_node) @entity.if_scope

(preproc_ifdef
  name: (identifier) @entity.condition_name
  (preproc_def
    name: (identifier) @entity.def_name
    value: (_)? @entity.def_value) @entity.def_node) @entity.ifdef_scope

; 捕获 #else 块内部定义的宏
(preproc_else
  (preproc_def
    name: (identifier) @entity.def_name
    value: (_)? @entity.def_value) @entity.def_node) @entity.else_scope

; 捕获 #elif 块内部定义的宏
(preproc_elif
  condition: (identifier) @entity.condition_name
  (preproc_def
    name: (identifier) @entity.def_name
    value: (_)? @entity.def_value) @entity.def_node) @entity.elif_scope
```

---

**result**

```json
[
  {
    "captureName": "entity.if_scope",
    "type": "preproc_if",
    "text": "#if TEST\r\n#define TEST\r\n#else \r\n#define DEV\r\n#endif",
    "startPosition": {
      "row": 0,
      "column": 0
    },
    "endPosition": {
      "row": 4,
      "column": 6
    }
  },
  {
    "captureName": "entity.condition_name",
    "type": "identifier",
    "text": "TEST",
    "startPosition": {
      "row": 0,
      "column": 4
    },
    "endPosition": {
      "row": 0,
      "column": 8
    }
  },
  {
    "captureName": "entity.def_node",
    "type": "preproc_def",
    "text": "#define TEST\r\n",
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
    "captureName": "entity.def_name",
    "type": "identifier",
    "text": "TEST",
    "startPosition": {
      "row": 1,
      "column": 8
    },
    "endPosition": {
      "row": 1,
      "column": 12
    }
  },
  {
    "captureName": "entity.else_scope",
    "type": "preproc_else",
    "text": "#else \r\n#define DEV\r\n",
    "startPosition": {
      "row": 2,
      "column": 0
    },
    "endPosition": {
      "row": 4,
      "column": 0
    }
  },
  {
    "captureName": "entity.def_node",
    "type": "preproc_def",
    "text": "#define DEV\r\n",
    "startPosition": {
      "row": 3,
      "column": 0
    },
    "endPosition": {
      "row": 4,
      "column": 0
    }
  },
  {
    "captureName": "entity.def_name",
    "type": "identifier",
    "text": "DEV",
    "startPosition": {
      "row": 3,
      "column": 8
    },
    "endPosition": {
      "row": 3,
      "column": 11
    }
  },
  {
    "captureName": "entity.ifdef_scope",
    "type": "preproc_ifdef",
    "text": "#ifdef TEST\r\n#define TEST\r\n#endif",
    "startPosition": {
      "row": 6,
      "column": 0
    },
    "endPosition": {
      "row": 8,
      "column": 6
    }
  },
  {
    "captureName": "entity.condition_name",
    "type": "identifier",
    "text": "TEST",
    "startPosition": {
      "row": 6,
      "column": 7
    },
    "endPosition": {
      "row": 6,
      "column": 11
    }
  },
  {
    "captureName": "entity.def_node",
    "type": "preproc_def",
    "text": "#define TEST\r\n",
    "startPosition": {
      "row": 7,
      "column": 0
    },
    "endPosition": {
      "row": 8,
      "column": 0
    }
  },
  {
    "captureName": "entity.def_name",
    "type": "identifier",
    "text": "TEST",
    "startPosition": {
      "row": 7,
      "column": 8
    },
    "endPosition": {
      "row": 7,
      "column": 12
    }
  }
]
```
