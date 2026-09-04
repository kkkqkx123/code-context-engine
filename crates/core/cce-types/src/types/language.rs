//! Language types - fundamental language definitions (routing layer)
//!
//! This module provides the most basic language-related types.
//! It has no dependencies on other modules and can be used anywhere.
//!
//! # Classification layering
//!
//! The project maintains three orthogonal classification systems:
//! - [`FileType`] (this module, routing layer): decides the processing
//!   pipeline (`Ast` vs `Document`). Its single source of truth is
//!   [`LanguageInfo::detect_from_path`] → [`builtin_language_for_extension`].
//! - [`crate::types::FileCategory`] (business layer): summary/importance
//!   categories. It delegates to `FileType` via
//!   [`crate::types::FileCategory::from_file_type`] to stay consistent.
//! - [`crate::types::ast_to_nl::ChunkContentType`] (chunk-payload layer):
//!   serialization shape of one chunk's payload.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Language type enumeration
///
/// Supported languages for AST parsing and code analysis.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Default,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub enum Language {
    // C family
    /// C language
    C,
    /// C++ language
    Cpp,
    /// C# language
    CSharp,

    // Web languages
    /// JavaScript language
    JavaScript,
    /// TypeScript language
    TypeScript,

    // Frontend languages
    /// HTML language
    Html,
    /// CSS language
    Css,
    /// SCSS/SASS language
    Scss,
    /// LESS language
    Less,
    /// Vue Single File Component
    Vue,
    /// Svelte Component
    Svelte,
    /// JSX (JavaScript XML)
    Jsx,
    /// TSX (TypeScript XML)
    Tsx,

    // Systems languages
    /// Rust language
    Rust,
    /// Go language
    Go,

    // JVM languages
    /// Java language
    Java,
    /// Kotlin language
    Kotlin,
    /// Scala language
    Scala,

    // Scripting languages
    /// Python language
    Python,
    /// Ruby language
    Ruby,
    /// PHP language
    Php,
    /// Dart language
    Dart,

    // Scripting languages (shell)
    /// Bash/Shell language
    Bash,
    /// Lua language
    Lua,

    // Other
    /// Unknown or unsupported language
    #[default]
    Unknown,

    // Data formats
    /// JSON data format
    Json,
    /// YAML data format
    Yaml,
    /// TOML data format
    Toml,
    /// XML data format
    Xml,

    // Plugin-registered custom language (Native `AstLanguage` plugins)
    /// A language provided by an `AstLanguage` plugin. The payload is an
    /// index into the process-global plugin-language table (see
    /// [`register_plugin_language`]); `Copy` is preserved so the rest of the
    /// pipeline treats `Language` as a cheap value type.
    Custom(u32),
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::C => write!(f, "C"),
            Language::Cpp => write!(f, "C++"),
            Language::CSharp => write!(f, "C#"),
            Language::JavaScript => write!(f, "JavaScript"),
            Language::TypeScript => write!(f, "TypeScript"),
            Language::Html => write!(f, "HTML"),
            Language::Css => write!(f, "CSS"),
            Language::Scss => write!(f, "SCSS"),
            Language::Less => write!(f, "LESS"),
            Language::Vue => write!(f, "Vue"),
            Language::Svelte => write!(f, "Svelte"),
            Language::Jsx => write!(f, "JSX"),
            Language::Tsx => write!(f, "TSX"),
            Language::Rust => write!(f, "Rust"),
            Language::Go => write!(f, "Go"),
            Language::Java => write!(f, "Java"),
            Language::Kotlin => write!(f, "Kotlin"),
            Language::Scala => write!(f, "Scala"),
            Language::Python => write!(f, "Python"),
            Language::Ruby => write!(f, "Ruby"),
            Language::Php => write!(f, "PHP"),
            Language::Dart => write!(f, "Dart"),
            Language::Bash => write!(f, "Bash"),
            Language::Lua => write!(f, "Lua"),
            Language::Unknown => write!(f, "Unknown"),
            Language::Json => write!(f, "JSON"),
            Language::Yaml => write!(f, "YAML"),
            Language::Toml => write!(f, "TOML"),
            Language::Xml => write!(f, "XML"),
            Language::Custom(index) => {
                if let Some(name) = plugin_language_name(*index) {
                    write!(f, "{name}")
                } else {
                    write!(f, "custom_{index}")
                }
            }
        }
    }
}

impl Language {
    /// Check if this language is supported for AST parsing
    pub fn is_supported_for_ast(&self) -> bool {
        matches!(
            self,
            Language::C
                | Language::Cpp
                | Language::CSharp
                | Language::JavaScript
                | Language::TypeScript
                | Language::Html
                | Language::Css
                | Language::Vue
                | Language::Svelte
                | Language::Jsx
                | Language::Tsx
                | Language::Rust
                | Language::Go
                | Language::Java
                | Language::Kotlin
                | Language::Scala
                | Language::Python
                | Language::Php
                | Language::Ruby
                | Language::Dart
                | Language::Bash
                | Language::Lua
                | Language::Custom(_)
        )
    }

    /// Check if this is an SFC (Single File Component) language
    pub fn is_sfc(&self) -> bool {
        matches!(self, Language::Vue | Language::Svelte)
    }

    /// Check if this language can contain embedded code blocks
    /// (HTML can contain <script> and <style> blocks)
    pub fn has_embedded_blocks(&self) -> bool {
        matches!(self, Language::Vue | Language::Svelte | Language::Html)
    }

    /// Get file extensions commonly associated with this language
    pub fn common_extensions(&self) -> &'static [&'static str] {
        match self {
            Language::C => &["c", "h"],
            Language::Cpp => &["cpp", "cc", "cxx", "hpp", "hxx"],
            Language::CSharp => &["cs"],
            Language::JavaScript => &["js", "mjs", "cjs"],
            Language::TypeScript => &["ts", "mts", "cts"],
            Language::Html => &["html", "htm"],
            Language::Css => &["css"],
            Language::Scss => &["scss", "sass"],
            Language::Less => &["less"],
            Language::Vue => &["vue"],
            Language::Svelte => &["svelte"],
            Language::Jsx => &["jsx"],
            Language::Tsx => &["tsx"],
            Language::Rust => &["rs"],
            Language::Go => &["go"],
            Language::Java => &["java"],
            Language::Kotlin => &["kt", "kts"],
            Language::Scala => &["scala", "sc"],
            Language::Python => &["py", "pyi"],
            Language::Ruby => &["rb"],
            Language::Php => &["php"],
            Language::Dart => &["dart"],
            Language::Bash => &["sh", "bash", "bats"],
            Language::Lua => &["lua"],
            Language::Unknown => &[],
            Language::Json => &["json"],
            Language::Yaml => &["yaml", "yml"],
            Language::Toml => &["toml"],
            Language::Xml => &["xml"],
            // Custom-language extensions live in the plugin-language table
            // (see `plugin_language_extensions`); the static list is empty.
            Language::Custom(_) => &[],
        }
    }

    /// Get file extensions for this language (alias for common_extensions)
    pub fn extensions(&self) -> &'static [&'static str] {
        self.common_extensions()
    }

    /// Resolve a host built-in language from a common name
    /// (case-insensitive; accepts aliases like "c++"/"cpp", "c#"/"csharp",
    /// "js"/"javascript"). Used to resolve `LanguageRemap` grammar
    /// references. Returns `None` for unknown names and for
    /// [`Language::Custom`] (a remap target must be a built-in).
    pub fn from_name(name: &str) -> Option<Language> {
        let n = name.trim().to_lowercase();
        Some(match n.as_str() {
            "c" => Language::C,
            "c++" | "cpp" | "cc" => Language::Cpp,
            "c#" | "csharp" | "cs" => Language::CSharp,
            "javascript" | "js" | "mjs" | "cjs" => Language::JavaScript,
            "typescript" | "ts" | "mts" | "cts" => Language::TypeScript,
            "html" | "htm" => Language::Html,
            "css" => Language::Css,
            "scss" | "sass" => Language::Scss,
            "less" => Language::Less,
            "vue" => Language::Vue,
            "svelte" => Language::Svelte,
            "jsx" => Language::Jsx,
            "tsx" => Language::Tsx,
            "rust" | "rs" => Language::Rust,
            "go" | "golang" => Language::Go,
            "java" => Language::Java,
            "kotlin" | "kt" => Language::Kotlin,
            "scala" => Language::Scala,
            "python" | "py" => Language::Python,
            "ruby" | "rb" => Language::Ruby,
            "php" => Language::Php,
            "dart" => Language::Dart,
            "bash" | "sh" | "shell" => Language::Bash,
            "lua" => Language::Lua,
            "json" => Language::Json,
            "yaml" | "yml" => Language::Yaml,
            "toml" => Language::Toml,
            "xml" => Language::Xml,
            _ => return None,
        })
    }
}

/// File type enumeration (routing layer)
///
/// Decides the processing pipeline. The single source of truth for the
/// mapping is [`LanguageInfo::detect_from_path`] → [`builtin_language_for_extension`].
/// Business-layer consumers must not re-derive the category from the path;
/// they should convert via [`crate::types::FileCategory::from_file_type`] to
/// keep the three classification systems consistent.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    SerdeSerialize,
    SerdeDeserialize,
    Default,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub enum FileType {
    /// Source file (main implementation files)
    Source,
    /// Header file (.h, .hpp, etc.)
    ///
    /// Test and build classification is handled by the orthogonal
    /// `TestInfo` / `FileCategory` path rules instead of a `FileType`
    /// variant, so every variant here has a deterministic routing target.
    Header,
    /// Configuration file (.toml, .yaml, .json, etc.)
    Config,
    /// Documentation file (README, CHANGELOG, etc.)
    Documentation,
    /// Schema definition file (.proto, .graphql, .thrift, .avsc).
    ///
    /// Routed through the document pipeline (no tree-sitter grammar) while
    /// keeping the dedicated [`crate::types::FileCategory::Schema`] business
    /// category.
    Schema,
    /// Text file (generic text content: logs, `.txt`, unknown extensions)
    #[default]
    Text,
}

/// Language information
#[derive(Debug, Clone, SerdeSerialize, SerdeDeserialize, Archive, RkyvDeserialize, Serialize)]
pub struct LanguageInfo {
    /// Language type
    pub language: Language,
    /// File type
    pub file_type: FileType,
    /// File extensions
    pub extensions: Vec<String>,
}

impl Default for LanguageInfo {
    fn default() -> Self {
        Self {
            language: Language::Unknown,
            file_type: FileType::Text,
            extensions: vec![],
        }
    }
}

impl LanguageInfo {
    /// Detect language from file path using static matching
    ///
    /// This is a static function that can be called from any module without
    /// requiring a LanguageDetector instance. It's useful for early detection
    /// in the scanner to avoid repeated detection downstream.
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (absolute or relative)
    ///
    /// # Returns
    /// * `LanguageInfo` - Detected language information
    pub fn detect_from_path(file_path: &str) -> Self {
        let path = std::path::Path::new(file_path);
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

        // Filename check for well-known build-config names. Must run before
        // extension-based lookup so `Makefile`/`Dockerfile` (no extension) and
        // `CMakeLists.txt` (extension `txt` would otherwise map to `Text`) are
        // correctly classified as `Config`. Kept in sync with
        // `FileCategory::is_config_file` and the canonical
        // `BUILD_CONFIG_FILE_NAMES` list.
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            let lower = file_name.to_lowercase();

            // Extensionless well-known documentation names (README, LICENSE, ...)
            if extension.is_empty() && crate::path::is_extensionless_doc_name(&lower) {
                return Self {
                    language: Language::Unknown,
                    file_type: FileType::Documentation,
                    extensions: Vec::new(),
                };
            }

            // Makefile / Dockerfile variants are build configs, not plain text.
            // Handles `Makefile`, `GNUMakefile`, `Dockerfile`, `Dockerfile.*`,
            // `Makefile.*` case-insensitively.
            if lower == "makefile"
                || lower == "gnumakefile"
                || lower == "dockerfile"
                || lower.starts_with("dockerfile.")
                || lower.starts_with("makefile.")
            {
                return Self {
                    language: Language::Unknown,
                    file_type: FileType::Config,
                    extensions: Vec::new(),
                };
            }

            // Canonical build-config file names (Cargo.toml, CMakeLists.txt, etc.)
            // This makes `CMakeLists.txt` route to `Config` instead of `Text`
            // (its `.txt` extension) and stays in sync with
            // `utils::path::is_build_config_name_lower`.
            if crate::path::is_build_config_name_lower(&lower) {
                // Preserve a structured data-format language (JSON/YAML/TOML/
                // XML) so the document pipeline can select the matching
                // config sub-pipeline (`Cargo.toml` stays TOML, `package.json`
                // stays JSON). Any other extension keeps the generic
                // `Unknown` language while the routing type is forced to
                // `Config`.
                let language = match Self::detect_from_extension(extension).0 {
                    lang @ (Language::Json | Language::Yaml | Language::Toml | Language::Xml) => {
                        lang
                    }
                    _ => Language::Unknown,
                };
                return Self {
                    language,
                    file_type: FileType::Config,
                    extensions: vec![extension.to_string()],
                };
            }
        }

        let (language, file_type) = Self::detect_from_extension(extension);

        Self {
            language,
            file_type,
            extensions: vec![extension.to_string()],
        }
    }

    /// Whether this file belongs to the non-AST document flow
    /// (documentation, config, schema, text).
    ///
    /// Such files carry no tree-sitter semantics and must be routed through
    /// the document pipeline instead of the AST pipeline.
    pub fn is_document_like(&self) -> bool {
        matches!(
            self.file_type,
            FileType::Documentation | FileType::Config | FileType::Schema | FileType::Text
        )
    }

    /// Business-layer category for this file.
    ///
    /// Delegates to [`crate::types::FileCategory::from_file_type`] so routing
    /// and business decisions share the same source of truth.
    pub fn file_category(&self) -> crate::types::FileCategory {
        crate::types::FileCategory::from_file_type(self.file_type.clone())
    }

    /// Chunk-payload content type for this file.
    ///
    /// Unified classification entry alongside [`Self::file_category`]: both
    /// derive from `file_type`, so routing, business and chunk-layer labels
    /// always agree. Schema files reuse the `Document` payload (matching
    /// `ChunkMetadata::for_schema`) while keeping their dedicated category.
    /// The `Config` format label is derived from the detected extension;
    /// paths without an extension should use [`Self::chunk_content_type_for_path`]
    /// so extensionless build files get a precise format.
    pub fn chunk_content_type(&self) -> crate::types::ast_to_nl::ChunkContentType {
        use crate::types::ast_to_nl::ChunkContentType;
        match self.file_type {
            FileType::Source | FileType::Header => ChunkContentType::Code {
                language: self.language,
            },
            FileType::Config => ChunkContentType::Config {
                format: self.payload_format(),
            },
            FileType::Documentation | FileType::Schema => ChunkContentType::Document,
            FileType::Text => ChunkContentType::PlainText,
        }
    }

    /// Same as [`Self::chunk_content_type`] but derives the `Config` format
    /// label from the full path, so extensionless build files (`Makefile`,
    /// `Dockerfile`) receive fixed non-empty formats.
    pub fn chunk_content_type_for_path(
        &self,
        file_path: &str,
    ) -> crate::types::ast_to_nl::ChunkContentType {
        use crate::types::ast_to_nl::ChunkContentType;
        match self.file_type {
            FileType::Config => ChunkContentType::Config {
                format: Self::payload_format_from_path(file_path),
            },
            _ => self.chunk_content_type(),
        }
    }

    /// Non-empty config payload format derived from the detected extensions.
    ///
    /// Files with an extension use the lowercased extension; anything else
    /// reports `other`. Prefer [`Self::payload_format_from_path`] when the
    /// full path is available so extensionless build files keep precise
    /// labels. The empty string is never produced (debug-asserted).
    pub fn payload_format(&self) -> String {
        let format = self
            .extensions
            .first()
            .filter(|ext| !ext.is_empty())
            .cloned()
            .unwrap_or_else(|| "other".to_string());
        debug_assert!(
            !format.is_empty(),
            "config payload format must not be empty"
        );
        format
    }

    /// Non-empty config payload format derived from a file path.
    ///
    /// Rule: lowercased extension when present; extensionless well-known
    /// build files get fixed names (`Makefile`/`GNUmakefile` → `make`,
    /// `Dockerfile` → `docker`); everything else reports `other`. The empty
    /// string is never produced (debug-asserted).
    pub fn payload_format_from_path(file_path: &str) -> String {
        if let Some(ext) = crate::path::extension_lower(file_path) {
            return ext;
        }
        let name = crate::path::file_name_str(file_path).to_lowercase();
        let format = if name == "makefile" || name == "gnumakefile" || name.starts_with("makefile.")
        {
            "make"
        } else if name == "dockerfile" || name.starts_with("dockerfile.") {
            "docker"
        } else {
            "other"
        };
        debug_assert!(
            !format.is_empty(),
            "config payload format must not be empty"
        );
        format.to_string()
    }

    /// Static extension detection using match
    fn detect_from_extension(ext: &str) -> (Language, FileType) {
        // Plugin-registered custom languages win over the static table.
        if let Some(index) = plugin_language_for_extension(ext) {
            return (Language::Custom(index), FileType::Source);
        }
        if let Some((language, file_type)) = builtin_language_for_extension(ext) {
            return (language, file_type);
        }
        (Language::Unknown, FileType::Text)
    }
}

/// How a file's content is routed through processing pipelines.
///
/// Decided once at parse time and carried explicitly on parse results so
/// downstream stages never have to re-infer it from the file path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, SerdeSerialize, SerdeDeserialize)]
pub enum ContentRoute {
    /// Tree-sitter AST pipeline (source/header code files).
    #[default]
    Ast,
    /// Documentation files (`.md`, `.txt`, `.rst`, etc.)
    Documentation,
    /// Configuration files (`.toml`, `.yaml`, `.json`, `Makefile`, etc.)
    Config,
    /// Schema definition files (`.proto`, `.graphql`, `.thrift`, `.avsc`);
    /// document-pipeline processing with the dedicated `Schema` category.
    Schema,
    /// Generic plain-text files (logs, unknown text)
    PlainText,
}

impl ContentRoute {
    /// Derive the route from detected language information.
    pub fn from_language_info(info: &LanguageInfo) -> Self {
        match info.file_type {
            FileType::Source | FileType::Header => Self::Ast,
            FileType::Documentation => Self::Documentation,
            FileType::Config => Self::Config,
            FileType::Schema => Self::Schema,
            FileType::Text => Self::PlainText,
        }
    }

    /// Derive the route from a file path.
    ///
    /// Recovery and sweep boundaries that only hold a path (no parse result)
    /// use this to reconstruct the route with the same predicate the parse
    /// stage used.
    pub fn detect_from_path(file_path: &str) -> Self {
        Self::from_language_info(&LanguageInfo::detect_from_path(file_path))
    }

    /// Whether this route is the non-AST document pipeline.
    pub fn is_document(&self) -> bool {
        !matches!(self, Self::Ast)
    }
}

/// Static extension → language/type table for built-in languages.
///
/// Extracted from [`LanguageInfo::detect_from_extension`] so the extension
/// conflict gate (plugin claims a built-in extension) can reuse the same
/// table.
pub fn builtin_language_for_extension(ext: &str) -> Option<(Language, FileType)> {
    match ext {
        // C family
        "c" => Some((Language::C, FileType::Source)),
        "h" => Some((Language::C, FileType::Header)),
        "cpp" | "cc" | "cxx" => Some((Language::Cpp, FileType::Source)),
        "hpp" | "hxx" => Some((Language::Cpp, FileType::Header)),
        "cs" => Some((Language::CSharp, FileType::Source)),

        // Web languages
        "js" | "mjs" | "cjs" => Some((Language::JavaScript, FileType::Source)),
        "jsx" => Some((Language::Jsx, FileType::Source)),
        "ts" | "mts" | "cts" => Some((Language::TypeScript, FileType::Source)),
        "tsx" => Some((Language::Tsx, FileType::Source)),

        // Frontend files - treat as source for embedded code parsing
        // HTML files contain embedded <script> and <style> blocks that need deep parsing
        "html" | "htm" => Some((Language::Html, FileType::Source)),
        "vue" => Some((Language::Vue, FileType::Source)),
        "svelte" => Some((Language::Svelte, FileType::Source)),

        // Style files
        "css" => Some((Language::Css, FileType::Source)),
        // SCSS/Less have no tree-sitter grammar in this project, so they
        // ride the document pipeline as plain text instead of failing the
        // AST pipeline. The language tag is kept for metadata.
        "scss" | "sass" => Some((Language::Scss, FileType::Text)),
        "less" => Some((Language::Less, FileType::Text)),

        // Systems languages
        "rs" => Some((Language::Rust, FileType::Source)),
        "go" => Some((Language::Go, FileType::Source)),

        // JVM languages
        "java" => Some((Language::Java, FileType::Source)),
        "kt" | "kts" => Some((Language::Kotlin, FileType::Source)),
        "scala" => Some((Language::Scala, FileType::Source)),
        "sc" => Some((Language::Scala, FileType::Source)),

        // Scripting languages
        "py" => Some((Language::Python, FileType::Source)),
        "pyi" => Some((Language::Python, FileType::Header)),
        "rb" => Some((Language::Ruby, FileType::Source)),
        "php" => Some((Language::Php, FileType::Source)),
        "dart" => Some((Language::Dart, FileType::Source)),

        // Shell/Scripting languages
        "sh" | "bash" => Some((Language::Bash, FileType::Source)),
        // bats (Bash Automated Testing System) files are bash source code
        // (test classification is handled by the TestInfo path rule)
        "bats" => Some((Language::Bash, FileType::Source)),
        "lua" => Some((Language::Lua, FileType::Source)),

        // Documentation files
        "md" | "markdown" => Some((Language::Unknown, FileType::Documentation)),
        "rst" | "adoc" => Some((Language::Unknown, FileType::Documentation)),

        // Schema definition files (document pipeline; dedicated category)
        "proto" | "graphql" | "thrift" | "avsc" => Some((Language::Unknown, FileType::Schema)),

        // Text files
        "txt" | "log" => Some((Language::Unknown, FileType::Text)),

        // Configuration files. Data-format languages are preserved so the
        // document pipeline can select the matching config sub-pipeline.
        "toml" => Some((Language::Toml, FileType::Config)),
        "yaml" | "yml" => Some((Language::Yaml, FileType::Config)),
        "json" => Some((Language::Json, FileType::Config)),
        "ini" => Some((Language::Unknown, FileType::Config)),
        // XML is a structured config/data format handled by the document
        // pipeline's dedicated XML sub-pipeline (no tree-sitter AST).
        "xml" => Some((Language::Xml, FileType::Config)),

        // Build-config filename extensions (e.g. `*.mk`, `*.dockerfile`)
        // Real `Makefile`/`Dockerfile` without extension are handled in
        // `detect_from_path`; this covers the suffixed forms.
        "mk" | "makefile" => Some((Language::Unknown, FileType::Config)),
        "dockerfile" => Some((Language::Unknown, FileType::Config)),

        // Unknown
        _ => None,
    }
}

// ── Plugin-registered custom languages ─────────────────────────────────
//
// `AstLanguage` plugins register their language name + extensions here so
// `detect_from_extension` can route files to `Language::Custom(index)`. The
// table is global and process-lifetime (plugins are loaded once at startup).
// cce_core stores only the name↔extension mapping; the tree-sitter grammar
// pointer and query schemes live in the parser crate's plugin table.

use std::sync::{Mutex, OnceLock};

/// A registered plugin language.
#[derive(Debug, Clone)]
pub struct PluginLanguageRecord {
    /// Language name (e.g. "zig").
    pub name: String,
    /// File extensions (lowercased, without the leading dot).
    pub extensions: Vec<String>,
    /// Grammar source: `Some(builtin_language_name)` for a `LanguageRemap`
    /// plugin that reuses a host built-in grammar, `None` for a native
    /// `AstLanguage` plugin (grammar pointer lives in the parser table).
    pub remap_grammar: Option<String>,
}

/// Process-global plugin-language records; index = `Language::Custom(idx)`.
static PLUGIN_LANGUAGE_TABLE: OnceLock<Mutex<Vec<PluginLanguageRecord>>> = OnceLock::new();

fn plugin_language_table() -> &'static Mutex<Vec<PluginLanguageRecord>> {
    PLUGIN_LANGUAGE_TABLE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register a custom language and its file extensions.
///
/// Returns the stable index used by `Language::Custom(index)`. Re-registering
/// the same language name updates its records and returns the original index.
pub fn register_plugin_language(name: &str, extensions: &[String]) -> u32 {
    register_plugin_language_with_source(name, extensions, None)
}

/// Register a custom language backed by a host built-in grammar
/// (`LanguageRemap` plugins).
///
/// Same registration semantics as [`register_plugin_language`]; the grammar
/// source is recorded for diagnostics and for the parser's grammar lookup.
pub fn register_remap_plugin_language(name: &str, extensions: &[String], builtin: &str) -> u32 {
    register_plugin_language_with_source(name, extensions, Some(builtin.to_string()))
}

fn register_plugin_language_with_source(
    name: &str,
    extensions: &[String],
    remap_grammar: Option<String>,
) -> u32 {
    let mut guard = plugin_language_table()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = guard.iter().position(|r| r.name.eq_ignore_ascii_case(name)) {
        guard[existing].extensions = extensions.to_vec();
        guard[existing].remap_grammar = remap_grammar;
        return existing as u32;
    }
    guard.push(PluginLanguageRecord {
        name: name.to_string(),
        extensions: extensions.iter().map(|e| e.to_lowercase()).collect(),
        remap_grammar,
    });
    (guard.len() - 1) as u32
}

/// Clear all registered plugin languages (used in tests / teardown).
pub fn clear_plugin_languages() {
    if let Ok(mut guard) = plugin_language_table().lock() {
        guard.clear();
    }
}

/// Look up the plugin language index registered for `ext` (lowercased).
pub fn plugin_language_for_extension(ext: &str) -> Option<u32> {
    let guard = plugin_language_table().lock().ok()?;
    let ext = ext.to_lowercase();
    guard
        .iter()
        .position(|r| r.extensions.contains(&ext))
        .map(|idx| idx as u32)
}

/// Look up a registered plugin language name by index.
pub fn plugin_language_name(index: u32) -> Option<String> {
    let guard = plugin_language_table().lock().ok()?;
    guard.get(index as usize).map(|r| r.name.clone())
}

/// Look up a registered plugin language's extensions by index.
pub fn plugin_language_extensions(index: u32) -> Vec<String> {
    plugin_language_table()
        .lock()
        .map(|guard| {
            guard
                .get(index as usize)
                .map(|r| r.extensions.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Extensions in `extensions` that are already owned by a built-in language.
///
/// The plugin language table is consulted too, so re-claiming an extension of
/// another registered plugin language is also reported (the caller arbitrates
/// by priority). Returned entries are the conflicting extensions (lowercased).
pub fn plugin_extension_conflicts(extensions: &[String]) -> Vec<String> {
    let mut conflicts = Vec::new();
    for ext in extensions {
        let ext = ext.to_lowercase();
        if builtin_language_for_extension(&ext).is_some() {
            conflicts.push(ext);
        }
    }
    conflicts
}

/// Whether `name` is a registered plugin language.
pub fn is_plugin_language(name: &str) -> bool {
    plugin_language_table()
        .lock()
        .map(|guard| guard.iter().any(|r| r.name.eq_ignore_ascii_case(name)))
        .unwrap_or(false)
}

/// Stable fingerprint of the current plugin-language registry.
///
/// Computed as SHA-256 over all records sorted canonically by
/// `(name, extensions)` (extensions sorted too), so the value depends only on
/// the *set* of registered plugin languages — never on their registration
/// order. Persisted payloads referencing [`Language::Custom`] indices record
/// this fingerprint; a mismatch after a restart (plugins added/removed or
/// registered differently) means those indices may dangle, so such payloads
/// must be treated as invalid and regenerated.
pub fn plugin_language_fingerprint() -> String {
    let records: Vec<(String, Vec<String>)> = plugin_language_table()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .map(|r| (r.name.clone(), r.extensions.clone()))
        .collect();
    let borrowed: Vec<(&str, &[String])> = records
        .iter()
        .map(|(name, extensions)| (name.as_str(), extensions.as_slice()))
        .collect();
    fingerprint_from_records(&borrowed)
}

/// Pure fingerprint computation over `(name, extensions)` records.
fn fingerprint_from_records(records: &[(&str, &[String])]) -> String {
    use sha2::{Digest, Sha256};

    let mut canonical = records
        .iter()
        .map(|(name, extensions)| {
            let mut sorted = extensions.to_vec();
            sorted.sort();
            format!("{name}={}", sorted.join(","))
        })
        .collect::<Vec<_>>();
    canonical.sort();

    let mut hasher = Sha256::new();
    for record in &canonical {
        hasher.update(record.as_bytes());
        hasher.update(b";");
    }
    hex::encode(hasher.finalize())
}

impl Language {
    /// For a `Language::Custom(index)`, return its registered extensions.
    pub fn custom_language_extensions(&self) -> Vec<String> {
        match self {
            Language::Custom(index) => plugin_language_extensions(*index),
            _ => Vec::new(),
        }
    }

    /// For a `Language::Custom(index)`, return its registered name.
    pub fn custom_language_name(&self) -> Option<String> {
        match self {
            Language::Custom(index) => plugin_language_name(*index),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_file_type_is_source() {
        let info = LanguageInfo::detect_from_path("test.html");
        assert_eq!(info.language, Language::Html);
        assert_eq!(info.file_type, FileType::Source);
    }

    #[test]
    fn test_extensionless_doc_names_are_documentation() {
        for path in [
            "README",
            "readme",
            "CHANGELOG",
            "LICENSE",
            "COPYING",
            "CONTRIBUTING",
            "AUTHORS",
            "NOTICE",
            "docs/README",
        ] {
            let info = LanguageInfo::detect_from_path(path);
            assert_eq!(
                info.file_type,
                FileType::Documentation,
                "expected {path} to be Documentation"
            );
            assert!(
                info.is_document_like(),
                "documentation files must report is_document_like"
            );
        }
        let generic = LanguageInfo::detect_from_path("docs/guide");
        assert_eq!(generic.file_type, FileType::Text);
    }

    /// The document-flow gate used by index/hot-update routing.
    #[test]
    fn test_is_document_like_classification() {
        for path in [
            "README.md",
            "Cargo.toml",
            "app.log",
            "config.yaml",
            "data.json",
        ] {
            assert!(
                LanguageInfo::detect_from_path(path).is_document_like(),
                "{path} must belong to the document flow"
            );
        }
        for path in ["src/main.rs", "lib.py", "index.html", "style.css"] {
            assert!(
                !LanguageInfo::detect_from_path(path).is_document_like(),
                "{path} must belong to the AST flow"
            );
        }
    }

    #[test]
    fn test_content_route_detection() {
        assert_eq!(
            ContentRoute::detect_from_path("docs/guide.md"),
            ContentRoute::Documentation
        );
        assert!(ContentRoute::detect_from_path("docs/guide.md").is_document());
        assert_eq!(
            ContentRoute::detect_from_path("LICENSE"),
            ContentRoute::Documentation
        );
        assert_eq!(
            ContentRoute::detect_from_path("src/main.rs"),
            ContentRoute::Ast
        );
        let info = LanguageInfo::detect_from_path("settings.toml");
        assert_eq!(
            ContentRoute::from_language_info(&info),
            ContentRoute::Config
        );
        assert_eq!(
            ContentRoute::detect_from_path("notes.txt"),
            ContentRoute::PlainText
        );
        assert_eq!(
            ContentRoute::detect_from_path("Makefile"),
            ContentRoute::Config
        );
        // Schema definitions carry their own route through the document flow.
        for path in [
            "api.proto",
            "schema.graphql",
            "service.thrift",
            "types.avsc",
        ] {
            assert_eq!(
                ContentRoute::detect_from_path(path),
                ContentRoute::Schema,
                "{path} must route as Schema"
            );
            assert!(ContentRoute::Schema.is_document());
        }
        assert!(ContentRoute::Config.is_document());
        assert!(ContentRoute::PlainText.is_document());
        assert!(ContentRoute::Documentation.is_document());
        assert!(!ContentRoute::Ast.is_document());
        assert_eq!(ContentRoute::default(), ContentRoute::Ast);
    }

    /// Routing, business and chunk-layer labels must all derive from the
    /// same `FileType`, so every pair stays consistent.
    #[test]
    fn test_unified_classification_entries_stay_consistent() {
        use crate::types::ast_to_nl::ChunkContentType;

        for path in [
            "src/main.rs",
            "docs/guide.md",
            "settings.toml",
            "Makefile",
            "notes.txt",
            "run.log",
            "api.proto",
        ] {
            let info = LanguageInfo::detect_from_path(path);
            let category = info.file_category();
            let chunk_type = info.chunk_content_type_for_path(path);
            assert!(
                chunk_type.matches_category(category),
                "{path}: chunk payload {chunk_type:?} must match category {category:?}"
            );
            // The derived pair equals the single-source derivation.
            let route = ContentRoute::from_language_info(&info);
            assert_eq!(route.is_document(), !matches!(route, ContentRoute::Ast));
        }

        // Spot checks for the payload shapes
        let rust = LanguageInfo::detect_from_path("src/main.rs");
        assert_eq!(
            rust.chunk_content_type(),
            ChunkContentType::Code {
                language: Language::Rust
            }
        );
        let toml = LanguageInfo::detect_from_path("settings.toml");
        assert_eq!(
            toml.chunk_content_type(),
            ChunkContentType::Config {
                format: "toml".to_string()
            }
        );
        let log = LanguageInfo::detect_from_path("run.log");
        assert_eq!(log.chunk_content_type(), ChunkContentType::PlainText);

        // Schema: Document payload, dedicated category, same entry points.
        let proto = LanguageInfo::detect_from_path("api/user.proto");
        assert_eq!(proto.file_type, FileType::Schema);
        assert_eq!(proto.file_category(), crate::types::FileCategory::Schema);
        assert_eq!(
            proto.chunk_content_type(),
            ChunkContentType::Document,
            "schema payloads must match ChunkMetadata::for_schema"
        );

        // Config format labels are never empty, including extensionless names.
        assert_eq!(LanguageInfo::payload_format_from_path("Makefile"), "make");
        assert_eq!(
            LanguageInfo::payload_format_from_path("GNUmakefile"),
            "make"
        );
        assert_eq!(
            LanguageInfo::payload_format_from_path("Dockerfile"),
            "docker"
        );
        // Files with any extension keep the extension as the format.
        assert_eq!(
            LanguageInfo::payload_format_from_path("Dockerfile.dev"),
            "dev"
        );
        assert_eq!(
            LanguageInfo::payload_format_from_path("conf/app.ini"),
            "ini"
        );
        assert_eq!(LanguageInfo::payload_format_from_path("justname"), "other");
    }

    #[test]
    fn test_htm_file_type_is_source() {
        let info = LanguageInfo::detect_from_path("test.htm");
        assert_eq!(info.language, Language::Html);
        assert_eq!(info.file_type, FileType::Source);
    }

    /// The plugin-language fingerprint must depend only on the *set* of
    /// registered languages, never on their registration order. Tested
    /// against the pure helper so the process-global table stays untouched.
    #[test]
    fn test_plugin_language_fingerprint_order_independent() {
        use crate::types::language::fingerprint_from_records;

        let zig = ("zig", vec!["zig".to_string(), "zrs".to_string()]);
        let alpha = ("alpha", vec!["al".to_string()]);

        let forward = fingerprint_from_records(&[("zig", &zig.1), ("alpha", &alpha.1)]);
        let reversed = fingerprint_from_records(&[("alpha", &alpha.1), ("zig", &zig.1)]);
        assert_eq!(forward, reversed, "order must not affect the fingerprint");
        assert_eq!(forward.len(), 64, "SHA-256 hex digest");

        // Extension order within a record is normalized too.
        let zig_flipped = ("zig", vec!["zrs".to_string(), "zig".to_string()]);
        let flipped = fingerprint_from_records(&[("zig", &zig_flipped.1), ("alpha", &alpha.1)]);
        assert_eq!(forward, flipped);

        // Changing the set changes the fingerprint; empty registry is stable.
        let gamma = ("gamma", vec!["ga".to_string()]);
        let with_gamma =
            fingerprint_from_records(&[("zig", &zig.1), ("gamma", &gamma.1), ("alpha", &alpha.1)]);
        assert_ne!(forward, with_gamma);
        let empty = fingerprint_from_records(&[]);
        assert_eq!(
            empty,
            fingerprint_from_records(&[]),
            "empty registry has a fixed fingerprint"
        );
    }

    #[test]
    fn test_vue_file_type_is_source() {
        let info = LanguageInfo::detect_from_path("component.vue");
        assert_eq!(info.language, Language::Vue);
        assert_eq!(info.file_type, FileType::Source);
    }

    #[test]
    fn test_svelte_file_type_is_source() {
        let info = LanguageInfo::detect_from_path("component.svelte");
        assert_eq!(info.language, Language::Svelte);
        assert_eq!(info.file_type, FileType::Source);
    }

    #[test]
    fn test_jsx_file_type_is_source() {
        let info = LanguageInfo::detect_from_path("component.jsx");
        assert_eq!(info.language, Language::Jsx);
        assert_eq!(info.file_type, FileType::Source);
    }

    #[test]
    fn test_tsx_file_type_is_source() {
        let info = LanguageInfo::detect_from_path("component.tsx");
        assert_eq!(info.language, Language::Tsx);
        assert_eq!(info.file_type, FileType::Source);
    }

    #[test]
    fn test_ts_module_extension_detection() {
        let info = LanguageInfo::detect_from_path("component.mts");
        assert_eq!(info.language, Language::TypeScript);
        assert_eq!(info.file_type, FileType::Source);
        let info = LanguageInfo::detect_from_path("component.cts");
        assert_eq!(info.language, Language::TypeScript);
        assert_eq!(info.file_type, FileType::Source);
    }

    #[test]
    fn test_css_file_type_is_source() {
        let info = LanguageInfo::detect_from_path("style.css");
        assert_eq!(info.language, Language::Css);
        assert_eq!(info.file_type, FileType::Source);
    }

    #[test]
    fn test_scss_routes_to_document_pipeline() {
        let info = LanguageInfo::detect_from_path("style.scss");
        assert_eq!(info.language, Language::Scss);
        assert_eq!(info.file_type, FileType::Text);
        assert_eq!(
            ContentRoute::from_language_info(&info),
            ContentRoute::PlainText
        );
    }

    #[test]
    fn test_less_routes_to_document_pipeline() {
        let info = LanguageInfo::detect_from_path("style.less");
        assert_eq!(info.language, Language::Less);
        assert_eq!(info.file_type, FileType::Text);
        assert_eq!(
            ContentRoute::from_language_info(&info),
            ContentRoute::PlainText
        );
    }

    #[test]
    fn test_html_has_embedded_blocks() {
        assert!(Language::Html.has_embedded_blocks());
        assert!(Language::Vue.has_embedded_blocks());
        assert!(Language::Svelte.has_embedded_blocks());
    }

    #[test]
    fn test_html_is_supported_for_ast() {
        assert!(Language::Html.is_supported_for_ast());
        assert!(Language::Vue.is_supported_for_ast());
        assert!(Language::Svelte.is_supported_for_ast());
        assert!(Language::Jsx.is_supported_for_ast());
        assert!(Language::Tsx.is_supported_for_ast());
    }
}
