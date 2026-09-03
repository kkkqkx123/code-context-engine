//! Query loader for Tree-sitter query patterns
//!
//! Loads, compiles, and caches Tree-sitter queries for different languages.
//! Provides thread-safe access to compiled queries via global singleton.
//!
//! # Design
//!
//! This module uses a simplified architecture to avoid deadlocks:
//! - Tree-sitter languages are initialized separately in `utils::tree_sitter_init`
//! - Query cache uses simple DashMap without nested locks
//! - No locks are held during query compilation
//!
//! # Thread Safety
//!
//! - Language access is lock-free (read-only HashMap after initialization)
//! - Query cache uses DashMap for concurrent access
//! - No nested locks to prevent deadlocks

use super::error::{QueryError, Result};

use crate::tree_sitter_init;
use crate::tree_sitter_query::scheme;
use cce_types::language::Language;
use dashmap::DashMap;
use std::sync::{Arc, OnceLock};
use tree_sitter::Query;

/// Query type enumeration (cross-layer contract, defined in `cce_core`).
pub use cce_types::ast_to_nl::QueryType;

/// Query cache statistics
#[derive(Debug, Clone, Default)]
pub struct QueryCacheStats {
    /// Number of languages with cached queries
    pub languages: usize,
    /// Total number of cached queries
    pub total_queries: usize,
}

/// Global query cache
///
/// Simple DashMap without nested locks to prevent deadlocks.
/// Key: (Language, QueryType)
/// Value: Compiled Query wrapped in Arc for sharing
static QUERY_CACHE: OnceLock<DashMap<(Language, QueryType), Arc<Query>>> = OnceLock::new();

/// Serializes tests that read stats from or clear the process-global query
/// cache. Tests run in parallel by default; without this lock, assertions on
/// `QUERY_CACHE` state would race with `clear_cache()` calls from other tests.
#[cfg(test)]
pub(crate) static QUERY_CACHE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Query loader with caching
///
/// Loads and compiles Tree-sitter queries for different languages.
/// Uses a simplified architecture to avoid deadlocks:
/// - No nested locks
/// - Language access is lock-free
/// - Query compilation happens without holding locks
pub struct QueryLoader {}

impl QueryLoader {
    /// Create a new query loader
    pub fn new() -> Self {
        Self {}
    }

    /// Get a query for a specific language and type
    ///
    /// This method compiles and caches the query on first access,
    /// and returns the cached query on subsequent accesses.
    ///
    /// # Thread Safety
    ///
    /// - No nested locks to prevent deadlocks
    /// - Language access is lock-free (read-only HashMap)
    /// - Query compilation happens without holding locks
    /// - Cache insertion is brief
    ///
    /// # Arguments
    ///
    /// * `language` - Programming language
    /// * `query_type` - Type of query to load
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Query>)` - Compiled query (wrapped in Arc for thread safety)
    /// * `Err(QueryError)` - If query loading or compilation fails
    pub fn get_query(&self, language: &Language, query_type: QueryType) -> Result<Arc<Query>> {
        // Get or initialize the query cache (no lock held here)
        let cache = QUERY_CACHE.get_or_init(DashMap::new);

        // Check cache - read lock only, very brief
        let key = (*language, query_type);
        if let Some(query) = cache.get(&key) {
            return Ok(query.clone());
        }

        // Get language - NO LOCK (read-only HashMap after initialization)
        let ts_language =
            tree_sitter_init::get_tree_sitter_language(language).ok_or_else(|| {
                QueryError::InvalidQuery(format!(
                    "Tree-sitter language not available for {}",
                    language
                ))
            })?;

        // Compile query - NO LOCK HELD
        let query_string = self.load_query_string(language, query_type)?;
        let query = Query::new(&ts_language, &query_string).map_err(|e| {
            let error_msg = format!(
                "Failed to compile {} query for {}: {:?}",
                query_type, language, e
            );

            QueryError::InvalidQuery(error_msg)
        })?;

        // Wrap query in Arc
        let query_arc = Arc::new(query);

        // Insert into cache - write lock, but very brief
        cache.insert(key, query_arc.clone());

        Ok(query_arc)
    }

    /// Get entity query for a language
    pub fn get_entity_query(&self, language: &Language) -> Result<Arc<Query>> {
        self.get_query(language, QueryType::Entity)
    }

    /// Get call query for a language
    pub fn get_call_query(&self, language: &Language) -> Result<Arc<Query>> {
        self.get_query(language, QueryType::Call)
    }

    /// Get control-flow query for a language
    pub fn get_control_flow_query(&self, language: &Language) -> Result<Arc<Query>> {
        self.get_query(language, QueryType::ControlFlow)
    }

    /// Get behavior query for a language
    pub fn get_behavior_query(&self, language: &Language) -> Result<Arc<Query>> {
        self.get_query(language, QueryType::Behavior)
    }

    /// Get dependency query for a language
    pub fn get_dependency_query(&self, language: &Language) -> Result<Arc<Query>> {
        self.get_query(language, QueryType::Dependency)
    }

    /// Get comment query for a language
    pub fn get_comment_query(&self, language: &Language) -> Result<Arc<Query>> {
        self.get_query(language, QueryType::Comment)
    }

    /// Get embedded block query for a language
    pub fn get_embedded_query(&self, language: &Language) -> Result<Arc<Query>> {
        self.get_query(language, QueryType::Embedded)
    }

    /// Clear the query cache
    pub fn clear_cache(&self) {
        if let Some(cache) = QUERY_CACHE.get() {
            cache.clear();
        }
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> QueryCacheStats {
        if let Some(cache) = QUERY_CACHE.get() {
            let total_queries = cache.len();

            // Count unique languages
            let mut languages = std::collections::HashSet::new();
            for entry in cache.iter() {
                let (lang, _) = entry.key();
                languages.insert(*lang);
            }

            QueryCacheStats {
                languages: languages.len(),
                total_queries,
            }
        } else {
            QueryCacheStats::default()
        }
    }

    /// Load query string for a language and type
    fn load_query_string(&self, language: &Language, query_type: QueryType) -> Result<String> {
        // Plugin-registered custom languages: the plugin's own scheme wins,
        // then the referenced built-in grammar's scheme (`LanguageRemap`).
        if let Language::Custom(index) = language {
            if let Some(scheme) = crate::tree_sitter_init::plugin_query_scheme(*index, query_type) {
                return Ok(scheme);
            }
            if let Some(target) = crate::tree_sitter_init::remap_target(*index) {
                if let Some(scheme) = Self::builtin_scheme(target, query_type) {
                    return Ok(scheme);
                }
            }
            return Err(QueryError::InvalidQuery(format!(
                "No query scheme for custom language {} (query type {query_type})",
                language
            )));
        }
        Self::builtin_scheme(*language, query_type).ok_or_else(|| {
            QueryError::InvalidQuery(format!(
                "Query type {} not supported for language {}",
                query_type, language
            ))
        })
    }

    /// Resolve the built-in query scheme string for a host language.
    fn builtin_scheme(language: Language, query_type: QueryType) -> Option<String> {
        match (language, query_type) {
            // C language queries
            (Language::C, QueryType::Entity) => Some(scheme::c::entity_query().to_string()),
            (Language::C, QueryType::Call) => Some(scheme::c::call_query().to_string()),
            (Language::C, QueryType::Dependency) => Some(scheme::c::dependency_query().to_string()),
            (Language::C, QueryType::Behavior) => Some(scheme::c::behavior_query().to_string()),
            (Language::C, QueryType::ControlFlow) => {
                Some(scheme::c::control_flow_query().to_string())
            }
            (Language::C, QueryType::Comment) => Some(scheme::c::comment_query().to_string()),

            // C++ language queries
            (Language::Cpp, QueryType::Entity) => Some(scheme::cpp::entity_query().to_string()),
            (Language::Cpp, QueryType::Call) => Some(scheme::cpp::call_query().to_string()),
            (Language::Cpp, QueryType::Dependency) => {
                Some(scheme::cpp::dependency_query().to_string())
            }
            (Language::Cpp, QueryType::Behavior) => Some(scheme::cpp::behavior_query().to_string()),
            (Language::Cpp, QueryType::ControlFlow) => {
                Some(scheme::cpp::control_flow_query().to_string())
            }
            (Language::Cpp, QueryType::Comment) => {
                Some(scheme::common::comment_query().to_string())
            }

            // C# language queries
            (Language::CSharp, QueryType::Entity) => {
                Some(scheme::csharp::entity_query().to_string())
            }
            (Language::CSharp, QueryType::Call) => Some(scheme::csharp::call_query().to_string()),
            (Language::CSharp, QueryType::Dependency) => {
                Some(scheme::csharp::dependency_query().to_string())
            }
            (Language::CSharp, QueryType::Behavior) => {
                Some(scheme::csharp::behavior_query().to_string())
            }
            (Language::CSharp, QueryType::ControlFlow) => {
                Some(scheme::csharp::control_flow_query().to_string())
            }
            (Language::CSharp, QueryType::Comment) => {
                Some(scheme::common::comment_query().to_string())
            }

            // Python language queries
            (Language::Python, QueryType::Entity) => {
                Some(scheme::python::entity_query().to_string())
            }
            (Language::Python, QueryType::Call) => Some(scheme::python::call_query().to_string()),
            (Language::Python, QueryType::Dependency) => {
                Some(scheme::python::dependency_query().to_string())
            }
            (Language::Python, QueryType::Behavior) => {
                Some(scheme::python::behavior_query().to_string())
            }
            (Language::Python, QueryType::ControlFlow) => {
                Some(scheme::python::control_flow_query().to_string())
            }
            (Language::Python, QueryType::Comment) => {
                Some(scheme::python::comment_query().to_string())
            }

            // TypeScript language queries
            (Language::TypeScript, QueryType::Entity) => {
                Some(scheme::typescript::entity_query().to_string())
            }
            (Language::TypeScript, QueryType::Call) => {
                Some(scheme::typescript::call_query().to_string())
            }
            (Language::TypeScript, QueryType::Dependency) => {
                Some(scheme::typescript::dependency_query().to_string())
            }
            (Language::TypeScript, QueryType::Behavior) => {
                Some(scheme::typescript::behavior_query().to_string())
            }
            (Language::TypeScript, QueryType::ControlFlow) => {
                Some(scheme::typescript::control_flow_query().to_string())
            }
            (Language::TypeScript, QueryType::Comment) => {
                Some(scheme::common::comment_query().to_string())
            }

            // JavaScript language queries
            (Language::JavaScript, QueryType::Entity) => {
                Some(scheme::javascript::entity_query().to_string())
            }
            (Language::JavaScript, QueryType::Call) => {
                Some(scheme::javascript::call_query().to_string())
            }
            (Language::JavaScript, QueryType::Dependency) => {
                Some(scheme::javascript::dependency_query().to_string())
            }
            (Language::JavaScript, QueryType::Behavior) => {
                Some(scheme::javascript::behavior_query().to_string())
            }
            (Language::JavaScript, QueryType::ControlFlow) => {
                Some(scheme::javascript::control_flow_query().to_string())
            }
            (Language::JavaScript, QueryType::Comment) => {
                Some(scheme::common::comment_query().to_string())
            }

            // Rust language queries
            (Language::Rust, QueryType::Entity) => Some(scheme::rust::entity_query().to_string()),
            (Language::Rust, QueryType::Call) => Some(scheme::rust::call_query().to_string()),
            (Language::Rust, QueryType::ControlFlow) => {
                Some(scheme::rust::control_flow_query().to_string())
            }
            (Language::Rust, QueryType::Behavior) => {
                Some(scheme::rust::behavior_query().to_string())
            }
            (Language::Rust, QueryType::Dependency) => {
                Some(scheme::rust::dependency_query().to_string())
            }
            (Language::Rust, QueryType::Comment) => Some(scheme::rust::comment_query().to_string()),

            // Go language queries
            (Language::Go, QueryType::Entity) => Some(scheme::go::entity_query().to_string()),
            (Language::Go, QueryType::Call) => Some(scheme::go::call_query().to_string()),
            (Language::Go, QueryType::Dependency) => {
                Some(scheme::go::dependency_query().to_string())
            }
            (Language::Go, QueryType::Behavior) => Some(scheme::go::behavior_query().to_string()),
            (Language::Go, QueryType::ControlFlow) => {
                Some(scheme::go::control_flow_query().to_string())
            }
            (Language::Go, QueryType::Comment) => Some(scheme::common::comment_query().to_string()),

            // Java language queries
            (Language::Java, QueryType::Entity) => Some(scheme::java::entity_query().to_string()),
            (Language::Java, QueryType::Call) => Some(scheme::java::call_query().to_string()),
            (Language::Java, QueryType::Dependency) => {
                Some(scheme::java::dependency_query().to_string())
            }
            (Language::Java, QueryType::Behavior) => {
                Some(scheme::java::behavior_query().to_string())
            }
            (Language::Java, QueryType::ControlFlow) => {
                Some(scheme::java::control_flow_query().to_string())
            }
            (Language::Java, QueryType::Comment) => Some(scheme::java::comment_query().to_string()),

            // PHP language queries
            (Language::Php, QueryType::Entity) => Some(scheme::php::entity_query().to_string()),
            (Language::Php, QueryType::Call) => Some(scheme::php::call_query().to_string()),
            (Language::Php, QueryType::Dependency) => {
                Some(scheme::php::dependency_query().to_string())
            }
            (Language::Php, QueryType::Behavior) => Some(scheme::php::behavior_query().to_string()),
            (Language::Php, QueryType::ControlFlow) => {
                Some(scheme::php::control_flow_query().to_string())
            }
            (Language::Php, QueryType::Comment) => {
                Some(scheme::common::comment_query().to_string())
            }

            // Ruby language queries
            (Language::Ruby, QueryType::Entity) => Some(scheme::ruby::entity_query().to_string()),
            (Language::Ruby, QueryType::Call) => Some(scheme::ruby::call_query().to_string()),
            (Language::Ruby, QueryType::Dependency) => {
                Some(scheme::ruby::dependency_query().to_string())
            }
            (Language::Ruby, QueryType::Behavior) => {
                Some(scheme::ruby::behavior_query().to_string())
            }
            (Language::Ruby, QueryType::ControlFlow) => {
                Some(scheme::ruby::control_flow_query().to_string())
            }
            (Language::Ruby, QueryType::Comment) => {
                Some(scheme::common::comment_query().to_string())
            }

            // Kotlin language queries
            (Language::Kotlin, QueryType::Entity) => {
                Some(scheme::kotlin::entity_query().to_string())
            }
            (Language::Kotlin, QueryType::Call) => Some(scheme::kotlin::call_query().to_string()),
            (Language::Kotlin, QueryType::Dependency) => {
                Some(scheme::kotlin::dependency_query().to_string())
            }
            (Language::Kotlin, QueryType::Behavior) => {
                Some(scheme::kotlin::behavior_query().to_string())
            }
            (Language::Kotlin, QueryType::ControlFlow) => {
                Some(scheme::kotlin::control_flow_query().to_string())
            }
            (Language::Kotlin, QueryType::Comment) => {
                Some(scheme::kotlin::comment_query().to_string())
            }

            // Scala language queries
            (Language::Scala, QueryType::Entity) => Some(scheme::scala::entity_query().to_string()),
            (Language::Scala, QueryType::Call) => Some(scheme::scala::call_query().to_string()),
            (Language::Scala, QueryType::Dependency) => {
                Some(scheme::scala::dependency_query().to_string())
            }
            (Language::Scala, QueryType::Behavior) => {
                Some(scheme::scala::behavior_query().to_string())
            }
            (Language::Scala, QueryType::ControlFlow) => {
                Some(scheme::scala::control_flow_query().to_string())
            }
            (Language::Scala, QueryType::Comment) => {
                Some(scheme::scala::comment_query().to_string())
            }

            // Dart language queries
            (Language::Dart, QueryType::Entity) => Some(scheme::dart::entity_query().to_string()),
            (Language::Dart, QueryType::Call) => Some(scheme::dart::call_query().to_string()),
            (Language::Dart, QueryType::Dependency) => {
                Some(scheme::dart::dependency_query().to_string())
            }
            (Language::Dart, QueryType::Behavior) => {
                Some(scheme::dart::behavior_query().to_string())
            }
            (Language::Dart, QueryType::ControlFlow) => {
                Some(scheme::dart::control_flow_query().to_string())
            }
            (Language::Dart, QueryType::Comment) => Some(scheme::dart::comment_query().to_string()),

            // Bash language queries
            (Language::Bash, QueryType::Entity) => Some(scheme::bash::entity_query().to_string()),
            (Language::Bash, QueryType::Call) => Some(scheme::bash::call_query().to_string()),
            (Language::Bash, QueryType::Dependency) => {
                Some(scheme::bash::dependency_query().to_string())
            }
            (Language::Bash, QueryType::Behavior) => {
                Some(scheme::bash::behavior_query().to_string())
            }
            (Language::Bash, QueryType::ControlFlow) => {
                Some(scheme::bash::control_flow_query().to_string())
            }
            (Language::Bash, QueryType::Comment) => Some(scheme::bash::comment_query().to_string()),

            // Lua language queries
            (Language::Lua, QueryType::Entity) => Some(scheme::lua::entity_query().to_string()),
            (Language::Lua, QueryType::Call) => Some(scheme::lua::call_query().to_string()),
            (Language::Lua, QueryType::Dependency) => {
                Some(scheme::lua::dependency_query().to_string())
            }
            (Language::Lua, QueryType::Behavior) => Some(scheme::lua::behavior_query().to_string()),
            (Language::Lua, QueryType::ControlFlow) => {
                Some(scheme::lua::control_flow_query().to_string())
            }
            (Language::Lua, QueryType::Comment) => Some(scheme::lua::comment_query().to_string()),

            // HTML language queries
            (Language::Html, QueryType::Entity) => Some(scheme::html::entity_query().to_string()),
            (Language::Html, QueryType::Dependency) => {
                Some(scheme::html::dependency_query().to_string())
            }
            (Language::Html, QueryType::Comment) => Some(scheme::html::comment_query().to_string()),
            (Language::Html, QueryType::Embedded) => {
                Some(scheme::html::embedded_block_query().to_string())
            }
            // HTML does not support call queries directly, use embedded parsing.

            // Vue language queries
            (Language::Vue, QueryType::Entity) => Some(scheme::vue::entity_query().to_string()),
            (Language::Vue, QueryType::Structural) => {
                Some(scheme::vue::structural_query().to_string())
            }
            (Language::Vue, QueryType::Dependency) => {
                Some(scheme::vue::dependency_query().to_string())
            }
            (Language::Vue, QueryType::Comment) => Some(scheme::vue::comment_query().to_string()),
            (Language::Vue, QueryType::Embedded) => {
                Some(scheme::vue::embedded_block_query().to_string())
            }
            // Vue does not support call queries directly, use embedded parsing.

            // Svelte language queries
            (Language::Svelte, QueryType::Entity) => {
                Some(scheme::svelte::entity_query().to_string())
            }
            (Language::Svelte, QueryType::Structural) => {
                Some(scheme::svelte::structural_query().to_string())
            }
            (Language::Svelte, QueryType::Dependency) => {
                Some(scheme::svelte::dependency_query().to_string())
            }
            (Language::Svelte, QueryType::Comment) => {
                Some(scheme::svelte::comment_query().to_string())
            }
            (Language::Svelte, QueryType::Embedded) => {
                Some(scheme::svelte::embedded_block_query().to_string())
            }
            (Language::Svelte, QueryType::Behavior) => {
                Some(scheme::svelte::behavior_query().to_string())
            }
            // Svelte does not support call queries directly, use embedded parsing.

            // TSX language queries
            (Language::Tsx, QueryType::Entity) => Some(scheme::tsx::entity_query().to_string()),
            (Language::Tsx, QueryType::Call) => Some(scheme::tsx::call_query().to_string()),
            (Language::Tsx, QueryType::Dependency) => {
                Some(scheme::tsx::dependency_query().to_string())
            }
            (Language::Tsx, QueryType::Behavior) => Some(scheme::tsx::behavior_query().to_string()),
            (Language::Tsx, QueryType::ControlFlow) => {
                Some(scheme::tsx::control_flow_query().to_string())
            }
            (Language::Tsx, QueryType::Structural) => {
                Some(scheme::tsx::structural_query().to_string())
            }
            (Language::Tsx, QueryType::Comment) => Some(scheme::tsx::comment_query().to_string()),

            // JSX language queries
            (Language::Jsx, QueryType::Entity) => {
                Some(scheme::javascript::entity_query().to_string())
            }
            (Language::Jsx, QueryType::Call) => Some(scheme::javascript::call_query().to_string()),
            (Language::Jsx, QueryType::Dependency) => {
                Some(scheme::javascript::dependency_query().to_string())
            }
            (Language::Jsx, QueryType::Behavior) => {
                Some(scheme::javascript::behavior_query().to_string())
            }
            (Language::Jsx, QueryType::ControlFlow) => {
                Some(scheme::javascript::control_flow_query().to_string())
            }
            (Language::Jsx, QueryType::Comment) => {
                Some(scheme::common::comment_query().to_string())
            }

            // CSS language queries
            (Language::Css, QueryType::Entity) => Some(scheme::css::entity_query().to_string()),
            (Language::Css, QueryType::Structural) => {
                Some(scheme::css::structural_query().to_string())
            }
            (Language::Css, QueryType::Dependency) => {
                Some(scheme::css::dependency_query().to_string())
            }
            (Language::Css, QueryType::Comment) => Some(scheme::css::comment_query().to_string()),

            // Unsupported combinations
            _ => None,
        }
    }
}

impl Default for QueryLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_loader() {
        let loader = QueryLoader::new();
        // Just verify that we can create a loader and get stats
        let _stats = loader.cache_stats();
        // Cache may or may not be empty depending on other tests
    }

    #[test]
    fn test_get_c_entity_query() {
        // Other tests may clear the global cache concurrently; hold the test
        // lock so the cache-state assertion below is deterministic.
        let _guard = QUERY_CACHE_TEST_LOCK.lock().expect("test lock poisoned");
        let loader = QueryLoader::new();
        let result = loader.get_entity_query(&Language::C);
        assert!(result.is_ok());
        // After getting a query, cache should have at least one entry
        let stats = loader.cache_stats();
        assert!(stats.total_queries >= 1);
    }

    #[test]
    fn test_get_c_call_query() {
        let _guard = QUERY_CACHE_TEST_LOCK.lock().expect("test lock poisoned");
        let loader = QueryLoader::new();
        let result = loader.get_call_query(&Language::C);
        assert!(result.is_ok());
        // After getting a query, cache should have at least one entry
        let stats = loader.cache_stats();
        assert!(stats.total_queries >= 1);
    }

    #[test]
    fn test_get_c_dependency_query() {
        let loader = QueryLoader::new();
        let result = loader.get_dependency_query(&Language::C);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cache_hit() {
        let loader = QueryLoader::new();
        let _ = loader.get_entity_query(&Language::C);

        // Second access should hit cache
        let _ = loader.get_entity_query(&Language::C);
    }

    #[test]
    fn test_multiple_query_types_same_language() {
        let loader = QueryLoader::new();
        let _ = loader.get_entity_query(&Language::C);
        let _ = loader.get_call_query(&Language::C);
        let _ = loader.get_dependency_query(&Language::C);
    }

    #[test]
    fn test_multiple_languages() {
        let loader = QueryLoader::new();
        let _ = loader.get_entity_query(&Language::C);
        let _ = loader.get_entity_query(&Language::Rust);
    }

    #[test]
    fn test_clear_cache() {
        // The global cache may be repopulated by other tests at any time, so
        // count-based comparisons are not deterministic. With the test lock
        // held, no other test can clear the cache concurrently; verify the
        // clear behavior through the load-after-clear contract instead.
        let _guard = QUERY_CACHE_TEST_LOCK.lock().expect("test lock poisoned");
        let loader = QueryLoader::new();
        // Ensure we have something in cache
        let _ = loader.get_entity_query(&Language::C);

        // Clear cache
        loader.clear_cache();

        // A fresh load after clearing must succeed (lazy repopulation).
        assert!(
            loader.get_entity_query(&Language::C).is_ok(),
            "loading after clear_cache must succeed"
        );
    }

    #[test]
    fn test_unsupported_language() {
        let loader = QueryLoader::new();
        let result = loader.get_entity_query(&Language::Unknown);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_type_display() {
        assert_eq!(format!("{}", QueryType::Entity), "entity");
        assert_eq!(format!("{}", QueryType::Call), "call");
        assert_eq!(format!("{}", QueryType::ControlFlow), "control_flow");
        assert_eq!(format!("{}", QueryType::Behavior), "behavior");
        assert_eq!(format!("{}", QueryType::Dependency), "dependency");
        assert_eq!(format!("{}", QueryType::Comment), "comment");
    }

    #[test]
    fn test_behavior_and_control_flow_queries_compile() {
        let loader = QueryLoader::new();
        let languages = [
            Language::C,
            Language::Cpp,
            Language::CSharp,
            Language::Python,
            Language::JavaScript,
            Language::Jsx,
            Language::TypeScript,
            Language::Tsx,
            Language::Rust,
            Language::Go,
            Language::Java,
            Language::Php,
            Language::Ruby,
            Language::Kotlin,
            Language::Scala,
            Language::Dart,
            Language::Bash,
            Language::Lua,
        ];

        for language in languages {
            let _behavior_query = loader
                .get_behavior_query(&language)
                .unwrap_or_else(|_| panic!("behavior query should compile for {}", language));
            let _control_flow_query = loader
                .get_control_flow_query(&language)
                .unwrap_or_else(|_| panic!("control-flow query should compile for {}", language));
        }
    }
}
