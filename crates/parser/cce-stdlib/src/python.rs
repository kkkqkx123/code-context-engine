// Python Standard Library Detector
// Handles detection of Python standard library entities

pub struct PythonStdlibDetector;

impl PythonStdlibDetector {
    // Standard library modules
    pub const STDLIB_MODULES: &[&str] = &[
        // Core and builtins
        "builtins",
        "sys",
        "gc",
        "warnings",
        "contextlib",
        "abc",
        "atexit",
        "time",
        "argparse",
        "pathlib",
        // Data types
        "collections",
        "datetime",
        "decimal",
        "fractions",
        "enum",
        "typing",
        "dataclasses",
        "types",
        // Text processing
        "string",
        "re",
        "codecs",
        "difflib",
        "textwrap",
        "unicodedata",
        "stringprep",
        // Binary data
        "struct",
        "base64",
        "binascii",
        "quopri",
        // File formats
        "csv",
        "configparser",
        "toml",
        "json",
        "xml",
        // Network
        "http",
        "urllib",
        "ftplib",
        "poplib",
        "imaplib",
        "smtplib",
        "socket",
        "ssl",
        "email",
        "mime",
        // Concurrency
        "threading",
        "multiprocessing",
        "asyncio",
        "concurrent",
        // System
        "os",
        "io",
        "shutil",
        "tempfile",
        "glob",
        "fnmatch",
        "linecache",
        "shlex",
        "stat",
        // Database
        "sqlite3",
        "dbm",
        // Math
        "math",
        "cmath",
        "random",
        "statistics",
        // Testing
        "unittest",
        "doctest",
        // Debugging
        "pdb",
        "trace",
        "traceback",
    ];

    // Built-in functions
    pub const BUILTIN_FUNCTIONS: &[&str] = &[
        "abs",
        "all",
        "any",
        "ascii",
        "bin",
        "bool",
        "breakpoint",
        "bytearray",
        "bytes",
        "callable",
        "chr",
        "classmethod",
        "compile",
        "complex",
        "delattr",
        "dict",
        "dir",
        "divmod",
        "enumerate",
        "eval",
        "exec",
        "filter",
        "float",
        "format",
        "frozenset",
        "getattr",
        "globals",
        "hasattr",
        "hash",
        "help",
        "hex",
        "id",
        "input",
        "int",
        "isinstance",
        "issubclass",
        "iter",
        "len",
        "list",
        "locals",
        "map",
        "max",
        "memoryview",
        "min",
        "next",
        "object",
        "oct",
        "open",
        "ord",
        "pow",
        "print",
        "property",
        "range",
        "repr",
        "reversed",
        "round",
        "set",
        "setattr",
        "slice",
        "sorted",
        "staticmethod",
        "str",
        "sum",
        "super",
        "tuple",
        "type",
        "vars",
        "zip",
        "__import__",
    ];

    // Built-in types
    pub const BUILTIN_TYPES: &[&str] = &[
        "int",
        "float",
        "complex",
        "str",
        "list",
        "tuple",
        "range",
        "dict",
        "set",
        "frozenset",
        "bool",
        "bytes",
        "bytearray",
        "memoryview",
        "type",
        "object",
        "None",
        "Ellipsis",
        "NotImplemented",
    ];
}

// Generate simple containment check functions using macro
impl_list_checker!(
    PythonStdlibDetector,
    [
        (STDLIB_MODULES, is_stdlib_module),
        (BUILTIN_FUNCTIONS, is_builtin_function),
        (BUILTIN_TYPES, is_builtin_type),
    ]
);

// Generate is_stdlib_call using macro
impl_stdlib_call!(PythonStdlibDetector, {
    builtin_fn: BUILTIN_FUNCTIONS,
    module: STDLIB_MODULES,
});

// Generate is_stdlib_by_type using simple macro
impl_stdlib_by_type_simple!(
    PythonStdlibDetector,
    [
        DirectCall,
        InstanceMethodCall,
        StaticMethodCall,
        ChainedMethodCall,
        ConstructorCall,
        CallbackCall,
        GenericCall,
    ]
);

impl PythonStdlibDetector {
    /// Check if a type is a standard library type (builtin type)
    /// This provides a unified interface with other language detectors
    pub fn is_stdlib_type(name: &str) -> bool {
        Self::is_builtin_type(name)
    }

    /// Check if a name is a Python stdlib entity
    pub fn is_stdlib_name(name: &str) -> bool {
        Self::is_stdlib_module(name)
            || Self::is_builtin_function(name)
            || Self::is_builtin_type(name)
            || Self::get_category(name).is_some()
    }
}

// Generate get_category using macro
// This consolidates ~100 lines of boilerplate OnceLock + HashMap initialization
impl_stdlib_categorizer!(
    PythonStdlibDetector,
    [
        // Collection types
        (
            cce_types::stdlib_category::StdlibCategory::Collection,
            ["list", "dict", "set", "frozenset", "tuple", "range"]
        ),
        // I/O and system types
        (
            cce_types::stdlib_category::StdlibCategory::Io,
            [
                "open",
                "file",
                "input",
                "print",
                "FileIO",
                "StringIO",
                "BytesIO",
                "TextIOWrapper"
            ]
        ),
        // Concurrency types
        (
            cce_types::stdlib_category::StdlibCategory::Concurrency,
            [
                "Thread",
                "threading",
                "multiprocessing",
                "asyncio",
                "concurrent"
            ]
        ),
        // String types
        (
            cce_types::stdlib_category::StdlibCategory::String,
            ["str", "bytes", "bytearray", "codecs"]
        ),
        // Numeric types
        (
            cce_types::stdlib_category::StdlibCategory::Numeric,
            [
                "int",
                "float",
                "complex",
                "math",
                "decimal",
                "fractions",
                "random"
            ]
        ),
        // Error types
        (
            cce_types::stdlib_category::StdlibCategory::Error,
            [
                "Exception",
                "BaseException",
                "ValueError",
                "TypeError",
                "RuntimeError",
                "KeyError",
                "IndexError",
                "StopIteration"
            ]
        ),
        // Utility types
        (
            cce_types::stdlib_category::StdlibCategory::Utility,
            [
                "object",
                "type",
                "classmethod",
                "staticmethod",
                "property",
                "enum"
            ]
        ),
    ]
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stdlib_module() {
        assert!(PythonStdlibDetector::is_stdlib_module("os"));
        assert!(PythonStdlibDetector::is_stdlib_module("json"));
        assert!(!PythonStdlibDetector::is_stdlib_module("mymodule"));
    }

    #[test]
    fn test_is_builtin_function() {
        assert!(PythonStdlibDetector::is_builtin_function("print"));
        assert!(PythonStdlibDetector::is_builtin_function("len"));
        assert!(!PythonStdlibDetector::is_builtin_function("my_function"));
    }

    #[test]
    fn test_is_stdlib_call() {
        assert!(PythonStdlibDetector::is_stdlib_call("print"));
        assert!(PythonStdlibDetector::is_stdlib_call("os.path.join"));
        assert!(!PythonStdlibDetector::is_stdlib_call("custom_function"));
    }

    #[test]
    fn test_get_category() {
        use cce_types::stdlib_category::StdlibCategory;

        assert_eq!(
            PythonStdlibDetector::get_category("list"),
            Some(StdlibCategory::Collection)
        );
        assert_eq!(
            PythonStdlibDetector::get_category("print"),
            Some(StdlibCategory::Io)
        );
        assert_eq!(
            PythonStdlibDetector::get_category("threading"),
            Some(StdlibCategory::Concurrency)
        );
        assert_eq!(
            PythonStdlibDetector::get_category("str"),
            Some(StdlibCategory::String)
        );
    }
}
