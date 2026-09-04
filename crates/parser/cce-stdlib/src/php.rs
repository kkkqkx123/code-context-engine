// PHP Standard Library Detector
// Handles detection of PHP standard library entities

pub struct PhpStdlibDetector;

impl PhpStdlibDetector {
    // Core extensions and modules
    pub const CORE_EXTENSIONS: &[&str] = &[
        // Core
        "Core",
        "standard",
        "date",
        "libxml",
        "openssl",
        "pcre",
        "zlib",
        "filter",
        "hash",
        "json",
        "mbstring",
        "SPL",
        "session",
        "Reflection",
        "Phar",
        // Database
        "PDO",
        "pdo_mysql",
        "pdo_pgsql",
        "pdo_sqlite",
        "pdo_oci",
        "pdo_odbc",
        "mysqli",
        "mysqlnd",
        // XML
        "SimpleXML",
        "XML",
        "XMLReader",
        "XMLWriter",
        "DOM",
        "XSL",
        // Compression
        "bz2",
        "zip",
        // Cryptography
        "mcrypt",
        "openssl",
        "sodium",
        // Image processing
        "gd",
        "exif",
        "imagick",
        // Network
        "curl",
        "ftp",
        "soap",
        "socket",
        // Multibyte String
        "mbstring",
        "iconv",
        // Internationalization
        "intl",
        "gettext",
        // Process Control
        "pcntl",
        "posix",
        // System
        "sysvmsg",
        "sysvsem",
        "sysvshm",
        "shmop",
        // Other
        "tidy",
        "xsl",
        "xmlrpc",
        "wddx",
        // PHP 8+ extensions
        "FFI",
        "random",
        "sodium",
    ];

    // Built-in functions (global functions)
    pub const BUILTIN_FUNCTIONS: &[&str] = &[
        // Array functions
        "array",
        "array_change_key_case",
        "array_chunk",
        "array_column",
        "array_combine",
        "array_count_values",
        "array_diff",
        "array_diff_assoc",
        "array_diff_key",
        "array_diff_uassoc",
        "array_diff_ukey",
        "array_fill",
        "array_fill_keys",
        "array_filter",
        "array_flip",
        "array_intersect",
        "array_intersect_assoc",
        "array_intersect_key",
        "array_intersect_uassoc",
        "array_intersect_ukey",
        "array_key_exists",
        "array_key_first",
        "array_key_last",
        "array_keys",
        "array_map",
        "array_merge",
        "array_merge_recursive",
        "array_multisort",
        "array_pad",
        "array_pop",
        "array_product",
        "array_push",
        "array_rand",
        "array_reduce",
        "array_replace",
        "array_replace_recursive",
        "array_reverse",
        "array_search",
        "array_shift",
        "array_slice",
        "array_splice",
        "array_sum",
        "array_udiff",
        "array_udiff_assoc",
        "array_udiff_uassoc",
        "array_uintersect",
        "array_uintersect_assoc",
        "array_uintersect_uassoc",
        "array_unique",
        "array_unshift",
        "array_values",
        "array_walk",
        "array_walk_recursive",
        "arsort",
        "asort",
        "compact",
        "count",
        "current",
        "each",
        "end",
        "extract",
        "in_array",
        "key",
        "krsort",
        "ksort",
        "list",
        "natcasesort",
        "natsort",
        "next",
        "pos",
        "prev",
        "range",
        "reset",
        "rsort",
        "shuffle",
        "sizeof",
        "sort",
        "uasort",
        "uksort",
        "usort",
        // String functions
        "addcslashes",
        "addslashes",
        "bin2hex",
        "chop",
        "chr",
        "chunk_split",
        "convert_cyr_string",
        "convert_uudecode",
        "convert_uuencode",
        "count_chars",
        "crc32",
        "crypt",
        "echo",
        "explode",
        "fprintf",
        "get_html_translation_table",
        "hebrev",
        "hebrevc",
        "hex2bin",
        "html_entity_decode",
        "htmlentities",
        "htmlspecialchars",
        "htmlspecialchars_decode",
        "implode",
        "join",
        "lcfirst",
        "levenshtein",
        "localeconv",
        "ltrim",
        "md5",
        "md5_file",
        "metaphone",
        "money_format",
        "nl_langinfo",
        "nl2br",
        "number_format",
        "ord",
        "parse_str",
        "print",
        "printf",
        "quoted_printable_decode",
        "quoted_printable_encode",
        "quotemeta",
        "rtrim",
        "setlocale",
        "sha1",
        "sha1_file",
        "similar_text",
        "soundex",
        "sprintf",
        "sscanf",
        "str_getcsv",
        "str_ireplace",
        "str_pad",
        "str_repeat",
        "str_replace",
        "str_rot13",
        "str_shuffle",
        "str_split",
        "str_word_count",
        "strcasecmp",
        "strchr",
        "strcmp",
        "strcoll",
        "strcspn",
        "strip_tags",
        "stripcslashes",
        "stripos",
        "stripslashes",
        "stristr",
        "strlen",
        "strnatcasecmp",
        "strnatcmp",
        "strncasecmp",
        "strncmp",
        "strpbrk",
        "strpos",
        "strrchr",
        "strrev",
        "strripos",
        "strrpos",
        "strspn",
        "strstr",
        "strtok",
        "strtolower",
        "strtoupper",
        "strtr",
        "substr",
        "substr_compare",
        "substr_count",
        "substr_replace",
        "trim",
        "ucfirst",
        "ucwords",
        "vfprintf",
        "vprintf",
        "vsprintf",
        "wordwrap",
        // File system functions
        "basename",
        "chgrp",
        "chmod",
        "chown",
        "clearstatcache",
        "copy",
        "dirname",
        "disk_free_space",
        "disk_total_space",
        "diskfreespace",
        "fclose",
        "feof",
        "fflush",
        "fgetc",
        "fgetcsv",
        "fgets",
        "fgetss",
        "file",
        "file_exists",
        "file_get_contents",
        "file_put_contents",
        "fileatime",
        "filectime",
        "filegroup",
        "fileinode",
        "filemtime",
        "fileowner",
        "fileperms",
        "filesize",
        "filetype",
        "flock",
        "fnmatch",
        "fopen",
        "fpassthru",
        "fputcsv",
        "fputs",
        "fread",
        "fscanf",
        "fseek",
        "fstat",
        "ftell",
        "ftruncate",
        "fwrite",
        "glob",
        "is_dir",
        "is_executable",
        "is_file",
        "is_link",
        "is_readable",
        "is_uploaded_file",
        "is_writable",
        "is_writeable",
        "lchgrp",
        "lchown",
        "link",
        "linkinfo",
        "lstat",
        "mkdir",
        "move_uploaded_file",
        "parse_ini_file",
        "parse_ini_string",
        "pathinfo",
        "pclose",
        "popen",
        "readfile",
        "readlink",
        "realpath",
        "realpath_cache_get",
        "realpath_cache_size",
        "rename",
        "rewind",
        "rmdir",
        "set_file_buffer",
        "stat",
        "symlink",
        "tempnam",
        "tmpfile",
        "touch",
        "umask",
        "unlink",
        // Date/Time functions
        "checkdate",
        "date",
        "date_add",
        "date_create",
        "date_create_from_format",
        "date_date_set",
        "date_default_timezone_get",
        "date_default_timezone_set",
        "date_diff",
        "date_format",
        "date_get_last_errors",
        "date_interval_create_from_date_string",
        "date_interval_format",
        "date_isodate_set",
        "date_modify",
        "date_offset_get",
        "date_parse",
        "date_parse_from_format",
        "date_sub",
        "date_sun_info",
        "date_sunrise",
        "date_sunset",
        "date_time_set",
        "date_timestamp_get",
        "date_timestamp_set",
        "date_timezone_get",
        "date_timezone_set",
        "getdate",
        "gettimeofday",
        "gmdate",
        "gmmktime",
        "gmstrftime",
        "idate",
        "localtime",
        "microtime",
        "mktime",
        "strftime",
        "strptime",
        "strtotime",
        "time",
        "timezone_abbreviations_list",
        "timezone_identifiers_list",
        "timezone_location_get",
        "timezone_name_from_abbr",
        "timezone_name_get",
        "timezone_offset_get",
        "timezone_open",
        "timezone_transitions_get",
        "timezone_version_get",
        // Math functions
        "abs",
        "acos",
        "acosh",
        "asin",
        "asinh",
        "atan",
        "atan2",
        "atanh",
        "base_convert",
        "bindec",
        "ceil",
        "cos",
        "cosh",
        "decbin",
        "dechex",
        "decoct",
        "deg2rad",
        "exp",
        "expm1",
        "floor",
        "fmod",
        "getrandmax",
        "hexdec",
        "hypot",
        "intdiv",
        "is_finite",
        "is_infinite",
        "is_nan",
        "lcg_value",
        "log",
        "log10",
        "log1p",
        "max",
        "min",
        "mt_getrandmax",
        "mt_rand",
        "mt_srand",
        "octdec",
        "pi",
        "pow",
        "rad2deg",
        "rand",
        "round",
        "sin",
        "sinh",
        "sqrt",
        "srand",
        "tan",
        "tanh",
        // Network functions
        "checkdnsrr",
        "closelog",
        "dns_check_record",
        "dns_get_mx",
        "dns_get_record",
        "fsockopen",
        "gethostbyaddr",
        "gethostbyname",
        "gethostbynamel",
        "getmxrr",
        "getprotobyname",
        "getprotobynumber",
        "getservbyname",
        "getservbyport",
        "header",
        "header_remove",
        "headers_list",
        "headers_sent",
        "http_response_code",
        "inet_ntop",
        "inet_pton",
        "ip2long",
        "long2ip",
        "openlog",
        "pfsockopen",
        "setcookie",
        "setrawcookie",
        "socket_get_status",
        "socket_set_blocking",
        "socket_set_timeout",
        "syslog",
        // Variable handling functions
        "boolval",
        "debug_zval_dump",
        "doubleval",
        "empty",
        "floatval",
        "get_defined_vars",
        "get_resource_type",
        "gettype",
        "import_request_variables",
        "intval",
        "is_array",
        "is_bool",
        "is_callable",
        "is_countable",
        "is_double",
        "is_float",
        "is_int",
        "is_integer",
        "is_iterable",
        "is_long",
        "is_null",
        "is_numeric",
        "is_object",
        "is_real",
        "is_resource",
        "is_scalar",
        "is_string",
        "isset",
        "print_r",
        "serialize",
        "settype",
        "strval",
        "unserialize",
        "unset",
        "var_dump",
        "var_export",
        // Class/Object functions
        "class_alias",
        "class_exists",
        "get_called_class",
        "get_class",
        "get_class_methods",
        "get_class_vars",
        "get_declared_classes",
        "get_declared_interfaces",
        "get_declared_traits",
        "get_object_vars",
        "get_parent_class",
        "interface_exists",
        "is_a",
        "is_subclass_of",
        "method_exists",
        "property_exists",
        "trait_exists",
        // Function handling functions
        "call_user_func",
        "call_user_func_array",
        "forward_static_call",
        "forward_static_call_array",
        "func_get_arg",
        "func_get_args",
        "func_num_args",
        "function_exists",
        "get_defined_functions",
        "register_shutdown_function",
        "register_tick_function",
        "unregister_tick_function",
        // Error handling functions
        "debug_backtrace",
        "debug_print_backtrace",
        "error_clear_last",
        "error_get_last",
        "error_log",
        "error_reporting",
        "restore_error_handler",
        "restore_exception_handler",
        "set_error_handler",
        "set_exception_handler",
        "trigger_error",
        "user_error",
        // Other important functions
        "define",
        "defined",
        "die",
        "eval",
        "exit",
        "get_browser",
        "get_cfg_var",
        "get_current_user",
        "get_defined_constants",
        "get_extension_funcs",
        "get_include_path",
        "get_included_files",
        "get_loaded_extensions",
        "get_magic_quotes_gpc",
        "get_magic_quotes_runtime",
        "get_required_files",
        "getenv",
        "getlastmod",
        "getmygid",
        "getmyinode",
        "getmypid",
        "getmyuid",
        "getopt",
        "getrusage",
        "ini_get",
        "ini_get_all",
        "ini_restore",
        "ini_set",
        "memory_get_peak_usage",
        "memory_get_usage",
        "php_ini_loaded_file",
        "php_ini_scanned_files",
        "php_logo_guid",
        "php_sapi_name",
        "php_uname",
        "phpcredits",
        "phpinfo",
        "phpversion",
        "putenv",
        "set_include_path",
        "set_time_limit",
        "sys_get_temp_dir",
        "version_compare",
        "zend_logo_guid",
        "zend_thread_id",
        "zend_version",
    ];

    // Built-in classes and interfaces
    pub const BUILTIN_CLASSES: &[&str] = &[
        // Core classes
        "stdClass",
        "Closure",
        "Generator",
        "ClosedGeneratorException",
        "WeakReference",
        "WeakMap",
        "ArrayObject",
        "ArrayIterator",
        "RecursiveArrayIterator",
        "SplFixedArray",
        "SplHeap",
        "SplMinHeap",
        "SplMaxHeap",
        "SplPriorityQueue",
        "SplQueue",
        "SplStack",
        "SplDoublyLinkedList",
        "SplObjectStorage",
        // Error/Exception classes
        "Exception",
        "ErrorException",
        "Error",
        "ArgumentCountError",
        "ArithmeticError",
        "AssertionError",
        "DivisionByZeroError",
        "CompileError",
        "ParseError",
        "TypeError",
        "ValueError",
        "UnhandledMatchError",
        "FiberError",
        // DateTime classes
        "DateTime",
        "DateTimeImmutable",
        "DateTimeZone",
        "DateInterval",
        "DatePeriod",
        // Reflection classes
        "ReflectionClass",
        "ReflectionFunction",
        "ReflectionMethod",
        "ReflectionProperty",
        "ReflectionParameter",
        "ReflectionType",
        "ReflectionNamedType",
        "ReflectionUnionType",
        "ReflectionIntersectionType",
        "ReflectionAttribute",
        "ReflectionExtension",
        "ReflectionZendExtension",
        // SPL classes
        "SplFileInfo",
        "SplFileObject",
        "SplTempFileObject",
        "DirectoryIterator",
        "FilesystemIterator",
        "RecursiveDirectoryIterator",
        "GlobIterator",
        "SplFileInfo",
        // Iterators
        "AppendIterator",
        "CachingIterator",
        "CallbackFilterIterator",
        "FilterIterator",
        "InfiniteIterator",
        "IteratorIterator",
        "LimitIterator",
        "NoRewindIterator",
        "ParentIterator",
        "RecursiveCallbackFilterIterator",
        "RecursiveFilterIterator",
        "RecursiveIteratorIterator",
        "RecursiveTreeIterator",
        // PHP 8+ classes
        "Stringable",
        "UnhandledMatchError",
        "ValueError",
        "Attribute",
        "ReturnTypeWillChange",
        "AllowDynamicProperties",
        "SensitiveParameter",
        "SensitiveParameterValue",
        // Other important classes
        "PDO",
        "PDOStatement",
        "PDOException",
        "mysqli",
        "mysqli_stmt",
        "mysqli_result",
        "mysqli_driver",
        "mysqli_warning",
        "mysqli_sql_exception",
        "SimpleXMLElement",
        "SimpleXMLIterator",
        "DOMDocument",
        "DOMElement",
        "DOMNode",
        "DOMNodeList",
        "DOMXPath",
        "XMLReader",
        "XMLWriter",
        "XSLTProcessor",
        "ZipArchive",
        "Phar",
        "PharData",
        "PharFileInfo",
        "JsonSerializable",
        "Serializable",
        "Iterator",
        "IteratorAggregate",
        "ArrayAccess",
        "Countable",
        "Stringable",
        "Throwable",
        "Traversable",
        "Generator",
        "SessionHandlerInterface",
        "SessionIdInterface",
        "SessionUpdateTimestampHandlerInterface",
    ];

    // Built-in constants
    pub const BUILTIN_CONSTANTS: &[&str] = &[
        "PHP_VERSION",
        "PHP_MAJOR_VERSION",
        "PHP_MINOR_VERSION",
        "PHP_RELEASE_VERSION",
        "PHP_VERSION_ID",
        "PHP_EXTRA_VERSION",
        "PHP_ZTS",
        "PHP_DEBUG",
        "PHP_MAXPATHLEN",
        "PHP_OS",
        "PHP_OS_FAMILY",
        "PHP_SAPI",
        "PHP_EOL",
        "PHP_INT_MAX",
        "PHP_INT_MIN",
        "PHP_INT_SIZE",
        "PHP_FLOAT_DIG",
        "PHP_FLOAT_EPSILON",
        "PHP_FLOAT_MIN",
        "PHP_FLOAT_MAX",
        "DEFAULT_INCLUDE_PATH",
        "PEAR_INSTALL_DIR",
        "PEAR_EXTENSION_DIR",
        "PHP_EXTENSION_DIR",
        "PHP_PREFIX",
        "PHP_BINDIR",
        "PHP_BINARY",
        "PHP_MANDIR",
        "PHP_LIBDIR",
        "PHP_DATADIR",
        "PHP_SYSCONFDIR",
        "PHP_LOCALSTATEDIR",
        "PHP_CONFIG_FILE_PATH",
        "PHP_CONFIG_FILE_SCAN_DIR",
        "PHP_SHLIB_SUFFIX",
        "PHP_FD_SETSIZE",
        "E_ERROR",
        "E_WARNING",
        "E_PARSE",
        "E_NOTICE",
        "E_CORE_ERROR",
        "E_CORE_WARNING",
        "E_COMPILE_ERROR",
        "E_COMPILE_WARNING",
        "E_USER_ERROR",
        "E_USER_WARNING",
        "E_USER_NOTICE",
        "E_STRICT",
        "E_RECOVERABLE_ERROR",
        "E_DEPRECATED",
        "E_USER_DEPRECATED",
        "E_ALL",
        "DEBUG_BACKTRACE_PROVIDE_OBJECT",
        "DEBUG_BACKTRACE_IGNORE_ARGS",
        "TRUE",
        "FALSE",
        "NULL",
        "M_PI",
        "M_E",
        "M_LOG2E",
        "M_LOG10E",
        "M_LN2",
        "M_LN10",
        "M_PI_2",
        "M_PI_4",
        "M_1_PI",
        "M_2_PI",
        "M_2_SQRTPI",
        "M_SQRT2",
        "M_SQRT1_2",
        "NAN",
        "INF",
    ];
}

// Generate simple containment check functions
impl_list_checker!(
    PhpStdlibDetector,
    [
        (CORE_EXTENSIONS, is_core_extension),
        (BUILTIN_FUNCTIONS, is_builtin_function),
        (BUILTIN_CLASSES, is_builtin_class),
        (BUILTIN_CONSTANTS, is_builtin_constant),
    ]
);

impl PhpStdlibDetector {
    /// Check if a type name is a builtin PHP class
    pub fn is_stdlib_type(name: &str) -> bool {
        Self::is_builtin_class(name)
    }

    /// Check if a constant name is a builtin PHP constant
    pub fn is_stdlib_constant(name: &str) -> bool {
        Self::is_builtin_constant(name)
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for direct builtin function
        if Self::is_builtin_function(call_name) {
            return true;
        }

        // Check for builtin class
        if Self::is_builtin_class(call_name) {
            return true;
        }

        // Check for builtin constant
        if Self::is_builtin_constant(call_name) {
            return true;
        }

        // Check for namespaced call (e.g., \DateTime, \PDO::query)
        if let Some(without_backslash) = call_name.strip_prefix('\\') {
            // Check if it's a builtin class
            if Self::is_builtin_class(without_backslash) {
                return true;
            }
            // Check for static method call (e.g., \DateTime::createFromFormat)
            if without_backslash.contains("::") {
                let class_part = without_backslash.split("::").next().unwrap_or("");
                return Self::is_builtin_class(class_part);
            }
        }

        // Check for class::method or class->method
        if call_name.contains("::") || call_name.contains("->") {
            let separator = if call_name.contains("::") { "::" } else { "->" };
            let class_part = call_name.split(separator).next().unwrap_or("");

            // Check if it's a builtin class
            if Self::is_builtin_class(class_part) {
                return true;
            }

            // Check if it's a namespaced builtin class
            if let Some(class_without_ns) = class_part.strip_prefix('\\') {
                return Self::is_builtin_class(class_without_ns);
            }
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
            // Most call types in PHP use the same detection logic
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::CallbackCall
            | RelationType::GenericCall => {
                // For PHP, use the legacy detection logic
                Self::is_stdlib_call(call_name)
            }

            // Other relation types are not relevant for stdlib detection
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin_function() {
        assert!(PhpStdlibDetector::is_builtin_function("strlen"));
        assert!(PhpStdlibDetector::is_builtin_function("array_map"));
        assert!(PhpStdlibDetector::is_builtin_function("date"));
        assert!(!PhpStdlibDetector::is_builtin_function("my_function"));
    }

    #[test]
    fn test_is_builtin_class() {
        assert!(PhpStdlibDetector::is_builtin_class("DateTime"));
        assert!(PhpStdlibDetector::is_builtin_class("PDO"));
        assert!(PhpStdlibDetector::is_builtin_class("Exception"));
        assert!(!PhpStdlibDetector::is_builtin_class("MyClass"));
    }

    #[test]
    fn test_is_builtin_constant() {
        assert!(PhpStdlibDetector::is_builtin_constant("PHP_VERSION"));
        assert!(PhpStdlibDetector::is_builtin_constant("E_ERROR"));
        assert!(PhpStdlibDetector::is_builtin_constant("M_PI"));
        assert!(!PhpStdlibDetector::is_builtin_constant("MY_CONSTANT"));
    }

    #[test]
    fn test_is_stdlib_call() {
        // Builtin functions
        assert!(PhpStdlibDetector::is_stdlib_call("strlen"));
        assert!(PhpStdlibDetector::is_stdlib_call("array_map"));

        // Builtin classes
        assert!(PhpStdlibDetector::is_stdlib_call("DateTime"));
        assert!(PhpStdlibDetector::is_stdlib_call("PDO"));

        // Namespaced builtin classes
        assert!(PhpStdlibDetector::is_stdlib_call("\\DateTime"));
        assert!(PhpStdlibDetector::is_stdlib_call("\\PDO"));

        // Static method calls
        assert!(PhpStdlibDetector::is_stdlib_call(
            "DateTime::createFromFormat"
        ));
        assert!(PhpStdlibDetector::is_stdlib_call(
            "\\DateTime::createFromFormat"
        ));

        // Instance method calls (though less common in detection)
        assert!(PhpStdlibDetector::is_stdlib_call("DateTime->format"));

        // Builtin constants
        assert!(PhpStdlibDetector::is_stdlib_call("PHP_VERSION"));
        assert!(PhpStdlibDetector::is_stdlib_call("E_ERROR"));

        // Negative cases
        assert!(!PhpStdlibDetector::is_stdlib_call("my_function"));
        assert!(!PhpStdlibDetector::is_stdlib_call("MyClass"));
        assert!(!PhpStdlibDetector::is_stdlib_call("MyClass::myMethod"));
    }
}
