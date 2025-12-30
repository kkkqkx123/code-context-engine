```c
MyType variable;

struct Point p1;
struct Point *p2;

union Data u1;

enum Color c;


```

**parser**

```json
{
  "nodes": [
    {
      "depth": 0,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "",
        "type": "translation_unit",
        "startPosition": {
          "row": 0,
          "column": 0
        },
        "endPosition": {
          "row": 9,
          "column": 0
        }
      }
    },
    {
      "depth": 1,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "",
        "type": "declaration",
        "startPosition": {
          "row": 0,
          "column": 0
        },
        "endPosition": {
          "row": 0,
          "column": 16
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "type",
        "type": "type_identifier",
        "startPosition": {
          "row": 0,
          "column": 0
        },
        "endPosition": {
          "row": 0,
          "column": 6
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "declarator",
        "type": "identifier",
        "startPosition": {
          "row": 0,
          "column": 7
        },
        "endPosition": {
          "row": 0,
          "column": 15
        }
      }
    },
    {
      "depth": 1,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "",
        "type": "declaration",
        "startPosition": {
          "row": 2,
          "column": 0
        },
        "endPosition": {
          "row": 2,
          "column": 16
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "type",
        "type": "struct_specifier",
        "startPosition": {
          "row": 2,
          "column": 0
        },
        "endPosition": {
          "row": 2,
          "column": 12
        }
      }
    },
    {
      "depth": 3,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "name",
        "type": "type_identifier",
        "startPosition": {
          "row": 2,
          "column": 7
        },
        "endPosition": {
          "row": 2,
          "column": 12
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "declarator",
        "type": "identifier",
        "startPosition": {
          "row": 2,
          "column": 13
        },
        "endPosition": {
          "row": 2,
          "column": 15
        }
      }
    },
    {
      "depth": 1,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "",
        "type": "declaration",
        "startPosition": {
          "row": 3,
          "column": 0
        },
        "endPosition": {
          "row": 3,
          "column": 17
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "type",
        "type": "struct_specifier",
        "startPosition": {
          "row": 3,
          "column": 0
        },
        "endPosition": {
          "row": 3,
          "column": 12
        }
      }
    },
    {
      "depth": 3,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "name",
        "type": "type_identifier",
        "startPosition": {
          "row": 3,
          "column": 7
        },
        "endPosition": {
          "row": 3,
          "column": 12
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "declarator",
        "type": "pointer_declarator",
        "startPosition": {
          "row": 3,
          "column": 13
        },
        "endPosition": {
          "row": 3,
          "column": 16
        }
      }
    },
    {
      "depth": 3,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "declarator",
        "type": "identifier",
        "startPosition": {
          "row": 3,
          "column": 14
        },
        "endPosition": {
          "row": 3,
          "column": 16
        }
      }
    },
    {
      "depth": 1,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "",
        "type": "declaration",
        "startPosition": {
          "row": 5,
          "column": 0
        },
        "endPosition": {
          "row": 5,
          "column": 14
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "type",
        "type": "union_specifier",
        "startPosition": {
          "row": 5,
          "column": 0
        },
        "endPosition": {
          "row": 5,
          "column": 10
        }
      }
    },
    {
      "depth": 3,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "name",
        "type": "type_identifier",
        "startPosition": {
          "row": 5,
          "column": 6
        },
        "endPosition": {
          "row": 5,
          "column": 10
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "declarator",
        "type": "identifier",
        "startPosition": {
          "row": 5,
          "column": 11
        },
        "endPosition": {
          "row": 5,
          "column": 13
        }
      }
    },
    {
      "depth": 1,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "",
        "type": "declaration",
        "startPosition": {
          "row": 7,
          "column": 0
        },
        "endPosition": {
          "row": 7,
          "column": 13
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "type",
        "type": "enum_specifier",
        "startPosition": {
          "row": 7,
          "column": 0
        },
        "endPosition": {
          "row": 7,
          "column": 10
        }
      }
    },
    {
      "depth": 3,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "name",
        "type": "type_identifier",
        "startPosition": {
          "row": 7,
          "column": 5
        },
        "endPosition": {
          "row": 7,
          "column": 10
        }
      }
    },
    {
      "depth": 2,
      "uri": "vscode-notebook-cell:/d%3A/ide/tool/code-context-engine/src/service/query/scheme/c/dependency/tests/04_reference.tsqnb#W0sZmlsZQ%3D%3D",
      "node": {
        "fieldName": "declarator",
        "type": "identifier",
        "startPosition": {
          "row": 7,
          "column": 11
        },
        "endPosition": {
          "row": 7,
          "column": 12
        }
      }
    }
  ]
}
```

---

```scm
; 类型标识符引用
(declaration
  type: (type_identifier) @type.reference.name
) @type.reference

; 结构体类型引用
(declaration
  type: (struct_specifier
    name: (type_identifier) @struct.reference.name
  )
) @struct.reference

; 联合体类型引用
(declaration
  type: (union_specifier
    name: (type_identifier) @union.reference.name
  )
) @union.reference

; 枚举类型引用
(declaration
  type: (enum_specifier
    name: (type_identifier) @enum.reference.name
  )
) @enum.reference
```

---

**result**

```json
[
  {
    "captureName": "type.reference",
    "type": "declaration",
    "text": "MyType variable;",
    "startPosition": {
      "row": 0,
      "column": 0
    },
    "endPosition": {
      "row": 0,
      "column": 16
    }
  },
  {
    "captureName": "type.reference.name",
    "type": "type_identifier",
    "text": "MyType",
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
    "captureName": "struct.reference",
    "type": "declaration",
    "text": "struct Point p1;",
    "startPosition": {
      "row": 2,
      "column": 0
    },
    "endPosition": {
      "row": 2,
      "column": 16
    }
  },
  {
    "captureName": "struct.reference.name",
    "type": "type_identifier",
    "text": "Point",
    "startPosition": {
      "row": 2,
      "column": 7
    },
    "endPosition": {
      "row": 2,
      "column": 12
    }
  },
  {
    "captureName": "struct.reference",
    "type": "declaration",
    "text": "struct Point *p2;",
    "startPosition": {
      "row": 3,
      "column": 0
    },
    "endPosition": {
      "row": 3,
      "column": 17
    }
  },
  {
    "captureName": "struct.reference.name",
    "type": "type_identifier",
    "text": "Point",
    "startPosition": {
      "row": 3,
      "column": 7
    },
    "endPosition": {
      "row": 3,
      "column": 12
    }
  },
  {
    "captureName": "union.reference",
    "type": "declaration",
    "text": "union Data u1;",
    "startPosition": {
      "row": 5,
      "column": 0
    },
    "endPosition": {
      "row": 5,
      "column": 14
    }
  },
  {
    "captureName": "union.reference.name",
    "type": "type_identifier",
    "text": "Data",
    "startPosition": {
      "row": 5,
      "column": 6
    },
    "endPosition": {
      "row": 5,
      "column": 10
    }
  },
  {
    "captureName": "enum.reference",
    "type": "declaration",
    "text": "enum Color c;",
    "startPosition": {
      "row": 7,
      "column": 0
    },
    "endPosition": {
      "row": 7,
      "column": 13
    }
  },
  {
    "captureName": "enum.reference.name",
    "type": "type_identifier",
    "text": "Color",
    "startPosition": {
      "row": 7,
      "column": 5
    },
    "endPosition": {
      "row": 7,
      "column": 10
    }
  }
]
```
