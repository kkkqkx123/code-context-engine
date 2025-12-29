# C Preprocessor Tree-Sitter查询规则测试用例

本文档为C Preprocessor的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在TreeSitter Playground中进行验证。

## 1. 宏定义查询

### 查询规则
```
[
  (preproc_def
    name: (identifier) @name.definition.macro)
  (preproc_function_def
    name: (identifier) @name.definition.macro)
] @definition.macro
```

### 测试用例
```c
// 简单宏定义
#define PI 3.14159
#define MAX_SIZE 100
#define VERSION "1.0.0"

// 函数式宏定义
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define SQUARE(x) ((x) * (x))
#define PRINT_DEBUG(msg) printf("Debug: %s\n", msg)
```

## 2. 预处理器包含查询

### 查询规则
```
[
  (preproc_include
    path: (system_lib_string) @name.definition.include)
  (preproc_include
    path: (string_literal) @name.definition.include)
] @definition.include
```

### 测试用例
```c
// 系统头文件包含
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// 用户头文件包含
#include "myheader.h"
#include "utils/config.h"
#include "../common/types.h"
```

## 3. 预处理器条件编译查询

### 查询规则
```
(preproc_if
  condition: (_) @name.definition.preproc_condition) @definition.preproc_condition
```

### 测试用例
```c
#if defined(WIN32)
    #define PATH_SEPARATOR "\\"
#elif defined(UNIX)
    #define PATH_SEPARATOR "/"
#else
    #define PATH_SEPARATOR "/"
#endif

#if DEBUG_LEVEL > 0
    printf("Debug mode enabled\n");
#endif
```

## 4. 预处理器elif查询

### 查询规则
```
(preproc_elif
  condition: (_) @name.definition.preproc_condition) @definition.preproc_condition
```

### 测试用例
```c
#if defined(PLATFORM_A)
    #define PLATFORM_NAME "Platform A"
#elif defined(PLATFORM_B)
    #define PLATFORM_NAME "Platform B"
#elif defined(PLATFORM_C)
    #define PLATFORM_NAME "Platform C"
#else
    #define PLATFORM_NAME "Unknown Platform"
#endif

#if ARCH == 32
    #define ARCH_BITS 32
#elif ARCH == 64
    #define ARCH_BITS 64
#elif ARCH == 16
    #define ARCH_BITS 16
#endif
```

## 5. 预处理器ifdef查询

### 查询规则
```
(preproc_ifdef
  name: (identifier) @name.definition.preproc_ifdef) @definition.preproc_ifdef
```

### 测试用例
```c
#ifdef USE_CUSTOM_ALLOC
    #include "custom_alloc.h"
    void* (*alloc_func)(size_t) = custom_alloc;
    void (*free_func)(void*) = custom_free;
#else
    #include <stdlib.h>
    void* (*alloc_func)(size_t) = malloc;
    void (*free_func)(void*) = free;
#endif

#ifdef ENABLE_LOGGING
    #define LOG(msg) printf("[LOG] %s\n", msg)
#else
    #define LOG(msg)
#endif

#ifndef BUFFER_SIZE
    #define BUFFER_SIZE 1024
#endif
```

## 6. 预处理器else查询

### 查询规则
```
(preproc_else) @definition.preproc_else
```

### 测试用例
```c
#ifdef DEBUG
    #define PRINT_DEBUG(msg) printf("DEBUG: %s\n", msg)
#else
    #define PRINT_DEBUG(msg)
#endif

#if defined(USE_FAST_MATH)
    #define SQRT(x) fast_sqrt(x)
#else
    #define SQRT(x) sqrt(x)
#endif

#ifdef TESTING
    #define ASSERT(condition) \
        if (!(condition)) { \
            printf("Assertion failed: %s\n", #condition); \
            exit(1); \
        }
#else
    #define ASSERT(condition)
#endif