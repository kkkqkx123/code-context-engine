// Dart Standard Library Detector
// Handles detection of Dart standard library entities

pub struct DartStdlibDetector;

impl DartStdlibDetector {
    // Dart core packages
    pub const DART_PACKAGES: &[&str] = &[
        // Core libraries
        "dart:core",
        "dart:async",
        "dart:collection",
        "dart:convert",
        "dart:developer",
        "dart:ffi",
        "dart:html",
        "dart:indexed_db",
        "dart:io",
        "dart:isolate",
        "dart:js",
        "dart:js_interop",
        "dart:js_interop_unsafe",
        "dart:js_util",
        "dart:math",
        "dart:mirrors",
        "dart:svg",
        "dart:typed_data",
        "dart:ui",
        "dart:web_audio",
        "dart:web_gl",
        // Flutter packages (commonly used)
        "package:flutter",
        "package:flutter/material",
        "package:flutter/cupertino",
        "package:flutter/widgets",
        "package:flutter/services",
        "package:flutter/foundation",
        "package:flutter/gestures",
        "package:flutter/painting",
        "package:flutter/physics",
        "package:flutter/rendering",
        "package:flutter/scheduler",
        "package:flutter/semantics",
        "package:flutter/animation",
        // Common third-party packages
        "package:http",
        "package:path",
        "package:shared_preferences",
        "package:url_launcher",
        "package:image_picker",
        "package:flutter_bloc",
        "package:provider",
        "package:get",
        "package:dio",
        "package:equatable",
        "package:json_annotation",
        "package:freezed_annotation",
        "package:rxdart",
        "package:riverpod",
    ];

    // Built-in types
    pub const BUILTIN_TYPES: &[&str] = &[
        // Core types
        "Object",
        "bool",
        "int",
        "double",
        "num",
        "String",
        "Symbol",
        "Type",
        "Never",
        "Null",
        "void",
        "dynamic",
        // Collections
        "List",
        "Set",
        "Map",
        "Iterable",
        "Iterator",
        "Collection",
        "Queue",
        "LinkedList",
        "LinkedListEntry",
        "DoubleLinkedQueue",
        "DoubleLinkedQueueEntry",
        "ListQueue",
        "PriorityQueue",
        "SplayTreeSet",
        "SplayTreeMap",
        "UnmodifiableListView",
        "UnmodifiableSetView",
        "UnmodifiableMapView",
        // Async
        "Future",
        "Stream",
        "Completer",
        "StreamController",
        "StreamSubscription",
        "StreamTransformer",
        "StreamIterator",
        "Timer",
        "Zone",
        "ZoneSpecification",
        "ZoneDelegate",
        // Function types
        "Function",
        // Exceptions
        "Exception",
        "Error",
        "AssertionError",
        "TypeError",
        "ArgumentError",
        "RangeError",
        "IndexError",
        "FormatException",
        "UnsupportedError",
        "UnimplementedError",
        "StateError",
        "ConcurrentModificationError",
        "OutOfMemoryError",
        "StackOverflowError",
        "IntegerDivisionByZeroException",
        "LateInitializationError",
        "NoSuchMethodError",
        "RemoteError",
        "FileSystemException",
        "SocketException",
        "HttpException",
        "TimeoutException",
        "ProcessException",
        "SignalException",
        "StdinException",
        "StdoutException",
        "TlsException",
        // IO
        "File",
        "Directory",
        "Link",
        "FileSystemEntity",
        "FileStat",
        "FileMode",
        "FileLock",
        "FileSystemEvent",
        "RandomAccessFile",
        "IOSink",
        "StreamReader",
        "HttpClient",
        "HttpClientRequest",
        "HttpClientResponse",
        "HttpServer",
        "HttpRequest",
        "HttpResponse",
        "WebSocket",
        "Socket",
        "ServerSocket",
        "RawSocket",
        "RawServerSocket",
        "RawDatagramSocket",
        "InternetAddress",
        "NetworkInterface",
        "Process",
        "ProcessResult",
        "ProcessSignal",
        "Stdin",
        "Stdout",
        "Platform",
        "Isolate",
        "ReceivePort",
        "SendPort",
        // Convert
        "JsonCodec",
        "JsonDecoder",
        "JsonEncoder",
        "JsonCyclicError",
        "JsonUnsupportedObjectError",
        "Utf8Codec",
        "Utf8Decoder",
        "Utf8Encoder",
        "Latin1Codec",
        "Latin1Decoder",
        "Latin1Encoder",
        "AsciiCodec",
        "AsciiDecoder",
        "AsciiEncoder",
        "Base64Codec",
        "Base64Decoder",
        "Base64Encoder",
        "HtmlEscape",
        "HtmlEscapeMode",
        "StringConversionSink",
        "ByteConversionSink",
        "ChunkedConversionSink",
        "Codec",
        "Decoder",
        "Encoder",
        "Encoding",
        // Math
        "Random",
        "Rectangle",
        "MutableRectangle",
        "Point",
        "E",
        "PI",
        "ln2",
        "ln10",
        "log2e",
        "log10e",
        "sqrt2",
        "sqrt1_2",
        // Typed Data
        "ByteBuffer",
        "ByteData",
        "Float32List",
        "Float32x4",
        "Float32x4List",
        "Float64List",
        "Float64x2",
        "Float64x2List",
        "Int8List",
        "Int16List",
        "Int32List",
        "Int32x4",
        "Int32x4List",
        "Int64List",
        "Uint8List",
        "Uint8ClampedList",
        "Uint16List",
        "Uint32List",
        "Uint32x4",
        "Uint32x4List",
        "Uint64List",
        "TypedData",
        "Endianness",
        // FFI
        "NativeFunction",
        "Pointer",
        "Struct",
        "Union",
        "Array",
        "Allocator",
        "NativeTypes",
        "NativeType",
        "Void",
        "IntPtr",
        "Int8",
        "Int16",
        "Int32",
        "Int64",
        "Uint8",
        "Uint16",
        "Uint32",
        "Uint64",
        "Float",
        "Double",
        "NativePort",
        // Mirrors
        "Mirror",
        "ClassMirror",
        "MethodMirror",
        "VariableMirror",
        "ParameterMirror",
        "TypeMirror",
        "LibraryMirror",
        "InstanceMirror",
        "DeclarationMirror",
        "MirrorSystem",
        "IsolateMirror",
        // Developer
        "Extension",
        "ExtensionType",
        "ServiceExtension",
        "Timeline",
        "TimelineTask",
        "UserTag",
        "Profiler",
        "Debug",
        // UI (Flutter)
        "Color",
        "TextStyle",
        "EdgeInsets",
        "BorderRadius",
        "BoxDecoration",
        "Widget",
        "StatelessWidget",
        "StatefulWidget",
        "State",
        "BuildContext",
        "Key",
        "LocalKey",
        "UniqueKey",
        "ValueKey",
        "GlobalKey",
        "MaterialApp",
        "CupertinoApp",
        "Scaffold",
        "AppBar",
        "Text",
        "Icon",
        "IconButton",
        "ElevatedButton",
        "TextButton",
        "OutlinedButton",
        "FloatingActionButton",
        "ListView",
        "GridView",
        "Column",
        "Row",
        "Stack",
        "Container",
        "Padding",
        "Center",
        "Align",
        "Expanded",
        "Flexible",
        "SizedBox",
        "Spacer",
        "Divider",
        "Card",
        "Image",
        "NetworkImage",
        "AssetImage",
        "FileImage",
        "MemoryImage",
        "ExactAssetImage",
        "StreamBuilder",
        "FutureBuilder",
        "ValueListenableBuilder",
        "AnimatedBuilder",
        "LayoutBuilder",
        "OrientationBuilder",
        "MediaQuery",
        "Theme",
        "Navigator",
        "Route",
        "MaterialPageRoute",
        "PageRouteBuilder",
        "Dialog",
        "showDialog",
        "showModalBottomSheet",
        "showSnackBar",
        "SnackBar",
        "BottomSheet",
        "Drawer",
        "PopupMenuButton",
        "DropdownButton",
        "Checkbox",
        "Radio",
        "Switch",
        "Slider",
        "RangeSlider",
        "TextField",
        "TextFormField",
        "Form",
        "FormField",
        "InputDecoration",
        "FocusNode",
        "FocusScope",
        "ScrollController",
        "ScrollPhysics",
        "AlwaysScrollableScrollPhysics",
        "NeverScrollableScrollPhysics",
        "BouncingScrollPhysics",
        "ClampingScrollPhysics",
        "FixedExtentScrollPhysics",
        "PageScrollPhysics",
        "InfiniteScrollPhysics",
    ];

    // Built-in functions
    pub const BUILTIN_FUNCTIONS: &[&str] = &[
        // Core functions
        "print",
        "identical",
        "identityHashCode",
        "hashCode",
        "toString",
        "noSuchMethod",
        "runtimeType",
        // Type checks
        "is",
        "is!",
        "as",
        // Collection functions
        "length",
        "isEmpty",
        "isNotEmpty",
        "first",
        "last",
        "single",
        "firstWhere",
        "lastWhere",
        "singleWhere",
        "elementAt",
        "contains",
        "forEach",
        "map",
        "where",
        "whereType",
        "expand",
        "followedBy",
        "reduce",
        "fold",
        "any",
        "every",
        "join",
        "toList",
        "toSet",
        "asMap",
        "cast",
        "castFrom",
        "add",
        "addAll",
        "remove",
        "removeWhere",
        "retainWhere",
        "clear",
        "shuffle",
        "sort",
        "indexOf",
        "lastIndexOf",
        "insert",
        "insertAll",
        "removeAt",
        "removeLast",
        "removeRange",
        "fillRange",
        "setRange",
        "replaceRange",
        "getRange",
        "sublist",
        "asUnmodifiableView",
        "reversed",
        "setAll",
        "putIfAbsent",
        "update",
        "updateAll",
        "removeAll",
        "retainAll",
        "keys",
        "values",
        "entries",
        "addEntries",
        "mapEntries",
        // String functions
        "substring",
        "startsWith",
        "endsWith",
        "indexOf",
        "lastIndexOf",
        "contains",
        "replaceAll",
        "replaceFirst",
        "replaceRange",
        "split",
        "splitMapJoin",
        "trim",
        "trimLeft",
        "trimRight",
        "padLeft",
        "padRight",
        "toLowerCase",
        "toUpperCase",
        "codeUnits",
        "runes",
        "characters",
        "compareTo",
        "allMatches",
        "matchAsPrefix",
        // Math functions
        "min",
        "max",
        "pow",
        "exp",
        "log",
        "sqrt",
        "hypot",
        "sin",
        "cos",
        "tan",
        "asin",
        "acos",
        "atan",
        "atan2",
        "sinh",
        "cosh",
        "tanh",
        "round",
        "floor",
        "ceil",
        "truncate",
        "abs",
        "sign",
        "remainder",
        "modulo",
        "clamp",
        "toInt",
        "toDouble",
        "toStringAsFixed",
        "toStringAsExponential",
        "toStringAsPrecision",
        "isFinite",
        "isInfinite",
        "isNaN",
        "isNegative",
        "isEven",
        "isOdd",
        "gcd",
        "lcm",
        "truncateToDouble",
        "roundToDouble",
        "floorToDouble",
        "ceilToDouble",
        // Async functions
        "then",
        "catchError",
        "whenComplete",
        "timeout",
        "asStream",
        "await",
        "async",
        "async*",
        "sync*",
        "yield",
        "yield*",
        "listen",
        "onListen",
        "onPause",
        "onResume",
        "onCancel",
        "pause",
        "resume",
        "cancel",
        "pipe",
        "drain",
        "transform",
        "handleError",
        "handleError",
        "first",
        "last",
        "single",
        "elementAt",
        "forEach",
        "map",
        "asyncMap",
        "asyncExpand",
        "where",
        "skip",
        "skipWhile",
        "take",
        "takeWhile",
        "toList",
        "toSet",
        "reduce",
        "fold",
        "any",
        "every",
        "contains",
        "join",
        "length",
        "isEmpty",
        "isNotEmpty",
        "firstWhere",
        "lastWhere",
        "singleWhere",
        "elementAt",
        "distinct",
        "expand",
        "followedBy",
        "handleError",
        "intervalTransform",
        "sample",
        "debounce",
        "throttle",
        "buffer",
        "bufferCount",
        "bufferTime",
        "delay",
        "doOnData",
        "doOnError",
        "doOnCancel",
        "doOnListen",
        "doOnPause",
        "doOnResume",
        "retry",
        "retryWhen",
        "startWith",
        "startWithMany",
        "switchMap",
        "exhaustMap",
        "concatMap",
        "mergeMap",
        "zipWith",
        "combineLatestWith",
        "withLatestFrom",
        "scan",
        "onErrorResumeNext",
        "onErrorReturn",
        "onErrorReturnWith",
        // Convert functions
        "jsonDecode",
        "jsonEncode",
        "json.decode",
        "json.encode",
        "utf8.decode",
        "utf8.encode",
        "latin1.decode",
        "latin1.encode",
        "ascii.decode",
        "ascii.encode",
        "base64.encode",
        "base64.decode",
        "htmlEscape.convert",
        // IO functions
        "readAsString",
        "readAsStringSync",
        "readAsBytes",
        "readAsBytesSync",
        "readAsLines",
        "readAsLinesSync",
        "writeAsString",
        "writeAsStringSync",
        "writeAsBytes",
        "writeAsBytesSync",
        "writeAsLines",
        "writeAsLinesSync",
        "copy",
        "copySync",
        "move",
        "moveSync",
        "delete",
        "deleteSync",
        "create",
        "createSync",
        "exists",
        "existsSync",
        "stat",
        "statSync",
        "list",
        "listSync",
        "open",
        "openSync",
        "rename",
        "renameSync",
        "resolveSymbolicLinks",
        "resolveSymbolicLinksSync",
        "watch",
        "absolute",
        "parent",
        "path",
        "uri",
        "directory",
        "file",
        "link",
        "type",
        // Isolate functions
        "spawn",
        "spawnUri",
        "kill",
        "ping",
        "addOnExitListener",
        "removeOnExitListener",
        "setErrorsFatal",
        "addErrorListener",
        "removeErrorListener",
        "pause",
        "resume",
        "getErrors",
        "errors",
        // Mirror functions
        "reflect",
        "reflectClass",
        "reflectLibrary",
        "reflectType",
        "current",
        "newInstance",
        "invoke",
        "getField",
        "setField",
        "delegate",
        "metadata",
        "annotations",
        "isPrivate",
        "isTopLevel",
        "owner",
        "qualifiedName",
        "simpleName",
        // Developer functions
        "debugger",
        "log",
        "inspect",
        "registerExtension",
        "postEvent",
        "getServiceExtensions",
        "getIsolateID",
        "getMajorVersion",
        "getMinorVersion",
        "getUserTag",
        "clearUserTag",
        // Flutter functions
        "setState",
        "initState",
        "dispose",
        "build",
        "didUpdateWidget",
        "didChangeDependencies",
        "deactivate",
        "activate",
        "reassemble",
        "didChangeAppLifecycleState",
        "didHaveMemoryPressure",
        "didChangeAccessibilityFeatures",
        "didChangeLocales",
        "didChangeMetrics",
        "didChangeTextScaleFactor",
        "didChangePlatformBrightness",
        "didChangeInputMode",
        "performRebuild",
        "performResize",
        "performLayout",
        "paint",
        "hitTest",
        "handleEvent",
        "applyPaintTransform",
        "compute",
        "scheduleFrame",
        "scheduleBuildFor",
        "scheduleMicrotask",
        "scheduleTask",
        "scheduleDelayedTask",
        "scheduleTick",
        "unscheduleTick",
        "runApp",
        "WidgetsBinding",
        "renderObject",
        "element",
        "findRenderObject",
        "findAncestorWidgetOfExactType",
        "findAncestorStateOfType",
        "findRootAncestorStateOfType",
        "visitAncestorElements",
        "visitChildElements",
        "dependOnInheritedWidgetOfExactType",
        "dependOnInheritedElement",
        "getInheritedWidgetOfExactType",
        "notifyDependent",
        "updateShouldNotify",
        "mount",
        "unmount",
        "update",
        "insertRenderObjectChild",
        "moveRenderObjectChild",
        "removeRenderObjectChild",
        "attach",
        "detach",
        "adoptChild",
        "dropChild",
        "markNeedsLayout",
        "markNeedsPaint",
        "markNeedsCompositingBitsUpdate",
        "markNeedsSemanticsUpdate",
        "scheduleInitialLayout",
        "scheduleInitialPaint",
        "replaceRootLayer",
        "debugRegisterRepaintBoundary",
        "debugResetShouldPaintInheritance",
        "debugFillProperties",
        "debugDescribeChildren",
        "toDiagnosticsNode",
        "toStringShort",
        "toStringDeep",
        "toStringShallow",
    ];

    // Common methods
    pub const COMMON_METHODS: &[&str] = &[
        // Object methods
        "toString",
        "hashCode",
        "runtimeType",
        "noSuchMethod",
        "==",
        // String methods
        "length",
        "isEmpty",
        "isNotEmpty",
        "substring",
        "startsWith",
        "endsWith",
        "contains",
        "indexOf",
        "lastIndexOf",
        "replaceAll",
        "replaceFirst",
        "split",
        "trim",
        "trimLeft",
        "trimRight",
        "padLeft",
        "padRight",
        "toLowerCase",
        "toUpperCase",
        "compareTo",
        "codeUnits",
        "runes",
        // Collection methods
        "add",
        "addAll",
        "remove",
        "removeAt",
        "removeLast",
        "removeWhere",
        "retainWhere",
        "clear",
        "shuffle",
        "sort",
        "indexOf",
        "lastIndexOf",
        "insert",
        "insertAll",
        "getRange",
        "setRange",
        "removeRange",
        "fillRange",
        "replaceRange",
        "sublist",
        "asMap",
        "reversed",
        "first",
        "last",
        "single",
        "firstWhere",
        "lastWhere",
        "singleWhere",
        "elementAt",
        "forEach",
        "map",
        "where",
        "whereType",
        "expand",
        "followedBy",
        "reduce",
        "fold",
        "any",
        "every",
        "join",
        "toList",
        "toSet",
        "cast",
        "contains",
        "keys",
        "values",
        "entries",
        "putIfAbsent",
        "update",
        "updateAll",
        "addEntries",
        // Map methods
        "length",
        "isEmpty",
        "isNotEmpty",
        "containsKey",
        "containsValue",
        "[]",
        "[]=",
        "remove",
        "clear",
        "forEach",
        "keys",
        "values",
        "entries",
        "addAll",
        "addEntries",
        "cast",
        "copy",
        "map",
        // Future methods
        "then",
        "catchError",
        "whenComplete",
        "timeout",
        "asStream",
        // Stream methods
        "listen",
        "pipe",
        "drain",
        "transform",
        "first",
        "last",
        "single",
        "elementAt",
        "forEach",
        "map",
        "asyncMap",
        "asyncExpand",
        "where",
        "skip",
        "skipWhile",
        "take",
        "takeWhile",
        "toList",
        "toSet",
        "reduce",
        "fold",
        "any",
        "every",
        "contains",
        "join",
        "length",
        "isEmpty",
        "isNotEmpty",
        "firstWhere",
        "lastWhere",
        "singleWhere",
        "distinct",
        "expand",
        "followedBy",
        "handleError",
        // File/Directory methods
        "readAsString",
        "readAsBytes",
        "readAsLines",
        "writeAsString",
        "writeAsBytes",
        "writeAsLines",
        "copy",
        "move",
        "delete",
        "create",
        "exists",
        "stat",
        "list",
        "open",
        "rename",
        "absolute",
        "parent",
        "path",
        "uri",
        // Widget methods
        "build",
        "setState",
        "initState",
        "dispose",
        "didUpdateWidget",
        "didChangeDependencies",
        "deactivate",
        "createElement",
        "canUpdate",
        "debugFillProperties",
    ];
}

// Generate simple containment check functions
impl_list_checker!(
    DartStdlibDetector,
    [
        (BUILTIN_TYPES, is_builtin_type),
        (BUILTIN_FUNCTIONS, is_builtin_function),
        (COMMON_METHODS, is_common_method),
    ]
);

impl DartStdlibDetector {
    /// Check if a qualified path is from Dart stdlib
    pub fn is_dart_path(path: &str) -> bool {
        // Dart uses : for core libraries (dart:core, dart:async, etc.)
        // and package: for packages (package:flutter/material, etc.)
        for &package in Self::DART_PACKAGES {
            if path == package || path.starts_with(&format!("{}/", package)) {
                return true;
            }
        }

        // Check for dart: prefix
        if path.starts_with("dart:") {
            return true;
        }

        // Check for package: prefix (common packages)
        if path.starts_with("package:") {
            // Extract package name
            let package_part = path.strip_prefix("package:").unwrap_or("");
            let package_name = package_part.split('/').next().unwrap_or("");

            // Check if it's a known package
            for &known_package in Self::DART_PACKAGES {
                if known_package.starts_with("package:") {
                    let known_name = known_package
                        .strip_prefix("package:")
                        .unwrap_or("")
                        .split('/')
                        .next()
                        .unwrap_or("");
                    if package_name == known_name {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if a type name is from the Dart standard library
    pub fn is_stdlib_type(name: &str) -> bool {
        Self::is_builtin_type(name) || Self::is_dart_path(name)
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for builtin type
        if Self::is_builtin_type(call_name) {
            return true;
        }

        // Check for builtin function
        if Self::is_builtin_function(call_name) {
            return true;
        }

        // Check for common method
        if Self::is_common_method(call_name) {
            return true;
        }

        // Check for qualified path (e.g., dart:core.print, package:flutter/material.dart)
        if call_name.contains(':') || call_name.contains('.') {
            // Check if the full path is from Dart stdlib
            if Self::is_dart_path(call_name) {
                return true;
            }

            // Check for method calls on builtin types
            if call_name.contains('.') {
                let parts: Vec<&str> = call_name.split('.').collect();
                if parts.len() >= 2 {
                    let receiver = parts[0];
                    let _method = parts[1];

                    // Check if receiver is a builtin type
                    if Self::is_builtin_type(receiver) {
                        return true;
                    }

                    // Check for common collection method patterns
                    if Self::is_common_method(_method) {
                        // If the method is a common stdlib method, it's likely stdlib
                        return true;
                    }
                }
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
            // Most call types in Dart use the same detection logic
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::CallbackCall
            | RelationType::GenericCall => {
                // For Dart, use the legacy detection logic
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
    fn test_is_builtin_type() {
        assert!(DartStdlibDetector::is_builtin_type("String"));
        assert!(DartStdlibDetector::is_builtin_type("int"));
        assert!(DartStdlibDetector::is_builtin_type("List"));
        assert!(DartStdlibDetector::is_builtin_type("Future"));
        assert!(DartStdlibDetector::is_builtin_type("Widget"));
        assert!(!DartStdlibDetector::is_builtin_type("MyClass"));
    }

    #[test]
    fn test_is_builtin_function() {
        assert!(DartStdlibDetector::is_builtin_function("print"));
        assert!(DartStdlibDetector::is_builtin_function("setState"));
        assert!(DartStdlibDetector::is_builtin_function("jsonEncode"));
        assert!(!DartStdlibDetector::is_builtin_function("myFunction"));
    }

    #[test]
    fn test_is_common_method() {
        assert!(DartStdlibDetector::is_common_method("toString"));
        assert!(DartStdlibDetector::is_common_method("length"));
        assert!(DartStdlibDetector::is_common_method("map"));
        assert!(DartStdlibDetector::is_common_method("where"));
        assert!(!DartStdlibDetector::is_common_method("myMethod"));
    }

    #[test]
    fn test_is_dart_path() {
        assert!(DartStdlibDetector::is_dart_path("dart:core"));
        assert!(DartStdlibDetector::is_dart_path("dart:async"));
        assert!(DartStdlibDetector::is_dart_path("dart:io/File"));
        assert!(DartStdlibDetector::is_dart_path("package:flutter/material"));
        assert!(DartStdlibDetector::is_dart_path(
            "package:flutter/material.dart"
        ));
        assert!(!DartStdlibDetector::is_dart_path("my_package/my_file"));
    }

    #[test]
    fn test_is_stdlib_call() {
        // Builtin types
        assert!(DartStdlibDetector::is_stdlib_call("String"));
        assert!(DartStdlibDetector::is_stdlib_call("int"));
        assert!(DartStdlibDetector::is_stdlib_call("List"));
        assert!(DartStdlibDetector::is_stdlib_call("Future"));
        assert!(DartStdlibDetector::is_stdlib_call("Widget"));

        // Builtin functions
        assert!(DartStdlibDetector::is_stdlib_call("print"));
        assert!(DartStdlibDetector::is_stdlib_call("setState"));
        assert!(DartStdlibDetector::is_stdlib_call("jsonEncode"));

        // Common methods
        assert!(DartStdlibDetector::is_stdlib_call("toString"));
        assert!(DartStdlibDetector::is_stdlib_call("length"));
        assert!(DartStdlibDetector::is_stdlib_call("map"));
        assert!(DartStdlibDetector::is_stdlib_call("where"));

        // Qualified paths
        assert!(DartStdlibDetector::is_stdlib_call("dart:core.print"));
        assert!(DartStdlibDetector::is_stdlib_call("dart:io.File"));
        assert!(DartStdlibDetector::is_stdlib_call(
            "package:flutter/material.dart"
        ));

        // Method calls on builtin types
        assert!(DartStdlibDetector::is_stdlib_call("list.map"));
        assert!(DartStdlibDetector::is_stdlib_call("string.length"));
        assert!(DartStdlibDetector::is_stdlib_call("future.then"));

        // Negative cases
        assert!(!DartStdlibDetector::is_stdlib_call("MyClass"));
        assert!(!DartStdlibDetector::is_stdlib_call("myFunction"));
        assert!(!DartStdlibDetector::is_stdlib_call("my_package.myFunction"));
    }
}
