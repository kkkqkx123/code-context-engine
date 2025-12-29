# Lambda表达式转换样例

## 概述

本文档展示如何将不同编程语言的Lambda表达式（匿名函数、箭头函数、闭包）转换为自然语言表示，以提高通用NLP嵌入模型的语义理解能力。

## 转换模板

### Lambda表达式模板

```
Lambda expression {description} with parameters {parameters} returns {returnType} {context}
```

### 简化模板（无返回类型）

```
Lambda expression {description} with parameters {parameters} {context}
```

## 样例1：JavaScript箭头函数

### 输入代码

```javascript
// Filter active users from the user list
const activeUsers = users.filter(user => user.isActive && user.lastLogin > thirtyDaysAgo);

// Calculate total price
const totalPrice = items.reduce((sum, item) => sum + item.price * item.quantity, 0);

// Sort by name
const sorted = names.sort((a, b) => a.localeCompare(b));
```

### AST解析信息

#### Lambda 1: `user => user.isActive && user.lastLogin > thirtyDaysAgo`

- **参数**: `user`
- **表达式**: `user.isActive && user.lastLogin > thirtyDaysAgo`
- **用途**: 过滤活跃用户
- **上下文**: 文件 `user_utils.js`

#### Lambda 2: `(sum, item) => sum + item.price * item.quantity`

- **参数**: `sum`, `item`
- **表达式**: `sum + item.price * item.quantity`
- **用途**: 计算总价
- **上下文**: 文件 `cart.js`

#### Lambda 3: `(a, b) => a.localeCompare(b)`

- **参数**: `a`, `b`
- **表达式**: `a.localeCompare(b)`
- **用途**: 按名称排序
- **上下文**: 文件 `sort.js`

### 转换步骤

#### Lambda 1

1. **参数规范化**:
   - `user` → `user`

2. **表达式描述**:
   - `user.isActive && user.lastLogin > thirtyDaysAgo`
   - → `check if user is active and last login is recent`

3. **文本组装**:
   ```
   Lambda expression filter active users with parameters user returns boolean file user utils
   ```

#### Lambda 2

1. **参数规范化**:
   - `sum` → `sum`
   - `item` → `item`

2. **表达式描述**:
   - `sum + item.price * item.quantity`
   - → `accumulate sum by adding item price times quantity`

3. **文本组装**:
   ```
   Lambda expression calculate total price with parameters sum item returns number file cart
   ```

#### Lambda 3

1. **参数规范化**:
   - `a` → `a`
   - `b` → `b`

2. **表达式描述**:
   - `a.localeCompare(b)`
   - → `compare two strings alphabetically`

3. **文本组装**:
   ```
   Lambda expression sort by name with parameters a b returns number file sort
   ```

### 输出结果

```
Lambda expression filter active users with parameters user returns boolean file user utils
Lambda expression calculate total price with parameters sum item returns number file cart
Lambda expression sort by name with parameters a b returns number file sort
```

---

## 样例2：Python Lambda表达式

### 输入代码

```python
# Sort users by age
sorted_users = sorted(users, key=lambda user: user.age)

# Filter even numbers
even_numbers = list(filter(lambda x: x % 2 == 0, numbers))

# Map to uppercase
uppercase_names = list(map(lambda name: name.upper(), names))

# Complex lambda with multiple operations
processed = list(map(lambda x: x * 2 + 1 if x > 0 else 0, numbers))
```

### AST解析信息

#### Lambda 1: `lambda user: user.age`

- **参数**: `user`
- **表达式**: `user.age`
- **用途**: 按年龄排序
- **上下文**: 文件 `user_utils.py`

#### Lambda 2: `lambda x: x % 2 == 0`

- **参数**: `x`
- **表达式**: `x % 2 == 0`
- **用途**: 过滤偶数
- **上下文**: 文件 `number_utils.py`

#### Lambda 3: `lambda name: name.upper()`

- **参数**: `name`
- **表达式**: `name.upper()`
- **用途**: 转换为大写
- **上下文**: 文件 `string_utils.py`

#### Lambda 4: `lambda x: x * 2 + 1 if x > 0 else 0`

- **参数**: `x`
- **表达式**: `x * 2 + 1 if x > 0 else 0`
- **用途**: 条件处理
- **上下文**: 文件 `process.py`

### 转换步骤

#### Lambda 1

1. **参数规范化**:
   - `user` → `user`

2. **表达式描述**:
   - `user.age`
   - → `get user age`

3. **文本组装**:
   ```
   Lambda expression sort users by age with parameters user returns number file user utils
   ```

#### Lambda 2

1. **参数规范化**:
   - `x` → `x`

2. **表达式描述**:
   - `x % 2 == 0`
   - → `check if number is even`

3. **文本组装**:
   ```
   Lambda expression filter even numbers with parameters x returns boolean file number utils
   ```

#### Lambda 3

1. **参数规范化**:
   - `name` → `name`

2. **表达式描述**:
   - `name.upper()`
   - → `convert name to uppercase`

3. **文本组装**:
   ```
   Lambda expression map to uppercase with parameters name returns string file string utils
   ```

#### Lambda 4

1. **参数规范化**:
   - `x` → `x`

2. **表达式描述**:
   - `x * 2 + 1 if x > 0 else 0`
   - → `multiply by 2 and add 1 if positive else return 0`

3. **文本组装**:
   ```
   Lambda expression conditional processing with parameters x returns number file process
   ```

### 输出结果

```
Lambda expression sort users by age with parameters user returns number file user utils
Lambda expression filter even numbers with parameters x returns boolean file number utils
Lambda expression map to uppercase with parameters name returns string file string utils
Lambda expression conditional processing with parameters x returns number file process
```

---

## 样例3：Java Lambda表达式

### 输入代码

```java
// Filter active users
List<User> activeUsers = users.stream()
    .filter(user -> user.isActive())
    .collect(Collectors.toList());

// Map to names
List<String> names = users.stream()
    .map(user -> user.getName())
    .collect(Collectors.toList());

// Sort by age
users.sort((a, b) -> Integer.compare(a.getAge(), b.getAge()));

// Complex lambda with multiple operations
List<Integer> processed = numbers.stream()
    .map(x -> x * 2 + 1)
    .filter(x -> x > 10)
    .collect(Collectors.toList());
```

### AST解析信息

#### Lambda 1: `user -> user.isActive()`

- **参数**: `user`
- **表达式**: `user.isActive()`
- **用途**: 过滤活跃用户
- **上下文**: 文件 `UserUtils.java`

#### Lambda 2: `user -> user.getName()`

- **参数**: `user`
- **表达式**: `user.getName()`
- **用途**: 提取用户名
- **上下文**: 文件 `UserUtils.java`

#### Lambda 3: `(a, b) -> Integer.compare(a.getAge(), b.getAge())`

- **参数**: `a`, `b`
- **表达式**: `Integer.compare(a.getAge(), b.getAge())`
- **用途**: 按年龄排序
- **上下文**: 文件 `SortUtils.java`

#### Lambda 4: `x -> x * 2 + 1`

- **参数**: `x`
- **表达式**: `x * 2 + 1`
- **用途**: 数学运算
- **上下文**: 文件 `ProcessUtils.java`

### 转换步骤

#### Lambda 1

1. **参数规范化**:
   - `user` → `user`

2. **表达式描述**:
   - `user.isActive()`
   - → `check if user is active`

3. **文本组装**:
   ```
   Lambda expression filter active users with parameters user returns boolean file user utils java
   ```

#### Lambda 2

1. **参数规范化**:
   - `user` → `user`

2. **表达式描述**:
   - `user.getName()`
   - → `get user name`

3. **文本组装**:
   ```
   Lambda expression map to names with parameters user returns string file user utils java
   ```

#### Lambda 3

1. **参数规范化**:
   - `a` → `a`
   - `b` → `b`

2. **表达式描述**:
   - `Integer.compare(a.getAge(), b.getAge())`
   - → `compare ages of two users`

3. **文本组装**:
   ```
   Lambda expression sort by age with parameters a b returns integer file sort utils java
   ```

#### Lambda 4

1. **参数规范化**:
   - `x` → `x`

2. **表达式描述**:
   - `x * 2 + 1`
   - → `multiply by 2 and add 1`

3. **文本组装**:
   ```
   Lambda expression mathematical operation with parameters x returns integer file process utils java
   ```

### 输出结果

```
Lambda expression filter active users with parameters user returns boolean file user utils java
Lambda expression map to names with parameters user returns string file user utils java
Lambda expression sort by age with parameters a b returns integer file sort utils java
Lambda expression mathematical operation with parameters x returns integer file process utils java
```

---

## 样例4：Rust闭包

### 输入代码

```rust
// Filter even numbers
let even_numbers: Vec<i32> = numbers.iter().filter(|&x| x % 2 == 0).cloned().collect();

// Map to squares
let squares: Vec<i32> = numbers.iter().map(|x| x * x).cloned().collect();

// Sort by value
let mut sorted = numbers.clone();
sorted.sort_by(|a, b| a.cmp(b));

// Closure capturing environment
let threshold = 10;
let filtered: Vec<i32> = numbers.iter().filter(|&x| *x > threshold).cloned().collect();
```

### AST解析信息

#### Closure 1: `|&x| x % 2 == 0`

- **参数**: `x` (引用)
- **表达式**: `x % 2 == 0`
- **用途**: 过滤偶数
- **上下文**: 文件 `number_utils.rs`

#### Closure 2: `|x| x * x`

- **参数**: `x`
- **表达式**: `x * x`
- **用途**: 计算平方
- **上下文**: 文件 `number_utils.rs`

#### Closure 3: `|a, b| a.cmp(b)`

- **参数**: `a`, `b`
- **表达式**: `a.cmp(b)`
- **用途**: 排序
- **上下文**: 文件 `sort_utils.rs`

#### Closure 4: `|&x| *x > threshold`

- **参数**: `x` (引用)
- **表达式**: `*x > threshold`
- **用途**: 过滤大于阈值的数
- **捕获变量**: `threshold`
- **上下文**: 文件 `filter.rs`

### 转换步骤

#### Closure 1

1. **参数规范化**:
   - `x` → `x`

2. **表达式描述**:
   - `x % 2 == 0`
   - → `check if number is even`

3. **文本组装**:
   ```
   Lambda expression filter even numbers with parameters x returns boolean file number utils rs
   ```

#### Closure 2

1. **参数规范化**:
   - `x` → `x`

2. **表达式描述**:
   - `x * x`
   - → `calculate square of number`

3. **文本组装**:
   ```
   Lambda expression map to squares with parameters x returns integer file number utils rs
   ```

#### Closure 3

1. **参数规范化**:
   - `a` → `a`
   - `b` → `b`

2. **表达式描述**:
   - `a.cmp(b)`
   - → `compare two numbers`

3. **文本组装**:
   ```
   Lambda expression sort by value with parameters a b returns ordering file sort utils rs
   ```

#### Closure 4

1. **参数规范化**:
   - `x` → `x`

2. **表达式描述**:
   - `*x > threshold`
   - → `check if number is greater than threshold`

3. **捕获变量**:
   - `threshold`

4. **文本组装**:
   ```
   Lambda expression filter by threshold with parameters x captures threshold returns boolean file filter
   ```

### 输出结果

```
Lambda expression filter even numbers with parameters x returns boolean file number utils
Lambda expression map to squares with parameters x returns integer file number utils
Lambda expression sort by value with parameters a b returns ordering file sort utils
Lambda expression filter by threshold with parameters x captures threshold returns boolean file filter
```

---

## 样例5：C函数指针（类似Lambda）

### 输入代码

```c
// Comparison function for sorting
int compare_ints(const void *a, const void *b) {
    int int_a = *((int *)a);
    int int_b = *((int *)b);
    return (int_a > int_b) - (int_a < int_b);
}

// Use comparison function
qsort(array, size, sizeof(int), compare_ints);

// Callback function
void process_item(int item, void (*callback)(int)) {
    callback(item);
}

// Inline callback usage
process_item(42, [](int x) {
    printf("Processing: %d\n", x);
});
```

### AST解析信息

#### Function Pointer: `compare_ints`

- **参数**: `a`, `b` (void指针)
- **表达式**: 比较逻辑
- **用途**: 排序比较
- **上下文**: 文件 `sort.c`

#### Callback: `[](int x) { printf("Processing: %d\n", x); }`

- **参数**: `x`
- **表达式**: 打印处理
- **用途**: 回调处理
- **上下文**: 文件 `callback.c`

### 转换步骤

#### Function Pointer

1. **参数规范化**:
   - `a` → `a`
   - `b` → `b`

2. **表达式描述**:
   - 比较两个整数
   - → `compare two integers`

3. **文本组装**:
   ```
   Lambda expression comparison function for sorting with parameters a b returns integer file sort
   ```

#### Callback

1. **参数规范化**:
   - `x` → `x`

2. **表达式描述**:
   - 打印处理信息
   - → `print processing information`

3. **文本组装**:
   ```
   Lambda expression callback function with parameters x returns void file callback
   ```

### 输出结果

```
Lambda expression comparison function for sorting with parameters a b returns integer file sort
Lambda expression callback function with parameters x returns void file callback
```

---

## 转换要点总结

### 1. 参数处理

| 原始参数 | 转换后 | 说明 |
|---------|--------|------|
| `user` | `user` | 单个参数 |
| `(sum, item)` | `sum item` | 多个参数 |
| `|&x|` | `x` | 引用参数 |
| `|a, b|` | `a b` | Rust闭包参数 |

### 2. 表达式描述

- **简单表达式**：直接描述操作（如 `user.age` → `get user age`）
- **复杂表达式**：描述整体逻辑（如 `x * 2 + 1` → `multiply by 2 and add 1`）
- **条件表达式**：描述条件逻辑（如 `x > 0 ? x : 0` → `return x if positive else 0`）

### 3. 返回类型推断

- **布尔表达式**：`returns boolean`
- **数值运算**：`returns number/integer`
- **字符串操作**：`returns string`
- **集合操作**：`returns collection`

### 4. 上下文信息

- **文件路径**：标识Lambda所在文件
- **使用场景**：描述Lambda的用途（过滤、映射、排序等）

## 复杂场景处理

### 1. 捕获外部变量

#### 输入代码

```javascript
const threshold = 10;
const filtered = numbers.filter(x => x > threshold);
```

#### 处理方式

在描述中注明捕获的变量：

```
Lambda expression filter by threshold with parameters x captures threshold returns boolean file filter
```

### 2. 嵌套Lambda

#### 输入代码

```python
processed = list(map(lambda x: x * 2, filter(lambda y: y > 0, numbers)))
```

#### 处理方式

分别转换每个Lambda：

```
Lambda expression filter positive numbers with parameters y returns boolean file process
Lambda expression multiply by 2 with parameters x returns number file process
```

### 3. 多行Lambda

#### 输入代码

```javascript
const complex = items.map(item => {
    const processed = item.value * 2;
    return processed + 1;
});
```

#### 处理方式

描述整体逻辑：

```
Lambda expression complex processing with parameters item returns number file process js
```

## AST提取逻辑

### Tree-sitter查询示例

```typescript
// JavaScript箭头函数查询
const jsArrowQuery = `
  (arrow_function
    parameters: (formal_parameters
      (identifier) @param
    )
    body: (expression_statement
      (binary_expression) @expression
    )
  )
`;

// Python Lambda查询
const pythonLambdaQuery = `
  (lambda
    parameters: (lambda_parameters
      (identifier) @param
    )
    body: (expression) @expression
  )
`;

// Java Lambda查询
const javaLambdaQuery = `
  (lambda_expression
    parameters: (lambda_parameters
      (identifier) @param
    )
    body: (expression) @expression
  )
`;

// Rust闭包查询
const rustClosureQuery = `
  (closure_expression
    parameters: (closure_parameters
      (identifier) @param
    )
    body: (expression) @expression
  )
`;
```

## 总结

### 核心优势

1. **简洁表达**：Lambda表达式通常用于简短操作，自然语言描述简洁明了
2. **上下文清晰**：通过使用场景（过滤、映射、排序）快速理解Lambda用途
3. **参数明确**：参数列表清晰，便于理解输入输出
4. **易于搜索**：通过描述和参数可以快速找到相关Lambda表达式

### 应用场景

1. **代码搜索**：搜索特定用途的Lambda表达式
2. **代码理解**：快速理解Lambda的功能
3. **重构分析**：分析Lambda的使用模式
4. **文档生成**：自动生成Lambda的文档说明

### 后续扩展

1. **Lambda链分析**：分析多个Lambda组合的复杂逻辑
2. **性能分析**：识别可能影响性能的Lambda使用
3. **可读性评估**：评估Lambda表达式的可读性
4. **重构建议**：建议将复杂Lambda提取为命名函数