0.0.3
根据您提供的 JSON 数据，以下是 tree-sitter-vue 支持的**命名节点**（named nodes）类型列表，已过滤掉纯符号节点：

1. **attribute** - 普通属性
2. **component** - 组件
3. **directive_argument** - 指令参数
4. **directive_attribute** - 指令属性
5. **directive_dynamic_argument** - 动态指令参数
6. **directive_modifier** - 指令修饰符
7. **directive_modifiers** - 指令修饰符列表
8. **element** - 元素
9. **end_tag** - 结束标签
10. **erroneous_end_tag** - 错误结束标签
11. **interpolation** - 插值
12. **props** - props
13. **quoted_attribute_value** - 带引号的属性值
14. **script_element** - script 元素
15. **script_lang** - script 语言
16. **self_closing_tag** - 自闭合标签
17. **start_tag** - 开始标签
18. **style_element** - style 元素
19. **style_lang** - style 语言
20. **suspense** - suspense 组件
21. **template_element** - template 元素
22. **text** - 文本
23. **vue_component** - Vue 组件
24. **attribute_name** - 属性名
25. **attribute_value** - 属性值
26. **comment** - 注释
27. **css_val** - CSS 值
28. **directive_dynamic_argument_value** - 动态指令参数值
29. **directive_name** - 指令名
30. **erroneous_end_tag_name** - 错误结束标签名
31. **raw_text** - 原始文本
32. **scss_val** - SCSS 值
33. **tag_name** - 标签名
34. **ts_lang** - TypeScript 语言标识
35. **tsx_lang** - TSX 语言标识

以上共 **35** 个命名节点类型，覆盖了 Vue 单文件组件中的核心语法结构。
