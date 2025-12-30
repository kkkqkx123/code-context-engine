```c
const x = 1;
```

**parser**

```
// Parser output will be generated here
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
[]
```
