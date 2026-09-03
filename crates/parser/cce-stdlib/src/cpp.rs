// C++ Standard Library Detector
// Handles detection of C++ standard library entities

pub struct CppStdlibDetector;

impl CppStdlibDetector {
    // Standard library headers
    pub const STDLIB_HEADERS: &[&str] = &[
        // C headers
        "assert",
        "ctype",
        "errno",
        "float",
        "limits",
        "locale",
        "math",
        "setjmp",
        "signal",
        "stdarg",
        "stddef",
        "stdio",
        "stdlib",
        "string",
        "time",
        "wchar",
        "wctype",
        // C++ headers
        "algorithm",
        "array",
        "atomic",
        "bit",
        "bitset",
        "charconv",
        "chrono",
        "codecvt",
        "compare",
        "complex",
        "concepts",
        "condition_variable",
        "coroutine",
        "csetjmp",
        "csignal",
        "cstdarg",
        "cstddef",
        "cstdint",
        "cstdio",
        "cstdlib",
        "cstring",
        "ctime",
        "cuchar",
        "cwchar",
        "cwctype",
        "deque",
        "exception",
        "execution",
        "expected",
        "filesystem",
        "format",
        "forward_list",
        "fstream",
        "functional",
        "future",
        "initializer_list",
        "iomanip",
        "ios",
        "iosfwd",
        "iostream",
        "istream",
        "iterator",
        "limits",
        "list",
        "locale",
        "map",
        "memory",
        "memory_resource",
        "mutex",
        "new",
        "numbers",
        "numeric",
        "optional",
        "ostream",
        "queue",
        "random",
        "ranges",
        "ratio",
        "regex",
        "scoped_allocator",
        "set",
        "shared_mutex",
        "source_location",
        "span",
        "sstream",
        "stack",
        "stdexcept",
        "stop_token",
        "streambuf",
        "string",
        "string_view",
        "strstream",
        "syncstream",
        "system_error",
        "thread",
        "tuple",
        "type_traits",
        "typeindex",
        "typeinfo",
        "unordered_map",
        "unordered_set",
        "utility",
        "valarray",
        "variant",
        "vector",
        "version",
        // C++17
        "any",
        "optional",
        "variant",
        "string_view",
        "filesystem",
        "memory_resource",
        "execution",
        // C++20
        "barrier",
        "latch",
        "semaphore",
        "stop_token",
        "syncstream",
        "format",
        "ranges",
        "span",
        "bit",
        "compare",
        "concepts",
        "coroutine",
        "numbers",
        // C++23
        "generator",
        "print",
        "stdfloat",
        "expected",
        "flat_map",
        "flat_set",
        "mdspan",
    ];

    // Common stdlib types
    pub const STDLIB_TYPES: &[&str] = &[
        // Containers
        "vector",
        "deque",
        "list",
        "forward_list",
        "array",
        "stack",
        "queue",
        "priority_queue",
        "set",
        "multiset",
        "map",
        "multimap",
        "unordered_set",
        "unordered_map",
        "unordered_multiset",
        "unordered_multimap",
        // Smart pointers
        "unique_ptr",
        "shared_ptr",
        "weak_ptr",
        "auto_ptr",
        // Strings
        "string",
        "wstring",
        "u16string",
        "u32string",
        "string_view",
        "basic_string",
        "basic_string_view",
        // Streams
        "istream",
        "ostream",
        "iostream",
        "ifstream",
        "ofstream",
        "fstream",
        "stringstream",
        "istringstream",
        "ostringstream",
        // Iterators
        "iterator",
        "input_iterator_tag",
        "output_iterator_tag",
        "forward_iterator_tag",
        "bidirectional_iterator_tag",
        "random_access_iterator_tag",
        // Other
        "pair",
        "tuple",
        "optional",
        "variant",
        "any",
        "function",
        "bind",
        "reference_wrapper",
        "span",
        "array",
        "bitset",
        "complex",
        "valarray",
        "ratio",
        "exception",
        "runtime_error",
        "logic_error",
        "invalid_argument",
        "domain_error",
        "length_error",
        "out_of_range",
        "range_error",
        "overflow_error",
        "underflow_error",
        "future",
        "promise",
        "packaged_task",
        "shared_future",
        "thread",
        "mutex",
        "lock_guard",
        "unique_lock",
        "condition_variable",
        "atomic",
        "atomic_flag",
        "chrono::duration",
        "chrono::time_point",
        // Type traits (partial list)
        "is_same",
        "is_base_of",
        "is_convertible",
        "is_integral",
        "is_floating_point",
        "is_arithmetic",
        "is_pointer",
        "is_reference",
        "is_const",
        "is_volatile",
        "is_trivial",
        "is_standard_layout",
        "is_pod",
        "is_empty",
        "is_polymorphic",
        "is_abstract",
        "is_final",
        "is_signed",
        "is_unsigned",
        "is_constructible",
        "is_default_constructible",
        "is_copy_constructible",
        "is_move_constructible",
        "is_assignable",
        "is_copy_assignable",
        "is_move_assignable",
        "is_destructible",
        "is_trivially_constructible",
        "is_trivially_default_constructible",
        "is_trivially_copy_constructible",
        "is_trivially_move_constructible",
        "is_trivially_assignable",
        "is_trivially_copy_assignable",
        "is_trivially_move_assignable",
        "is_trivially_destructible",
        "is_nothrow_constructible",
        "is_nothrow_default_constructible",
        "is_nothrow_copy_constructible",
        "is_nothrow_move_constructible",
        "is_nothrow_assignable",
        "is_nothrow_copy_assignable",
        "is_nothrow_move_assignable",
        "is_nothrow_destructible",
        "has_virtual_destructor",
        "alignment_of",
        "rank",
        "extent",
        "remove_const",
        "remove_volatile",
        "remove_cv",
        "add_const",
        "add_volatile",
        "add_cv",
        "remove_reference",
        "add_lvalue_reference",
        "add_rvalue_reference",
        "remove_pointer",
        "add_pointer",
        "make_signed",
        "make_unsigned",
        "remove_extent",
        "remove_all_extents",
        "decay",
        "enable_if",
        "conditional",
        "common_type",
        "underlying_type",
        "result_of",
        "invoke_result",
        "is_invocable",
        "is_invocable_r",
        "is_nothrow_invocable",
        "is_nothrow_invocable_r",
        // C++20 concepts
        "same_as",
        "derived_from",
        "convertible_to",
        "common_reference_with",
        "common_with",
        "integral",
        "signed_integral",
        "unsigned_integral",
        "floating_point",
        "assignable_from",
        "swappable",
        "swappable_with",
        "destructible",
        "constructible_from",
        "default_initializable",
        "move_constructible",
        "copy_constructible",
        "equality_comparable",
        "totally_ordered",
        "movable",
        "copyable",
        "semiregular",
        "regular",
        "invocable",
        "predicate",
        "relation",
        "strict_weak_order",
    ];

    // Common stdlib functions
    pub const STDLIB_FUNCTIONS: &[&str] = &[
        // std:: algorithms
        "sort",
        "find",
        "for_each",
        "transform",
        "copy",
        "fill",
        "accumulate",
        "count",
        "min",
        "max",
        "min_element",
        "max_element",
        "swap",
        "move",
        "forward",
        // More algorithms
        "find_if",
        "remove",
        "remove_if",
        "replace",
        "replace_if",
        "reverse",
        "rotate",
        "shuffle",
        "unique",
        "partition",
        "stable_partition",
        "nth_element",
        "partial_sort",
        "stable_sort",
        "binary_search",
        "lower_bound",
        "upper_bound",
        "equal_range",
        "merge",
        "inplace_merge",
        "includes",
        "set_union",
        "set_intersection",
        "set_difference",
        "set_symmetric_difference",
        // Numeric algorithms
        "inner_product",
        "adjacent_difference",
        "partial_sum",
        "iota",
        // Memory management
        "make_unique",
        "make_shared",
        "move",
        "forward",
        "allocator",
        "allocator_traits",
        "construct_at",
        "destroy_at",
        "uninitialized_copy",
        "uninitialized_fill",
        // Utility functions
        "exchange",
        "forward_like",
        "to_underlying",
        "in_range",
        "cmp_equal",
        "cmp_not_equal",
        "cmp_less",
        "cmp_greater",
        // std:: iostream
        "cout",
        "cin",
        "cerr",
        "clog",
        "endl",
        "flush",
        "setw",
        "setprecision",
        // std:: string
        "stoi",
        "stol",
        "stoll",
        "stof",
        "stod",
        "stold",
        "to_string",
        "to_wstring",
        // std:: thread
        "thread",
        "async",
        "launch",
        "this_thread",
        // std:: chrono
        "duration_cast",
        "time_point_cast",
        // std:: filesystem
        "current_path",
        "absolute",
        "canonical",
        "relative",
        "copy",
        "create_directory",
        "create_directories",
        "remove",
        "remove_all",
        "rename",
        "file_size",
        "exists",
        "is_directory",
        "is_regular_file",
        "is_symlink",
        // std:: regex
        "regex_match",
        "regex_search",
        "regex_replace",
        // std:: random
        "default_random_engine",
        "random_device",
        "uniform_int_distribution",
        "uniform_real_distribution",
        "normal_distribution",
        "bernoulli_distribution",
        // std:: atomic
        "atomic_load",
        "atomic_store",
        "atomic_exchange",
        "atomic_compare_exchange_weak",
        "atomic_compare_exchange_strong",
        "atomic_fetch_add",
        "atomic_fetch_sub",
        "atomic_fetch_and",
        "atomic_fetch_or",
        "atomic_fetch_xor",
    ];

    // Standard library macros and constants
    pub const STDLIB_MACROS: &[&str] = &[
        // Common macros
        "NULL",
        "nullptr",
        "EOF",
        "EXIT_SUCCESS",
        "EXIT_FAILURE",
        "RAND_MAX",
        "BUFSIZ",
        "FOPEN_MAX",
        "FILENAME_MAX",
        "L_tmpnam",
        "TMP_MAX",
        "SEEK_SET",
        "SEEK_CUR",
        "SEEK_END",
        "stdin",
        "stdout",
        "stderr",
        // Limits
        "CHAR_BIT",
        "SCHAR_MIN",
        "SCHAR_MAX",
        "UCHAR_MAX",
        "CHAR_MIN",
        "CHAR_MAX",
        "MB_LEN_MAX",
        "SHRT_MIN",
        "SHRT_MAX",
        "USHRT_MAX",
        "INT_MIN",
        "INT_MAX",
        "UINT_MAX",
        "LONG_MIN",
        "LONG_MAX",
        "ULONG_MAX",
        "LLONG_MIN",
        "LLONG_MAX",
        "ULLONG_MAX",
        // Float limits
        "FLT_RADIX",
        "FLT_MANT_DIG",
        "DBL_MANT_DIG",
        "LDBL_MANT_DIG",
        "FLT_DIG",
        "DBL_DIG",
        "LDBL_DIG",
        "FLT_MIN_EXP",
        "DBL_MIN_EXP",
        "LDBL_MIN_EXP",
        "FLT_MIN_10_EXP",
        "DBL_MIN_10_EXP",
        "LDBL_MIN_10_EXP",
        "FLT_MAX_EXP",
        "DBL_MAX_EXP",
        "LDBL_MAX_EXP",
        "FLT_MAX_10_EXP",
        "DBL_MAX_10_EXP",
        "LDBL_MAX_10_EXP",
        "FLT_MAX",
        "DBL_MAX",
        "LDBL_MAX",
        "FLT_EPSILON",
        "DBL_EPSILON",
        "LDBL_EPSILON",
        "FLT_MIN",
        "DBL_MIN",
        "LDBL_MIN",
        // Math constants
        "HUGE_VAL",
        "HUGE_VALF",
        "HUGE_VALL",
        "INFINITY",
        "NAN",
        "FP_NAN",
        "FP_INFINITE",
        "FP_ZERO",
        "FP_SUBNORMAL",
        "FP_NORMAL",
        "math_errhandling",
        "MATH_ERRNO",
        "MATH_ERREXCEPT",
        // Error codes
        "errno",
        "EDOM",
        "ERANGE",
        "EILSEQ",
        // Signal macros
        "SIG_DFL",
        "SIG_IGN",
        "SIG_ERR",
        "SIGABRT",
        "SIGFPE",
        "SIGILL",
        "SIGINT",
        "SIGSEGV",
        "SIGTERM",
        // C++ specific
        "alignas",
        "alignof",
        "noexcept",
        "constexpr",
        "consteval",
        "constinit",
        "decltype",
        "static_assert",
        "thread_local",
        "override",
        "final",
        "export",
        "import",
        "module",
    ];

    // Standard library constants (subset of macros that represent constant values)
    pub const STDLIB_CONSTANTS: &[&str] = &[
        // Common constants
        "NULL",
        "nullptr",
        "EOF",
        "EXIT_SUCCESS",
        "EXIT_FAILURE",
        "RAND_MAX",
        "BUFSIZ",
        "FOPEN_MAX",
        "FILENAME_MAX",
        "L_tmpnam",
        "TMP_MAX",
        "SEEK_SET",
        "SEEK_CUR",
        "SEEK_END",
        "stdin",
        "stdout",
        "stderr",
        // Limits
        "CHAR_BIT",
        "SCHAR_MIN",
        "SCHAR_MAX",
        "UCHAR_MAX",
        "CHAR_MIN",
        "CHAR_MAX",
        "MB_LEN_MAX",
        "SHRT_MIN",
        "SHRT_MAX",
        "USHRT_MAX",
        "INT_MIN",
        "INT_MAX",
        "UINT_MAX",
        "LONG_MIN",
        "LONG_MAX",
        "ULONG_MAX",
        "LLONG_MIN",
        "LLONG_MAX",
        "ULLONG_MAX",
        // Float limits
        "FLT_RADIX",
        "FLT_MANT_DIG",
        "DBL_MANT_DIG",
        "LDBL_MANT_DIG",
        "FLT_DIG",
        "DBL_DIG",
        "LDBL_DIG",
        "FLT_MIN_EXP",
        "DBL_MIN_EXP",
        "LDBL_MIN_EXP",
        "FLT_MIN_10_EXP",
        "DBL_MIN_10_EXP",
        "LDBL_MIN_10_EXP",
        "FLT_MAX_EXP",
        "DBL_MAX_EXP",
        "LDBL_MAX_EXP",
        "FLT_MAX_10_EXP",
        "DBL_MAX_10_EXP",
        "LDBL_MAX_10_EXP",
        "FLT_MAX",
        "DBL_MAX",
        "LDBL_MAX",
        "FLT_EPSILON",
        "DBL_EPSILON",
        "LDBL_EPSILON",
        "FLT_MIN",
        "DBL_MIN",
        "LDBL_MIN",
        // Math constants
        "HUGE_VAL",
        "HUGE_VALF",
        "HUGE_VALL",
        "INFINITY",
        "NAN",
        "FP_NAN",
        "FP_INFINITE",
        "FP_ZERO",
        "FP_SUBNORMAL",
        "FP_NORMAL",
        "math_errhandling",
        "MATH_ERRNO",
        "MATH_ERREXCEPT",
        // Error codes
        "EDOM",
        "ERANGE",
        "EILSEQ",
        // Signal constants
        "SIG_DFL",
        "SIG_IGN",
        "SIG_ERR",
        "SIGABRT",
        "SIGFPE",
        "SIGILL",
        "SIGINT",
        "SIGSEGV",
        "SIGTERM",
    ];

    pub fn is_stdlib_header(header: &str) -> bool {
        // Remove angle brackets or quotes
        let clean_header = header
            .trim_start_matches('<')
            .trim_start_matches('"')
            .trim_end_matches('>')
            .trim_end_matches('"');

        Self::STDLIB_HEADERS.contains(&clean_header)
    }

    pub fn is_stdlib_type(name: &str) -> bool {
        // Handle std:: prefix
        let clean_name = name.strip_prefix("std::").unwrap_or(name);
        Self::STDLIB_TYPES.contains(&clean_name)
    }

    pub fn is_stdlib_macro(name: &str) -> bool {
        Self::STDLIB_MACROS.contains(&name)
    }

    pub fn is_stdlib_constant(name: &str) -> bool {
        Self::STDLIB_CONSTANTS.contains(&name)
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for std:: prefix
        if call_name.starts_with("std::") {
            let clean_name = call_name.strip_prefix("std::").unwrap_or("");
            return Self::STDLIB_FUNCTIONS.contains(&clean_name)
                || Self::STDLIB_TYPES.contains(&clean_name);
        }

        // Check for macros and constants (no std:: prefix)
        if Self::is_stdlib_macro(call_name) {
            return true;
        }

        // Check for C standard library functions (available in C++)
        // Note: We can't directly reference CStdlibDetector here because it would create a circular dependency
        // Instead, we'll check common C functions directly
        if Self::is_c_stdlib_function(call_name) || Self::is_c_stdlib_macro(call_name) {
            return true;
        }

        false
    }

    /// Check if a call is to stdlib using relation type (optimized)
    ///
    /// This is the optimized interface that uses static dispatch based on RelationType
    /// for O(1) performance. This method should be preferred over `is_stdlib_call`
    /// when RelationType information is available.
    pub fn is_stdlib_by_type(
        call_name: &str,
        relation_type: &cce_types::relation::RelationType,
    ) -> bool {
        use cce_types::relation::RelationType;

        match relation_type {
            // Direct function call
            RelationType::DirectCall => {
                // Check for std:: prefix
                if call_name.starts_with("std::") {
                    let clean_name = call_name.strip_prefix("std::").unwrap_or("");
                    return Self::STDLIB_FUNCTIONS.contains(&clean_name)
                        || Self::STDLIB_TYPES.contains(&clean_name);
                }
                // Check for macros and constants (no std:: prefix)
                if Self::is_stdlib_macro(call_name) {
                    return true;
                }
                // Check for C standard library functions
                if Self::is_c_stdlib_function(call_name) || Self::is_c_stdlib_macro(call_name) {
                    return true;
                }
                false
            }

            // Instance method call: check type name
            RelationType::InstanceMethodCall => {
                if let Some(type_name) = call_name.split("::").next() {
                    let clean_name = type_name.strip_prefix("std::").unwrap_or(type_name);
                    Self::STDLIB_TYPES.contains(&clean_name)
                } else {
                    false
                }
            }

            // Static method call: check type name
            RelationType::StaticMethodCall => {
                if let Some(type_name) = call_name.split("::").next() {
                    let clean_name = type_name.strip_prefix("std::").unwrap_or(type_name);
                    Self::STDLIB_TYPES.contains(&clean_name)
                } else {
                    false
                }
            }

            // Chained method call: check type name (first part)
            RelationType::ChainedMethodCall => {
                if let Some(type_name) = call_name.split("::").next() {
                    let clean_name = type_name.strip_prefix("std::").unwrap_or(type_name);
                    Self::STDLIB_TYPES.contains(&clean_name)
                } else {
                    false
                }
            }

            // Constructor call: check type name
            RelationType::ConstructorCall => {
                // Extract type name before '(' using find instead of split
                // to avoid issues with nested parentheses
                if let Some(paren_pos) = call_name.find('(') {
                    let type_name = &call_name[..paren_pos];
                    let clean_name = type_name
                        .trim()
                        .strip_prefix("std::")
                        .unwrap_or(type_name.trim());
                    Self::STDLIB_TYPES.contains(&clean_name)
                } else {
                    false
                }
            }

            // Generic/template call: check type name (before '<')
            RelationType::GenericCall => {
                if let Some(type_name) = call_name.split('<').next() {
                    if let Some(base_name) = type_name.split("::").next() {
                        let clean_name = base_name.strip_prefix("std::").unwrap_or(base_name);
                        Self::STDLIB_TYPES.contains(&clean_name)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }

            // Other relation types: use legacy detection
            _ => Self::is_stdlib_call(call_name),
        }
    }

    /// Check if a name is a C standard library function (available in C++)
    fn is_c_stdlib_function(name: &str) -> bool {
        // Use C standard library detector to check functions
        super::c::CStdlibDetector::is_stdlib_function(name)
    }

    /// Check if a name is a C standard library macro (available in C++)
    fn is_c_stdlib_macro(name: &str) -> bool {
        // Use C standard library detector to check macros
        super::c::CStdlibDetector::is_stdlib_macro(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stdlib_header() {
        assert!(CppStdlibDetector::is_stdlib_header("vector"));
        assert!(CppStdlibDetector::is_stdlib_header("<algorithm>"));
        assert!(CppStdlibDetector::is_stdlib_header("\"iostream\""));
        assert!(CppStdlibDetector::is_stdlib_header("filesystem")); // C++17
        assert!(CppStdlibDetector::is_stdlib_header("span")); // C++20
        assert!(CppStdlibDetector::is_stdlib_header("print")); // C++23
        assert!(!CppStdlibDetector::is_stdlib_header("myheader"));
    }

    #[test]
    fn test_is_stdlib_type() {
        assert!(CppStdlibDetector::is_stdlib_type("std::vector"));
        assert!(CppStdlibDetector::is_stdlib_type("std::string"));
        assert!(CppStdlibDetector::is_stdlib_type("std::unique_ptr"));
        assert!(CppStdlibDetector::is_stdlib_type("std::optional"));
        assert!(CppStdlibDetector::is_stdlib_type("std::variant"));
        assert!(CppStdlibDetector::is_stdlib_type("std::is_same")); // type trait
        assert!(CppStdlibDetector::is_stdlib_type("std::same_as")); // C++20 concept
        assert!(!CppStdlibDetector::is_stdlib_type("MyClass"));
    }

    #[test]
    fn test_is_stdlib_macro() {
        assert!(CppStdlibDetector::is_stdlib_macro("NULL"));
        assert!(CppStdlibDetector::is_stdlib_macro("nullptr"));
        assert!(CppStdlibDetector::is_stdlib_macro("EOF"));
        assert!(CppStdlibDetector::is_stdlib_macro("EXIT_SUCCESS"));
        assert!(CppStdlibDetector::is_stdlib_macro("alignas"));
        assert!(CppStdlibDetector::is_stdlib_macro("noexcept"));
        assert!(CppStdlibDetector::is_stdlib_macro("constexpr"));
        assert!(!CppStdlibDetector::is_stdlib_macro("MY_MACRO"));
    }

    #[test]
    fn test_is_stdlib_call() {
        // C++ standard library calls
        assert!(CppStdlibDetector::is_stdlib_call("std::sort"));
        assert!(CppStdlibDetector::is_stdlib_call("std::cout"));
        assert!(CppStdlibDetector::is_stdlib_call("std::vector"));
        assert!(CppStdlibDetector::is_stdlib_call("std::optional"));

        // C standard library calls (available in C++)
        assert!(CppStdlibDetector::is_stdlib_call("printf"));
        assert!(CppStdlibDetector::is_stdlib_call("malloc"));
        assert!(CppStdlibDetector::is_stdlib_call("NULL"));
        assert!(CppStdlibDetector::is_stdlib_call("EOF"));

        // Macros
        assert!(CppStdlibDetector::is_stdlib_call("alignas"));
        assert!(CppStdlibDetector::is_stdlib_call("noexcept"));

        // Negative cases
        assert!(!CppStdlibDetector::is_stdlib_call("my_function"));
        assert!(!CppStdlibDetector::is_stdlib_call("MyClass"));
        assert!(!CppStdlibDetector::is_stdlib_call("MY_MACRO"));
    }

    #[test]
    fn test_c_stdlib_in_cpp() {
        // Test that C standard library functions are detected in C++ context
        assert!(CppStdlibDetector::is_stdlib_call("printf"));
        assert!(CppStdlibDetector::is_stdlib_call("malloc"));
        assert!(CppStdlibDetector::is_stdlib_call("strlen"));
        assert!(CppStdlibDetector::is_stdlib_call("fopen"));
        assert!(CppStdlibDetector::is_stdlib_call("exit"));
    }

    #[test]
    fn test_cpp_modern_features() {
        // Test C++17 features
        assert!(CppStdlibDetector::is_stdlib_type("std::optional"));
        assert!(CppStdlibDetector::is_stdlib_type("std::variant"));
        assert!(CppStdlibDetector::is_stdlib_type("std::string_view"));

        // Test C++20 features
        assert!(CppStdlibDetector::is_stdlib_type("std::same_as"));
        assert!(CppStdlibDetector::is_stdlib_type("std::integral"));
        assert!(CppStdlibDetector::is_stdlib_call("std::cmp_equal"));

        // Test C++23 features
        assert!(CppStdlibDetector::is_stdlib_header("print"));
        assert!(CppStdlibDetector::is_stdlib_header("expected"));
    }
}
