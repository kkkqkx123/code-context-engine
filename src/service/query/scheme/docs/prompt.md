#if TEST
#define TEST
#else 
#define DEV
#endif 

#ifdef TEST
#define TEST
#endif

#ifdef TEST

---

对于上述C代码，tree-sitter查询结果为：
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

---

我的主要目的是完成代码解析，后续使用这些处理结果来转自然语言，之后用于向量嵌入。我该如何修改这段查询，以捕获内部结构？
; #ifdef/ifndef 条件编译
(preproc_ifdef
  name: (identifier) @ifdef.name
) @ifdef

; #if 条件编译
(preproc_if
  condition: (_) @if.condition
) @if

; #elif 条件编译
(preproc_elif
  condition: (_) @elif.condition
) @elif

; #else 条件编译
(preproc_else) @else

