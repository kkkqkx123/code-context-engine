// Groovy Standard Library Detector
// Handles detection of Groovy (GDK/JDK default imports) standard library entities

pub struct GroovyStdlibDetector;

impl GroovyStdlibDetector {
    // Groovy standard packages (GDK plus the JDK packages every Groovy
    // file imports by default: java.io/java.lang/java.math/java.net/
    // java.util, java.math.BigDecimal/BigInteger).
    pub const GROOVY_PACKAGES: &[&str] = &[
        // Core GDK packages
        "groovy",
        "groovy.cli",
        "groovy.inspect",
        "groovy.json",
        "groovy.lang",
        "groovy.model",
        "groovy.servlet",
        "groovy.sql",
        "groovy.swing",
        "groovy.template",
        "groovy.test",
        "groovy.time",
        "groovy.transform",
        "groovy.ui",
        "groovy.util",
        "groovy.xml",
        "groovy.yaml",
        // Codehaus runtime packages
        "org.codehaus.groovy",
        "org.codehaus.groovy.ast",
        "org.codehaus.groovy.control",
        "org.codehaus.groovy.reflection",
        "org.codehaus.groovy.runtime",
        "org.codehaus.groovy.syntax",
        "org.codehaus.groovy.tools",
        "org.codehaus.groovy.transform",
        "org.codehaus.groovy.util",
        // JDK default imports (also visible from Groovy without import)
        "java.io",
        "java.lang",
        "java.math",
        "java.net",
        "java.util",
    ];

    // Built-in functions (GDK DefaultGroovyMethods available on every object)
    pub const BUILTIN_FUNCTIONS: &[&str] = &[
        // Output
        "print",
        "printf",
        "println",
        "sprintf",
        // Object helpers
        "inspect",
        "is",
        "isCase",
        "sleep",
        "use",
        "with",
        "withTraits",
        "tap",
        // Collection builders and combinators
        "each",
        "eachWithIndex",
        "collect",
        "collectMany",
        "find",
        "findAll",
        "findIndexOf",
        "findLast",
        "findResult",
        "grep",
        "groupBy",
        "groupEntriesBy",
        "inject",
        "count",
        "countBy",
        "sum",
        "join",
        "flatten",
        "flattenTo",
        "unique",
        "sort",
        "max",
        "min",
        "any",
        "every",
        "drop",
        "dropWhile",
        "take",
        "takeWhile",
        "collate",
        "combinations",
        "permutations",
        "transpose",
        "toSet",
        "toList",
        "asList",
        "asSet",
        "asImmutable",
        // Map helpers
        "each",
        "collectEntries",
        "findAll",
        "groupBy",
        "subMap",
        "minus",
        "plus",
        // String helpers
        "tokenize",
        "readLines",
        "eachLine",
        "splitEachLine",
        "execute",
        "decodeBase64",
        "encodeBase64",
        "md5",
        "sha1",
        "normalize",
        "denormalize",
        "stripIndent",
        "stripMargin",
        // Range helpers
        "step",
        // File helpers
        "eachFile",
        "eachDir",
        "eachFileRecurse",
        "traverse",
        // Type checks
        "isArray",
        "isCase",
        // Power assertion / testing helpers
        "assert",
        // Metaprogramming entry points
        "invokeMethod",
        "methodMissing",
        "propertyMissing",
    ];

    pub fn is_groovy_package(package: &str) -> bool {
        Self::GROOVY_PACKAGES.contains(&package)
    }

    pub fn is_builtin_function(name: &str) -> bool {
        Self::BUILTIN_FUNCTIONS.contains(&name)
    }

    /// Check if a qualified path is from the Groovy stdlib
    pub fn is_groovy_path(path: &str) -> bool {
        for &package in Self::GROOVY_PACKAGES {
            if path == package || path.starts_with(&format!("{package}.")) {
                return true;
            }
        }

        let first_component = path.split('.').next().unwrap_or("");
        if first_component == "groovy" || first_component == "org" {
            // `org.*` alone is too broad: only the codehaus runtime counts.
            return path == "org.codehaus.groovy" || path.starts_with("org.codehaus.groovy.");
        }

        false
    }

    /// Standard library constants.
    pub const STDLIB_CONSTANTS: &[&str] = &["GroovySystem.VERSION"];

    /// Check if a name is a standard library constant.
    pub fn is_stdlib_constant(name: &str) -> bool {
        Self::STDLIB_CONSTANTS.contains(&name)
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for builtin function
        if Self::is_builtin_function(call_name) {
            return true;
        }

        // Check for qualified path (e.g., groovy.json.JsonSlurper)
        if call_name.contains('.') {
            if Self::is_groovy_path(call_name) {
                return true;
            }

            // GDK extension calls on receivers (e.g., "list.each").
            // The receiver check is intentionally narrow: only the
            // `it`-style implicit receiver names that cannot be confused
            // with project classes keep the bare-method fate.
            if let Some((receiver, _)) = call_name.rsplit_once('.')
                && (receiver == "it" || receiver == "delegate" || receiver == "owner")
            {
                return true;
            }

            // Check for package-level functions
            if call_name
                .split('.')
                .next()
                .is_some_and(Self::is_groovy_package)
            {
                return true;
            }
        }

        false
    }
}

// Groovy refines the answer for call relations: GDK extension calls on the
// implicit `it`/`delegate`/`owner` receivers carry stdlib meaning that a bare
// name check misses.
impl_stdlib_by_type_calls!(GroovyStdlibDetector, |name, _relation| {
    Self::is_builtin_function(name) || name.split('.').next().is_some_and(Self::is_groovy_package)
});

impl_stdlib_categorizer!(
    GroovyStdlibDetector,
    [
        (
            StdlibCategory::Utility,
            [
                "print", "printf", "println", "sprintf", "inspect", "sleep", "use", "with", "tap",
                "assert"
            ]
        ),
        (
            StdlibCategory::Collection,
            [
                "each",
                "eachWithIndex",
                "collect",
                "collectMany",
                "find",
                "findAll",
                "grep",
                "groupBy",
                "inject",
                "count",
                "countBy",
                "sum",
                "join",
                "flatten",
                "unique",
                "sort",
                "toSet",
                "toList"
            ]
        ),
        (
            StdlibCategory::String,
            [
                "tokenize",
                "readLines",
                "eachLine",
                "splitEachLine",
                "decodeBase64",
                "encodeBase64",
                "normalize",
                "denormalize",
                "stripIndent",
                "stripMargin"
            ]
        ),
        (
            StdlibCategory::Io,
            [
                "eachFile",
                "eachDir",
                "eachFileRecurse",
                "traverse",
                "execute"
            ]
        ),
    ]
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_groovy_package() {
        assert!(GroovyStdlibDetector::is_groovy_package("groovy"));
        assert!(GroovyStdlibDetector::is_groovy_package("groovy.json"));
        assert!(GroovyStdlibDetector::is_groovy_package(
            "org.codehaus.groovy.runtime"
        ));
        assert!(!GroovyStdlibDetector::is_groovy_package("com.example"));
    }

    #[test]
    fn test_is_builtin_function() {
        assert!(GroovyStdlibDetector::is_builtin_function("println"));
        assert!(GroovyStdlibDetector::is_builtin_function("each"));
        assert!(GroovyStdlibDetector::is_builtin_function("collect"));
        assert!(!GroovyStdlibDetector::is_builtin_function("myFunction"));
    }

    #[test]
    fn test_is_groovy_path() {
        assert!(GroovyStdlibDetector::is_groovy_path("groovy"));
        assert!(GroovyStdlibDetector::is_groovy_path(
            "groovy.json.JsonSlurper"
        ));
        assert!(GroovyStdlibDetector::is_groovy_path(
            "org.codehaus.groovy.runtime.DefaultGroovyMethods"
        ));
        assert!(!GroovyStdlibDetector::is_groovy_path("com.example.MyClass"));
        // `org.*` outside codehaus is not Groovy stdlib.
        assert!(!GroovyStdlibDetector::is_groovy_path("org.example.Foo"));
    }

    #[test]
    fn test_is_stdlib_call() {
        // Builtin functions
        assert!(GroovyStdlibDetector::is_stdlib_call("println"));
        assert!(GroovyStdlibDetector::is_stdlib_call("each"));

        // Qualified paths
        assert!(GroovyStdlibDetector::is_stdlib_call(
            "groovy.json.JsonSlurper"
        ));

        // Negative cases
        assert!(!GroovyStdlibDetector::is_stdlib_call("MyClass"));
        assert!(!GroovyStdlibDetector::is_stdlib_call("com.example.MyClass"));
        assert!(!GroovyStdlibDetector::is_stdlib_call("myFunction"));
    }
}
