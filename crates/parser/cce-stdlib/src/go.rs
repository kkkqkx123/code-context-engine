// Go Standard Library Detector
// Handles detection of Go standard library entities

pub struct GoStdlibDetector;

impl GoStdlibDetector {
    // Builtin types
    pub const BUILTIN_TYPES: &[&str] = &[
        "int",
        "int8",
        "int16",
        "int32",
        "int64",
        "uint",
        "uint8",
        "uint16",
        "uint32",
        "uint64",
        "uintptr",
        "float32",
        "float64",
        "complex64",
        "complex128",
        "bool",
        "string",
        "byte",
        "rune",
        "error",
        "any",
        "comparable",
    ];

    // Builtin functions
    pub const BUILTIN_FUNCTIONS: &[&str] = &[
        "append", "cap", "close", "complex", "copy", "delete", "imag", "len", "make", "new",
        "panic", "print", "println", "real", "recover",
    ];

    // Standard library packages
    pub const STDLIB_PACKAGES: &[&str] = &[
        // Core
        "builtin",
        "unsafe",
        "errors",
        "fmt",
        "math",
        "math/cmplx",
        "math/big",
        "math/rand",
        "math/bits",
        "cmp",
        "slices",
        "maps",
        "iter",
        "unique",
        "structs",
        "weak",
        // Concurrency
        "sync",
        "sync/atomic",
        "time",
        "context",
        // I/O
        "io",
        "io/fs",
        "io/ioutil",
        "bufio",
        "bytes",
        "strings",
        "strconv",
        // Network
        "net",
        "net/http",
        "net/http/cgi",
        "net/http/cookiejar",
        "net/http/fcgi",
        "net/http/httptest",
        "net/http/httptrace",
        "net/http/httputil",
        "net/http/pprof",
        "net/netip",
        "net/url",
        "net/mail",
        "net/rpc",
        "net/rpc/jsonrpc",
        "net/smtp",
        "net/textproto",
        // OS
        "os",
        "os/signal",
        "os/exec",
        "os/user",
        "path",
        "path/filepath",
        "flag",
        "log",
        "log/syslog",
        "expvar",
        "plugin",
        "runtime",
        "runtime/debug",
        "runtime/metrics",
        "runtime/pprof",
        "runtime/trace",
        "go/ast",
        "go/parser",
        "go/token",
        "go/types",
        "go/format",
        "go/doc",
        "go/constant",
        "go/importer",
        "go/scanner",
        "go/printer",
        "go/build",
        "go/build/constraint",
        // Encoding
        "encoding",
        "encoding/json",
        "encoding/xml",
        "encoding/base64",
        "encoding/csv",
        "encoding/gob",
        "encoding/hex",
        "encoding/pem",
        "encoding/ascii85",
        // Cryptography
        "crypto",
        "crypto/aes",
        "crypto/cipher",
        "crypto/des",
        "crypto/ecdsa",
        "crypto/elliptic",
        "crypto/hmac",
        "crypto/md5",
        "crypto/rand",
        "crypto/rc4",
        "crypto/rsa",
        "crypto/sha1",
        "crypto/sha256",
        "crypto/sha512",
        "crypto/subtle",
        "crypto/tls",
        // Database
        "database",
        "database/sql",
        "database/sql/driver",
        // Containers
        "container",
        "container/heap",
        "container/list",
        "container/ring",
        // Debug
        "debug",
        "debug/buildinfo",
        "debug/dwarf",
        "debug/elf",
        "debug/macho",
        "debug/pe",
        "debug/plan9obj",
        // Text processing
        "text",
        "text/scanner",
        "text/tabwriter",
        "text/template",
        "text/template/parse",
        // Testing
        "testing",
        "testing/iotest",
        "testing/quick",
        // Other
        "archive",
        "archive/tar",
        "archive/zip",
        "compress",
        "compress/bzip2",
        "compress/flate",
        "compress/gzip",
        "compress/lzw",
        "compress/zlib",
        "hash",
        "hash/adler32",
        "hash/crc32",
        "hash/crc64",
        "hash/fnv",
        "hash/maphash",
        "html",
        "html/template",
        "image",
        "image/color",
        "image/draw",
        "image/gif",
        "image/jpeg",
        "image/png",
        "index",
        "index/suffixarray",
        "mime",
        "mime/multipart",
        "reflect",
        "sort",
        "strconv",
        "strings",
        "sync",
        "syscall",
        "time",
        "unicode",
        "unicode/utf16",
        "unicode/utf8",
    ];

    // Common stdlib types (from packages)
    pub const STDLIB_TYPES: &[&str] = &[
        // io package
        "io.Reader",
        "io.Writer",
        "io.ReadWriter",
        "io.Closer",
        "io.ReadCloser",
        "io.WriteCloser",
        "io.ReadWriteCloser",
        "io.ReadSeeker",
        "io.WriteSeeker",
        "io.ReadWriteSeeker",
        "io.Seeker",
        "io.LimitedReader",
        "io.PipeReader",
        "io.PipeWriter",
        "io.SectionReader",
        "io.ByteReader",
        "io.ByteWriter",
        "io.RuneReader",
        "io.RuneScanner",
        "io.StringWriter",
        "error",
        // fmt package
        "fmt.Stringer",
        "fmt.Formatter",
        "fmt.GoStringer",
        "fmt.State",
        // time package
        "time.Time",
        "time.Duration",
        "time.Location",
        "time.Ticker",
        "time.Timer",
        // sync package
        "sync.Mutex",
        "sync.RWMutex",
        "sync.WaitGroup",
        "sync.Once",
        "sync.Map",
        "sync.Cond",
        "sync.Pool",
        "sync.Locker",
        // context package
        "context.Context",
        "context.CancelFunc",
        // net package
        "net.Conn",
        "net.Listener",
        "net.Addr",
        "net.PacketConn",
        "net.IP",
        "net.IPNet",
        "net.IPAddr",
        "net.IPMask",
        "net.TCPAddr",
        "net.UDPAddr",
        "net.UnixAddr",
        // net/http package
        "http.Handler",
        "http.ResponseWriter",
        "http.Request",
        "http.Client",
        "http.Server",
        "http.Flusher",
        "http.Hijacker",
        "http.CloseNotifier",
        "http.Pusher",
        "http.Cookie",
        "http.Header",
        "http.Transport",
        "http.ServeMux",
        // os package
        "os.File",
        "os.FileInfo",
        "os.Process",
        "os.Signal",
        "os.PathError",
        "os.LinkError",
        "os.SyscallError",
        // bytes package
        "bytes.Buffer",
        "bytes.Reader",
        // strings package
        "strings.Builder",
        "strings.Reader",
        // regexp package
        "regexp.Regexp",
        // encoding/json package
        "encoding/json.Encoder",
        "encoding/json.Decoder",
        "encoding/json.Marshaler",
        "encoding/json.Unmarshaler",
        // encoding/xml package
        "encoding/xml.Encoder",
        "encoding/xml.Decoder",
        // math/big package
        "math/big.Int",
        "math/big.Float",
        "math/big.Rat",
        // reflect package
        "reflect.Type",
        "reflect.Value",
        "reflect.Kind",
        "reflect.StructField",
        "reflect.Method",
        // sort package
        "sort.Interface",
        // database/sql package
        "database/sql.DB",
        "database/sql.Tx",
        "database/sql.Rows",
        "database/sql.Row",
        "database/sql.Result",
        "database/sql.Stmt",
        "database/sql.Conn",
        // html/template package
        "html/template.Template",
        "html/template.FuncMap",
        // text/template package
        "text/template.Template",
        "text/template.FuncMap",
        // flag package
        "flag.Flag",
        "flag.Value",
        // log package
        "log.Logger",
        // testing package
        "testing.T",
        "testing.B",
        "testing.F",
        "testing.TB",
        // container package
        "container/list.List",
        "container/list.Element",
        "container/heap.Interface",
        "container/ring.Ring",
        // hash package
        "hash.Hash",
        "hash.Hash32",
        "hash.Hash64",
        // image package
        "image.Image",
        "image.Rectangle",
        "image.Point",
        "image.Color",
        "image/color.Color",
        "image/color.RGBA",
        "image/color.RGBA64",
        "image/color.NRGBA",
        "image/color.Alpha",
        "image/color.Gray",
        "image/color.Gray16",
        "image/color.CMYK",
        "image/color.YCbCr",
        "image/color.Palette",
    ];

    // Common stdlib functions (from packages)
    pub const STDLIB_FUNCTIONS: &[&str] = &[
        // fmt package
        "fmt.Print",
        "fmt.Println",
        "fmt.Printf",
        "fmt.Sprint",
        "fmt.Sprintln",
        "fmt.Sprintf",
        "fmt.Fprint",
        "fmt.Fprintln",
        "fmt.Fprintf",
        "fmt.Scan",
        "fmt.Scanln",
        "fmt.Scanf",
        "fmt.Fscan",
        "fmt.Fscanln",
        "fmt.Fscanf",
        "fmt.Errorf",
        "fmt.Sscanf",
        // strings package
        "strings.Split",
        "strings.Join",
        "strings.Contains",
        "strings.HasPrefix",
        "strings.HasSuffix",
        "strings.Replace",
        "strings.ReplaceAll",
        "strings.ToLower",
        "strings.ToUpper",
        "strings.TrimSpace",
        "strings.Trim",
        "strings.TrimLeft",
        "strings.TrimRight",
        "strings.TrimPrefix",
        "strings.TrimSuffix",
        "strings.Index",
        "strings.LastIndex",
        "strings.Count",
        "strings.Repeat",
        "strings.Compare",
        "strings.EqualFold",
        "strings.Builder",
        // bytes package
        "bytes.Split",
        "bytes.Join",
        "bytes.Contains",
        "bytes.HasPrefix",
        "bytes.HasSuffix",
        "bytes.ToLower",
        "bytes.ToUpper",
        "bytes.TrimSpace",
        "bytes.Compare",
        "bytes.Equal",
        "bytes.Index",
        "bytes.LastIndex",
        "bytes.Count",
        "bytes.Repeat",
        "bytes.Replace",
        "bytes.ReplaceAll",
        // regexp package
        "regexp.MustCompile",
        "regexp.Compile",
        "regexp.Match",
        "regexp.MatchString",
        "regexp.QuoteMeta",
        // json package
        "json.Marshal",
        "json.Unmarshal",
        "json.MarshalIndent",
        "json.NewEncoder",
        "json.NewDecoder",
        "json.Valid",
        // os package
        "os.Exit",
        "os.Getenv",
        "os.Setenv",
        "os.Unsetenv",
        "os.Args",
        "os.Open",
        "os.Create",
        "os.OpenFile",
        "os.ReadFile",
        "os.WriteFile",
        "os.Mkdir",
        "os.MkdirAll",
        "os.Remove",
        "os.RemoveAll",
        "os.Rename",
        "os.Stat",
        "os.Lstat",
        "os.IsExist",
        "os.IsNotExist",
        "os.IsPermission",
        // ioutil package (deprecated but still common)
        "ioutil.ReadFile",
        "ioutil.WriteFile",
        "ioutil.ReadAll",
        "ioutil.ReadDir",
        "ioutil.TempDir",
        "ioutil.TempFile",
        // log package
        "log.Print",
        "log.Println",
        "log.Printf",
        "log.Fatal",
        "log.Fatalf",
        "log.Fatalln",
        "log.Panic",
        "log.Panicf",
        "log.Panicln",
        "log.New",
        // time package
        "time.Now",
        "time.Sleep",
        "time.Since",
        "time.Until",
        "time.After",
        "time.NewTicker",
        "time.NewTimer",
        "time.Parse",
        "time.ParseDuration",
        "time.Date",
        "time.Unix",
        "time.UnixMicro",
        "time.UnixMilli",
        // context package
        "context.Background",
        "context.TODO",
        "context.WithCancel",
        "context.WithTimeout",
        "context.WithDeadline",
        "context.WithValue",
        // sync package
        "sync.NewCond",
        // math package
        "math.Abs",
        "math.Floor",
        "math.Ceil",
        "math.Round",
        "math.Sqrt",
        "math.Pow",
        "math.Max",
        "math.Min",
        "math.Mod",
        "math.Sin",
        "math.Cos",
        "math.Tan",
        // strconv package
        "strconv.Atoi",
        "strconv.Itoa",
        "strconv.ParseInt",
        "strconv.ParseUint",
        "strconv.ParseFloat",
        "strconv.ParseBool",
        "strconv.FormatInt",
        "strconv.FormatUint",
        "strconv.FormatFloat",
        "strconv.FormatBool",
        "strconv.Quote",
        "strconv.Unquote",
        // sort package
        "sort.Sort",
        "sort.Strings",
        "sort.Ints",
        "sort.Float64s",
        "sort.Slice",
        "sort.SliceStable",
        "sort.Search",
        "sort.SearchInts",
        "sort.SearchStrings",
        // slices package (Go 1.21+)
        "slices.Sort",
        "slices.Reverse",
        "slices.Contains",
        "slices.Index",
        "slices.Clone",
        "slices.Equal",
        "slices.Compare",
        "slices.Min",
        "slices.Max",
        "maps.Delete",
        // maps package (Go 1.21+)
        "maps.Keys",
        "maps.Values",
        "maps.Clone",
        "maps.Equal",
        "maps.Copy",
        // reflect package
        "reflect.ValueOf",
        "reflect.TypeOf",
        "reflect.DeepEqual",
    ];

    // Common stdlib constants (from packages)
    pub const STDLIB_CONSTANTS: &[&str] = &[
        // math package constants
        "math.Pi",
        "math.E",
        "math.Phi",
        "math.Sqrt2",
        "math.SqrtE",
        "math.SqrtPi",
        "math.SqrtPhi",
        "math.Ln2",
        "math.Ln10",
        "math.Log2E",
        "math.Log10E",
        "math.MaxFloat32",
        "math.MaxFloat64",
        "math.SmallestNonzeroFloat32",
        "math.SmallestNonzeroFloat64",
        "math.MaxInt",
        "math.MinInt",
        "math.MaxInt8",
        "math.MinInt8",
        "math.MaxInt16",
        "math.MinInt16",
        "math.MaxInt32",
        "math.MinInt32",
        "math.MaxInt64",
        "math.MinInt64",
        "math.MaxUint",
        "math.MaxUint8",
        "math.MaxUint16",
        "math.MaxUint32",
        "math.MaxUint64",
        // time package constants
        "time.Nanosecond",
        "time.Microsecond",
        "time.Millisecond",
        "time.Second",
        "time.Minute",
        "time.Hour",
        // io package constants
        "io.EOF",
        "io.SeekStart",
        "io.SeekCurrent",
        "io.SeekEnd",
        // os package constants
        "os.O_RDONLY",
        "os.O_WRONLY",
        "os.O_RDWR",
        "os.O_CREATE",
        "os.O_APPEND",
        "os.O_EXCL",
        "os.O_SYNC",
        "os.O_TRUNC",
        "os.PathSeparator",
        "os.PathListSeparator",
        "os.DevNull",
        // syscall package constants (platform-specific, commonly used)
        "syscall.SIGTERM",
        "syscall.SIGINT",
        "syscall.SIGKILL",
        "syscall.SIGQUIT",
        // net package constants
        "net.IPv4len",
        "net.IPv6len",
        // strconv package constants
        "strconv.IntSize",
        // http package constants
        "http.MethodGet",
        "http.MethodHead",
        "http.MethodPost",
        "http.MethodPut",
        "http.MethodPatch",
        "http.MethodDelete",
        "http.MethodConnect",
        "http.MethodOptions",
        "http.MethodTrace",
        // json package constants
        "json.Compact",
        "json.Indent",
        // flag package constants
        "flag.ContinueOnError",
        "flag.ExitOnError",
        "flag.PanicOnError",
        // log package constants
        "log.Ldate",
        "log.Ltime",
        "log.Lmicroseconds",
        "log.Llongfile",
        "log.Lshortfile",
        "log.LUTC",
        "log.Lmsgprefix",
        "log.LstdFlags",
    ];

    pub fn is_builtin_type(name: &str) -> bool {
        Self::BUILTIN_TYPES.contains(&name)
    }

    pub fn is_builtin_function(name: &str) -> bool {
        Self::BUILTIN_FUNCTIONS.contains(&name)
    }

    pub fn is_stdlib_package(package: &str) -> bool {
        Self::STDLIB_PACKAGES.contains(&package)
    }

    pub fn is_stdlib_constant(name: &str) -> bool {
        Self::STDLIB_CONSTANTS.contains(&name)
    }

    /// Check if a type is from the standard library
    /// This provides a unified interface with other language detectors
    pub fn is_stdlib_type(name: &str) -> bool {
        // Check builtin types first
        if Self::is_builtin_type(name) {
            return true;
        }

        // Check for qualified stdlib types (e.g., "time.Time", "io.Reader", "builtin.error")
        if name.contains('.') {
            // First, try exact match with STDLIB_TYPES
            if Self::STDLIB_TYPES.contains(&name) {
                return true;
            }

            // Fallback: check if the package is from stdlib or is builtin
            let first_component = name.split('.').next().unwrap_or("");
            return first_component == "builtin" || Self::is_stdlib_package(first_component);
        }

        false
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for direct builtin function
        if Self::is_builtin_function(call_name) {
            return true;
        }

        // Check for qualified path
        if call_name.contains('.') {
            // First, try exact match with STDLIB_FUNCTIONS
            if Self::STDLIB_FUNCTIONS.contains(&call_name) {
                return true;
            }

            // Fallback: check if the package is from stdlib
            let first_component = call_name.split('.').next().unwrap_or("");
            return Self::is_stdlib_package(first_component);
        }

        false
    }
}

// Generate is_stdlib_by_type using simple macro
impl_stdlib_by_type_simple!(
    GoStdlibDetector,
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

// Generate get_category using macro
impl_stdlib_categorizer!(
    GoStdlibDetector,
    [
        (
            StdlibCategory::Collection,
            [
                "container/heap",
                "container/list",
                "container/ring",
                "slices",
                "maps"
            ]
        ),
        (
            StdlibCategory::Io,
            [
                "io",
                "io/fs",
                "io/ioutil",
                "bufio",
                "os",
                "os/exec",
                "os/signal",
                "os/user",
                "path",
                "path/filepath",
                "flag"
            ]
        ),
        (
            StdlibCategory::Concurrency,
            ["sync", "sync/atomic", "time", "context"]
        ),
        (
            StdlibCategory::Utility,
            [
                "fmt",
                "strings",
                "strconv",
                "bytes",
                "errors",
                "cmp",
                "sort",
                "math",
                "math/cmplx",
                "math/big",
                "math/rand",
                "math/bits",
                "reflect",
                "unsafe",
                "runtime",
                "runtime/debug",
                "runtime/metrics",
                "runtime/pprof",
                "runtime/trace",
                "log",
                "log/syslog",
                "expvar",
                "plugin",
                "unique",
                "structs",
                "weak",
                "iter"
            ]
        ),
        (
            StdlibCategory::String,
            [
                "strings",
                "strconv",
                "bytes",
                "text/template",
                "text/template/parse",
                "text/scanner",
                "text/tabwriter"
            ]
        ),
        (
            StdlibCategory::Numeric,
            [
                "math",
                "math/cmplx",
                "math/big",
                "math/rand",
                "math/bits",
                "cmp"
            ]
        ),
        (StdlibCategory::Error, ["errors"]),
        (StdlibCategory::Trait, []),
        (StdlibCategory::Macro, []),
    ]
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin_type() {
        assert!(GoStdlibDetector::is_builtin_type("int"));
        assert!(GoStdlibDetector::is_builtin_type("string"));
        assert!(!GoStdlibDetector::is_builtin_type("MyType"));
    }

    #[test]
    fn test_is_builtin_function() {
        assert!(GoStdlibDetector::is_builtin_function("append"));
        assert!(GoStdlibDetector::is_builtin_function("len"));
        assert!(!GoStdlibDetector::is_builtin_function("my_function"));
    }

    #[test]
    fn test_is_stdlib_package() {
        assert!(GoStdlibDetector::is_stdlib_package("fmt"));
        assert!(GoStdlibDetector::is_stdlib_package("io"));
        assert!(!GoStdlibDetector::is_stdlib_package("my_package"));
    }

    #[test]
    fn test_is_stdlib_call() {
        assert!(GoStdlibDetector::is_stdlib_call("fmt.Println"));
        assert!(GoStdlibDetector::is_stdlib_call("strings.Contains"));
        assert!(!GoStdlibDetector::is_stdlib_call("my_function"));
    }

    #[test]
    fn test_is_stdlib_type() {
        // Builtin types
        assert!(GoStdlibDetector::is_stdlib_type("int"));
        assert!(GoStdlibDetector::is_stdlib_type("string"));
        assert!(GoStdlibDetector::is_stdlib_type("error"));

        // Qualified stdlib types
        assert!(GoStdlibDetector::is_stdlib_type("time.Time"));
        assert!(GoStdlibDetector::is_stdlib_type("io.Reader"));
        assert!(GoStdlibDetector::is_stdlib_type("context.Context"));

        // Builtin package types
        assert!(GoStdlibDetector::is_stdlib_type("builtin.error"));
        assert!(GoStdlibDetector::is_stdlib_type("builtin.string"));
        assert!(GoStdlibDetector::is_stdlib_type("builtin.int"));

        // Negative cases
        assert!(!GoStdlibDetector::is_stdlib_type("MyType"));
        assert!(!GoStdlibDetector::is_stdlib_type("mypackage.Type"));
    }

    #[test]
    fn test_stdlib_packages_coverage() {
        // Test new Go 1.21+ packages
        assert!(GoStdlibDetector::is_stdlib_package("cmp"));
        assert!(GoStdlibDetector::is_stdlib_package("slices"));
        assert!(GoStdlibDetector::is_stdlib_package("maps"));
        assert!(GoStdlibDetector::is_stdlib_package("iter"));

        // Test network packages
        assert!(GoStdlibDetector::is_stdlib_package("net/http/cgi"));
        assert!(GoStdlibDetector::is_stdlib_package("net/http/httptest"));
        assert!(GoStdlibDetector::is_stdlib_package("net/netip"));

        // Test runtime packages
        assert!(GoStdlibDetector::is_stdlib_package("runtime"));
        assert!(GoStdlibDetector::is_stdlib_package("runtime/debug"));
        assert!(GoStdlibDetector::is_stdlib_package("runtime/pprof"));

        // Test go/* packages
        assert!(GoStdlibDetector::is_stdlib_package("go/ast"));
        assert!(GoStdlibDetector::is_stdlib_package("go/parser"));
        assert!(GoStdlibDetector::is_stdlib_package("go/types"));
    }

    #[test]
    fn test_stdlib_types_coverage() {
        // Test io types
        assert!(GoStdlibDetector::is_stdlib_type("io.LimitedReader"));
        assert!(GoStdlibDetector::is_stdlib_type("io.PipeReader"));
        assert!(GoStdlibDetector::is_stdlib_type("io.PipeWriter"));
        assert!(GoStdlibDetector::is_stdlib_type("io.SectionReader"));

        // Test os types
        assert!(GoStdlibDetector::is_stdlib_type("os.File"));
        assert!(GoStdlibDetector::is_stdlib_type("os.FileInfo"));
        assert!(GoStdlibDetector::is_stdlib_type("os.Process"));
        assert!(GoStdlibDetector::is_stdlib_type("os.Signal"));

        // Test bytes types
        assert!(GoStdlibDetector::is_stdlib_type("bytes.Buffer"));
        assert!(GoStdlibDetector::is_stdlib_type("bytes.Reader"));

        // Test strings types
        assert!(GoStdlibDetector::is_stdlib_type("strings.Builder"));
        assert!(GoStdlibDetector::is_stdlib_type("strings.Reader"));

        // Test net types
        assert!(GoStdlibDetector::is_stdlib_type("net.IPMask"));
        assert!(GoStdlibDetector::is_stdlib_type("net.TCPAddr"));
        assert!(GoStdlibDetector::is_stdlib_type("net.UDPAddr"));
        assert!(GoStdlibDetector::is_stdlib_type("net.UnixAddr"));

        // Test http types
        assert!(GoStdlibDetector::is_stdlib_type("http.Cookie"));
        assert!(GoStdlibDetector::is_stdlib_type("http.Header"));
        assert!(GoStdlibDetector::is_stdlib_type("http.Transport"));
        assert!(GoStdlibDetector::is_stdlib_type("http.ServeMux"));
    }

    #[test]
    fn test_stdlib_functions_coverage() {
        // Test strings functions
        assert!(GoStdlibDetector::is_stdlib_call("strings.Split"));
        assert!(GoStdlibDetector::is_stdlib_call("strings.Join"));
        assert!(GoStdlibDetector::is_stdlib_call("strings.Contains"));
        assert!(GoStdlibDetector::is_stdlib_call("strings.ToLower"));

        // Test bytes functions
        assert!(GoStdlibDetector::is_stdlib_call("bytes.Split"));
        assert!(GoStdlibDetector::is_stdlib_call("bytes.Join"));
        assert!(GoStdlibDetector::is_stdlib_call("bytes.Contains"));

        // Test regexp functions
        assert!(GoStdlibDetector::is_stdlib_call("regexp.MustCompile"));
        assert!(GoStdlibDetector::is_stdlib_call("regexp.Compile"));

        // Test json functions
        assert!(GoStdlibDetector::is_stdlib_call("json.Marshal"));
        assert!(GoStdlibDetector::is_stdlib_call("json.Unmarshal"));
        assert!(GoStdlibDetector::is_stdlib_call("json.NewEncoder"));

        // Test os functions
        assert!(GoStdlibDetector::is_stdlib_call("os.Open"));
        assert!(GoStdlibDetector::is_stdlib_call("os.Create"));
        assert!(GoStdlibDetector::is_stdlib_call("os.ReadFile"));

        // Test ioutil functions
        assert!(GoStdlibDetector::is_stdlib_call("ioutil.ReadFile"));
        assert!(GoStdlibDetector::is_stdlib_call("ioutil.WriteFile"));

        // Test slices functions (Go 1.21+)
        assert!(GoStdlibDetector::is_stdlib_call("slices.Sort"));
        assert!(GoStdlibDetector::is_stdlib_call("slices.Contains"));
        assert!(GoStdlibDetector::is_stdlib_call("slices.Clone"));

        // Test maps functions (Go 1.21+)
        assert!(GoStdlibDetector::is_stdlib_call("maps.Keys"));
        assert!(GoStdlibDetector::is_stdlib_call("maps.Values"));
        assert!(GoStdlibDetector::is_stdlib_call("maps.Clone"));
    }
}
