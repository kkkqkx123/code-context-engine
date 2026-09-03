// JavaScript/TypeScript Standard Library Detector
// Handles detection of JavaScript/TypeScript standard library entities
//
// NOTE: This is the ONLY place where JavaScript stdlib categorization is defined.
// All subsequent layers (grouper, call_merger, etc.) must use the categories
// set in the Relation.stdlib_category field - they must NOT re-detect or re-categorize.

pub struct JavaScriptStdlibDetector;

impl JavaScriptStdlibDetector {
    // Global objects
    pub const GLOBAL_OBJECTS: &[&str] = &[
        "global",
        "globalThis",
        "window",
        "self",
        "console",
        "process",
        "Buffer",
    ];

    // Built-in objects
    pub const BUILTIN_OBJECTS: &[&str] = &[
        "Object",
        "Function",
        "Boolean",
        "Symbol",
        "Error",
        "EvalError",
        "RangeError",
        "ReferenceError",
        "SyntaxError",
        "TypeError",
        "URIError",
        "AggregateError",
        "InternalError",
        "Number",
        "BigInt",
        "Math",
        "Date",
        "String",
        "RegExp",
        "Array",
        "Int8Array",
        "Uint8Array",
        "Uint8ClampedArray",
        "Int16Array",
        "Uint16Array",
        "Int32Array",
        "Uint32Array",
        "Float32Array",
        "Float64Array",
        "BigInt64Array",
        "BigUint64Array",
        "ArrayBuffer",
        "SharedArrayBuffer",
        "DataView",
        "Map",
        "Set",
        "WeakMap",
        "WeakSet",
        "Promise",
        "Proxy",
        "Reflect",
        "JSON",
        "Intl",
    ];

    // Built-in functions
    pub const BUILTIN_FUNCTIONS: &[&str] = &[
        "eval",
        "isFinite",
        "isNaN",
        "parseFloat",
        "parseInt",
        "decodeURI",
        "decodeURIComponent",
        "encodeURI",
        "encodeURIComponent",
    ];

    // Console methods
    pub const CONSOLE_METHODS: &[&str] = &[
        "log",
        "error",
        "warn",
        "info",
        "debug",
        "trace",
        "assert",
        "clear",
        "count",
        "countReset",
        "dir",
        "dirxml",
        "group",
        "groupCollapsed",
        "groupEnd",
        "table",
        "time",
        "timeEnd",
        "timeLog",
    ];

    // Math methods
    pub const MATH_METHODS: &[&str] = &[
        "abs", "acos", "asin", "atan", "atan2", "ceil", "cos", "exp", "floor", "log", "max", "min",
        "pow", "random", "round", "sin", "sqrt", "tan",
    ];

    // JSON methods
    pub const JSON_METHODS: &[&str] = &["parse", "stringify"];

    // Node.js stdlib modules
    pub const NODE_STDLIB_MODULES: &[&str] = &[
        "assert",
        "async_hooks",
        "buffer",
        "child_process",
        "cluster",
        "crypto",
        "dgram",
        "dns",
        "domain",
        "events",
        "fs",
        "globals",
        "http",
        "https",
        "http2",
        "inspector",
        "module",
        "net",
        "os",
        "path",
        "perf_hooks",
        "process",
        "punycode",
        "querystring",
        "readline",
        "repl",
        "stream",
        "string_decoder",
        "sys",
        "timers",
        "tls",
        "trace_events",
        "tty",
        "url",
        "util",
        "v8",
        "vm",
        "worker_threads",
        "zlib",
    ];
}

// Generate simple containment check functions using macro
impl_list_checker!(
    JavaScriptStdlibDetector,
    [
        (GLOBAL_OBJECTS, is_global_object),
        (BUILTIN_OBJECTS, is_builtin_object),
        (BUILTIN_FUNCTIONS, is_builtin_function),
        (CONSOLE_METHODS, is_console_method),
        (MATH_METHODS, is_math_method),
        (JSON_METHODS, is_json_method),
        (NODE_STDLIB_MODULES, is_node_stdlib),
    ]
);

// Generate get_category using macro
impl_stdlib_categorizer!(
    JavaScriptStdlibDetector,
    [
        // Collection types - Arrays, Sets, Maps, etc.
        (
            StdlibCategory::Collection,
            [
                "Array",
                "Map",
                "Set",
                "WeakMap",
                "WeakSet",
                "WeakRef",
                "FinalizationRegistry",
                "Int8Array",
                "Uint8Array",
                "Uint8ClampedArray",
                "Int16Array",
                "Uint16Array",
                "Int32Array",
                "Uint32Array",
                "Float32Array",
                "Float64Array",
                "BigInt64Array",
                "BigUint64Array",
                "ArrayBuffer",
                "SharedArrayBuffer",
                "DataView",
            ]
        ),
        // I/O and Network types
        (
            StdlibCategory::Io,
            [
                "console",
                "fetch",
                "File",
                "Blob",
                "FileReader",
                "FileList",
                "URL",
                "URLSearchParams",
                "TextEncoder",
                "TextDecoder",
                "FormData",
                "Request",
                "Response",
                "Headers",
                "WebSocket",
                "WebSocketStream",
                "EventSource",
                "Storage",
                "localStorage",
                "sessionStorage",
                "Event",
                "EventTarget",
                "CustomEvent",
                "MessageEvent",
                "ErrorEvent",
                "CloseEvent",
                "FetchEvent",
            ]
        ),
        // Concurrency types - Workers, AbortController, etc.
        (
            StdlibCategory::Concurrency,
            [
                "Worker",
                "SharedWorker",
                "ServiceWorker",
                "AbortController",
                "AbortSignal",
            ]
        ),
        // Utility types - Promise, Proxy, Performance, IndexedDB, etc.
        (
            StdlibCategory::Utility,
            [
                "Promise",
                "Symbol",
                "Proxy",
                "Reflect",
                "Performance",
                "PerformanceEntry",
                "PerformanceMark",
                "PerformanceMeasure",
                "PerformanceResourceTiming",
                "IndexedDB",
                "IDBDatabase",
                "IDBTransaction",
                "IDBObjectStore",
                "IDBIndex",
                "IDBCursor",
                "IDBRequest",
                "IDBKeyRange",
            ]
        ),
        // String types
        (StdlibCategory::String, ["String", "TemplateLiteral"]),
        // Numeric types
        (StdlibCategory::Numeric, ["Number", "BigInt", "Math"]),
        // Error types
        (
            StdlibCategory::Error,
            [
                "Error",
                "EvalError",
                "RangeError",
                "ReferenceError",
                "SyntaxError",
                "TypeError",
                "URIError",
                "AggregateError",
            ]
        ),
        // Other types
        (
            StdlibCategory::Other,
            [
                "CanvasRenderingContext2D",
                "WebGLRenderingContext",
                "WebGL2RenderingContext",
                "WebGLProgram",
                "WebGLShader",
                "WebGLBuffer",
                "WebGLFramebuffer",
                "WebGLRenderbuffer",
                "WebGLTexture",
                "WebGLUniformLocation",
                "WebGLShaderPrecisionFormat",
                "HTMLCanvasElement",
                "HTMLElement",
                "SVGElement",
                "Object",
                "Function",
                "Boolean",
                "Date",
                "RegExp",
                "JSON",
                "Intl",
                "global",
                "globalThis",
                "window",
                "self",
            ]
        ),
    ]
);

impl JavaScriptStdlibDetector {
    pub fn is_stdlib_call(call_name: &str) -> bool {
        use cce_types::relation::RelationType;
        Self::is_stdlib_by_type(call_name, &RelationType::DirectCall)
    }

    pub fn is_stdlib_by_type(
        call_name: &str,
        relation_type: &cce_types::relation::RelationType,
    ) -> bool {
        use cce_types::relation::RelationType;

        match relation_type {
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::CallbackCall
            | RelationType::GenericCall => {
                // For JavaScript, check builtin functions and object.method calls
                if Self::is_builtin_function(call_name) {
                    return true;
                }
                if call_name.contains('.') {
                    let parts: Vec<&str> = call_name.split('.').collect();
                    if parts.len() >= 2 {
                        let obj = parts[0];
                        let method = parts[1];

                        match obj {
                            "console" => Self::is_console_method(method),
                            "Math" => Self::is_math_method(method),
                            "JSON" => Self::is_json_method(method),
                            _ if Self::is_builtin_object(obj) => true,
                            _ => false,
                        }
                    } else {
                        false
                    }
                } else {
                    Self::is_builtin_object(call_name) || Self::is_global_object(call_name)
                }
            }

            // Other relation types are not relevant for stdlib detection
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::stdlib_category::StdlibCategory;

    #[test]
    fn test_get_category_collection() {
        assert_eq!(
            JavaScriptStdlibDetector::get_category("Array"),
            Some(StdlibCategory::Collection)
        );
        assert_eq!(
            JavaScriptStdlibDetector::get_category("Map"),
            Some(StdlibCategory::Collection)
        );
        assert_eq!(
            JavaScriptStdlibDetector::get_category("Int8Array"),
            Some(StdlibCategory::Collection)
        );
    }

    #[test]
    fn test_get_category_io() {
        assert_eq!(
            JavaScriptStdlibDetector::get_category("console"),
            Some(StdlibCategory::Io)
        );
        assert_eq!(
            JavaScriptStdlibDetector::get_category("fetch"),
            Some(StdlibCategory::Io)
        );
        assert_eq!(
            JavaScriptStdlibDetector::get_category("WebSocket"),
            Some(StdlibCategory::Io)
        );
    }

    #[test]
    fn test_get_category_concurrency() {
        assert_eq!(
            JavaScriptStdlibDetector::get_category("Worker"),
            Some(StdlibCategory::Concurrency)
        );
        assert_eq!(
            JavaScriptStdlibDetector::get_category("AbortController"),
            Some(StdlibCategory::Concurrency)
        );
    }

    #[test]
    fn test_get_category_error() {
        assert_eq!(
            JavaScriptStdlibDetector::get_category("Error"),
            Some(StdlibCategory::Error)
        );
        assert_eq!(
            JavaScriptStdlibDetector::get_category("TypeError"),
            Some(StdlibCategory::Error)
        );
    }

    #[test]
    fn test_get_category_unknown() {
        assert_eq!(
            JavaScriptStdlibDetector::get_category("MyCustomClass"),
            None
        );
        assert_eq!(
            JavaScriptStdlibDetector::get_category("unknownFunction"),
            None
        );
    }

    #[test]
    fn test_is_builtin_object() {
        assert!(JavaScriptStdlibDetector::is_builtin_object("Array"));
        assert!(JavaScriptStdlibDetector::is_builtin_object("Promise"));
        assert!(!JavaScriptStdlibDetector::is_builtin_object("MyClass"));
    }

    #[test]
    fn test_is_console_method() {
        assert!(JavaScriptStdlibDetector::is_console_method("log"));
        assert!(JavaScriptStdlibDetector::is_console_method("error"));
        assert!(!JavaScriptStdlibDetector::is_console_method("myMethod"));
    }

    #[test]
    fn test_is_stdlib_call() {
        assert!(JavaScriptStdlibDetector::is_stdlib_call("console.log"));
        assert!(JavaScriptStdlibDetector::is_stdlib_call("JSON.parse"));
        assert!(JavaScriptStdlibDetector::is_stdlib_call("Array.from"));
        assert!(!JavaScriptStdlibDetector::is_stdlib_call("customFunction"));
    }
}
