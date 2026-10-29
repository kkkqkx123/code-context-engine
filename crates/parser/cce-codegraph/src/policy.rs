pub mod c;
pub mod common;
pub mod cpp;
pub mod csharp;
pub mod dart;
pub mod go;
pub mod hierarchy;
pub mod java;
pub mod javascript;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod type_member;

use cce_types::Entity;
use cce_types::language::Language;

use crate::symbol::{ScopeContext, Visibility};

/// Dispatch visibility signal parsing to the language-specific table.
///
/// Languages without a dedicated signal table share the generic catch-all.
/// This is intentional for Bash and Lua, which have no visibility system.
/// C, PHP and Ruby have dedicated tables backed by storage-class,
/// modifier and access-section signals.
pub fn visibility_from_signal(signal: &str, language: &Language) -> Option<Visibility> {
    // Kotlin modifiers differ from Java: `open` is orthogonal to visibility
    // (an `open class` is still public) while `internal` is Kotlin-specific.
    if *language == Language::Kotlin {
        let trimmed = signal.to_lowercase();
        match trimmed.trim() {
            "public" | "pub" | "export" | "exported" | "open" => {
                return Some(Visibility::Public);
            }
            "protected" => return Some(Visibility::Protected),
            "private" | "pub(self)" | "self" => return Some(Visibility::Private),
            "internal" | "package" | "crate" | "pub(crate)" => {
                return Some(Visibility::Internal);
            }
            "protected internal" => return Some(Visibility::ProtectedInternal),
            "private protected" => return Some(Visibility::PrivateProtected),
            _ => {}
        }
    }
    // Shell `local` declares a function-local variable, never a public symbol.
    if matches!(language, Language::Bash) && signal.trim().eq_ignore_ascii_case("local") {
        return Some(Visibility::Private);
    }
    match language {
        Language::Rust => rust::visibility_from_signal(signal),
        Language::Go => go::visibility_from_signal(signal),
        Language::Python => python::visibility_from_signal(signal),
        Language::Dart => dart::visibility_from_signal(signal),
        Language::Java | Language::Kotlin | Language::Scala => java::visibility_from_signal(signal),
        Language::CSharp => csharp::visibility_from_signal(signal),
        Language::C => c::visibility_from_signal(signal),
        Language::Cpp => cpp::visibility_from_signal(signal),
        Language::Php => php::visibility_from_signal(signal),
        Language::Ruby => ruby::visibility_from_signal(signal),
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            javascript::visibility_from_signal(signal)
        }
        _ => {
            let trimmed = signal.to_lowercase();
            let trimmed = trimmed.trim();
            if trimmed == "pub"
                || trimmed == "public"
                || trimmed == "export"
                || trimmed == "exported"
            {
                return Some(Visibility::Public);
            }
            if trimmed == "pub(crate)" || trimmed == "crate" {
                return Some(Visibility::Package);
            }
            if trimmed == "pub(super)" || trimmed == "super" {
                return Some(Visibility::Super);
            }
            if trimmed == "pub(self)" || trimmed == "self" {
                return Some(Visibility::Private);
            }
            if trimmed.starts_with("pub(in") {
                if let Some(open) = trimmed.find('(') {
                    if let Some(close) = trimmed.rfind(')') {
                        if close > open {
                            let inside = trimmed[open + 1..close].trim();
                            let path = inside
                                .strip_prefix("in")
                                .unwrap_or(inside)
                                .trim()
                                .to_string();
                            return Some(Visibility::Restricted { path });
                        }
                    }
                }
                return None;
            }
            if trimmed == "protected internal" {
                return Some(Visibility::ProtectedInternal);
            }
            if trimmed == "private protected" {
                return Some(Visibility::PrivateProtected);
            }
            if trimmed == "protected" {
                return Some(Visibility::Protected);
            }
            if trimmed == "internal" || trimmed == "package" {
                return Some(Visibility::Internal);
            }
            if trimmed == "private" {
                return Some(Visibility::Private);
            }
            if trimmed.starts_with("friend") {
                return Some(Visibility::Friend {
                    allowed: Vec::new(),
                });
            }
            None
        }
    }
}

/// Dispatch naming-convention visibility to the language-specific handler.
pub fn visibility_from_name(name: &str, language: &Language) -> Option<Visibility> {
    // Ruby's `initialize` is always private, regardless of position.
    if *language == Language::Ruby && name == "initialize" {
        return Some(Visibility::Private);
    }
    match language {
        Language::Go => go::visibility_from_name(name),
        Language::Python => python::visibility_from_name(name),
        Language::Dart => dart::visibility_from_name(name),
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            javascript::visibility_from_name(name)
        }
        Language::Rust => rust::visibility_from_name(name),
        Language::Java | Language::Kotlin | Language::Scala => java::visibility_from_name(name),
        Language::CSharp => csharp::visibility_from_name(name),
        Language::C => c::visibility_from_name(name),
        Language::Cpp => cpp::visibility_from_name(name),
        Language::Php => php::visibility_from_name(name),
        Language::Ruby => ruby::visibility_from_name(name),
        _ => None,
    }
}

/// Detect entity visibility using language-specific rules.
///
/// This is the single determination function for cross-file addressability,
/// shared by export indexing and symbol table construction.
pub fn detect_entity_visibility(entity: &Entity, language: &Language) -> Visibility {
    for modifier in &entity.modifiers {
        if let Some(vis) = visibility_from_signal(&modifier.to_lowercase(), language) {
            return vis;
        }
    }
    if let Some(signal) = entity.metadata.get("visibility") {
        if let Some(vis) = visibility_from_signal(&signal.to_lowercase(), language) {
            return vis;
        }
    }

    if *language == Language::Python {
        if let Some(flag) = entity.metadata.get("is_exported_by_all") {
            if flag == "true" {
                return Visibility::Public;
            } else if flag == "false" {
                return Visibility::Private;
            }
        }
    }

    if let Some(vis) = visibility_from_name(&entity.name, language) {
        match language {
            Language::Go | Language::Python | Language::Dart => return vis,
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx
                if vis == Visibility::Private =>
            {
                return vis;
            }
            _ => {}
        }
    }

    // Ruby's `initialize` is naming-private; check before modifiers so an
    // absent modifier does not fall through to the public default.
    if *language == Language::Ruby && entity.name == "initialize" {
        return Visibility::Private;
    }

    match language {
        Language::Rust => rust::default_visibility(),
        Language::Go => go::default_visibility(&entity.name),
        Language::Python => python::default_visibility(&entity.name),
        Language::Dart => dart::default_visibility(&entity.name),
        // Kotlin defaults to public; Java/Scala stay package-private.
        Language::Kotlin => Visibility::Public,
        Language::Java | Language::Scala => java::default_visibility(),
        Language::CSharp => csharp::default_visibility(),
        Language::C => c::default_visibility(),
        Language::Cpp => cpp::default_visibility(),
        Language::Php => php::default_visibility(),
        Language::Ruby => ruby::default_visibility(),
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            javascript::default_visibility()
        }
        // Shell/Lua have no visibility system; keep symbols file-scoped
        // instead of advertising them as public exports.
        Language::Bash | Language::Lua => Visibility::Module,
        _ => Visibility::Public,
    }
}

/// Whether an entity is exported according to language-specific visibility.
pub fn is_entity_exported(entity: &Entity, language: Language) -> bool {
    let vis = detect_entity_visibility(entity, &language);
    common::is_exported_visibility(&vis)
}

/// Language-aware visibility check.
pub fn is_visible(
    visibility: &Visibility,
    from_scope: &ScopeContext,
    defined_in: &ScopeContext,
    language: Language,
) -> bool {
    match language {
        Language::Rust => rust::is_visible(visibility, from_scope, defined_in),
        Language::Java | Language::Kotlin | Language::Scala => {
            java::is_visible(visibility, from_scope, defined_in)
        }
        Language::Go => go::is_visible(visibility, from_scope, defined_in),
        Language::CSharp => csharp::is_visible(visibility, from_scope, defined_in),
        Language::C => c::is_visible(visibility, from_scope, defined_in),
        Language::Cpp => cpp::is_visible(visibility, from_scope, defined_in),
        Language::Php => php::is_visible(visibility, from_scope, defined_in),
        Language::Ruby => ruby::is_visible(visibility, from_scope, defined_in),
        Language::Python => python::is_visible(visibility, from_scope, defined_in),
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            javascript::is_visible(visibility, from_scope, defined_in)
        }
        Language::Dart => dart::is_visible(visibility, from_scope, defined_in),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TypeScript delegates to the JavaScript policy.
    #[test]
    fn typescript_delegates_to_javascript_signal() {
        assert_eq!(
            visibility_from_signal("public", &Language::TypeScript),
            Some(Visibility::Public)
        );
        assert_eq!(
            visibility_from_signal("protected", &Language::TypeScript),
            Some(Visibility::Protected)
        );
        assert_eq!(
            visibility_from_signal("private", &Language::TypeScript),
            Some(Visibility::Private)
        );
        assert_eq!(
            visibility_from_signal("unknown", &Language::TypeScript),
            None
        );
    }

    #[test]
    fn typescript_hash_private_naming() {
        assert_eq!(
            visibility_from_name("#field", &Language::TypeScript),
            Some(Visibility::Private)
        );
        assert_eq!(visibility_from_name("normal", &Language::TypeScript), None);
    }

    #[test]
    fn typescript_is_visible_same_file_only_for_private() {
        let def = ScopeContext::new("a.ts", "pkg");
        let same_file = ScopeContext::new("a.ts", "pkg");
        let other_file = ScopeContext::new("b.ts", "pkg");
        assert!(is_visible(
            &Visibility::Private,
            &same_file,
            &def,
            Language::TypeScript
        ));
        assert!(!is_visible(
            &Visibility::Private,
            &other_file,
            &def,
            Language::TypeScript
        ));
    }

    // Kotlin delegates to the Java (JVM) policy.
    #[test]
    fn kotlin_delegates_to_jvm_signal() {
        assert_eq!(
            visibility_from_signal("public", &Language::Kotlin),
            Some(Visibility::Public)
        );
        assert_eq!(
            visibility_from_signal("protected", &Language::Kotlin),
            Some(Visibility::Protected)
        );
        assert_eq!(
            visibility_from_signal("internal", &Language::Kotlin),
            Some(Visibility::Internal)
        );
        assert_eq!(
            visibility_from_signal("private", &Language::Kotlin),
            Some(Visibility::Private)
        );
    }

    #[test]
    fn kotlin_no_naming_visibility() {
        assert_eq!(visibility_from_name("anything", &Language::Kotlin), None);
    }

    #[test]
    fn kotlin_package_visibility_scoped_by_package() {
        let def = ScopeContext::new("A.kt", "pkg");
        let same_pkg = ScopeContext::new("B.kt", "pkg");
        let other_pkg = ScopeContext::new("C.kt", "other");
        assert!(is_visible(
            &Visibility::Package,
            &same_pkg,
            &def,
            Language::Kotlin
        ));
        assert!(!is_visible(
            &Visibility::Package,
            &other_pkg,
            &def,
            Language::Kotlin
        ));
    }

    // Scala delegates to the Java (JVM) policy, same as Kotlin.
    #[test]
    fn scala_delegates_to_jvm_signal() {
        assert_eq!(
            visibility_from_signal("public", &Language::Scala),
            Some(Visibility::Public)
        );
        assert_eq!(
            visibility_from_signal("protected", &Language::Scala),
            Some(Visibility::Protected)
        );
        assert_eq!(
            visibility_from_signal("private", &Language::Scala),
            Some(Visibility::Private)
        );
    }

    #[test]
    fn scala_jvm_crate_signal_maps_to_internal() {
        // Distinct from the generic catch-all: JVM maps `pub(crate)` to Internal.
        assert_eq!(
            visibility_from_signal("pub(crate)", &Language::Scala),
            Some(Visibility::Internal)
        );
    }

    // Ruby uses a dedicated handler.
    #[test]
    fn ruby_dedicated_signal() {
        assert_eq!(
            visibility_from_signal("public", &Language::Ruby),
            Some(Visibility::Public)
        );
        assert_eq!(
            visibility_from_signal("private", &Language::Ruby),
            Some(Visibility::Private)
        );
        assert_eq!(
            visibility_from_signal("protected", &Language::Ruby),
            Some(Visibility::Protected)
        );
        assert_eq!(visibility_from_signal("pub(crate)", &Language::Ruby), None);
    }

    #[test]
    fn ruby_permissive_is_visible() {
        let def = ScopeContext::new("a.rb", "pkg");
        let other = ScopeContext::new("b.rb", "other");
        assert!(is_visible(
            &Visibility::Private,
            &other,
            &def,
            Language::Ruby
        ));
    }

    #[test]
    fn ruby_naming_visibility() {
        assert_eq!(
            visibility_from_name("initialize", &Language::Ruby),
            Some(Visibility::Private)
        );
        assert_eq!(visibility_from_name("_helper", &Language::Ruby), None);
    }

    // PHP uses a dedicated handler.
    #[test]
    fn php_dedicated_signal() {
        assert_eq!(
            visibility_from_signal("public", &Language::Php),
            Some(Visibility::Public)
        );
        assert_eq!(
            visibility_from_signal("private", &Language::Php),
            Some(Visibility::Private)
        );
        assert_eq!(
            visibility_from_signal("protected", &Language::Php),
            Some(Visibility::Protected)
        );
        assert_eq!(visibility_from_signal("static", &Language::Php), None);
    }

    #[test]
    fn php_scoped_is_visible() {
        let def = ScopeContext::new("a.php", "App");
        let same_file = ScopeContext::new("a.php", "App");
        let same_pkg = ScopeContext::new("b.php", "App");
        let other = ScopeContext::new("c.php", "Other");
        assert!(is_visible(&Visibility::Public, &other, &def, Language::Php));
        assert!(is_visible(
            &Visibility::Protected,
            &same_pkg,
            &def,
            Language::Php
        ));
        assert!(!is_visible(
            &Visibility::Protected,
            &other,
            &def,
            Language::Php
        ));
        assert!(is_visible(
            &Visibility::Private,
            &same_file,
            &def,
            Language::Php
        ));
        assert!(!is_visible(
            &Visibility::Private,
            &same_pkg,
            &def,
            Language::Php
        ));
    }

    // Bash has no real visibility model; everything resolves to public/visible.
    #[test]
    fn bash_generic_signal_and_visibility() {
        assert_eq!(
            visibility_from_signal("public", &Language::Bash),
            Some(Visibility::Public)
        );
        assert_eq!(visibility_from_name("foo", &Language::Bash), None);
        let def = ScopeContext::new("a.sh", "");
        assert!(is_visible(&Visibility::Private, &def, &def, Language::Bash));
    }

    // C uses storage-class signals.
    #[test]
    fn c_dedicated_signal_and_naming() {
        assert_eq!(
            visibility_from_signal("static", &Language::C),
            Some(Visibility::Private)
        );
        assert_eq!(
            visibility_from_signal("extern", &Language::C),
            Some(Visibility::Public)
        );
        assert_eq!(visibility_from_signal("public", &Language::C), None);
        assert_eq!(visibility_from_name("foo", &Language::C), None);
    }

    #[test]
    fn c_scoped_is_visible() {
        let def = ScopeContext::new("a.c", "pkg");
        let same = ScopeContext::new("a.c", "pkg");
        let other = ScopeContext::new("b.c", "pkg");
        assert!(is_visible(&Visibility::Public, &other, &def, Language::C));
        assert!(is_visible(&Visibility::Private, &same, &def, Language::C));
        assert!(!is_visible(&Visibility::Private, &other, &def, Language::C));
    }

    // Lua has no real visibility model; everything resolves to public/visible.
    #[test]
    fn lua_generic_signal_and_visibility() {
        assert_eq!(
            visibility_from_signal("public", &Language::Lua),
            Some(Visibility::Public)
        );
        assert_eq!(visibility_from_name("foo", &Language::Lua), None);
        let def = ScopeContext::new("a.lua", "");
        assert!(is_visible(&Visibility::Private, &def, &def, Language::Lua));
    }
}
