在 Tree-sitter 查询语言中，`#match?` 和 `#eq?` 是两种用于对捕获的节点内容进行条件过滤的谓词（predicates）。

它们的主要区别在于**匹配方式**：

1.  **`#eq?` (Equal)**:
    *   用于**精确的字符串比较**。
    *   它检查捕获的节点文本内容是否**完全等于**指定的字符串。
    *   例如，`(#eq? @thread.method "join")` 只有在 `@thread.method` 捕获的节点内容恰好是 "join" 这个字符串时，才会匹配成功。它不会匹配 "join_other" 或 "myjoin"。

2.  **`#match?` (Match)**:
    *   用于**正则表达式匹配**。
    *   它将指定的字符串视为一个正则表达式，并检查捕获的节点文本内容是否与该正则表达式匹配。
    *   例如，`(#match? @thread.method "join")` 会匹配任何包含 "join" 子串的节点内容，如 "join", "join_other", "myjoin", "rejoin" 等。如果要精确匹配 "join"，需要使用 `(#match? @thread.method "^join$")`，其中 `^` 表示字符串开始，`$` 表示字符串结束。

**总结**:

*   如果你需要**精确匹配**一个固定的字符串（如特定的方法名 `join`, `detach`, `lock`），使用 `#eq?` 更为**精确、高效**，并且意图更清晰。
*   如果你需要匹配一个**模式**（如匹配多个可能的值 `^(lock|unlock)$`，或者检查一个字符串是否包含某个子串），则使用 `#match?`。

在我们之前修改的 C++ 并发关系查询中，对于像 "join"、"detach" 这样特定的、固定的 C++ 方法名，使用 `#eq?` 是更优的选择。而对于需要匹配多个不同值的模式（如 `^(lock_guard|unique_lock|shared_lock)$`），则必须使用 `#match?`。