# Tree-sitter 查询系统死锁分析与解决方案

## 问题现象

在集成测试和单元测试中,tree-sitter查询相关的测试完全卡死,无法执行。即使使用lazy_static或OnceLock都无法解决问题。

## 死锁根本原因

### 1. 嵌套锁问题

当前实现中存在多层嵌套锁:

```
初始化调用链:
init_test_environment()
  → QueryLoader::global()                    // 获取GLOBAL_LOADER的OnceLock
    → QueryLoader::new()                     // 在OnceLock内部调用
      → {空初始化,返回空QueryLoader}          // ✅ 这里没问题

  → loader.get_entity_query(lang)            // 调用get_query
    → self.cache.entry(lang).or_default()    // 🔒 获取DashMap的entry锁
    → Self::get_tree_sitter_language(lang)   // ⚠️ 在持有entry锁时调用
      → LANGUAGE_CACHE.get_or_init(...)      // 🔒 尝试获取LANGUAGE_CACHE的OnceLock
        → DashMap::new()                     // ✅ 第一次调用,成功
        → cache.get(lang)                    // 🔒 尝试获取DashMap的读锁
        → cache.insert(lang, ts_lang)        // 🔒 尝试获取DashMap的写锁
```

**关键问题代码** (loader.rs:194-207):

```rust
// 在get_query中
let mut lang_entry = self.cache.entry(language.clone()).or_default();
// 🔒 此时持有self.cache的entry锁

let ts_language = Self::get_tree_sitter_language(language)?;
// ⚠️ 在持有entry锁时调用get_tree_sitter_language
// 这会尝试获取LANGUAGE_CACHE的OnceLock和内部DashMap的锁
```

### 2. 锁的嵌套层次

```
锁A: QueryLoader.cache的entry锁 (DashMap)
  └─ 锁B: LANGUAGE_CACHE的OnceLock
      └─ 锁C: LANGUAGE_CACHE内部DashMap的读锁
      └─ 锁D: LANGUAGE_CACHE内部DashMap的写锁
```

这种嵌套锁结构在多线程环境下极易死锁。

### 3. #[ctor::ctor]与系统锁竞争

`#[ctor::ctor]`在DLL加载时执行,此时:
- 可能持有Windows loader lock
- 与OnceLock/DashMap的锁形成竞争
- 在Windows平台上特别容易死锁

### 4. lazy_static同样的问题

lazy_static内部也使用锁,嵌套访问时形成死锁:

```rust
lazy_static! {
    static ref GLOBAL_LOADER: QueryLoader = ...;  // 锁A
}

lazy_static! {
    static ref LANGUAGE_CACHE: DashMap<...> = ...;  // 锁B
}

// 访问GLOBAL_LOADER时持有锁A,内部又访问LANGUAGE_CACHE获取锁B
```

## 解决方案

### 核心原则

1. **初始化阶段不持有任何锁**
2. **初始化完成后,资源只读或使用简单锁**
3. **避免在DLL加载时执行复杂初始化**
4. **避免嵌套获取不同类型的锁**

### 方案: 分离初始化 + 移除ctor

#### 1. 分离语言初始化

将tree-sitter语言初始化完全独立出来,在程序启动时同步完成:

```rust
// src/utils/tree_sitter_init.rs
use std::collections::HashMap;
use std::sync::OnceLock;
use tree_sitter::Language as TsLanguage;
use crate::types::language::Language;

/// Global tree-sitter language instances
/// Initialized once at program startup, read-only afterwards
static LANGUAGES: OnceLock<HashMap<Language, TsLanguage>> = OnceLock::new();

/// Initialize all tree-sitter languages
/// This should be called early in main() or test setup
pub fn init_tree_sitter_languages() -> &'static HashMap<Language, TsLanguage> {
    LANGUAGES.get_or_init(|| {
        let mut map = HashMap::new();

        // Core languages
        map.insert(Language::C, tree_sitter_c::LANGUAGE.into());
        map.insert(Language::Cpp, tree_sitter_cpp::LANGUAGE.into());
        map.insert(Language::CSharp, tree_sitter_c_sharp::LANGUAGE.into());
        map.insert(Language::Python, tree_sitter_python::LANGUAGE.into());
        map.insert(Language::Rust, tree_sitter_rust::LANGUAGE.into());
        map.insert(Language::Go, tree_sitter_go::LANGUAGE.into());
        map.insert(Language::Java, tree_sitter_java::LANGUAGE.into());
        map.insert(Language::JavaScript, tree_sitter_javascript::LANGUAGE.into());
        map.insert(Language::TypeScript, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        map.insert(Language::Ruby, tree_sitter_ruby::LANGUAGE.into());
        map.insert(Language::Php, tree_sitter_php::LANGUAGE_PHP.into());
        map.insert(Language::Kotlin, tree_sitter_kotlin_ng::LANGUAGE.into());

        // Frontend languages
        map.insert(Language::Html, tree_sitter_html::LANGUAGE.into());
        map.insert(Language::Css, tree_sitter_css::LANGUAGE.into());
        map.insert(Language::Vue, tree_sitter_vue::language());
        map.insert(Language::Svelte, tree_sitter_svelte::language());
        map.insert(Language::Tsx, tree_sitter_typescript::LANGUAGE_TSX.into());
        map.insert(Language::Jsx, tree_sitter_javascript::LANGUAGE.into());

        map
    })
}

/// Get a tree-sitter language instance
/// Returns None if language is not supported or not initialized
pub fn get_tree_sitter_language(lang: &Language) -> Option<&'static TsLanguage> {
    LANGUAGES.get()?.get(lang)
}
```

**关键点**:
- 使用`HashMap`而不是`DashMap`,因为初始化后只读
- 初始化函数返回静态引用,无需锁保护
- 在程序启动时调用,避免延迟初始化

#### 2. 简化查询缓存

移除嵌套锁,使用简单的DashMap:

```rust
// src/tree_sitter_query/loader.rs
use std::sync::OnceLock;
use dashmap::DashMap;
use std::sync::Arc;
use tree_sitter::Query;

/// Query cache: (Language, QueryType) -> Query
static QUERY_CACHE: OnceLock<DashMap<(Language, QueryType), Arc<Query>>> = OnceLock::new();

impl QueryLoader {
    pub fn get_query(
        &self,
        language: &Language,
        query_type: QueryType,
    ) -> Result<Arc<Query>, QueryError> {
        // Get or initialize cache (no lock held here)
        let cache = QUERY_CACHE.get_or_init(|| DashMap::new());

        // Check cache - read lock only
        let key = (language.clone(), query_type);
        if let Some(query) = cache.get(&key) {
            return Ok(query.clone());
        }

        // Get language - no lock needed (read-only HashMap)
        let ts_lang = get_tree_sitter_language(language)
            .ok_or_else(|| QueryError::UnsupportedLanguage(language.clone()))?;

        // Compile query - no lock held
        let query_string = self.load_query_string(language, query_type)?;
        let query = Arc::new(Query::new(ts_lang, &query_string)?);

        // Insert into cache - write lock, but very brief
        cache.insert(key, query.clone());

        Ok(query)
    }
}
```

**关键点**:
- 语言获取无锁(只读HashMap)
- 查询编译不持有锁
- 缓存插入只短暂持锁
- 无嵌套锁

#### 3. 移除#[ctor::ctor]

移除所有测试中的`#[ctor::ctor]`,改用显式初始化:

```rust
// 旧代码
#[cfg(test)]
mod tests {
    #[ctor::ctor]
    fn init() {
        crate::test_utils::init_test_environment();
    }

    #[test]
    fn test_query() { ... }
}

// 新代码
#[cfg(test)]
mod tests {
    use crate::utils::tree_sitter_init::init_tree_sitter_languages;

    #[test]
    fn test_query() {
        // Ensure languages are initialized
        init_tree_sitter_languages();

        // Test code
        ...
    }
}
```

或者使用测试fixture:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct TestFixture;

    impl TestFixture {
        fn new() -> Self {
            crate::utils::tree_sitter_init::init_tree_sitter_languages();
            Self
        }
    }

    #[test]
    fn test_query() {
        let _fixture = TestFixture::new();
        // Test code
    }
}
```

#### 4. 更新测试工具

```rust
// src/test_utils/mod.rs
use std::sync::Once;
use crate::utils::tree_sitter_init::init_tree_sitter_languages;

static INIT: Once = Once::new();

/// Initialize test environment
/// Call this at the beginning of each test
pub fn init_test_environment() {
    INIT.call_once(|| {
        // Initialize tree-sitter languages
        init_tree_sitter_languages();

        // Pre-warm queries if needed
        // ...

        eprintln!("✓ Test environment initialized");
    });
}
```

## 实现步骤

1. **创建`src/utils/tree_sitter_init.rs`**: 实现语言初始化模块
2. **重构`QueryLoader`**: 移除嵌套锁,使用简化的缓存结构
3. **移除所有`#[ctor::ctor]`**: 在测试中改用显式初始化
4. **更新`AstParser`**: 使用新的语言获取接口
5. **更新测试工具**: 提供简单的初始化函数

## 验证

重构后应满足:
1. 所有单元测试可以正常执行
2. 所有集成测试可以正常执行
3. 无死锁现象
4. 初始化时间合理(语言初始化应在毫秒级完成)

## 性能考虑

- 语言初始化: 一次性成本,在程序启动时完成
- 查询编译: 延迟进行,首次使用时编译并缓存
- 内存占用: 每种语言一个实例,查询按需编译

## 兼容性

- 保持`QueryLoader`的公共API不变
- `AstParser`继续使用相同的接口
- 测试代码需要更新初始化方式
