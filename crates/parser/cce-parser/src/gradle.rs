//! Gradle build-script dependency extraction via tree-sitter.
//!
//! Purpose-built parsing over `build.gradle` (Groovy DSL) and
//! `build.gradle.kts` (Kotlin DSL), mirroring the reference implementation:
//! only Maven-coordinate dependencies are returned. Plugin declarations
//! target the plugin portal, local project and file dependencies carry no
//! registry identity, and both are skipped. Unparsable sources yield nothing.

use std::collections::HashMap;

use streaming_iterator::StreamingIterator;

/// One Maven-coordinate dependency found in a build script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradleDependency {
    /// `group:artifact` coordinate.
    pub name: String,
    /// Version requirement (empty when the build script pins none).
    pub version: String,
}

/// Version-catalog alias table (`gradle/libs.versions.toml`).
///
/// Keys are normalized alias segments (`lowercase`, split on `-_.`), so
/// `libs.foo-bar` and `libs.foo.bar` resolve to the same entry.
#[derive(Debug, Clone, Default)]
pub struct VersionCatalog {
    aliases: HashMap<Vec<String>, (String, String)>,
}

impl VersionCatalog {
    /// Empty catalog (no alias resolution).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Register one alias (`libs.<alias>`) mapping to a `group:artifact`
    /// module with the given version requirement.
    pub fn insert(&mut self, alias: &str, module: &str, version: &str) {
        self.aliases.insert(
            catalog_key(alias),
            (module.to_string(), version.to_string()),
        );
    }

    fn lookup(&self, alias_path: &str) -> Option<&(String, String)> {
        let key = catalog_key(alias_path);
        self.aliases.get(&key).or_else(|| {
            let lowered: Vec<String> = key.iter().map(|s| s.to_lowercase()).collect();
            self.aliases
                .iter()
                .find(|(stored, _)| {
                    stored.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>() == lowered
                })
                .map(|(_, value)| value)
        })
    }
}

/// Extract Maven-coordinate dependencies from a Gradle build script.
///
/// `is_kotlin` selects the Kotlin DSL (`build.gradle.kts`) grammar,
/// otherwise the Groovy DSL (`build.gradle`) grammar is used. Alias
/// references (`libs.*`) resolve through `catalog`; anything else
/// (plugin blocks, project/file dependencies, catalog misses) is skipped.
pub fn extract_gradle_dependencies(
    source: &str,
    is_kotlin: bool,
    catalog: &VersionCatalog,
) -> Vec<GradleDependency> {
    let (language, query) = if is_kotlin {
        (tree_sitter_kotlin_ng::LANGUAGE.into(), KOTLIN_DEP_QUERY)
    } else {
        (tree_sitter_groovy::language(), GROOVY_DEP_QUERY)
    };
    let query = match tree_sitter::Query::new(&language, query) {
        Ok(query) => query,
        Err(_) => return Vec::new(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(source, None) {
        Some(tree) => tree,
        None => return Vec::new(),
    };

    let bytes = source.as_bytes();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), bytes);
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    while let Some(mat) = matches.next() {
        let mut config = None;
        let mut args = None;
        let mut call_end = None;
        for capture in mat.captures {
            let name = query.capture_names()[capture.index as usize];
            let Ok(text) = capture.node.utf8_text(bytes) else {
                continue;
            };
            match name {
                "dep.config" => config = Some(text.trim().to_string()),
                "dep.args" => args = Some(text.to_string()),
                "dep.call" => call_end = Some(capture.node.end_byte()),
                _ => {}
            }
        }
        let (Some(config), Some(args), Some(call_end)) = (config, args, call_end) else {
            continue;
        };
        if !is_dep_config(&config) {
            continue;
        }
        let mut resolved = resolve_arg(&args, catalog);
        if resolved.is_none() && is_bare_platform(&args) {
            // Juxtaposed `platform` wrappers can split across siblings
            // (`implementation platform ('g:a:v')`); recover the version
            // string from the parenthesized group following the call.
            if let Some(follow_up) = platform_follow_up(source, call_end) {
                resolved = resolve_arg(&follow_up, catalog);
            }
        }
        if let Some((name, version)) = resolved
            && seen.insert(name.clone())
        {
            out.push(GradleDependency { name, version });
        }
    }
    out
}

const GROOVY_DEP_QUERY: &str = r#"
(juxt_function_call
  [(identifier) (dotted_identifier)] @dep.config
  (argument_list) @dep.args) @dep.call
(function_call
  function: [(identifier) (dotted_identifier)] @dep.config
  (argument_list) @dep.args) @dep.call
"#;

const KOTLIN_DEP_QUERY: &str = r#"
(call_expression
  [(identifier) (navigation_expression)] @dep.config
  [(value_arguments) (annotated_lambda)] @dep.args) @dep.call
"#;

const DEP_SUFFIXES: &[&str] = &["implementation", "compileonly", "runtimeonly"];

fn is_dep_config(name: &str) -> bool {
    let normalized = name.trim().to_lowercase();
    normalized == "api" || DEP_SUFFIXES.iter().any(|s| normalized.ends_with(s))
}

fn is_bare_platform(args: &str) -> bool {
    matches!(
        strip_parens(args).trim().to_lowercase().as_str(),
        "platform" | "enforcedplatform"
    )
}

fn unquote(text: &str) -> Option<&str> {
    let text = text.trim();
    if text.len() >= 2 {
        let (first, last) = (text.as_bytes()[0], text.as_bytes()[text.len() - 1]);
        if (first == b'\'' || first == b'"') && first == last {
            return Some(&text[1..text.len() - 1]);
        }
    }
    None
}

fn strip_parens(text: &str) -> &str {
    let text = text.trim();
    if text.starts_with('(') && text.ends_with(')') && text.len() >= 2 {
        text[1..text.len() - 1].trim()
    } else {
        text
    }
}

fn split_coordinate(text: &str) -> Option<(String, String)> {
    let mut parts = text.split(':');
    let group = parts.next()?.trim();
    let artifact = parts.next()?.trim();
    if group.is_empty() || artifact.is_empty() {
        return None;
    }
    let version = parts.next().unwrap_or("").trim().to_string();
    Some((format!("{group}:{artifact}"), version))
}

fn unwrap_platform(text: &str) -> &str {
    let lowered = text.trim().to_lowercase();
    for wrapper in ["platform(", "enforcedplatform("] {
        if lowered.starts_with(wrapper) && text.trim().ends_with(')') {
            let inner = text.trim();
            return strip_parens(&inner[wrapper.len() - 1..]);
        }
    }
    text
}

fn catalog_key(alias: &str) -> Vec<String> {
    alias
        .split(['-', '_', '.'])
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn resolve_arg(arg_text: &str, catalog: &VersionCatalog) -> Option<(String, String)> {
    let inner = strip_parens(arg_text);
    let inner = unwrap_platform(inner);
    if let Some(literal) = unquote(inner) {
        return split_coordinate(literal);
    }
    if let Some(alias) = inner.strip_prefix("libs.") {
        return catalog.lookup(alias).cloned();
    }
    None
}

/// Recover the version string when a juxtaposed platform wrapper splits
/// across siblings: scan the parenthesized group following `call_end` for
/// its first quoted literal.
fn platform_follow_up(source: &str, call_end: usize) -> Option<String> {
    let rest = source.get(call_end..)?.trim_start();
    let group = rest.strip_prefix('(')?;
    let mut depth = 1usize;
    let mut end = None;
    for (i, c) in group.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let group = &group[..end?];
    let mut chars = group.char_indices();
    while let Some((i, c)) = chars.next() {
        if c == '\'' || c == '"' {
            let quote = c;
            let mut literal = String::new();
            for (_, c) in chars.by_ref() {
                if c == quote {
                    return Some(format!("{quote}{literal}{quote}"));
                }
                literal.push(c);
            }
            return None;
        }
        let _ = i;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_groovy(source: &str) -> Vec<GradleDependency> {
        extract_gradle_dependencies(source, false, &VersionCatalog::empty())
    }

    fn extract_kotlin(source: &str) -> Vec<GradleDependency> {
        extract_gradle_dependencies(source, true, &VersionCatalog::empty())
    }

    #[test]
    fn test_groovy_string_form() {
        let deps = extract_groovy(
            r#"
dependencies {
    implementation 'org.springframework:spring-core:6.0.0'
    testImplementation 'junit:junit:4.13.2'
}
"#,
        );
        let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"org.springframework:spring-core"));
        assert!(names.contains(&"junit:junit"));
        assert_eq!(
            deps.iter()
                .find(|d| d.name == "junit:junit")
                .map(|d| d.version.as_str()),
            Some("4.13.2")
        );
    }

    #[test]
    fn test_groovy_paren_form() {
        let deps = extract_groovy(
            "dependencies {\n    implementation('com.google.guava:guava:32.0.0')\n}\n",
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "com.google.guava:guava");
        assert_eq!(deps[0].version, "32.0.0");
    }

    #[test]
    fn test_groovy_skips_plugins_and_projects() {
        let deps = extract_groovy(
            r#"
plugins {
    id 'java'
    id 'org.springframework.boot' version '3.0.0'
}
dependencies {
    implementation project(':core')
    implementation files('libs/local.jar')
    api 'org.apache.commons:commons-lang3:3.14.0'
}
"#,
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "org.apache.commons:commons-lang3");
    }

    #[test]
    fn test_groovy_platform_wrapper() {
        let deps = extract_groovy(
            "dependencies {\n    implementation platform('org.springframework.boot:spring-boot-dependencies:3.0.0')\n}\n",
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(
            deps[0].name,
            "org.springframework.boot:spring-boot-dependencies"
        );
    }

    #[test]
    fn test_groovy_catalog_reference() {
        let mut catalog = VersionCatalog::empty();
        catalog.insert("spring-core", "org.springframework:spring-core", "6.0.0");
        let deps = extract_gradle_dependencies(
            "dependencies {\n    implementation libs.spring.core\n}\n",
            false,
            &catalog,
        );
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "org.springframework:spring-core");
        assert_eq!(deps[0].version, "6.0.0");
    }

    #[test]
    fn test_kotlin_string_forms() {
        let deps = extract_kotlin(
            r#"
dependencies {
    implementation("org.jetbrains.kotlin:kotlin-stdlib:1.9.0")
    testImplementation("junit:junit:4.13.2")
}
"#,
        );
        let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"org.jetbrains.kotlin:kotlin-stdlib"));
        assert!(names.contains(&"junit:junit"));
    }

    #[test]
    fn test_kotlin_catalog_and_platform() {
        let mut catalog = VersionCatalog::empty();
        catalog.insert("guava", "com.google.guava:guava", "32.0.0");
        let deps = extract_gradle_dependencies(
            "dependencies {\n    implementation(libs.guava)\n    implementation(platform(\"org.junit:junit-bom:5.10.0\"))\n}\n",
            true,
            &catalog,
        );
        let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"com.google.guava:guava"));
        assert!(names.contains(&"org.junit:junit-bom"));
    }

    #[test]
    fn test_unparsable_source_yields_nothing() {
        assert!(extract_groovy("(((not valid groovy ((( ").is_empty());
    }

    #[test]
    fn test_plugin_block_yields_nothing() {
        assert!(extract_kotlin("plugins {\n    kotlin(\"jvm\") version \"1.9.0\"\n}\n").is_empty());
    }
}
