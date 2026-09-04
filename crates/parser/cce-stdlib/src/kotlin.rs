// Kotlin Standard Library Detector
// Handles detection of Kotlin standard library entities

pub struct KotlinStdlibDetector;

impl KotlinStdlibDetector {
    // Kotlin standard packages
    pub const KOTLIN_PACKAGES: &[&str] = &[
        // Core packages
        "kotlin",
        "kotlin.annotation",
        "kotlin.collections",
        "kotlin.comparisons",
        "kotlin.concurrent",
        "kotlin.contracts",
        "kotlin.coroutines",
        "kotlin.coroutines.intrinsics",
        "kotlin.experimental",
        "kotlin.io",
        "kotlin.jvm",
        "kotlin.jvm.functions",
        "kotlin.jvm.internal",
        "kotlin.jvm.optionals",
        "kotlin.math",
        "kotlin.native",
        "kotlin.native.concurrent",
        "kotlin.properties",
        "kotlin.random",
        "kotlin.ranges",
        "kotlin.reflect",
        "kotlin.reflect.full",
        "kotlin.reflect.jvm",
        "kotlin.sequences",
        "kotlin.text",
        "kotlin.time",
        "kotlin.wasm",
        // Collections
        "kotlin.collections",
        "kotlin.collections.unsigned",
        // Coroutines
        "kotlinx.coroutines",
        "kotlinx.coroutines.channels",
        "kotlinx.coroutines.flow",
        "kotlinx.coroutines.selects",
        "kotlinx.coroutines.sync",
        "kotlinx.coroutines.test",
        // Serialization
        "kotlinx.serialization",
        "kotlinx.serialization.builtins",
        "kotlinx.serialization.json",
        "kotlinx.serialization.protobuf",
        "kotlinx.serialization.cbor",
        "kotlinx.serialization.properties",
        // DateTime
        "kotlinx.datetime",
        // HTML
        "kotlinx.html",
        "kotlinx.html.dom",
        "kotlinx.html.js",
        // Testing
        "kotlin.test",
        "kotlin.test.junit",
        "kotlin.test.junit5",
        "kotlin.test.testng",
        // Scripting
        "kotlin.script",
        "kotlin.script.experimental",
        "kotlin.script.experimental.api",
        "kotlin.script.experimental.host",
        "kotlin.script.experimental.jvm",
        "kotlin.script.experimental.jvmhost",
        // Java interop
        "kotlin.jvm",
        "kotlin.jvm.functions",
        "kotlin.jvm.internal",
        "kotlin.jvm.optionals",
        // JavaScript interop
        "kotlin.js",
        "kotlin.js.json",
        // Native interop
        "kotlin.native",
        "kotlin.native.concurrent",
        "kotlin.native.internal",
        // Multiplatform
        "kotlinx.atomicfu",
        "kotlinx.benchmark",
        "kotlinx.cli",
        "kotlinx.collections.immutable",
        "kotlinx.collections.immutable.internal",
        "kotlinx.collections.immutable.implementations",
        // Also include Java packages for interop
        "java.lang",
        "java.util",
        "java.io",
        "java.nio",
        "java.net",
        "java.math",
        "java.text",
        "java.time",
        "java.sql",
        "javax.sql",
    ];

    // Built-in types
    pub const BUILTIN_TYPES: &[&str] = &[
        // Basic types
        "Any",
        "Any?",
        "Unit",
        "Unit?",
        "Nothing",
        "Nothing?",
        "Boolean",
        "Boolean?",
        "Byte",
        "Byte?",
        "Short",
        "Short?",
        "Int",
        "Int?",
        "Long",
        "Long?",
        "Float",
        "Float?",
        "Double",
        "Double?",
        "Char",
        "Char?",
        "String",
        "String?",
        "Array",
        "Array?",
        "ByteArray",
        "ByteArray?",
        "ShortArray",
        "ShortArray?",
        "IntArray",
        "IntArray?",
        "LongArray",
        "LongArray?",
        "FloatArray",
        "FloatArray?",
        "DoubleArray",
        "DoubleArray?",
        "CharArray",
        "CharArray?",
        "BooleanArray",
        "BooleanArray?",
        // Unsigned types
        "UByte",
        "UByte?",
        "UShort",
        "UShort?",
        "UInt",
        "UInt?",
        "ULong",
        "ULong?",
        "UByteArray",
        "UByteArray?",
        "UShortArray",
        "UShortArray?",
        "UIntArray",
        "UIntArray?",
        "ULongArray",
        "ULongArray?",
        // Function types
        "Function",
        "Function0",
        "Function1",
        "Function2",
        "Function3",
        "Function4",
        "Function5",
        "Function6",
        "Function7",
        "Function8",
        "Function9",
        "Function10",
        "Function11",
        "Function12",
        "Function13",
        "Function14",
        "Function15",
        "Function16",
        "Function17",
        "Function18",
        "Function19",
        "Function20",
        "Function21",
        "Function22",
        // KFunction types
        "KFunction",
        "KFunction0",
        "KFunction1",
        "KFunction2",
        "KFunction3",
        "KFunction4",
        "KFunction5",
        "KFunction6",
        "KFunction7",
        "KFunction8",
        "KFunction9",
        "KFunction10",
        "KFunction11",
        "KFunction12",
        "KFunction13",
        "KFunction14",
        "KFunction15",
        "KFunction16",
        "KFunction17",
        "KFunction18",
        "KFunction19",
        "KFunction20",
        "KFunction21",
        "KFunction22",
        // KProperty types
        "KProperty",
        "KProperty0",
        "KProperty1",
        "KProperty2",
        "KMutableProperty",
        "KMutableProperty0",
        "KMutableProperty1",
        "KMutableProperty2",
        // Collections
        "List",
        "List?",
        "MutableList",
        "MutableList?",
        "Set",
        "Set?",
        "MutableSet",
        "MutableSet?",
        "Map",
        "Map?",
        "MutableMap",
        "MutableMap?",
        "Iterable",
        "Iterable?",
        "MutableIterable",
        "MutableIterable?",
        "Collection",
        "Collection?",
        "MutableCollection",
        "MutableCollection?",
        "Sequence",
        "Sequence?",
        "Iterator",
        "Iterator?",
        "MutableIterator",
        "MutableIterator?",
        "ListIterator",
        "ListIterator?",
        "MutableListIterator",
        "MutableListIterator?",
        // Ranges
        "IntRange",
        "IntRange?",
        "LongRange",
        "LongRange?",
        "CharRange",
        "CharRange?",
        "UIntRange",
        "UIntRange?",
        "ULongRange",
        "ULongRange?",
        "ClosedRange",
        "ClosedRange?",
        // Comparables
        "Comparable",
        "Comparable?",
        "Comparator",
        "Comparator?",
        // Tuples
        "Pair",
        "Pair?",
        "Triple",
        "Triple?",
        // Result
        "Result",
        "Result?",
        // Enum
        "Enum",
        "Enum?",
        // Annotation
        "Annotation",
        "Annotation?",
        // Throwable
        "Throwable",
        "Throwable?",
        "Exception",
        "Exception?",
        "RuntimeException",
        "RuntimeException?",
        "Error",
        "Error?",
        // Deprecation
        "DeprecationLevel",
        "DeprecationLevel?",
        "ReplaceWith",
        "ReplaceWith?",
        // Coroutines
        "Continuation",
        "Continuation?",
        "CoroutineContext",
        "CoroutineContext?",
        "CoroutineScope",
        "CoroutineScope?",
        "Job",
        "Job?",
        "Deferred",
        "Deferred?",
        "Channel",
        "Channel?",
        "ReceiveChannel",
        "ReceiveChannel?",
        "SendChannel",
        "SendChannel?",
        "Flow",
        "Flow?",
        "MutableStateFlow",
        "MutableStateFlow?",
        "MutableSharedFlow",
        "MutableSharedFlow?",
        // Time
        "Duration",
        "Duration?",
        "Instant",
        "Instant?",
        "Clock",
        "Clock?",
        "TimeSource",
        "TimeSource?",
        "TimeMark",
        "TimeMark?",
        // Random
        "Random",
        "Random?",
        "Random.Default",
        // Math
        "Math",
        "Math?",
        // Reflection
        "KClass",
        "KClass?",
        "KCallable",
        "KCallable?",
        "KProperty",
        "KProperty?",
        "KFunction",
        "KFunction?",
        "KType",
        "KType?",
        "KTypeParameter",
        "KTypeParameter?",
        "KTypeProjection",
        "KTypeProjection?",
        "KVariance",
        "KVariance?",
    ];

    // Built-in functions (top-level)
    pub const BUILTIN_FUNCTIONS: &[&str] = &[
        // Type checks and casts
        "is",
        "!is",
        "as",
        "as?",
        "typeOf",
        "typeOfNullable",
        "assert",
        "check",
        "require",
        "requireNotNull",
        "checkNotNull",
        "error",
        "TODO",
        "lazy",
        "lazyOf",
        // Scope functions
        "let",
        "run",
        "with",
        "apply",
        "also",
        "takeIf",
        "takeUnless",
        "repeat",
        "runCatching",
        // Collection builders
        "listOf",
        "listOfNotNull",
        "mutableListOf",
        "arrayListOf",
        "setOf",
        "mutableSetOf",
        "hashSetOf",
        "linkedSetOf",
        "sortedSetOf",
        "mapOf",
        "mutableMapOf",
        "hashMapOf",
        "linkedMapOf",
        "sortedMapOf",
        "emptyList",
        "emptySet",
        "emptyMap",
        "emptyArray",
        "arrayOf",
        "arrayOfNulls",
        "booleanArrayOf",
        "byteArrayOf",
        "shortArrayOf",
        "intArrayOf",
        "longArrayOf",
        "floatArrayOf",
        "doubleArrayOf",
        "charArrayOf",
        "ubyteArrayOf",
        "ushortArrayOf",
        "uintArrayOf",
        "ulongArrayOf",
        "sequenceOf",
        "generateSequence",
        "buildList",
        "buildSet",
        "buildMap",
        // Pair and Triple
        "to",
        "Pair",
        "Triple",
        // Comparators
        "compareBy",
        "compareByDescending",
        "thenBy",
        "thenByDescending",
        "naturalOrder",
        "reverseOrder",
        "nullsFirst",
        "nullsLast",
        // Ranges
        "rangeTo",
        "rangeUntil",
        "downTo",
        "until",
        "coerceAtLeast",
        "coerceAtMost",
        "coerceIn",
        // Math functions
        "abs",
        "sign",
        "ceil",
        "floor",
        "truncate",
        "round",
        "roundToInt",
        "roundToLong",
        "sqrt",
        "hypot",
        "exp",
        "ln",
        "log10",
        "log2",
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
        "asinh",
        "acosh",
        "atanh",
        "pow",
        "min",
        "max",
        "minOf",
        "maxOf",
        "minOfThree",
        "maxOfThree",
        // Random functions
        "random",
        "randomOrNull",
        "shuffled",
        "shuffle",
        // String functions
        "toString",
        "toByte",
        "toShort",
        "toInt",
        "toLong",
        "toFloat",
        "toDouble",
        "toChar",
        "toBoolean",
        "toUByte",
        "toUShort",
        "toUInt",
        "toULong",
        "decodeToString",
        "encodeToByteArray",
        "capitalize",
        "decapitalize",
        "commonPrefixWith",
        "commonSuffixWith",
        "contains",
        "endsWith",
        "startsWith",
        "equals",
        "equalsIgnoreCase",
        "filter",
        "filterIndexed",
        "filterNot",
        "find",
        "findLast",
        "first",
        "firstOrNull",
        "firstNotNullOf",
        "firstNotNullOfOrNull",
        "forEach",
        "forEachIndexed",
        "indexOf",
        "indexOfFirst",
        "indexOfLast",
        "isBlank",
        "isEmpty",
        "isNotEmpty",
        "isNotBlank",
        "isNullOrBlank",
        "isNullOrEmpty",
        "last",
        "lastOrNull",
        "lastIndexOf",
        "length",
        "lines",
        "lowercase",
        "uppercase",
        "matches",
        "padEnd",
        "padStart",
        "plus",
        "removePrefix",
        "removeRange",
        "removeSuffix",
        "removeSurrounding",
        "replace",
        "replaceAfter",
        "replaceAfterLast",
        "replaceBefore",
        "replaceBeforeLast",
        "replaceFirst",
        "replaceFirstChar",
        "replaceIndent",
        "replaceIndentByMargin",
        "replaceRange",
        "reversed",
        "slice",
        "split",
        "splitToSequence",
        "substring",
        "substringAfter",
        "substringAfterLast",
        "substringBefore",
        "substringBeforeLast",
        "take",
        "takeLast",
        "takeWhile",
        "takeLastWhile",
        "takeIf",
        "takeUnless",
        "trim",
        "trimEnd",
        "trimStart",
        "trimMargin",
        "trimIndent",
        "windowed",
        "withIndex",
        "zip",
        "zipWithNext",
        // Collection functions
        "all",
        "any",
        "associate",
        "associateBy",
        "associateWith",
        "associateByTo",
        "associateWithTo",
        "associateTo",
        "average",
        "chunked",
        "contains",
        "count",
        "distinct",
        "distinctBy",
        "drop",
        "dropLast",
        "dropLastWhile",
        "dropWhile",
        "elementAt",
        "elementAtOrElse",
        "elementAtOrNull",
        "filter",
        "filterIndexed",
        "filterIsInstance",
        "filterNot",
        "filterNotNull",
        "find",
        "findLast",
        "first",
        "firstNotNullOf",
        "firstNotNullOfOrNull",
        "firstOrNull",
        "flatMap",
        "flatMapTo",
        "flatten",
        "fold",
        "foldIndexed",
        "foldRight",
        "foldRightIndexed",
        "forEach",
        "forEachIndexed",
        "groupBy",
        "groupByTo",
        "groupingBy",
        "ifEmpty",
        "indexOf",
        "indexOfFirst",
        "indexOfLast",
        "intersect",
        "joinTo",
        "joinToString",
        "last",
        "lastIndexOf",
        "lastOrNull",
        "map",
        "mapIndexed",
        "mapIndexedNotNull",
        "mapIndexedNotNullTo",
        "mapIndexedTo",
        "mapNotNull",
        "mapNotNullTo",
        "mapTo",
        "max",
        "maxBy",
        "maxByOrNull",
        "maxOf",
        "maxOfOrNull",
        "maxOfWith",
        "maxOfWithOrNull",
        "maxOrNull",
        "maxWith",
        "maxWithOrNull",
        "min",
        "minBy",
        "minByOrNull",
        "minOf",
        "minOfOrNull",
        "minOfWith",
        "minOfWithOrNull",
        "minOrNull",
        "minWith",
        "minWithOrNull",
        "minus",
        "minusElement",
        "none",
        "onEach",
        "onEachIndexed",
        "partition",
        "plus",
        "plusElement",
        "random",
        "randomOrNull",
        "reduce",
        "reduceIndexed",
        "reduceOrNull",
        "reduceIndexedOrNull",
        "reduceRight",
        "reduceRightIndexed",
        "reduceRightOrNull",
        "reduceRightIndexedOrNull",
        "requireNoNulls",
        "reversed",
        "runningFold",
        "runningFoldIndexed",
        "runningReduce",
        "runningReduceIndexed",
        "scan",
        "scanIndexed",
        "scanReduce",
        "scanReduceIndexed",
        "shuffled",
        "shuffle",
        "single",
        "singleOrNull",
        "slice",
        "sorted",
        "sortedBy",
        "sortedByDescending",
        "sortedDescending",
        "sortedWith",
        "subtract",
        "sum",
        "sumBy",
        "sumByDouble",
        "sumOf",
        "take",
        "takeLast",
        "takeLastWhile",
        "takeWhile",
        "toCollection",
        "toHashSet",
        "toList",
        "toMutableList",
        "toMutableSet",
        "toSet",
        "toSortedSet",
        "toSortedSetWith",
        "union",
        "windowed",
        "withIndex",
        "zip",
        "zipWithNext",
        // Coroutine functions
        "async",
        "launch",
        "runBlocking",
        "withContext",
        "withTimeout",
        "withTimeoutOrNull",
        "delay",
        "yield",
        "awaitAll",
        "awaitCancellation",
        "cancelAndJoin",
        "cancelChildren",
        "ensureActive",
        "isActive",
        "isCompleted",
        "isCancelled",
        "start",
        "join",
        "cancel",
        "invokeOnCompletion",
        "getCompletionExceptionOrNull",
        // Flow functions
        "flow",
        "flowOf",
        "asFlow",
        "channelFlow",
        "callbackFlow",
        "stateFlow",
        "sharedFlow",
        "emit",
        "emitAll",
        "catch",
        "retry",
        "retryWhen",
        "buffer",
        "conflate",
        "collect",
        "collectLatest",
        "combine",
        "combineTransform",
        "debounce",
        "distinctUntilChanged",
        "filter",
        "filterNot",
        "filterIsInstance",
        "filterNotNull",
        "flatMapConcat",
        "flatMapMerge",
        "flatMapLatest",
        "fold",
        "launchIn",
        "map",
        "mapLatest",
        "mapNotNull",
        "merge",
        "onEach",
        "onStart",
        "onCompletion",
        "onEmpty",
        "onSubscription",
        "reduce",
        "runningFold",
        "runningReduce",
        "sample",
        "scan",
        "single",
        "singleOrNull",
        "take",
        "takeWhile",
        "transform",
        "transformLatest",
        "withIndex",
        "zip",
        // Reflection functions
        "typeOf",
        "typeOfNullable",
        "KClass",
        "KFunction",
        "KProperty",
        "KType",
        "KTypeParameter",
        "KTypeProjection",
        "KVariance",
        "createInstance",
        "createType",
        "starProjectedType",
        "jvmErasure",
        "isSubtypeOf",
        "isSupertypeOf",
        "withNullability",
        // Time functions
        "Duration",
        "toDuration",
        "toIsoString",
        "parse",
        "parseIsoString",
        "plus",
        "minus",
        "times",
        "div",
        "compareTo",
        "inWholeDays",
        "inWholeHours",
        "inWholeMinutes",
        "inWholeSeconds",
        "inWholeMilliseconds",
        "inWholeMicroseconds",
        "inWholeNanoseconds",
        "toDouble",
        "toInt",
        "toLong",
        "abs",
        "absoluteValue",
        "isNegative",
        "isPositive",
        "isFinite",
        "isInfinite",
        "isNaN",
        // Random functions
        "Random",
        "nextInt",
        "nextLong",
        "nextFloat",
        "nextDouble",
        "nextBoolean",
        "nextBytes",
        "nextUInt",
        "nextULong",
        "nextUBytes",
        "nextBits",
        "nextPrintableChar",
        // Annotation functions
        "Annotation",
        "annotationClass",
        "annotations",
        "findAnnotation",
        "findAnnotations",
        "hasAnnotation",
    ];

    // Extension properties
    pub const EXTENSION_PROPERTIES: &[&str] = &[
        // String extensions
        "length",
        "indices",
        "lastIndex",
        // Collection extensions
        "size",
        "indices",
        "lastIndex",
        "isEmpty",
        "isNotEmpty",
        "asSequence",
        "asIterable",
        "asList",
        "asReversed",
        "asReversedView",
        // Array extensions
        "size",
        "indices",
        "lastIndex",
        "isEmpty",
        "isNotEmpty",
        "asList",
        "asSequence",
        "asIterable",
        // Range extensions
        "start",
        "endInclusive",
        "isEmpty",
        // Comparable extensions
        "coerceAtLeast",
        "coerceAtMost",
        "coerceIn",
        // Number extensions
        "toByte",
        "toShort",
        "toInt",
        "toLong",
        "toFloat",
        "toDouble",
        "toChar",
        "toUByte",
        "toUShort",
        "toUInt",
        "toULong",
        "dec",
        "inc",
        "unaryMinus",
        "unaryPlus",
        // Char extensions
        "code",
        "digitToInt",
        "digitToIntOrNull",
        "isDefined",
        "isDigit",
        "isLetter",
        "isLetterOrDigit",
        "isLowerCase",
        "isUpperCase",
        "isTitleCase",
        "isISOControl",
        "isWhitespace",
        "isHighSurrogate",
        "isLowSurrogate",
        "isSurrogate",
        "isSurrogatePair",
        "lowercaseChar",
        "uppercaseChar",
        "titlecaseChar",
        "lowercase",
        "uppercase",
        "titlecase",
        // Boolean extensions
        "and",
        "or",
        "xor",
        "not",
        // Function extensions
        "compose",
        "andThen",
        // Coroutine extensions
        "isActive",
        "isCompleted",
        "isCancelled",
        // Flow extensions
        "value",
        "tryEmit",
        "resetReplayCache",
        "subscriptionCount",
        "replayCache",
    ];

    pub fn is_kotlin_package(package: &str) -> bool {
        Self::KOTLIN_PACKAGES.contains(&package)
    }

    pub fn is_builtin_type(name: &str) -> bool {
        Self::BUILTIN_TYPES.contains(&name)
    }

    pub fn is_builtin_function(name: &str) -> bool {
        Self::BUILTIN_FUNCTIONS.contains(&name)
    }

    pub fn is_extension_property(name: &str) -> bool {
        Self::EXTENSION_PROPERTIES.contains(&name)
    }

    /// Check if a qualified path is from Kotlin stdlib
    pub fn is_kotlin_path(path: &str) -> bool {
        // Kotlin uses . as package separator
        // Check if the path starts with any known Kotlin package
        for &package in Self::KOTLIN_PACKAGES {
            if path == package || path.starts_with(&format!("{}.", package)) {
                return true;
            }
        }

        // Also check first component for top-level packages like "kotlin", "kotlinx"
        let first_component = path.split('.').next().unwrap_or("");
        if first_component == "kotlin" || first_component == "kotlinx" {
            return true;
        }

        false
    }

    /// Check if a type name is from the Kotlin standard library
    pub fn is_stdlib_type(name: &str) -> bool {
        Self::is_builtin_type(name) || Self::is_kotlin_path(name)
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

        // Check for extension property
        if Self::is_extension_property(call_name) {
            return true;
        }

        // Check for qualified path (e.g., kotlin.collections.listOf, kotlin.runCatching)
        if call_name.contains('.') {
            // Check if the full path is from Kotlin stdlib
            if Self::is_kotlin_path(call_name) {
                return true;
            }

            let parts: Vec<&str> = call_name.split('.').collect();
            if parts.len() >= 2 {
                // Check if it's a Kotlin package (top-level)
                let package = parts[0];
                if Self::is_kotlin_package(package) {
                    return true;
                }

                // Check for extension function calls (e.g., "list.map", "string.toUpperCase")
                // In Kotlin, extension functions are called on receivers
                // We need to check if the receiver type is a builtin type
                if parts.len() >= 2 {
                    let receiver = parts[0];
                    let _method = parts[1];

                    // Check if receiver is a builtin type (case-insensitive)
                    // If receiver is a builtin type, assume the method is from stdlib
                    let is_builtin_receiver = Self::BUILTIN_TYPES
                        .iter()
                        .any(|&t| t.trim_end_matches('?').eq_ignore_ascii_case(receiver));
                    if is_builtin_receiver {
                        return true;
                    }

                    // Check for package-level functions
                    if Self::is_kotlin_package(receiver) {
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
            // Most call types in Kotlin use the same detection logic
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::CallbackCall
            | RelationType::GenericCall => {
                // For Kotlin, validate using comprehensive stdlib checks
                Self::is_builtin_type(call_name)
                    || Self::is_builtin_function(call_name)
                    || Self::is_extension_property(call_name)
                    || call_name
                        .split('.')
                        .next()
                        .map(Self::is_kotlin_package)
                        .unwrap_or(false)
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
    fn test_is_kotlin_package() {
        assert!(KotlinStdlibDetector::is_kotlin_package("kotlin"));
        assert!(KotlinStdlibDetector::is_kotlin_package(
            "kotlin.collections"
        ));
        assert!(KotlinStdlibDetector::is_kotlin_package(
            "kotlinx.coroutines"
        ));
        assert!(!KotlinStdlibDetector::is_kotlin_package("com.example"));
    }

    #[test]
    fn test_is_builtin_type() {
        assert!(KotlinStdlibDetector::is_builtin_type("String"));
        assert!(KotlinStdlibDetector::is_builtin_type("Int"));
        assert!(KotlinStdlibDetector::is_builtin_type("List"));
        assert!(KotlinStdlibDetector::is_builtin_type("MutableList"));
        assert!(!KotlinStdlibDetector::is_builtin_type("MyClass"));
    }

    #[test]
    fn test_is_builtin_function() {
        assert!(KotlinStdlibDetector::is_builtin_function("listOf"));
        assert!(KotlinStdlibDetector::is_builtin_function("mapOf"));
        assert!(KotlinStdlibDetector::is_builtin_function("runCatching"));
        assert!(!KotlinStdlibDetector::is_builtin_function("myFunction"));
    }

    #[test]
    fn test_is_extension_property() {
        assert!(KotlinStdlibDetector::is_extension_property("length"));
        assert!(KotlinStdlibDetector::is_extension_property("size"));
        assert!(KotlinStdlibDetector::is_extension_property("isEmpty"));
        assert!(!KotlinStdlibDetector::is_extension_property("myProperty"));
    }

    #[test]
    fn test_is_kotlin_path() {
        assert!(KotlinStdlibDetector::is_kotlin_path("kotlin"));
        assert!(KotlinStdlibDetector::is_kotlin_path(
            "kotlin.collections.listOf"
        ));
        assert!(KotlinStdlibDetector::is_kotlin_path(
            "kotlinx.coroutines.launch"
        ));
        assert!(!KotlinStdlibDetector::is_kotlin_path("com.example.MyClass"));
    }

    #[test]
    fn test_is_stdlib_call() {
        // Builtin types
        assert!(KotlinStdlibDetector::is_stdlib_call("String"));
        assert!(KotlinStdlibDetector::is_stdlib_call("Int"));
        assert!(KotlinStdlibDetector::is_stdlib_call("List"));

        // Builtin functions
        assert!(KotlinStdlibDetector::is_stdlib_call("listOf"));
        assert!(KotlinStdlibDetector::is_stdlib_call("mapOf"));
        assert!(KotlinStdlibDetector::is_stdlib_call("runCatching"));

        // Extension properties
        assert!(KotlinStdlibDetector::is_stdlib_call("length"));
        assert!(KotlinStdlibDetector::is_stdlib_call("size"));

        // Qualified paths
        assert!(KotlinStdlibDetector::is_stdlib_call(
            "kotlin.collections.listOf"
        ));
        assert!(KotlinStdlibDetector::is_stdlib_call(
            "kotlinx.coroutines.launch"
        ));

        // Extension function calls (simplified check)
        assert!(KotlinStdlibDetector::is_stdlib_call("list.map")); // list is builtin type, map is builtin function
        assert!(KotlinStdlibDetector::is_stdlib_call("string.toUpperCase"));

        // Java interop (kotlin.jvm package)
        assert!(KotlinStdlibDetector::is_stdlib_call("kotlin.jvm"));
        assert!(KotlinStdlibDetector::is_stdlib_call("java.lang.String")); // Java package included

        // Negative cases
        assert!(!KotlinStdlibDetector::is_stdlib_call("MyClass"));
        assert!(!KotlinStdlibDetector::is_stdlib_call("com.example.MyClass"));
        assert!(!KotlinStdlibDetector::is_stdlib_call("myFunction"));
    }
}
