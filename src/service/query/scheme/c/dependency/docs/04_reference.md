```c
int a=b
```

**parser**

```
// Parser output will be generated here
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
[]
```
