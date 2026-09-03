// Java Standard Library Detector
// Handles detection of Java standard library (JDK) entities

pub struct JavaStdlibDetector;

impl JavaStdlibDetector {
    // Java standard packages (java.*, javax.*, etc.)
    pub const JAVA_PACKAGES: &[&str] = &[
        // Core packages
        "java.lang",
        "java.lang.annotation",
        "java.lang.invoke",
        "java.lang.module",
        "java.lang.ref",
        "java.lang.reflect",
        "java.lang.runtime",
        "java.lang.constant",
        "java.lang.instrument",
        // JDK internal packages (not recommended but exist)
        "jdk.internal",
        "jdk.internal.misc",
        "jdk.internal.jimage",
        "jdk.internal.loader",
        "jdk.internal.reflect",
        "jdk.internal.util",
        "jdk.internal.vm",
        // Smart card I/O
        "java.smartcardio",
        // Collections
        "java.util",
        "java.util.concurrent",
        "java.util.concurrent.atomic",
        "java.util.concurrent.locks",
        "java.util.function",
        "java.util.stream",
        "java.util.regex",
        "java.util.random",
        "java.util.spi",
        "java.util.zip",
        "java.util.jar",
        "java.util.logging",
        "java.util.prefs",
        "java.util.ResourceBundle",
        // I/O
        "java.io",
        "java.nio",
        "java.nio.channels",
        "java.nio.charset",
        "java.nio.file",
        "java.nio.file.attribute",
        "java.nio.file.spi",
        // Networking
        "java.net",
        "java.net.http",
        "java.net.spi",
        // Security
        "java.security",
        "java.security.acl",
        "java.security.cert",
        "java.security.interfaces",
        "java.security.spec",
        "java.security.auth",
        "java.security.auth.callback",
        "java.security.auth.login",
        "java.security.auth.spi",
        "java.security.auth.x500",
        "javax.security.auth.kerberos",
        // HTTP Server (com.sun)
        "com.sun.net.httpserver",
        "com.sun.net.httpserver.spi",
        // Cryptography
        "javax.crypto",
        "javax.crypto.interfaces",
        "javax.crypto.spec",
        "javax.net",
        "javax.net.ssl",
        "javax.security",
        "javax.security.auth",
        "javax.security.auth.callback",
        "javax.security.auth.login",
        "javax.security.auth.spi",
        "javax.security.auth.x500",
        "javax.security.cert",
        "javax.security.sasl",
        // XML
        "java.xml",
        "javax.xml",
        "javax.xml.bind",
        "javax.xml.bind.annotation",
        "javax.xml.bind.annotation.adapters",
        "javax.xml.bind.attachment",
        "javax.xml.bind.helpers",
        "javax.xml.bind.util",
        "javax.xml.crypto",
        "javax.xml.crypto.dom",
        "javax.xml.crypto.dsig",
        "javax.xml.crypto.dsig.dom",
        "javax.xml.crypto.dsig.keyinfo",
        "javax.xml.crypto.dsig.spec",
        "javax.xml.datatype",
        "javax.xml.namespace",
        "javax.xml.parsers",
        "javax.xml.soap",
        "javax.xml.stream",
        "javax.xml.transform",
        "javax.xml.transform.dom",
        "javax.xml.transform.sax",
        "javax.xml.transform.stax",
        "javax.xml.transform.stream",
        "javax.xml.validation",
        "javax.xml.xpath",
        // SQL
        "java.sql",
        "javax.sql",
        "javax.sql.rowset",
        "javax.sql.rowset.serial",
        "javax.sql.rowset.spi",
        // Time
        "java.time",
        "java.time.chrono",
        "java.time.format",
        "java.time.temporal",
        "java.time.zone",
        // Math
        "java.math",
        // Text
        "java.text",
        "java.text.spi",
        // Internationalization
        "java.util.locale",
        // Beans
        "java.beans",
        "java.beans.beancontext",
        // Management
        "java.lang.management",
        "javax.management",
        "javax.management.loading",
        "javax.management.modelmbean",
        "javax.management.monitor",
        "javax.management.openmbean",
        "javax.management.relation",
        "javax.management.remote",
        "javax.management.remote.rmi",
        "javax.management.timer",
        // RMI
        "java.rmi",
        "java.rmi.activation",
        "java.rmi.dgc",
        "java.rmi.registry",
        "java.rmi.server",
        "javax.rmi",
        "javax.rmi.CORBA",
        "javax.rmi.ssl",
        // CORBA
        "org.omg.CORBA",
        "org.omg.CORBA_2_3",
        "org.omg.CORBA_2_3.portable",
        "org.omg.CORBA.DynAnyPackage",
        "org.omg.CORBA.ORBPackage",
        "org.omg.CORBA.portable",
        "org.omg.CORBA.TypeCodePackage",
        "org.omg.CosNaming",
        "org.omg.CosNaming.NamingContextExtPackage",
        "org.omg.CosNaming.NamingContextPackage",
        "org.omg.Dynamic",
        "org.omg.DynamicAny",
        "org.omg.DynamicAny.DynAnyFactoryPackage",
        "org.omg.DynamicAny.DynAnyPackage",
        "org.omg.IOP",
        "org.omg.IOP.CodecFactoryPackage",
        "org.omg.IOP.CodecPackage",
        "org.omg.Messaging",
        "org.omg.PortableInterceptor",
        "org.omg.PortableInterceptor.ORBInitInfoPackage",
        "org.omg.PortableServer",
        "org.omg.PortableServer.CurrentPackage",
        "org.omg.PortableServer.POAManagerPackage",
        "org.omg.PortableServer.POAPackage",
        "org.omg.PortableServer.portable",
        "org.omg.PortableServer.ServantLocatorPackage",
        "org.omg.SendingContext",
        "org.omg.stub.java.rmi",
        // Serialization
        "java.io.serialization",
        // Compiler
        "javax.annotation.processing",
        "javax.lang.model",
        "javax.lang.model.element",
        "javax.lang.model.type",
        "javax.lang.model.util",
        "javax.tools",
        // NIO extensions (sun.nio)
        "sun.nio.fs",
        "sun.nio.ch",
        // Scripting
        "javax.script",
        // Swing (GUI)
        "java.awt",
        "java.awt.color",
        "java.awt.datatransfer",
        "java.awt.dnd",
        "java.awt.event",
        "java.awt.font",
        "java.awt.geom",
        "java.awt.im",
        "java.awt.im.spi",
        "java.awt.image",
        "java.awt.image.renderable",
        "java.awt.print",
        "javax.accessibility",
        "javax.swing",
        "javax.swing.border",
        "javax.swing.colorchooser",
        "javax.swing.event",
        "javax.swing.filechooser",
        "javax.swing.plaf",
        "javax.swing.plaf.basic",
        "javax.swing.plaf.metal",
        "javax.swing.plaf.nimbus",
        "javax.swing.plaf.synth",
        "javax.swing.table",
        "javax.swing.text",
        "javax.swing.text.html",
        "javax.swing.text.html.parser",
        "javax.swing.text.rtf",
        "javax.swing.tree",
        "javax.swing.undo",
        // Sound
        "javax.sound.midi",
        "javax.sound.midi.spi",
        "javax.sound.sampled",
        "javax.sound.sampled.spi",
        // Image I/O
        "javax.imageio",
        "javax.imageio.event",
        "javax.imageio.metadata",
        "javax.imageio.plugins.bmp",
        "javax.imageio.plugins.jpeg",
        "javax.imageio.plugins.tiff",
        "javax.imageio.spi",
        "javax.imageio.stream",
        // Printing
        "javax.print",
        "javax.print.attribute",
        "javax.print.attribute.standard",
        "javax.print.event",
        // Activation
        "javax.activation",
        // Transaction
        "javax.transaction",
        "javax.transaction.xa",
        // JAX-WS
        "javax.jws",
        "javax.jws.soap",
        "javax.xml.ws",
        "javax.xml.ws.handler",
        "javax.xml.ws.handler.soap",
        "javax.xml.ws.http",
        "javax.xml.ws.soap",
        "javax.xml.ws.spi",
        "javax.xml.ws.wsaddressing",
        // JAXB
        "javax.xml.bind",
        // Common third-party packages often considered "standard"
        "org.junit",
        "org.junit.jupiter",
        "org.junit.jupiter.api",
        "org.junit.jupiter.params",
        "org.junit.platform",
        "org.mockito",
        "org.hamcrest",
        "org.apache",
        "org.slf4j",
        "ch.qos.logback",
        "com.google",
        "com.fasterxml",
        "com.fasterxml.jackson",
    ];

    // Primitive types
    pub const PRIMITIVE_TYPES: &[&str] = &[
        "byte", "short", "int", "long", "float", "double", "char", "boolean", "void",
    ];

    // Primitive wrapper types
    pub const WRAPPER_TYPES: &[&str] = &[
        "Byte",
        "Short",
        "Integer",
        "Long",
        "Float",
        "Double",
        "Character",
        "Boolean",
        "Void",
    ];

    // Common JDK classes
    pub const JDK_CLASSES: &[&str] = &[
        // java.lang package
        "Object",
        "String",
        "StringBuffer",
        "StringBuilder",
        "CharSequence",
        "Number",
        "Byte",
        "Short",
        "Integer",
        "Long",
        "Float",
        "Double",
        "Character",
        "Boolean",
        "Void",
        "Thread",
        "Runnable",
        "ThreadLocal",
        "InheritableThreadLocal",
        "Class",
        "ClassLoader",
        "Package",
        "Module",
        "ModuleLayer",
        "ReflectiveOperationException",
        "Exception",
        "RuntimeException",
        "Error",
        "Throwable",
        "NullPointerException",
        "IndexOutOfBoundsException",
        "ArrayIndexOutOfBoundsException",
        "StringIndexOutOfBoundsException",
        "ClassCastException",
        "IllegalArgumentException",
        "NumberFormatException",
        "IllegalStateException",
        "UnsupportedOperationException",
        "ArithmeticException",
        "NegativeArraySizeException",
        "ArrayStoreException",
        "CloneNotSupportedException",
        "InterruptedException",
        "NoSuchMethodException",
        "NoSuchFieldException",
        "ClassNotFoundException",
        "System",
        "Runtime",
        "Process",
        "ProcessBuilder",
        "ProcessHandle",
        "SecurityManager",
        "Math",
        "StrictMath",
        "Comparable",
        "Cloneable",
        "Serializable",
        "AutoCloseable",
        "Iterable",
        "Iterator",
        "Enumeration",
        "Spliterator",
        "Spliterators",
        "Spliterator.OfPrimitive",
        "Spliterator.OfInt",
        "Spliterator.OfLong",
        "Spliterator.OfDouble",
        "FunctionalInterface",
        "Override",
        "Deprecated",
        "SuppressWarnings",
        "SafeVarargs",
        "Native",
        "Target",
        "Retention",
        "Documented",
        "Inherited",
        "Repeatable",
        // java.lang.invoke package
        "MethodHandle",
        "MethodHandles",
        "MethodHandles.Lookup",
        "MethodType",
        "CallSite",
        "ConstantCallSite",
        "MutableCallSite",
        "VolatileCallSite",
        "LambdaMetafactory",
        "SwitchPoint",
        "VarHandle",
        // java.lang.ref package
        "Reference",
        "SoftReference",
        "WeakReference",
        "PhantomReference",
        "ReferenceQueue",
        "Cleaner",
        "FinalReference",
        "Finalizer",
        // java.lang.module package
        "Module",
        "ModuleDescriptor",
        "ModuleDescriptor.Builder",
        "ModuleDescriptor.Exports",
        "ModuleDescriptor.Opens",
        "ModuleDescriptor.Provides",
        "ModuleDescriptor.Requires",
        "ModuleDescriptor.Uses",
        "ModuleFinder",
        "ModuleReader",
        "Configuration",
        "ResolvedModule",
        "Layer",
        "Layer.Controller",
        "ModuleReference",
        // java.util package
        "ArrayList",
        "LinkedList",
        "Vector",
        "Stack",
        "HashSet",
        "LinkedHashSet",
        "TreeSet",
        "HashMap",
        "LinkedHashMap",
        "TreeMap",
        "WeakHashMap",
        "IdentityHashMap",
        "Hashtable",
        "Properties",
        "PriorityQueue",
        "ArrayDeque",
        "ArrayBlockingQueue",
        "LinkedBlockingQueue",
        "PriorityBlockingQueue",
        "DelayQueue",
        "SynchronousQueue",
        "LinkedTransferQueue",
        "LinkedBlockingDeque",
        "ConcurrentHashMap",
        "ConcurrentSkipListMap",
        "ConcurrentSkipListSet",
        "CopyOnWriteArrayList",
        "CopyOnWriteArraySet",
        "ConcurrentLinkedQueue",
        "ConcurrentLinkedDeque",
        "Collections",
        "Arrays",
        "Objects",
        "Optional",
        "OptionalInt",
        "OptionalLong",
        "OptionalDouble",
        "Stream",
        "IntStream",
        "LongStream",
        "DoubleStream",
        "StreamSupport",
        "Collectors",
        "Comparator",
        "Comparators",
        "Random",
        "SecureRandom",
        "UUID",
        "BitSet",
        "Date",
        "Calendar",
        "GregorianCalendar",
        "TimeZone",
        "SimpleTimeZone",
        "Locale",
        "Currency",
        "Formatter",
        "Scanner",
        "StringTokenizer",
        "StringJoiner",
        "Base64",
        "Base64.Decoder",
        "Base64.Encoder",
        "Timer",
        "TimerTask",
        "EventListener",
        "EventListenerProxy",
        "EventObject",
        "PropertyChangeEvent",
        "PropertyChangeListener",
        "PropertyChangeSupport",
        "VetoableChangeListener",
        "VetoableChangeSupport",
        "EventListenerList",
        // java.util.regex package
        "Pattern",
        "Matcher",
        "MatchResult",
        // java.io package
        "File",
        "FileDescriptor",
        "FileInputStream",
        "FileOutputStream",
        "FileReader",
        "FileWriter",
        "BufferedInputStream",
        "BufferedOutputStream",
        "BufferedReader",
        "BufferedWriter",
        "ByteArrayInputStream",
        "ByteArrayOutputStream",
        "CharArrayReader",
        "CharArrayWriter",
        "DataInputStream",
        "DataOutputStream",
        "FilterInputStream",
        "FilterOutputStream",
        "FilterReader",
        "FilterWriter",
        "InputStream",
        "OutputStream",
        "Reader",
        "Writer",
        "InputStreamReader",
        "OutputStreamWriter",
        "ObjectInputStream",
        "ObjectOutputStream",
        "ObjectStreamClass",
        "ObjectStreamField",
        "PipedInputStream",
        "PipedOutputStream",
        "PipedReader",
        "PipedWriter",
        "PrintStream",
        "PrintWriter",
        "PushbackInputStream",
        "PushbackReader",
        "RandomAccessFile",
        "SequenceInputStream",
        "Serializable",
        "Externalizable",
        "FileFilter",
        "FilenameFilter",
        // java.nio package
        "Buffer",
        "ByteBuffer",
        "CharBuffer",
        "DoubleBuffer",
        "FloatBuffer",
        "IntBuffer",
        "LongBuffer",
        "ShortBuffer",
        "MappedByteBuffer",
        "BufferOverflowException",
        "BufferUnderflowException",
        "InvalidMarkException",
        "ReadOnlyBufferException",
        "ByteOrder",
        "Charset",
        "CharsetDecoder",
        "CharsetEncoder",
        "CoderResult",
        "CodingErrorAction",
        "StandardCharsets",
        // java.nio.file package
        "Path",
        "Paths",
        "FileSystem",
        "FileSystems",
        "FileStore",
        "FileSystemProvider",
        "Files",
        "StandardOpenOption",
        "StandardCopyOption",
        "LinkOption",
        "FileVisitOption",
        "FileVisitResult",
        "SimpleFileVisitor",
        "DirectoryStream",
        "DirectoryStream.Filter",
        "WatchService",
        "WatchKey",
        "WatchEvent",
        "WatchEvent.Kind",
        "WatchEvent.Modifier",
        // java.net package
        "URL",
        "URLConnection",
        "HttpURLConnection",
        "HttpsURLConnection",
        "URI",
        "InetAddress",
        "Inet4Address",
        "Inet6Address",
        "Socket",
        "ServerSocket",
        "DatagramSocket",
        "MulticastSocket",
        "SocketAddress",
        "InetSocketAddress",
        "Proxy",
        "ProxySelector",
        "CookieHandler",
        "CookieManager",
        "CookiePolicy",
        "CookieStore",
        "HttpCookie",
        "URLEncoder",
        "URLDecoder",
        "IDN",
        "InterfaceAddress",
        "NetworkInterface",
        "StandardSocketOptions",
        "SocketOption",
        // java.net.http package (Java 11+)
        "HttpClient",
        "HttpRequest",
        "HttpResponse",
        "WebSocket",
        "WebSocket.Builder",
        // java.time package
        "LocalDate",
        "LocalTime",
        "LocalDateTime",
        "ZonedDateTime",
        "OffsetDateTime",
        "OffsetTime",
        "Instant",
        "Duration",
        "Period",
        "ZoneId",
        "ZoneOffset",
        "Clock",
        "DayOfWeek",
        "Month",
        "MonthDay",
        "Year",
        "YearMonth",
        // java.math package
        "BigInteger",
        "BigDecimal",
        "MathContext",
        "RoundingMode",
        // java.text package
        "DateFormat",
        "SimpleDateFormat",
        "NumberFormat",
        "DecimalFormat",
        "ChoiceFormat",
        "MessageFormat",
        "BreakIterator",
        "Collator",
        "RuleBasedCollator",
        "CollationKey",
        "CollationElementIterator",
        "Normalizer",
        "Bidi",
        // java.util.concurrent package
        "Executor",
        "ExecutorService",
        "ScheduledExecutorService",
        "ThreadPoolExecutor",
        "ScheduledThreadPoolExecutor",
        "Executors",
        "Future",
        "FutureTask",
        "CompletableFuture",
        "CompletionStage",
        "CompletionService",
        "ExecutorCompletionService",
        "Callable",
        "RunnableFuture",
        "RunnableScheduledFuture",
        "ScheduledFuture",
        "ForkJoinPool",
        "ForkJoinTask",
        "RecursiveAction",
        "RecursiveTask",
        "CountedCompleter",
        "ForkJoinWorkerThread",
        "ManagedBlocker",
        // java.util.concurrent.atomic package
        "AtomicBoolean",
        "AtomicInteger",
        "AtomicLong",
        "AtomicReference",
        "AtomicIntegerArray",
        "AtomicLongArray",
        "AtomicReferenceArray",
        "AtomicIntegerFieldUpdater",
        "AtomicLongFieldUpdater",
        "AtomicReferenceFieldUpdater",
        "DoubleAccumulator",
        "DoubleAdder",
        "LongAccumulator",
        "LongAdder",
        "Striped64",
        // java.util.concurrent.locks package
        "Lock",
        "ReentrantLock",
        "ReadWriteLock",
        "ReentrantReadWriteLock",
        "Condition",
        "LockSupport",
        "StampedLock",
        // java.util.logging package
        "Logger",
        "Level",
        "LogRecord",
        "Handler",
        "ConsoleHandler",
        "FileHandler",
        "StreamHandler",
        "SocketHandler",
        "MemoryHandler",
        "SimpleFormatter",
        "XMLFormatter",
        "Formatter",
        "Filter",
        "LogManager",
        // java.util.prefs package
        "Preferences",
        "PreferencesFactory",
        "AbstractPreferences",
        "NodeChangeEvent",
        "NodeChangeListener",
        "PreferenceChangeEvent",
        "PreferenceChangeListener",
        // java.util.jar package
        "JarFile",
        "JarEntry",
        "JarInputStream",
        "JarOutputStream",
        "JarException",
        "JarOutputStream",
        "Manifest",
        "Attributes",
        "Attributes.Name",
        // java.util.zip package
        "ZipFile",
        "ZipEntry",
        "ZipInputStream",
        "ZipOutputStream",
        "GZIPInputStream",
        "GZIPOutputStream",
        "Inflater",
        "InflaterOutputStream",
        "Deflater",
        "DeflaterOutputStream",
        "CRC32",
        "Adler32",
        "CheckedInputStream",
        "CheckedOutputStream",
        "ZipException",
        "DataFormatException",
        // java.util.function package
        "Function",
        "BiFunction",
        "UnaryOperator",
        "BinaryOperator",
        "Predicate",
        "BiPredicate",
        "Consumer",
        "BiConsumer",
        "Supplier",
        "IntFunction",
        "IntToDoubleFunction",
        "IntToLongFunction",
        "IntUnaryOperator",
        "IntBinaryOperator",
        "IntPredicate",
        "IntConsumer",
        "IntSupplier",
        "LongFunction",
        "LongToDoubleFunction",
        "LongToIntFunction",
        "LongUnaryOperator",
        "LongBinaryOperator",
        "LongPredicate",
        "LongConsumer",
        "LongSupplier",
        "DoubleFunction",
        "DoubleToIntFunction",
        "DoubleToLongFunction",
        "DoubleUnaryOperator",
        "DoubleBinaryOperator",
        "DoublePredicate",
        "DoubleConsumer",
        "DoubleSupplier",
        // java.util.stream package
        "Stream",
        "IntStream",
        "LongStream",
        "DoubleStream",
        "StreamSupport",
        "Collector",
        "Collectors",
        "Stream.Builder",
        "IntStream.Builder",
        "LongStream.Builder",
        "DoubleStream.Builder",
        // Annotation processing (javax.annotation.processing)
        "AbstractProcessor",
        "ProcessingEnvironment",
        "RoundEnvironment",
        "Messager",
        "Filer",
        "SupportedAnnotationTypes",
        "SupportedSourceVersion",
        "SupportedOptions",
        // Language model (javax.lang.model)
        "SourceVersion",
        "Element",
        "ElementKind",
        "ElementVisitor",
        "TypeElement",
        "ExecutableElement",
        "VariableElement",
        "TypeParameterElement",
        "PackageElement",
        "AnnotationMirror",
        "AnnotationValue",
        "TypeMirror",
        "TypeKind",
        "TypeVisitor",
        "Name",
        // Java 9+ StackWalker
        "StackWalker",
        "StackWalker.StackFrame",
        "StackWalker.Option",
        // Java 9+ Flow API
        "Flow",
        "Flow.Publisher",
        "Flow.Subscriber",
        "Flow.Subscription",
        "Flow.Processor",
        "SubmissionPublisher",
        // Java 9+ VarHandle
        "VarHandle",
        "VarHandle.AccessMode",
        // Java 17+ HexFormat
        "HexFormat",
        // Java 21+ SequencedCollection
        "SequencedCollection",
        "SequencedSet",
        "SequencedMap",
        "SequencedMap.Entry",
    ];

    pub fn is_primitive_type(name: &str) -> bool {
        Self::PRIMITIVE_TYPES.contains(&name)
    }

    pub fn is_wrapper_type(name: &str) -> bool {
        Self::WRAPPER_TYPES.contains(&name)
    }

    pub fn is_jdk_class(name: &str) -> bool {
        Self::JDK_CLASSES.contains(&name)
    }

    /// Check if a qualified path is from JDK
    pub fn is_jdk_path(path: &str) -> bool {
        // Java uses . as package separator
        // Check if the path starts with any known Java package
        for &package in Self::JAVA_PACKAGES {
            if path == package || path.starts_with(&format!("{}.", package)) {
                return true;
            }
        }

        // Also check first component for top-level packages like "java", "javax", "jdk"
        let first_component = path.split('.').next().unwrap_or("");
        if first_component == "java" || first_component == "javax" || first_component == "jdk" {
            // Additional check: ensure the second component is valid
            let parts: Vec<&str> = path.split('.').collect();
            if parts.len() >= 2 {
                let second = parts[1];
                // Filter out common non-JDK second-level packages
                if second == "example" || second == "mycompany" || second == "company" {
                    return false;
                }
            }
            return true;
        }

        false
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for primitive type
        if Self::is_primitive_type(call_name) {
            return true;
        }

        // Check for wrapper type
        if Self::is_wrapper_type(call_name) {
            return true;
        }

        // Check for JDK class (short name)
        if Self::is_jdk_class(call_name) {
            return true;
        }

        // Check for qualified path (e.g., java.lang.String, java.util.ArrayList)
        if call_name.contains('.') {
            // Check if the path is from JDK
            if Self::is_jdk_path(call_name) {
                return true;
            }

            // Check for static method calls like System.out.println, Math.abs
            let parts: Vec<&str> = call_name.split('.').collect();
            if parts.len() >= 2 {
                // Check if the first part is a known JDK class (e.g., "System" in "System.out.println")
                if Self::is_jdk_class(parts[0]) {
                    return true;
                }

                // Check for class.method pattern
                if parts.len() == 2 && Self::is_jdk_class(parts[0]) {
                    return true;
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
            // Most call types in Java use the same detection logic
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::CallbackCall
            | RelationType::GenericCall => {
                // For Java, use the legacy detection logic
                Self::is_stdlib_call(call_name)
            }

            // Other relation types are not relevant for stdlib detection
            _ => false,
        }
    }
}

// Generate get_category using macro
impl_stdlib_categorizer!(
    JavaStdlibDetector,
    [
        (
            StdlibCategory::Collection,
            [
                "java.util",
                "java.util.concurrent",
                "java.util.concurrent.atomic",
                "java.util.concurrent.locks",
                "java.util.function",
                "java.util.stream"
            ]
        ),
        (
            StdlibCategory::Io,
            [
                "java.io",
                "java.nio",
                "java.nio.channels",
                "java.nio.charset",
                "java.nio.file",
                "java.nio.file.attribute",
                "java.nio.file.spi"
            ]
        ),
        (
            StdlibCategory::Concurrency,
            [
                "java.util.concurrent",
                "java.util.concurrent.atomic",
                "java.util.concurrent.locks"
            ]
        ),
        (
            StdlibCategory::Utility,
            [
                "java.lang",
                "java.lang.annotation",
                "java.lang.invoke",
                "java.lang.module",
                "java.lang.ref",
                "java.lang.reflect",
                "java.lang.runtime",
                "java.lang.constant",
                "java.lang.instrument"
            ]
        ),
        (StdlibCategory::String, ["java.text", "java.text.spi"]),
        (StdlibCategory::Numeric, ["java.math"]),
        (StdlibCategory::Error, []),
        (StdlibCategory::Trait, []),
        (StdlibCategory::Macro, []),
    ]
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_primitive_type() {
        assert!(JavaStdlibDetector::is_primitive_type("int"));
        assert!(JavaStdlibDetector::is_primitive_type("boolean"));
        assert!(JavaStdlibDetector::is_primitive_type("void"));
        assert!(!JavaStdlibDetector::is_primitive_type("String"));
    }

    #[test]
    fn test_is_wrapper_type() {
        assert!(JavaStdlibDetector::is_wrapper_type("Integer"));
        assert!(JavaStdlibDetector::is_wrapper_type("Boolean"));
        assert!(JavaStdlibDetector::is_wrapper_type("Void"));
        assert!(!JavaStdlibDetector::is_wrapper_type("MyClass"));
    }

    #[test]
    fn test_is_jdk_class() {
        // Core classes
        assert!(JavaStdlibDetector::is_jdk_class("String"));
        assert!(JavaStdlibDetector::is_jdk_class("ArrayList"));
        assert!(JavaStdlibDetector::is_jdk_class("HashMap"));

        // Java 8+ classes
        assert!(JavaStdlibDetector::is_jdk_class("Base64"));
        assert!(JavaStdlibDetector::is_jdk_class("Pattern"));
        assert!(JavaStdlibDetector::is_jdk_class("Matcher"));
        assert!(JavaStdlibDetector::is_jdk_class("Optional"));
        assert!(JavaStdlibDetector::is_jdk_class("Stream"));

        // Java 9+ classes
        assert!(JavaStdlibDetector::is_jdk_class("ProcessHandle"));
        assert!(JavaStdlibDetector::is_jdk_class("StackWalker"));
        assert!(JavaStdlibDetector::is_jdk_class("VarHandle"));

        // Java 17+ classes
        assert!(JavaStdlibDetector::is_jdk_class("HexFormat"));

        // Java 21+ classes
        assert!(JavaStdlibDetector::is_jdk_class("SequencedCollection"));

        assert!(!JavaStdlibDetector::is_jdk_class("MyClass"));
    }

    #[test]
    fn test_is_jdk_path() {
        assert!(JavaStdlibDetector::is_jdk_path("java.lang"));
        assert!(JavaStdlibDetector::is_jdk_path("java.util.ArrayList"));
        assert!(JavaStdlibDetector::is_jdk_path("javax.net.ssl.SSLSocket"));
        assert!(!JavaStdlibDetector::is_jdk_path("com.example.MyClass"));
    }

    #[test]
    fn test_is_stdlib_call() {
        // Primitive types
        assert!(JavaStdlibDetector::is_stdlib_call("int"));
        assert!(JavaStdlibDetector::is_stdlib_call("boolean"));

        // Wrapper types
        assert!(JavaStdlibDetector::is_stdlib_call("Integer"));
        assert!(JavaStdlibDetector::is_stdlib_call("Boolean"));

        // JDK classes
        assert!(JavaStdlibDetector::is_stdlib_call("String"));
        assert!(JavaStdlibDetector::is_stdlib_call("ArrayList"));
        assert!(JavaStdlibDetector::is_stdlib_call("HashMap"));

        // Fully qualified names
        assert!(JavaStdlibDetector::is_stdlib_call("java.lang.String"));
        assert!(JavaStdlibDetector::is_stdlib_call("java.util.ArrayList"));
        assert!(JavaStdlibDetector::is_stdlib_call(
            "javax.net.ssl.SSLSocket"
        ));

        // Static method calls (partial detection)
        assert!(JavaStdlibDetector::is_stdlib_call("System.out.println")); // System is JDK class
        assert!(JavaStdlibDetector::is_stdlib_call("Math.abs")); // Math is JDK class

        // Negative cases
        assert!(!JavaStdlibDetector::is_stdlib_call("MyClass"));
        assert!(!JavaStdlibDetector::is_stdlib_call("com.example.MyClass"));
        assert!(!JavaStdlibDetector::is_stdlib_call("MyClass.myMethod"));
    }

    #[test]
    fn test_new_classes_coverage() {
        // Test java.util.regex
        assert!(JavaStdlibDetector::is_jdk_class("Pattern"));
        assert!(JavaStdlibDetector::is_jdk_class("Matcher"));

        // Test java.util.logging
        assert!(JavaStdlibDetector::is_jdk_class("Logger"));
        assert!(JavaStdlibDetector::is_jdk_class("Level"));

        // Test java.util.prefs
        assert!(JavaStdlibDetector::is_jdk_class("Preferences"));

        // Test java.util.jar
        assert!(JavaStdlibDetector::is_jdk_class("JarFile"));
        assert!(JavaStdlibDetector::is_jdk_class("Manifest"));

        // Test java.util.zip
        assert!(JavaStdlibDetector::is_jdk_class("ZipFile"));
        assert!(JavaStdlibDetector::is_jdk_class("GZIPInputStream"));

        // Test java.lang.invoke
        assert!(JavaStdlibDetector::is_jdk_class("MethodHandle"));
        assert!(JavaStdlibDetector::is_jdk_class("VarHandle"));

        // Test java.lang.ref
        assert!(JavaStdlibDetector::is_jdk_class("WeakReference"));
        assert!(JavaStdlibDetector::is_jdk_class("Cleaner"));

        // Test java.lang.module
        assert!(JavaStdlibDetector::is_jdk_class("ModuleDescriptor"));
        assert!(JavaStdlibDetector::is_jdk_class("ModuleFinder"));

        // Test annotation processing
        assert!(JavaStdlibDetector::is_jdk_class("AbstractProcessor"));
        assert!(JavaStdlibDetector::is_jdk_class("TypeElement"));

        // Test Java 9+
        assert!(JavaStdlibDetector::is_jdk_class("StackWalker"));
        assert!(JavaStdlibDetector::is_jdk_class("ProcessHandle"));
        assert!(JavaStdlibDetector::is_jdk_class("Flow"));

        // Test Java 17+
        assert!(JavaStdlibDetector::is_jdk_class("HexFormat"));

        // Test Java 21+
        assert!(JavaStdlibDetector::is_jdk_class("SequencedCollection"));
    }
}
