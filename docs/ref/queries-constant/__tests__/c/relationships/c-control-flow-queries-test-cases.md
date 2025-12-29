# C语言控制流关系Tree-Sitter查询规则测试用例

本文档为C语言控制流关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. if语句控制流

### 查询规则
```
[
  (if_statement
    condition: (_) @source.condition
    consequence: (statement) @target.if.block) @control.flow.if
  (if_statement
    condition: (_) @source.condition
    consequence: (statement) @target.if.block
    alternative: (else_clause
      (statement) @target.else.block)) @control.flow.if.else
  (if_statement
    condition: (_) @source.outer.condition
    consequence: (compound_statement
      (if_statement
        condition: (_) @source.inner.condition
        consequence: (statement) @target.inner.block))) @control.flow.nested.if
  (if_statement
    condition: (_) @source.first.condition
    consequence: (statement) @target.first.block
    alternative: (else_clause
      (if_statement
        condition: (_) @source.second.condition
        consequence: (statement) @target.second.block))) @control.flow.else.if
] @control.flow.if.statement
```

### 测试用例
```c
// 基本if语句
void basic_if_statement() {
    int x = 10;
    if (x > 5) {
        printf("x is greater than 5\n");
    }
}

// if-else语句
void if_else_statement() {
    int x = 10;
    if (x > 5) {
        printf("x is greater than 5\n");
    } else {
        printf("x is not greater than 5\n");
    }
}

// 嵌套if语句
void nested_if_statement() {
    int x = 10, y = 20;
    if (x > 5) {
        if (y > 15) {
            printf("Both conditions are true\n");
        }
    }
}

// else-if语句
void else_if_statement() {
    int x = 10;
    if (x > 20) {
        printf("x is greater than 20\n");
    } else if (x > 10) {
        printf("x is greater than 10\n");
    } else {
        printf("x is 10 or less\n");
    }
}

// 复杂if语句
void complex_if_statement() {
    int a = 10, b = 20, c = 30;
    if (a > b && b > c) {
        printf("a > b > c\n");
    } else if (a > b || b > c) {
        printf("Either a > b or b > c\n");
    } else {
        printf("Neither condition is true\n");
    }
}
```

## 2. switch语句控制流

### 查询规则
```
(switch_statement
  condition: (_) @source.switch.variable
  body: (compound_statement) @target.switch.block) @control.flow.switch
```

### 测试用例
```c
// 基本switch语句
void basic_switch_statement() {
    int day = 3;
    switch (day) {
        case 1:
            printf("Monday\n");
            break;
        case 2:
            printf("Tuesday\n");
            break;
        case 3:
            printf("Wednesday\n");
            break;
        default:
            printf("Other day\n");
            break;
    }
}

// 复杂switch语句
void complex_switch_statement() {
    char grade = 'B';
    switch (grade) {
        case 'A':
            printf("Excellent\n");
            break;
        case 'B':
            printf("Good\n");
            break;
        case 'C':
            printf("Average\n");
            break;
        case 'D':
            printf("Below Average\n");
            break;
        case 'F':
            printf("Fail\n");
            break;
        default:
            printf("Invalid grade\n");
            break;
    }
}

// switch语句与枚举
void switch_with_enum() {
    enum Color { RED, GREEN, BLUE };
    enum Color color = GREEN;
    
    switch (color) {
        case RED:
            printf("Red color\n");
            break;
        case GREEN:
            printf("Green color\n");
            break;
        case BLUE:
            printf("Blue color\n");
            break;
    }
}
```

## 3. switch case控制流

### 查询规则
```
(case_statement
  value: (_)? @source.case.value
  (statement)? @target.case.block) @control.flow.switch.case
```

### 测试用例
```c
// 基本case语句
void basic_case_statement() {
    int option = 2;
    switch (option) {
        case 1:
            printf("Option 1 selected\n");
            break;
        case 2:
            printf("Option 2 selected\n");
            break;
        case 3:
            printf("Option 3 selected\n");
            break;
    }
}

// 多个case共享代码块
void multiple_case_statement() {
    char vowel = 'e';
    switch (vowel) {
        case 'a':
        case 'e':
        case 'i':
        case 'o':
        case 'u':
            printf("Vowel\n");
            break;
        default:
            printf("Consonant\n");
            break;
    }
}

// case语句与表达式
void case_with_expressions() {
    int value = 10;
    switch (value) {
        case 1 + 2:
            printf("Value is 3\n");
            break;
        case 5 * 2:
            printf("Value is 10\n");
            break;
        case 20 - 5:
            printf("Value is 15\n");
            break;
    }
}
```

## 4. switch default控制流

### 查询规则
```
(case_statement
  (statement)? @target.default.block) @control.flow.switch.default
```

### 测试用例
```c
// 基本default语句
void basic_default_statement() {
    int option = 5;
    switch (option) {
        case 1:
            printf("Option 1\n");
            break;
        case 2:
            printf("Option 2\n");
            break;
        case 3:
            printf("Option 3\n");
            break;
        default:
            printf("Default option\n");
            break;
    }
}

// default语句在开头
void default_at_beginning() {
    int value = 10;
    switch (value) {
        default:
            printf("Default case\n");
            break;
        case 5:
            printf("Value is 5\n");
            break;
        case 10:
            printf("Value is 10\n");
            break;
    }
}

// default语句与复杂逻辑
void complex_default_statement() {
    int status = 404;
    switch (status) {
        case 200:
            printf("OK\n");
            break;
        case 301:
            printf("Moved Permanently\n");
            break;
        case 404:
            printf("Not Found\n");
            break;
        default:
            printf("Unknown status code\n");
            break;
    }
}
```

## 5. while循环控制流

### 查询规则
```
(while_statement
  condition: (_) @source.while.condition
  body: (statement) @target.while.block) @control.flow.while
```

### 测试用例
```c
// 基本while循环
void basic_while_loop() {
    int i = 0;
    while (i < 10) {
        printf("%d ", i);
        i++;
    }
    printf("\n");
}

// 复杂while循环
void complex_while_loop() {
    int sum = 0;
    int i = 1;
    while (i <= 100) {
        sum += i;
        i++;
    }
    printf("Sum: %d\n", sum);
}

// while循环与条件表达式
void while_with_condition() {
    int x = 10;
    while (x > 0 && x < 100) {
        printf("x = %d\n", x);
        x *= 2;
    }
}

// 无限while循环
void infinite_while_loop() {
    int count = 0;
    while (1) {
        printf("Count: %d\n", count);
        count++;
        if (count >= 10) {
            break;
        }
    }
}
```

## 6. do-while循环控制流

### 查询规则
```
(do_statement
  body: (statement) @source.do.block
  condition: (_) @target.while.condition) @control.flow.do.while
```

### 测试用例
```c
// 基本do-while循环
void basic_do_while_loop() {
    int i = 0;
    do {
        printf("%d ", i);
        i++;
    } while (i < 10);
    printf("\n");
}

// do-while循环与用户输入
void do_while_with_input() {
    int number;
    do {
        printf("Enter a number (0 to exit): ");
        scanf("%d", &number);
        printf("You entered: %d\n", number);
    } while (number != 0);
}

// 复杂do-while循环
void complex_do_while_loop() {
    int sum = 0;
    int number;
    do {
        printf("Enter a positive number (negative to stop): ");
        scanf("%d", &number);
        if (number > 0) {
            sum += number;
        }
    } while (number > 0);
    printf("Sum: %d\n", sum);
}
```

## 7. for循环控制流

### 查询规则
```
(for_statement
  initializer: (_)? @source.for.init
  condition: (_) @source.for.condition
  update: (_)? @source.for.update
  body: (_) @target.for.block) @control.flow.for
```

### 测试用例
```c
// 基本for循环
void basic_for_loop() {
    for (int i = 0; i < 10; i++) {
        printf("%d ", i);
    }
    printf("\n");
}

// 复杂for循环
void complex_for_loop() {
    int sum = 0;
    for (int i = 1; i <= 100; i++) {
        sum += i;
    }
    printf("Sum: %d\n", sum);
}

// 省略初始化的for循环
void for_without_init() {
    int i = 0;
    for (; i < 10; i++) {
        printf("%d ", i);
    }
    printf("\n");
}

// 省略条件的for循环
void for_without_condition() {
    int i = 0;
    for (; ; i++) {
        if (i >= 10) {
            break;
        }
        printf("%d ", i);
    }
    printf("\n");
}

// 省略更新的for循环
void for_without_update() {
    for (int i = 0; i < 10; ) {
        printf("%d ", i);
        i++;
    }
    printf("\n");
}
```

## 8. 嵌套循环控制流

### 查询规则
```
(for_statement
  body: (compound_statement
    (for_statement
      condition: (_) @source.inner.condition
      body: (statement) @target.inner.block))) @control.flow.nested.loop
```

### 测试用例
```c
// 基本嵌套循环
void basic_nested_loop() {
    for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 3; j++) {
            printf("(%d, %d) ", i, j);
        }
        printf("\n");
    }
}

// 矩阵乘法嵌套循环
void matrix_multiplication() {
    int a[2][2] = {{1, 2}, {3, 4}};
    int b[2][2] = {{5, 6}, {7, 8}};
    int c[2][2] = {0};
    
    for (int i = 0; i < 2; i++) {
        for (int j = 0; j < 2; j++) {
            for (int k = 0; k < 2; k++) {
                c[i][j] += a[i][k] * b[k][j];
            }
        }
    }
}

// 不同类型循环嵌套
void mixed_nested_loops() {
    int i = 0;
    while (i < 3) {
        for (int j = 0; j < 3; j++) {
            printf("(%d, %d) ", i, j);
        }
        printf("\n");
        i++;
    }
}
```

## 9. 循环控制语句

### 查询规则
```
[
  (break_statement) @control.flow.loop.break
  (continue_statement) @control.flow.loop.continue
] @control.flow.loop.control
```

### 测试用例
```c
// break语句
void break_statement() {
    for (int i = 0; i < 10; i++) {
        if (i == 5) {
            break;
        }
        printf("%d ", i);
    }
    printf("\n");
}

// continue语句
void continue_statement() {
    for (int i = 0; i < 10; i++) {
        if (i % 2 == 0) {
            continue;
        }
        printf("%d ", i);
    }
    printf("\n");
}

// 嵌套循环中的break和continue
void nested_break_continue() {
    for (int i = 0; i < 3; i++) {
        for (int j = 0; j < 3; j++) {
            if (i == 1 && j == 1) {
                break;
            }
            if (j == 0) {
                continue;
            }
            printf("(%d, %d) ", i, j);
        }
        printf("\n");
    }
}

// switch语句中的break
void break_in_switch() {
    int option = 2;
    switch (option) {
        case 1:
            printf("Option 1\n");
            break;
        case 2:
            printf("Option 2\n");
            break;
        default:
            printf("Default\n");
            break;
    }
}
```

## 10. goto语句控制流

### 查询规则
```
(goto_statement
  (statement_identifier) @target.label) @control.flow.goto
```

### 测试用例
```c
// 基本goto语句
void basic_goto() {
    int i = 0;
    start:
    printf("%d ", i);
    i++;
    if (i < 10) {
        goto start;
    }
    printf("\n");
}

// 错误处理中的goto
void error_handling_with_goto() {
    FILE *file = fopen("test.txt", "r");
    if (file == NULL) {
        goto error;
    }
    
    // 文件操作
    if (fread(buffer, 1, sizeof(buffer), file) == 0) {
        goto close_file;
    }
    
    // 处理数据
    process_data(buffer);
    
close_file:
    fclose(file);
    return;
    
error:
    printf("Error occurred\n");
}

// 复杂goto使用
void complex_goto() {
    int state = 0;
    
    start:
    switch (state) {
        case 0:
            printf("State 0\n");
            state = 1;
            goto start;
        case 1:
            printf("State 1\n");
            state = 2;
            goto start;
        case 2:
            printf("State 2\n");
            goto end;
    }
    
end:
    printf("End of program\n");
}
```

## 11. 标签语句控制流

### 查询规则
```
(labeled_statement
  label: (statement_identifier) @source.label
  (statement) @target.labeled.statement) @control.flow.label
```

### 测试用例
```c
// 基本标签语句
void basic_label() {
    int i = 0;
    
    loop_start:
    printf("Iteration %d\n", i);
    i++;
    
    if (i < 5) {
        goto loop_start;
    }
}

// 多个标签
void multiple_labels() {
    int x = 10;
    
    if (x > 5) {
        goto positive;
    } else {
        goto negative;
    }
    
positive:
    printf("x is positive\n");
    goto end;
    
negative:
    printf("x is negative or zero\n");
    
end:
    printf("End of program\n");
}

// 嵌套标签
void nested_labels() {
    int i = 0, j = 0;
    
outer_loop:
    printf("Outer loop: %d\n", i);
    
inner_loop:
    printf("  Inner loop: %d\n", j);
    j++;
    
    if (j < 3) {
        goto inner_loop;
    }
    
    j = 0;
    i++;
    
    if (i < 3) {
        goto outer_loop;
    }
}
```

## 12. return语句控制流

### 查询规则
```
(return_statement
  (_)? @source.return.variable) @control.flow.return
```

### 测试用例
```c
// 基本return语句
int basic_return() {
    return 42;
}

// 带变量的return语句
int return_with_variable() {
    int result = 10 + 20;
    return result;
}

// 条件return语句
int conditional_return() {
    int x = 10;
    if (x > 5) {
        return 1;
    } else {
        return 0;
    }
}

// void函数的return语句
void void_return() {
    printf("Before return\n");
    return;
    printf("This will not be printed\n");
}

// 表达式return语句
int expression_return() {
    int a = 5, b = 3;
    return a + b * 2;
}

// 复杂return语句
int complex_return() {
    int* array = malloc(sizeof(int) * 10);
    if (array == NULL) {
        return -1;
    }
    
    // 处理数组
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        sum += array[i];
    }
    
    free(array);
    return sum;
}
```

## 13. 函数调用控制流

### 查询规则
```
[
  (expression_statement
    (call_expression
      function: (identifier) @target.function
      arguments: (argument_list
        (_)* @source.parameter))) @control.flow.function.call
  (call_expression
    function: (identifier) @target.recursive.function
    arguments: (argument_list
      (_)* @source.parameter)) @control.flow.recursive.call
] @control.flow.function.invocation
```

### 测试用例
```c
// 基本函数调用
void basic_function_call() {
    printf("Hello, World!\n");
    int result = calculate(10, 20);
    process_data(&result, sizeof(result));
}

// 递归函数调用
int recursive_function(int n) {
    if (n <= 1) {
        return n;
    }
    return recursive_function(n - 1) + recursive_function(n - 2);
}

// 复杂函数调用
void complex_function_calls() {
    int x = 10, y = 20;
    int result = calculate(
        add(x, y),
        multiply(x, y)
    );
    
    process_data(
        &result,
        sizeof(result),
        callback_function,
        user_data
    );
}

// 嵌套函数调用
void nested_function_calls() {
    int result = calculate(
        add(5, multiply(2, 3)),
        subtract(10, divide(20, 4))
    );
    
    printf("Result: %d\n", result);
}
```

## 14. 条件表达式控制流

### 查询规则
```
(conditional_expression
  condition: (_)? @source.condition
  consequence: (_)? @source.consequence
  alternative: (_)? @source.alternative) @control.flow.conditional.expression
```

### 测试用例
```c
// 基本条件表达式
void basic_conditional_expression() {
    int a = 10, b = 20;
    int max = (a > b) ? a : b;
    printf("Max: %d\n", max);
}

// 嵌套条件表达式
void nested_conditional_expression() {
    int a = 10, b = 20, c = 15;
    int max = (a > b) ? ((a > c) ? a : c) : ((b > c) ? b : c);
    printf("Max: %d\n", max);
}

// 条件表达式与函数调用
void conditional_with_function_call() {
    int x = 10;
    int result = (x > 0) ? calculate_positive(x) : calculate_negative(x);
    printf("Result: %d\n", result);
}

// 复杂条件表达式
void complex_conditional_expression() {
    int a = 10, b = 20, c = 30;
    char* message = (a > b) ? "a is greater" : 
                    ((b > c) ? "b is greater" : "c is greater");
    printf("%s\n", message);
}

// 条件表达式作为参数
void conditional_as_parameter() {
    int x = 10, y = 20;
    printf("Result: %d\n", calculate((x > y) ? x : y, (x < y) ? y : x));
}
```

## 15. 逻辑运算符控制流

### 查询规则
```
(binary_expression
  left: (_)? @source.left.operand
  operator: ["&&" "||"]
  right: (_)? @source.right.operand) @control.flow.logical.operator
```

### 测试用例
```c
// 基本逻辑运算符
void basic_logical_operators() {
    int a = 10, b = 20, c = 30;
    
    if (a > b && b > c) {
        printf("a > b > c\n");
    }
    
    if (a > b || b > c) {
        printf("Either a > b or b > c\n");
    }
}

// 复杂逻辑运算符
void complex_logical_operators() {
    int x = 10, y = 20, z = 30;
    
    if ((x > y && y > z) || (x < y && y < z)) {
        printf("Either strictly increasing or strictly decreasing\n");
    }
    
    if (x > 0 && y > 0 && z > 0) {
        printf("All positive\n");
    }
}

// 逻辑运算符与函数调用
void logical_with_function_calls() {
    int x = 10, y = 20;
    
    if (is_valid(x) && is_valid(y)) {
        printf("Both are valid\n");
    }
    
    if (is_positive(x) || is_positive(y)) {
        printf("At least one is positive\n");
    }
}

// 短路求值
void short_circuit_evaluation() {
    int x = 10, y = 0;
    
    // 短路求值：如果x > 5为false，则不会调用is_positive(y)
    if (x > 5 && is_positive(y)) {
        printf("This won't be printed if y is zero\n");
    }
    
    // 短路求值：如果x > 5为true，则不会调用is_positive(y)
    if (x > 5 || is_positive(y)) {
        printf("This will be printed even if y is zero\n");
    }
}
```

## 16. 逗号表达式控制流

### 查询规则
```
(comma_expression
  left: (_)? @source.left.expression
  right: (_)? @source.right.expression) @control.flow.comma.expression
```

### 测试用例
```c
// 基本逗号表达式
void basic_comma_expression() {
    int a = 10, b = 20;
    int result = (a++, b++);
    printf("a: %d, b: %d, result: %d\n", a, b, result);
}

// for循环中的逗号表达式
void comma_in_for_loop() {
    for (int i = 0, j = 10; i < 10; i++, j--) {
        printf("(%d, %d) ", i, j);
    }
    printf("\n");
}

// 复杂逗号表达式
void complex_comma_expression() {
    int a = 10, b = 20, c = 30;
    int result = (a += b, b += c, a + b);
    printf("a: %d, b: %d, result: %d\n", a, b, result);
}

// 逗号表达式与函数调用
void comma_with_function_calls() {
    int x = 10, y = 20;
    int result = (printf("First expression\n"), x + y);
    printf("Result: %d\n", result);
}

// 嵌套逗号表达式
void nested_comma_expression() {
    int a = 10, b = 20, c = 30;
    int result = ((a++, b++), (c++, a + b + c));
    printf("a: %d, b: %d, c: %d, result: %d\n", a, b, c, result);
}
```

## 17. 函数定义控制流

### 查询规则
```
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @source.function.name)
  body: (compound_statement) @target.function.body) @control.flow.function.definition
```

### 测试用例
```c
// 基本函数定义
int basic_function(int x, int y) {
    return x + y;
}

// 复杂函数定义
void complex_function(int* array, int size) {
    for (int i = 0; i < size; i++) {
        if (array[i] % 2 == 0) {
            printf("%d is even\n", array[i]);
        } else {
            printf("%d is odd\n", array[i]);
        }
    }
}

// 递归函数定义
int factorial(int n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

// 带多个控制流的函数定义
int multi_control_flow_function(int x, int y) {
    if (x > y) {
        return 1;
    } else if (x < y) {
        return -1;
    } else {
        return 0;
    }
}

// 复杂嵌套控制流的函数定义
void nested_control_flow_function(int n) {
    for (int i = 0; i < n; i++) {
        switch (i % 3) {
            case 0:
                printf("%d is divisible by 3\n", i);
                break;
            case 1:
                printf("%d leaves remainder 1\n", i);
                break;
            case 2:
                printf("%d leaves remainder 2\n", i);
                break;
        }
    }
}
```

## 18. 函数指针调用控制流

### 查询规则
```
(call_expression
  function: (_) @source.function.pointer
  arguments: (argument_list
    (_)* @source.parameter)) @control.flow.function.pointer.call
```

### 测试用例
```c
// 基本函数指针调用
void basic_function_pointer_call() {
    int (*operation)(int, int) = add;
    int result = operation(10, 20);
    printf("Result: %d\n", result);
}

// 函数指针数组调用
void function_pointer_array_call() {
    int (*operations[])(int, int) = {add, subtract, multiply, divide};
    
    for (int i = 0; i < 4; i++) {
        int result = operations[i](10, 5);
        printf("Operation %d result: %d\n", i, result);
    }
}

// 结构体中的函数指针调用
void struct_function_pointer_call() {
    struct Calculator {
        int (*add)(int, int);
        int (*subtract)(int, int);
    } calculator;
    
    calculator.add = add;
    calculator.subtract = subtract;
    
    int sum = calculator.add(10, 20);
    int diff = calculator.subtract(20, 10);
    
    printf("Sum: %d, Difference: %d\n", sum, diff);
}

// 函数指针作为参数的调用
void function_pointer_as_parameter() {
    int numbers[] = {1, 2, 3, 4, 5};
    int sum = process_array(numbers, 5, add);
    printf("Sum: %d\n", sum);
}

// 复杂函数指针调用
void complex_function_pointer_call() {
    int (*operations[])(int, int) = {add, subtract, multiply, divide};
    int (*get_operation(int))(int, int);
    
    for (int i = 0; i < 4; i++) {
        int (*op)(int, int) = get_operation(i);
        int result = op(10, 5);
        printf("Operation %d result: %d\n", i, result);
    }
}
```

## 19. 短路求值与控制流

### 查询规则
```
[
  (binary_expression
    left: (_)? @source.left.operand
    operator: "&&"
    right: (call_expression
      function: (identifier) @target.short.circuit.function)) @control.flow.short.circuit.and
  (binary_expression
    left: (_)? @source.left.operand
    operator: "||"
    right: (call_expression
      function: (identifier) @target.short.circuit.function)) @control.flow.short.circuit.or
] @control.flow.short.circuit
```

### 测试用例
```c
// 短路求值与&&运算符
void short_circuit_and() {
    int x = 10, y = 0;
    
    // 如果x > 5为false，则不会调用is_positive(y)
    if (x > 5 && is_positive(y)) {
        printf("Both conditions are true\n");
    } else {
        printf("At least one condition is false\n");
    }
    
    // 如果x > 15为false，则不会调用calculate(x, y)
    int result = (x > 15 && calculate(x, y)) ? 1 : 0;
    printf("Result: %d\n", result);
}

// 短路求值与||运算符
void short_circuit_or() {
    int x = 10, y = 0;
    
    // 如果x > 5为true，则不会调用is_positive(y)
    if (x > 5 || is_positive(y)) {
        printf("At least one condition is true\n");
    } else {
        printf("Both conditions are false\n");
    }
    
    // 如果x > 5为true，则不会调用calculate(x, y)
    int result = (x > 5 || calculate(x, y)) ? 1 : 0;
    printf("Result: %d\n", result);
}

// 复杂短路求值
void complex_short_circuit() {
    int a = 10, b = 20, c = 0;
    
    // 复杂的短路求值表达式
    if ((a > b && is_positive(c)) || (b > a && calculate(a, c))) {
        printf("Complex condition is true\n");
    }
    
    // 嵌套短路求值
    if ((a > 0 && (b > 0 || is_positive(c))) && calculate(a, b)) {
        printf("Nested short-circuit condition is true\n");
    }
}

// 短路求值与函数调用链
void short_circuit_with_function_chain() {
    int x = 10, y = 0;
    
    // 短路求值与函数调用链
    if (is_valid(x) && (is_positive(y) || calculate(x, y))) {
        printf("Complex function chain condition is true\n");
    }
    
    // 多层短路求值
    if (check_condition1(x) && 
        (check_condition2(y) || 
         (check_condition3(x, y) && calculate(x, y)))) {
        printf("Multi-level short-circuit condition is true\n");
    }
}
```

## 20. 函数声明控制流

### 查询规则
```
(declaration
  (function_declarator
    declarator: (identifier) @source.function.name)) @control.flow.function.declaration
```

### 测试用例
```c
// 基本函数声明
int basic_function(int x, int y);

// 带指针参数的函数声明
void pointer_function(int* array, int size);

// 带结构体参数的函数声明
struct Point {
    int x;
    int y;
};
void struct_function(struct Point p);

// 带函数指针参数的函数声明
void callback_function(int (*operation)(int, int));

// 带可变参数的函数声明
int variadic_function(int count, ...);

// 带复杂返回类型的函数声明
struct ComplexStruct* complex_return_function(int param1, float param2);

// 带const限定符的函数声明
const char* const_function(const char* input);

// 带restrict限定符的函数声明
void restrict_function(int* restrict ptr1, int* restrict ptr2);

// 内联函数声明
inline int inline_function(int x, int y);

// 带复杂参数的函数声明
int complex_parameter_function(
    int array[10],
    struct Point* points,
    void (*callback)(int, void*),
    const char* format,
    ...
);
```

## 21. 递归函数调用控制流

### 查询规则
```
(call_expression
  function: (identifier) @target.recursive.function
  arguments: (argument_list
    (_)* @source.parameter)) @control.flow.recursive.call
```

### 测试用例
```c
// 基本递归函数
int factorial(int n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);  // 递归调用
}

// 斐波那契递归函数
int fibonacci(int n) {
    if (n <= 1) {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);  // 递归调用
}

// 二叉树递归遍历
struct TreeNode {
    int value;
    struct TreeNode* left;
    struct TreeNode* right;
};

void traverse_tree(struct TreeNode* node) {
    if (node == NULL) {
        return;
    }
    
    printf("%d ", node->value);
    traverse_tree(node->left);   // 递归调用
    traverse_tree(node->right);  // 递归调用
}

// 快速排序递归函数
void quick_sort(int arr[], int low, int high) {
    if (low < high) {
        int pivot = partition(arr, low, high);
        quick_sort(arr, low, pivot - 1);   // 递归调用
        quick_sort(arr, pivot + 1, high);  // 递归调用
    }
}

// 复杂递归函数
int complex_recursive(int n, int* memo) {
    if (n <= 1) {
        return n;
    }
    
    if (memo[n] != -1) {
        return memo[n];
    }
    
    memo[n] = complex_recursive(n - 1, memo) + 
              complex_recursive(n - 2, memo);  // 递归调用
    return memo[n];
}

// 相互递归函数
int is_even(int n);
int is_odd(int n);

int is_even(int n) {
    if (n == 0) {
        return 1;
    }
    return is_odd(n - 1);  // 递归调用
}

int is_odd(int n) {
    if (n == 0) {
        return 0;
    }
    return is_even(n - 1);  // 递归调用
}
```

## 22. 综合测试用例

### 测试用例
```c
// 综合控制流示例
void comprehensive_control_flow() {
    int x = 10, y = 20, z = 30;
    
    // if语句与函数调用
    if (x > y) {
        printf("x is greater than y\n");
    } else if (y > z) {
        printf("y is greater than z\n");
    } else {
        printf("z is the greatest\n");
    }
    
    // switch语句
    switch (x % 3) {
        case 0:
            printf("x is divisible by 3\n");
            break;
        case 1:
            printf("x leaves remainder 1\n");
            break;
        case 2:
            printf("x leaves remainder 2\n");
            break;
    }
    
    // for循环与条件表达式
    for (int i = 0; i < 10; i++) {
        int result = (i % 2 == 0) ? i * 2 : i * 3;
        printf("%d: %d\n", i, result);
    }
    
    // while循环与短路求值
    while (x > 0 && y > 0) {
        if (is_positive(x) || is_positive(y)) {
            printf("Processing: %d, %d\n", x, y);
        }
        x--;
        y--;
    }
    
    // 递归函数调用
    int fact = factorial(5);
    printf("Factorial of 5: %d\n", fact);
    
    // 函数指针调用
    int (*op)(int, int) = add;
    int sum = op(x, y);
    printf("Sum: %d\n", sum);
    
    // 逗号表达式
    int a = 10, b = 20;
    int result = (a++, b++, a + b);
    printf("Result: %d\n", result);
    
    // goto语句
    int count = 0;
start:
    printf("Count: %d\n", count);
    count++;
    if (count < 3) {
        goto start;
    }
    
    // return语句
    return;
}