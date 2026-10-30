// Rust Standard Library Detector
// Handles detection of Rust standard library entities

pub struct RustStdlibDetector;

impl RustStdlibDetector {
    // Crates
    pub const STDLIB_CRATES: &[&str] = &["std", "core", "alloc", "proc_macro"];

    // Common stdlib types (merged from both stdlib and symbol_classifier)
    pub const STDLIB_TYPES: &[&str] = &[
        // Primitives
        "u8",
        "u16",
        "u32",
        "u64",
        "u128",
        "usize",
        "i8",
        "i16",
        "i32",
        "i64",
        "i128",
        "isize",
        "f32",
        "f64",
        "bool",
        "char",
        "str",
        // Option/Result
        "Option",
        "Result",
        // String types
        "String",
        "Cow",
        "OsStr",
        "OsString",
        "CString",
        "CStr",
        // Collections
        "Vec",
        "VecDeque",
        "LinkedList",
        "BTreeMap",
        "BTreeSet",
        "HashMap",
        "HashSet",
        "BinaryHeap",
        // Smart pointers
        "Box",
        "Rc",
        "Arc",
        "Weak",
        "Cell",
        "RefCell",
        "UnsafeCell",
        "Pin",
        "NonNull",
        // Sync primitives
        "Mutex",
        "RwLock",
        "Condvar",
        // Range types
        "Range",
        "RangeFrom",
        "RangeTo",
        "RangeFull",
        "RangeInclusive",
        "RangeToInclusive",
        "Bound",
        "Included",
        "Excluded",
        "Unbounded",
        "RangeBounds",
        // Time types
        "Duration",
        "Instant",
        "SystemTime",
        // I/O types
        "File",
        "BufReader",
        "BufWriter",
        "Cursor",
        "Stdin",
        "Stdout",
        "Stderr",
        "Read",
        "Write",
        "BufRead",
        "Seek",
        "SeekFrom",
        "Error",
        "ErrorKind",
        // Path types
        "Path",
        "PathBuf",
        "Prefix",
        "Component",
        "Components",
        "Iter",
        // Process types
        "Command",
        "Child",
        "ExitCode",
        "ExitStatus",
        // Thread types
        "Thread",
        "JoinHandle",
        "ThreadId",
        "LocalKey",
        "Scope",
        "ScopedJoinHandle",
        // Net types
        "TcpStream",
        "TcpListener",
        "UdpSocket",
        "SocketAddr",
        "SocketAddrV4",
        "SocketAddrV6",
        "Ipv4Addr",
        "Ipv6Addr",
        "AddrParseError",
        "InterfaceAddress",
        // Async types
        "Future",
        "Poll",
        "Context",
        "Waker",
        "Unpin",
        "RawWaker",
        "RawWakerVTable",
        "Wake",
        // Marker traits (also used as types in some contexts)
        "Send",
        "Sync",
        "Sized",
        "Unsize",
        "Copy",
        "Clone",
        // Atomic types
        "AtomicBool",
        "AtomicI8",
        "AtomicI16",
        "AtomicI32",
        "AtomicI64",
        "AtomicIsize",
        "AtomicU8",
        "AtomicU16",
        "AtomicU32",
        "AtomicU64",
        "AtomicUsize",
        "AtomicPtr",
        "Ordering",
        // Function types
        "Fn",
        "FnMut",
        "FnOnce",
        // Iterator types
        "Iterator",
        "IntoIterator",
        "FromIterator",
        "DoubleEndedIterator",
        "ExactSizeIterator",
        "FusedIterator",
        "Extend",
        // std::num types
        "Wrapping",
        "Saturating",
        "NonZeroI8",
        "NonZeroI16",
        "NonZeroI32",
        "NonZeroI64",
        "NonZeroI128",
        "NonZeroIsize",
        "NonZeroU8",
        "NonZeroU16",
        "NonZeroU32",
        "NonZeroU64",
        "NonZeroU128",
        "NonZeroUsize",
        "ParseIntError",
        "ParseFloatError",
        "TryFromIntError",
        // std::panic types
        "PanicInfo",
        "Location",
        "AssertUnwindSafe",
        "RefUnwindSafe",
        "UnwindSafe",
        // std::ffi types
        "VaList",
        "VaListImpl",
        "c_void",
        // std::ptr types
        "DynMetadata",
        "Pointee",
        // std::ops types
        "ControlFlow",
        // Once types (Rust 1.70+)
        "OnceCell",
        "OnceLock",
        "LazyCell",
        "LazyLock",
        // Other types
        "Any",
        "TypeId",
    ];

    // Common stdlib traits (merged from both stdlib and symbol_classifier)
    pub const STDLIB_TRAITS: &[&str] = &[
        // Core traits
        "Send",
        "Sync",
        "Sized",
        "Unsize",
        "Copy",
        "Clone",
        "Drop",
        "Deref",
        "DerefMut",
        "Borrow",
        "BorrowMut",
        "ToOwned",
        "AsRef",
        "AsMut",
        "From",
        "Into",
        "FromStr",
        "ToString",
        "TryFrom",
        "TryInto",
        "Default",
        "Eq",
        "PartialEq",
        "Ord",
        "PartialOrd",
        "Hash",
        // Iterator traits
        "Iterator",
        "IntoIterator",
        "FromIterator",
        "Extend",
        "ExactSizeIterator",
        "DoubleEndedIterator",
        "FusedIterator",
        "TrustedLen",
        "Product",
        "Sum",
        "Step",
        "TrustedStep",
        // Function traits
        "Fn",
        "FnMut",
        "FnOnce",
        // I/O traits
        "Read",
        "Write",
        "BufRead",
        "Seek",
        // Error traits
        "Error",
        "Try",
        "FromResidual",
        "Residual",
        // Display traits
        "Debug",
        "Display",
        "Binary",
        "Octal",
        "LowerHex",
        "UpperHex",
        "LowerExp",
        "UpperExp",
        "Pointer",
        "Write",
        // Index traits
        "Index",
        "IndexMut",
        // Arithmetic traits
        "Add",
        "Sub",
        "Mul",
        "Div",
        "Rem",
        "Neg",
        "Not",
        "BitAnd",
        "BitOr",
        "BitXor",
        "Shl",
        "Shr",
        "AddAssign",
        "SubAssign",
        "MulAssign",
        "DivAssign",
        "RemAssign",
        "BitAndAssign",
        "BitOrAssign",
        "BitXorAssign",
        "ShlAssign",
        "ShrAssign",
        // Special traits
        "CoerceUnsized",
        "DispatchFromDyn",
        // Other traits
        "Any",
    ];

    // Common stdlib macros (merged from both stdlib and symbol_classifier)
    pub const STDLIB_MACROS: &[&str] = &[
        // Output macros
        "println",
        "print",
        "eprintln",
        "eprint",
        // Formatting macros
        "format",
        "format_args",
        "concat",
        "stringify",
        // Collection macros
        "vec",
        "concat_idents",
        // Include macros
        "include",
        "include_str",
        "include_bytes",
        // Debug macros
        "assert",
        "assert_eq",
        "assert_ne",
        "debug_assert",
        "debug_assert_eq",
        "debug_assert_ne",
        "unreachable",
        "unimplemented",
        "todo",
        "compile_error",
        // Panic macro
        "panic",
        // Info macros
        "file",
        "line",
        "column",
        "module_path",
        "env",
        "option_env",
        // Configuration macros
        "cfg",
        "cfg_attr",
        // Other macros
        "thread_local",
        "write",
        "writeln",
        // Try macro
        "try",
        // Derive macros (commonly used)
        "derive",
        "Clone",
        "Copy",
        "Debug",
        "Default",
        "Eq",
        "PartialEq",
        "Ord",
        "PartialOrd",
        "Hash",
        "RustcEncodable",
        "RustcDecodable",
        // Pattern macros
        "matches",
    ];

    // Standard library modules (from symbol_classifier)
    pub const STDLIB_MODULES: &[&str] = &[
        "std",
        "core",
        "alloc",
        "std::collections",
        "std::sync",
        "std::thread",
        "std::time",
        "std::io",
        "std::fs",
        "std::path",
        "std::process",
        "std::env",
        "std::net",
        "std::os",
        "std::ffi",
        "std::mem",
        "std::ptr",
        "std::num",
        "std::char",
        "std::str",
        "std::slice",
        "std::array",
        "std::iter",
        "std::option",
        "std::result",
        "std::any",
        "std::borrow",
        "std::convert",
        "std::default",
        "std::error",
        "std::fmt",
        "std::hash",
        "std::ops",
        "std::marker",
        "std::cmp",
        "std::panic",
        "std::prelude",
        "std::sync::atomic",
        "std::sync::mpsc",
        "std::sync::once",
        "std::io::prelude",
        "std::task",
        "std::future",
        "std::pin",
        "std::os::unix",
        "std::os::windows",
        "std::os::raw",
        "std::os::fd",
    ];
}

// Generate simple containment check functions using macro
impl_list_checker!(
    RustStdlibDetector,
    [
        (STDLIB_CRATES, is_stdlib_crate),
        (STDLIB_TYPES, is_stdlib_type),
        (STDLIB_TRAITS, is_stdlib_trait),
        (STDLIB_MACROS, is_stdlib_macro),
        (STDLIB_MODULES, is_stdlib_module),
    ]
);

impl RustStdlibDetector {
    /// Check if a qualified path is from stdlib
    pub fn is_stdlib_path(path: &str) -> bool {
        let first_component = path.split("::").next().unwrap_or("");
        Self::is_stdlib_crate(first_component)
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for direct macro or function call
        if Self::is_stdlib_macro(call_name) {
            return true;
        }

        // Check for qualified path
        if call_name.contains("::") {
            // First check for stdlib type method calls (e.g., Vec::new)
            for std_type in Self::STDLIB_TYPES {
                if call_name.starts_with(std_type)
                    && call_name.chars().nth(std_type.len()) == Some(':')
                {
                    return true;
                }
            }
            // Then check for stdlib crate paths (e.g., std::fs::File)
            return Self::is_stdlib_path(call_name);
        }

        false
    }
}

// Generate get_category using macro
// This consolidates ~130 lines of boilerplate OnceLock + HashMap initialization
impl_stdlib_categorizer!(
    RustStdlibDetector,
    [
        // Collection types
        (
            cce_types::stdlib_category::StdlibCategory::Collection,
            [
                "Vec",
                "VecDeque",
                "LinkedList",
                "BTreeMap",
                "BTreeSet",
                "HashMap",
                "HashSet",
                "BinaryHeap",
            ]
        ),
        // I/O types
        (
            cce_types::stdlib_category::StdlibCategory::Io,
            [
                "File",
                "BufReader",
                "BufWriter",
                "Cursor",
                "Stdin",
                "Stdout",
                "Stderr",
                "Read",
                "Write",
                "BufRead",
                "Seek",
                "SeekFrom",
            ]
        ),
        // Concurrency types
        (
            cce_types::stdlib_category::StdlibCategory::Concurrency,
            [
                "Thread",
                "Mutex",
                "RwLock",
                "Arc",
                "Rc",
                "Barrier",
                "Condvar",
                "AtomicBool",
                "AtomicI8",
                "AtomicI16",
                "AtomicI32",
                "AtomicI64",
                "AtomicIsize",
                "AtomicU8",
                "AtomicU16",
                "AtomicU32",
                "AtomicU64",
                "AtomicUsize",
                "AtomicPtr",
                "Ordering",
            ]
        ),
        // String types
        (
            cce_types::stdlib_category::StdlibCategory::String,
            [
                "String", "str", "Cow", "OsStr", "OsString", "CString", "CStr"
            ]
        ),
        // Numeric types
        (
            cce_types::stdlib_category::StdlibCategory::Numeric,
            [
                "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128",
                "usize", "f32", "f64",
            ]
        ),
        // Utility types
        (
            cce_types::stdlib_category::StdlibCategory::Utility,
            [
                "Option",
                "Result",
                "Box",
                "Cell",
                "RefCell",
                "UnsafeCell",
                "Pin",
                "NonNull",
                "Weak",
                "Duration",
                "Instant",
                "SystemTime",
            ]
        ),
        // Error types
        (
            cce_types::stdlib_category::StdlibCategory::Error,
            ["Error", "ErrorKind"]
        ),
        // Macro types
        (
            cce_types::stdlib_category::StdlibCategory::Macro,
            [
                "println", "print", "eprintln", "eprint", "format", "vec", "assert", "panic",
            ]
        ),
        // Trait types
        (
            cce_types::stdlib_category::StdlibCategory::Trait,
            [
                "Iterator", "Clone", "Copy", "Display", "Debug", "Send", "Sync",
            ]
        ),
    ]
);

impl RustStdlibDetector {
    /// Standard library associated constants.
    pub const STDLIB_CONSTANTS: &[&str] = &[
        "u8::MAX",
        "u16::MAX",
        "u32::MAX",
        "u64::MAX",
        "i32::MAX",
        "i32::MIN",
        "i64::MAX",
        "usize::MAX",
        "f32::INFINITY",
        "f64::INFINITY",
        "f64::NAN",
    ];

    /// Check if a name is a standard library constant.
    pub fn is_stdlib_constant(name: &str) -> bool {
        Self::STDLIB_CONSTANTS.contains(&name)
    }
}

// Rust is the one language whose call shape changes the answer: a macro
// invocation consults the macro table and a generic call consults the base
// type, while every other call shape resolves through the path/type check.
impl_stdlib_by_type_calls!(RustStdlibDetector, |name, relation_type| {
    use cce_types::relation::RelationType;

    match relation_type {
        RelationType::MacroCall => Self::STDLIB_MACROS.contains(&name.trim_end_matches('!')),
        RelationType::GenericCall => name
            .split('<')
            .next()
            .and_then(|base| base.split("::").next())
            .is_some_and(|base| Self::STDLIB_TYPES.contains(&base)),
        _ => Self::is_stdlib_call(name),
    }
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stdlib_crate() {
        assert!(RustStdlibDetector::is_stdlib_crate("std"));
        assert!(RustStdlibDetector::is_stdlib_crate("core"));
        assert!(!RustStdlibDetector::is_stdlib_crate("serde"));
    }

    #[test]
    fn test_is_stdlib_type() {
        assert!(RustStdlibDetector::is_stdlib_type("Vec"));
        assert!(RustStdlibDetector::is_stdlib_type("Option"));
        assert!(RustStdlibDetector::is_stdlib_type("HashMap"));
        assert!(RustStdlibDetector::is_stdlib_type("Arc"));
        assert!(!RustStdlibDetector::is_stdlib_type("MyType"));
    }

    #[test]
    fn test_is_stdlib_trait() {
        assert!(RustStdlibDetector::is_stdlib_trait("Display"));
        assert!(RustStdlibDetector::is_stdlib_trait("Debug"));
        assert!(RustStdlibDetector::is_stdlib_trait("Iterator"));
        assert!(!RustStdlibDetector::is_stdlib_trait("MyTrait"));
    }

    #[test]
    fn test_is_stdlib_macro() {
        assert!(RustStdlibDetector::is_stdlib_macro("println"));
        assert!(RustStdlibDetector::is_stdlib_macro("vec"));
        assert!(RustStdlibDetector::is_stdlib_macro("format"));
        assert!(!RustStdlibDetector::is_stdlib_macro("my_macro"));
    }

    #[test]
    fn test_is_stdlib_module() {
        assert!(RustStdlibDetector::is_stdlib_module("std::collections"));
        assert!(RustStdlibDetector::is_stdlib_module("std::sync"));
        assert!(!RustStdlibDetector::is_stdlib_module("my_module"));
    }

    #[test]
    fn test_is_stdlib_path() {
        assert!(RustStdlibDetector::is_stdlib_path(
            "std::collections::HashMap"
        ));
        assert!(RustStdlibDetector::is_stdlib_path("core::fmt::Display"));
        assert!(!RustStdlibDetector::is_stdlib_path("serde::Serialize"));
    }

    #[test]
    fn test_is_stdlib_call() {
        assert!(RustStdlibDetector::is_stdlib_call("println"));
        assert!(RustStdlibDetector::is_stdlib_call("Vec::new"));
        assert!(RustStdlibDetector::is_stdlib_call("std::fs::File"));
        assert!(!RustStdlibDetector::is_stdlib_call("custom_function"));
    }

    #[test]
    fn test_is_stdlib_by_type_macro_call() {
        use cce_types::relation::RelationType;

        // Macro calls should be detected in STDLIB_MACROS
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "println!",
            &RelationType::MacroCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "format!",
            &RelationType::MacroCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "vec!",
            &RelationType::MacroCall
        ));
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "custom_macro!",
            &RelationType::MacroCall
        ));
    }

    #[test]
    fn test_is_stdlib_by_type_direct_call() {
        use cce_types::relation::RelationType;

        // Direct calls to stdlib macros (without '!')
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "println",
            &RelationType::DirectCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "format",
            &RelationType::DirectCall
        ));

        // Direct calls to stdlib types
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Vec::new",
            &RelationType::DirectCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "HashMap::new",
            &RelationType::DirectCall
        ));

        // Direct calls to stdlib paths
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "std::fs::read",
            &RelationType::DirectCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "core::mem::replace",
            &RelationType::DirectCall
        ));

        // Non-stdlib calls
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "custom_function",
            &RelationType::DirectCall
        ));
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "serde::Serialize",
            &RelationType::DirectCall
        ));
    }

    #[test]
    fn test_is_stdlib_by_type_method_call() {
        use cce_types::relation::RelationType;

        // Instance method calls on stdlib types
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Vec::push",
            &RelationType::InstanceMethodCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "String::len",
            &RelationType::InstanceMethodCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Option::unwrap",
            &RelationType::InstanceMethodCall
        ));

        // Static method calls on stdlib types
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Vec::new",
            &RelationType::StaticMethodCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "HashMap::new",
            &RelationType::StaticMethodCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Option::Some",
            &RelationType::StaticMethodCall
        ));

        // Chained method calls on stdlib types
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Vec::push",
            &RelationType::ChainedMethodCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "String::chars",
            &RelationType::ChainedMethodCall
        ));

        // Non-stdlib method calls
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "MyStruct::method",
            &RelationType::InstanceMethodCall
        ));
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "custom::Type::method",
            &RelationType::StaticMethodCall
        ));
    }

    #[test]
    fn test_is_stdlib_by_type_constructor_call() {
        use cce_types::relation::RelationType;

        // Constructor calls to stdlib types
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Vec::new()",
            &RelationType::ConstructorCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "HashMap::new()",
            &RelationType::ConstructorCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "String::new()",
            &RelationType::ConstructorCall
        ));

        // Constructor calls with arguments
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Vec::with_capacity(10)",
            &RelationType::ConstructorCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "String::from(\"hello\")",
            &RelationType::ConstructorCall
        ));

        // Non-stdlib constructor calls
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "MyStruct::new()",
            &RelationType::ConstructorCall
        ));
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "custom::Type::new()",
            &RelationType::ConstructorCall
        ));
    }

    #[test]
    fn test_is_stdlib_by_type_generic_call() {
        use cce_types::relation::RelationType;

        // Generic calls to stdlib types
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Vec<i32>",
            &RelationType::GenericCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "HashMap<String, i32>",
            &RelationType::GenericCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Option<&str>",
            &RelationType::GenericCall
        ));

        // Nested generics
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Vec<Vec<i32>>",
            &RelationType::GenericCall
        ));
        assert!(RustStdlibDetector::is_stdlib_by_type(
            "Result<i32, String>",
            &RelationType::GenericCall
        ));

        // Non-stdlib generic calls
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "MyStruct<i32>",
            &RelationType::GenericCall
        ));
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "custom::Type<String>",
            &RelationType::GenericCall
        ));
    }

    #[test]
    fn test_is_stdlib_by_type_other_relation_types() {
        use cce_types::relation::RelationType;

        // Other relation types should return false
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "Vec",
            &RelationType::IncludeLocal
        ));
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "Vec",
            &RelationType::ImportStandard
        ));
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "Vec",
            &RelationType::Inheritance
        ));
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "Vec",
            &RelationType::Implementation
        ));
        assert!(!RustStdlibDetector::is_stdlib_by_type(
            "Vec",
            &RelationType::TypeReference
        ));
    }

    #[test]
    fn test_is_stdlib_by_type_performance() {
        use cce_types::relation::RelationType;

        // Test that the method correctly identifies stdlib calls efficiently
        // These tests verify that the static dispatch works correctly

        // Macro call - should be O(1) lookup in STDLIB_MACROS
        let result = RustStdlibDetector::is_stdlib_by_type("println!", &RelationType::MacroCall);
        assert!(result);

        // Direct call - should be O(1) lookup in STDLIB_MACROS or STDLIB_TYPES
        let result = RustStdlibDetector::is_stdlib_by_type("Vec::new", &RelationType::DirectCall);
        assert!(result);

        // Method call - should be O(1) lookup in STDLIB_TYPES
        let result =
            RustStdlibDetector::is_stdlib_by_type("Vec::push", &RelationType::InstanceMethodCall);
        assert!(result);

        // Constructor call - should be O(1) lookup in STDLIB_TYPES
        let result =
            RustStdlibDetector::is_stdlib_by_type("Vec::new()", &RelationType::ConstructorCall);
        assert!(result);

        // Generic call - should be O(1) lookup in STDLIB_TYPES
        let result = RustStdlibDetector::is_stdlib_by_type("Vec<i32>", &RelationType::GenericCall);
        assert!(result);
    }
}
