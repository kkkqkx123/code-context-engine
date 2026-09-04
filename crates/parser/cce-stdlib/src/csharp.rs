// C# Standard Library Detector
// Handles detection of .NET Framework and .NET Core standard library entities

pub struct CSharpStdlibDetector;

impl CSharpStdlibDetector {
    // .NET namespaces (System.*, Microsoft.*, etc.)
    pub const DOTNET_NAMESPACES: &[&str] = &[
        // Core namespaces
        "System",
        "System.Collections",
        "System.Collections.Generic",
        "System.Collections.Concurrent",
        "System.Collections.ObjectModel",
        "System.Collections.Specialized",
        "System.Linq",
        "System.Linq.Expressions",
        "System.Text",
        "System.Text.RegularExpressions",
        "System.Text.Encoding",
        "System.Text.Json",
        "System.Text.Json.Serialization",
        "System.IO",
        "System.IO.Compression",
        "System.IO.Pipes",
        "System.IO.Ports",
        "System.IO.MemoryMappedFiles",
        "System.IO.IsolatedStorage",
        "System.Threading",
        "System.Threading.Tasks",
        "System.Threading.Channels",
        "System.Threading.Timer",
        "System.Threading.Tasks.Dataflow",
        "System.Net",
        "System.Net.Http",
        "System.Net.Sockets",
        "System.Net.Security",
        "System.Net.WebSockets",
        "System.Net.Mail",
        "System.Net.Mime",
        "System.Net.NetworkInformation",
        "System.Security",
        "System.Security.Cryptography",
        "System.Security.Cryptography.X509Certificates",
        "System.Security.Cryptography.Xml",
        "System.Security.Principal",
        "System.Security.Claims",
        "System.Security.AccessControl",
        "System.Security.Permissions",
        "System.Reflection",
        "System.Reflection.Emit",
        "System.Reflection.Metadata",
        "System.Reflection.PortableExecutable",
        "System.Runtime",
        "System.Runtime.InteropServices",
        "System.Runtime.Serialization",
        "System.Runtime.Serialization.Json",
        "System.Runtime.Serialization.Xml",
        "System.Runtime.Serialization.Formatters",
        "System.Runtime.CompilerServices",
        "System.Runtime.Versioning",
        "System.Runtime.Loader",
        "System.Runtime.Intrinsics",
        "System.Runtime.Intrinsics.X86",
        "System.Runtime.Intrinsics.Arm",
        "System.Xml",
        "System.Xml.Linq",
        "System.Xml.Serialization",
        "System.Xml.Schema",
        "System.Xml.XPath",
        "System.Xml.Xsl",
        "System.Data",
        "System.Data.Common",
        "System.Data.SqlClient",
        "System.Data.SqlTypes",
        "System.Data.Odbc",
        "System.Data.OleDb",
        "System.Data.Entity",
        "System.Data.Linq",
        "System.Data.Services",
        "System.Diagnostics",
        "System.Diagnostics.CodeAnalysis",
        "System.Diagnostics.Tracing",
        "System.Diagnostics.Contracts",
        "System.Diagnostics.SymbolStore",
        "System.Globalization",
        "System.Numerics",
        "System.Numerics.Vectors",
        "System.ComponentModel",
        "System.ComponentModel.DataAnnotations",
        "System.ComponentModel.Composition",
        "System.ComponentModel.Composition.Hosting",
        "System.ComponentModel.Composition.Primitives",
        "System.Drawing",
        "System.Drawing.Drawing2D",
        "System.Drawing.Imaging",
        "System.Drawing.Text",
        "System.Drawing.Printing",
        "System.Windows",
        "System.Windows.Controls",
        "System.Windows.Data",
        "System.Windows.Input",
        "System.Windows.Media",
        "System.Windows.Media.Animation",
        "System.Windows.Media.Imaging",
        "System.Windows.Media.Media3D",
        "System.Windows.Navigation",
        "System.Windows.Shapes",
        "System.Windows.Threading",
        "System.Windows.Forms",
        "System.Windows.Forms.Design",
        "System.Web",
        "System.Web.UI",
        "System.Web.UI.WebControls",
        "System.Web.UI.HtmlControls",
        "System.Web.Services",
        "System.Web.Services.Description",
        "System.Web.Services.Protocols",
        "System.Web.Security",
        "System.Web.Caching",
        "System.Web.SessionState",
        "System.Web.Configuration",
        "System.Web.Hosting",
        "System.Web.Management",
        "System.Web.Routing",
        "System.Web.Mvc",
        "System.Web.Http",
        "System.Web.Optimization",
        "System.Web.WebPages",
        "Microsoft",
        "Microsoft.Win32",
        "Microsoft.Win32.SafeHandles",
        "Microsoft.CSharp",
        "Microsoft.CSharp.RuntimeBinder",
        "Microsoft.Extensions",
        "Microsoft.Extensions.DependencyInjection",
        "Microsoft.Extensions.Logging",
        "Microsoft.Extensions.Configuration",
        "Microsoft.Extensions.Options",
        "Microsoft.Extensions.Hosting",
        "Microsoft.Extensions.FileProviders",
        "Microsoft.Extensions.FileSystemGlobbing",
        "Microsoft.Extensions.Primitives",
        "Microsoft.Extensions.Caching",
        "Microsoft.Extensions.Caching.Memory",
        "Microsoft.Extensions.Caching.Distributed",
        "Microsoft.Extensions.CommandLineUtils",
        "Microsoft.Extensions.DependencyModel",
        "Microsoft.Extensions.Diagnostics",
        "Microsoft.Extensions.Diagnostics.HealthChecks",
        "Microsoft.Extensions.Features",
        "Microsoft.Extensions.Globalization",
        "Microsoft.Extensions.Http",
        "Microsoft.Extensions.Identity.Core",
        "Microsoft.Extensions.Identity.Stores",
        "Microsoft.Extensions.Localization",
        "Microsoft.Extensions.ObjectPool",
        "Microsoft.Extensions.PlatformAbstractions",
        "Microsoft.Extensions.WebEncoders",
        // Common third-party namespaces that are often considered "standard"
        "Newtonsoft.Json",
        "Newtonsoft.Json.Linq",
        "Newtonsoft.Json.Schema",
        "Newtonsoft.Json.Bson",
        "Microsoft.EntityFrameworkCore",
        "Microsoft.EntityFrameworkCore.Design",
        "Microsoft.EntityFrameworkCore.Infrastructure",
        "Microsoft.EntityFrameworkCore.Metadata",
        "Microsoft.EntityFrameworkCore.Metadata.Builders",
        "Microsoft.EntityFrameworkCore.Metadata.Conventions",
        "Microsoft.EntityFrameworkCore.Metadata.Internal",
        "Microsoft.EntityFrameworkCore.Migrations",
        "Microsoft.EntityFrameworkCore.Migrations.Design",
        "Microsoft.EntityFrameworkCore.Migrations.Operations",
        "Microsoft.EntityFrameworkCore.Query",
        "Microsoft.EntityFrameworkCore.Query.Internal",
        "Microsoft.EntityFrameworkCore.Storage",
        "Microsoft.EntityFrameworkCore.Storage.ValueConversion",
        "Microsoft.EntityFrameworkCore.ValueGeneration",
        "Microsoft.AspNetCore",
        "Microsoft.AspNetCore.Mvc",
        "Microsoft.AspNetCore.Http",
        "Microsoft.AspNetCore.Routing",
        "Microsoft.AspNetCore.Hosting",
        "Microsoft.AspNetCore.Builder",
        "Microsoft.AspNetCore.Authentication",
        "Microsoft.AspNetCore.Authentication.Cookies",
        "Microsoft.AspNetCore.Authentication.JwtBearer",
        "Microsoft.AspNetCore.Authorization",
        "Microsoft.AspNetCore.Cors",
        "Microsoft.AspNetCore.Diagnostics",
        "Microsoft.AspNetCore.Html",
        "Microsoft.AspNetCore.Identity",
        "Microsoft.AspNetCore.Localization",
        "Microsoft.AspNetCore.Mvc.Razor",
        "Microsoft.AspNetCore.Mvc.RazorPages",
        "Microsoft.AspNetCore.Mvc.TagHelpers",
        "Microsoft.AspNetCore.Mvc.ViewFeatures",
        "Microsoft.AspNetCore.ResponseCaching",
        "Microsoft.AspNetCore.ResponseCompression",
        "Microsoft.AspNetCore.Server",
        "Microsoft.AspNetCore.Session",
        "Microsoft.AspNetCore.StaticFiles",
        "Microsoft.AspNetCore.WebUtilities",
        "Microsoft.AspNetCore.WebSockets",
        "Microsoft.Extensions.DependencyInjection",
        "Microsoft.Extensions.Logging",
        "Microsoft.Extensions.Configuration",
        "Microsoft.Extensions.Options",
        "Microsoft.Extensions.Hosting",
        "Microsoft.Extensions.FileProviders",
        "Microsoft.Extensions.FileSystemGlobbing",
        "Microsoft.Extensions.Primitives",
        "Microsoft.Extensions.Caching",
        "Microsoft.Extensions.Caching.Memory",
        "Microsoft.Extensions.Caching.Distributed",
        "Microsoft.Extensions.CommandLineUtils",
        "Microsoft.Extensions.DependencyModel",
        "Microsoft.Extensions.Diagnostics",
        "Microsoft.Extensions.Diagnostics.HealthChecks",
        "Microsoft.Extensions.Features",
        "Microsoft.Extensions.Globalization",
        "Microsoft.Extensions.Http",
        "Microsoft.Extensions.Identity.Core",
        "Microsoft.Extensions.Identity.Stores",
        "Microsoft.Extensions.Localization",
        "Microsoft.Extensions.ObjectPool",
        "Microsoft.Extensions.PlatformAbstractions",
        "Microsoft.Extensions.WebEncoders",
    ];

    // Built-in types (aliases)
    pub const BUILTIN_TYPES: &[&str] = &[
        "object", "string", "bool", "byte", "sbyte", "char", "decimal", "double", "float", "int",
        "uint", "long", "ulong", "short", "ushort", "nint", "nuint", "void", "dynamic", "var",
    ];

    // System types (full names)
    pub const SYSTEM_TYPES: &[&str] = &[
        "System.Object",
        "System.String",
        "System.Boolean",
        "System.Byte",
        "System.SByte",
        "System.Char",
        "System.Decimal",
        "System.Double",
        "System.Single",
        "System.Int32",
        "System.UInt32",
        "System.Int64",
        "System.UInt64",
        "System.Int16",
        "System.UInt16",
        "System.IntPtr",
        "System.UIntPtr",
        "System.Void",
        "System.Array",
        "System.Delegate",
        "System.MulticastDelegate",
        "System.Enum",
        "System.ValueType",
        "System.Type",
        "System.Attribute",
        "System.Exception",
        "System.Nullable",
        "System.Threading.Tasks.Task",
        "System.Threading.Tasks.Task`1",
        "System.Collections.Generic.IEnumerable`1",
        "System.Collections.Generic.IList`1",
        "System.Collections.Generic.IDictionary`2",
        "System.IDisposable",
        "System.IComparable",
        "System.IComparable`1",
        "System.IEquatable`1",
        "System.IFormattable",
        "System.ICloneable",
        "System.Collections.IEnumerable",
        "System.Collections.ICollection",
        "System.Collections.IList",
        "System.Collections.IDictionary",
        "System.Collections.Generic.List`1",
        "System.Collections.Generic.Dictionary`2",
        "System.Collections.Generic.HashSet`1",
        "System.Collections.Generic.Queue`1",
        "System.Collections.Generic.Stack`1",
        "System.Collections.Generic.LinkedList`1",
        "System.Collections.ObjectModel.ObservableCollection`1",
        "System.Collections.ObjectModel.ReadOnlyCollection`1",
        "System.Collections.ObjectModel.ReadOnlyDictionary`2",
        "System.Linq.Enumerable",
        "System.Linq.ILookup`2",
        "System.Linq.IGrouping`2",
        "System.Collections.Concurrent.ConcurrentDictionary`2",
        "System.Collections.Concurrent.ConcurrentBag`1",
        "System.Collections.Concurrent.ConcurrentQueue`1",
        "System.Collections.Concurrent.ConcurrentStack`1",
        "System.Text.StringBuilder",
        "System.Text.RegularExpressions.Regex",
        "System.DateTime",
        "System.TimeSpan",
        "System.DateTimeOffset",
        "System.TimeZoneInfo",
        "System.Guid",
        "System.Uri",
        "System.Version",
        "System.Random",
        "System.Math",
        "System.Environment",
        "System.Console",
        "System.Diagnostics.Debug",
        "System.Diagnostics.Trace",
        "System.IO.Stream",
        "System.IO.FileStream",
        "System.IO.MemoryStream",
        "System.IO.TextReader",
        "System.IO.TextWriter",
        "System.IO.StreamReader",
        "System.IO.StreamWriter",
        "System.IO.File",
        "System.IO.Directory",
        "System.IO.Path",
        "System.IO.BinaryReader",
        "System.IO.BinaryWriter",
        "System.Net.WebClient",
        "System.Net.Http.HttpClient",
        "System.Net.Http.HttpRequestMessage",
        "System.Net.Http.HttpResponseMessage",
        "System.Net.Http.HttpContent",
        "System.Net.Http.StringContent",
        "System.Threading.Thread",
        "System.Threading.Monitor",
        "System.Threading.Mutex",
        "System.Threading.Semaphore",
        "System.Threading.SemaphoreSlim",
        "System.Threading.ReaderWriterLockSlim",
        "System.Threading.CancellationToken",
        "System.Threading.CancellationTokenSource",
        "System.Threading.Tasks.Parallel",
        "System.Timers.Timer",
        "System.Reflection.Assembly",
        "System.Reflection.MemberInfo",
        "System.Reflection.TypeInfo",
        "System.Reflection.MethodInfo",
        "System.Reflection.PropertyInfo",
        "System.Reflection.FieldInfo",
        "System.Reflection.EventInfo",
        "System.Reflection.ParameterInfo",
        "System.Runtime.Serialization.ISerializable",
        "System.Runtime.Serialization.DataContractAttribute",
        "System.Runtime.Serialization.DataMemberAttribute",
        "System.Xml.XmlDocument",
        "System.Xml.XmlElement",
        "System.Xml.XmlNode",
        "System.Xml.Linq.XDocument",
        "System.Xml.Linq.XElement",
        "System.Xml.Linq.XNode",
        "System.Data.DataSet",
        "System.Data.DataTable",
        "System.Data.DataRow",
        "System.Data.Common.DbConnection",
        "System.Data.SqlClient.SqlConnection",
        "System.ComponentModel.INotifyPropertyChanged",
        "System.Collections.Specialized.NotifyCollectionChangedEventArgs",
        "System.Windows.Input.ICommand",
        "System.Action",
        "System.Action`1",
        "System.Action`2",
        "System.Action`3",
        "System.Action`4",
        "System.Func`1",
        "System.Func`2",
        "System.Func`3",
        "System.Func`4",
        "System.Func`5",
        "System.Predicate`1",
        "System.Comparison`1",
        "System.EventHandler",
        "System.EventHandler`1",
        "System.IProgress`1",
        "System.IObserver`1",
        "System.IObservable`1",
    ];

    // Common methods (static and instance)
    pub const COMMON_METHODS: &[&str] = &[
        // Console methods
        "Write",
        "WriteLine",
        "Read",
        "ReadLine",
        "ReadKey",
        "Clear",
        "SetCursorPosition",
        "SetWindowPosition",
        "SetWindowSize",
        "SetBufferSize",
        "Beep",
        // String methods
        "Length",
        "Substring",
        "Replace",
        "ToLower",
        "ToUpper",
        "Trim",
        "TrimStart",
        "TrimEnd",
        "PadLeft",
        "PadRight",
        "Contains",
        "StartsWith",
        "EndsWith",
        "IndexOf",
        "LastIndexOf",
        "Split",
        "Join",
        "Concat",
        "Format",
        "IsNullOrEmpty",
        "IsNullOrWhiteSpace",
        "Compare",
        "CompareOrdinal",
        "Equals",
        // Math methods
        "Abs",
        "Acos",
        "Asin",
        "Atan",
        "Atan2",
        "Ceiling",
        "Cos",
        "Cosh",
        "Exp",
        "Floor",
        "Log",
        "Log10",
        "Max",
        "Min",
        "Pow",
        "Round",
        "Sign",
        "Sin",
        "Sinh",
        "Sqrt",
        "Tan",
        "Tanh",
        "Truncate",
        // DateTime methods
        "Now",
        "UtcNow",
        "Today",
        "AddDays",
        "AddHours",
        "AddMinutes",
        "AddSeconds",
        "AddMilliseconds",
        "AddTicks",
        "AddMonths",
        "AddYears",
        "Subtract",
        "ToLocalTime",
        "ToUniversalTime",
        "ToString",
        "Parse",
        "ParseExact",
        "TryParse",
        "TryParseExact",
        // List methods
        "Add",
        "AddRange",
        "Insert",
        "InsertRange",
        "Remove",
        "RemoveAt",
        "RemoveRange",
        "RemoveAll",
        "Clear",
        "Contains",
        "IndexOf",
        "LastIndexOf",
        "Find",
        "FindAll",
        "FindIndex",
        "FindLast",
        "FindLastIndex",
        "ForEach",
        "GetRange",
        "Reverse",
        "Sort",
        "ToArray",
        "TrimExcess",
        "TrueForAll",
        // Dictionary methods
        "Add",
        "Remove",
        "Clear",
        "ContainsKey",
        "ContainsValue",
        "TryGetValue",
        "Keys",
        "Values",
        // LINQ extension methods
        "Where",
        "Select",
        "SelectMany",
        "OrderBy",
        "OrderByDescending",
        "ThenBy",
        "ThenByDescending",
        "GroupBy",
        "Join",
        "GroupJoin",
        "Reverse",
        "Distinct",
        "Union",
        "Intersect",
        "Except",
        "Concat",
        "Zip",
        "Skip",
        "SkipWhile",
        "Take",
        "TakeWhile",
        "First",
        "FirstOrDefault",
        "Last",
        "LastOrDefault",
        "Single",
        "SingleOrDefault",
        "ElementAt",
        "ElementAtOrDefault",
        "Any",
        "All",
        "Contains",
        "Count",
        "LongCount",
        "Sum",
        "Min",
        "Max",
        "Average",
        "Aggregate",
        "ToList",
        "ToArray",
        "ToDictionary",
        "ToLookup",
        "Cast",
        "OfType",
        "AsEnumerable",
        "AsQueryable",
        "DefaultIfEmpty",
        "SequenceEqual",
        "Empty",
        "Range",
        "Repeat",
    ];

    // C# keywords
    pub const KEYWORDS: &[&str] = &[
        "abstract",
        "as",
        "base",
        "bool",
        "break",
        "byte",
        "case",
        "catch",
        "char",
        "checked",
        "class",
        "const",
        "continue",
        "decimal",
        "default",
        "delegate",
        "do",
        "double",
        "else",
        "enum",
        "event",
        "explicit",
        "extern",
        "false",
        "finally",
        "fixed",
        "float",
        "for",
        "foreach",
        "goto",
        "if",
        "implicit",
        "in",
        "int",
        "interface",
        "internal",
        "is",
        "lock",
        "long",
        "namespace",
        "new",
        "null",
        "object",
        "operator",
        "out",
        "override",
        "params",
        "private",
        "protected",
        "public",
        "readonly",
        "ref",
        "return",
        "sbyte",
        "sealed",
        "short",
        "sizeof",
        "stackalloc",
        "static",
        "string",
        "struct",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "uint",
        "ulong",
        "unchecked",
        "unsafe",
        "ushort",
        "using",
        "virtual",
        "void",
        "volatile",
        "while",
    ];

    // C# context keywords
    pub const CONTEXT_KEYWORDS: &[&str] = &[
        "add",
        "alias",
        "ascending",
        "async",
        "await",
        "by",
        "descending",
        "dynamic",
        "equals",
        "from",
        "get",
        "global",
        "group",
        "into",
        "join",
        "let",
        "nameof",
        "on",
        "orderby",
        "partial",
        "remove",
        "select",
        "set",
        "value",
        "var",
        "when",
        "where",
        "yield",
    ];

    // .NET standard library constants
    pub const STDLIB_CONSTANTS: &[&str] = &[
        // Math constants
        "Math.PI",
        "Math.E",
        "Math.Tau",
        // String constants
        "String.Empty",
        "StringComparison.CurrentCulture",
        "StringComparison.CurrentCultureIgnoreCase",
        "StringComparison.InvariantCulture",
        "StringComparison.InvariantCultureIgnoreCase",
        "StringComparison.Ordinal",
        "StringComparison.OrdinalIgnoreCase",
        // Environment constants
        "Environment.NewLine",
        "Environment.CurrentDirectory",
        "Environment.MachineName",
        "Environment.OSVersion",
        "Environment.ProcessorCount",
        "Environment.SystemDirectory",
        "Environment.UserName",
        "Environment.UserDomainName",
        "Environment.Version",
        // TimeSpan constants
        "TimeSpan.Zero",
        "TimeSpan.MaxValue",
        "TimeSpan.MinValue",
        "TimeSpan.TicksPerDay",
        "TimeSpan.TicksPerHour",
        "TimeSpan.TicksPerMillisecond",
        "TimeSpan.TicksPerMinute",
        "TimeSpan.TicksPerSecond",
        // DateTime constants
        "DateTime.MinValue",
        "DateTime.MaxValue",
        "DateTime.UnixEpoch",
        // Guid constants
        "Guid.Empty",
        // ConsoleKey constants
        "ConsoleKey.Enter",
        "ConsoleKey.Escape",
        "ConsoleKey.Spacebar",
        "ConsoleKey.Tab",
        "ConsoleKey.Backspace",
        "ConsoleKey.Delete",
        "ConsoleKey.Insert",
        "ConsoleKey.Home",
        "ConsoleKey.End",
        "ConsoleKey.PageUp",
        "ConsoleKey.PageDown",
        "ConsoleKey.UpArrow",
        "ConsoleKey.DownArrow",
        "ConsoleKey.LeftArrow",
        "ConsoleKey.RightArrow",
        "ConsoleKey.F1",
        "ConsoleKey.F2",
        "ConsoleKey.F3",
        "ConsoleKey.F4",
        "ConsoleKey.F5",
        "ConsoleKey.F6",
        "ConsoleKey.F7",
        "ConsoleKey.F8",
        "ConsoleKey.F9",
        "ConsoleKey.F10",
        "ConsoleKey.F11",
        "ConsoleKey.F12",
        // ConsoleColor constants
        "ConsoleColor.Black",
        "ConsoleColor.DarkBlue",
        "ConsoleColor.DarkGreen",
        "ConsoleColor.DarkCyan",
        "ConsoleColor.DarkRed",
        "ConsoleColor.DarkMagenta",
        "ConsoleColor.DarkYellow",
        "ConsoleColor.Gray",
        "ConsoleColor.DarkGray",
        "ConsoleColor.Blue",
        "ConsoleColor.Green",
        "ConsoleColor.Cyan",
        "ConsoleColor.Red",
        "ConsoleColor.Magenta",
        "ConsoleColor.Yellow",
        "ConsoleColor.White",
        // DateTimeKind constants
        "DateTimeKind.Unspecified",
        "DateTimeKind.Utc",
        "DateTimeKind.Local",
        // DayOfWeek constants
        "DayOfWeek.Sunday",
        "DayOfWeek.Monday",
        "DayOfWeek.Tuesday",
        "DayOfWeek.Wednesday",
        "DayOfWeek.Thursday",
        "DayOfWeek.Friday",
        "DayOfWeek.Saturday",
        // StringComparison constants (duplicate, but commonly used)
        "StringComparison.CurrentCulture",
        "StringComparison.CurrentCultureIgnoreCase",
        "StringComparison.InvariantCulture",
        "StringComparison.InvariantCultureIgnoreCase",
        "StringComparison.Ordinal",
        "StringComparison.OrdinalIgnoreCase",
        // UriScheme constants
        "Uri.UriSchemeHttp",
        "Uri.UriSchemeHttps",
        "Uri.UriSchemeFtp",
        "Uri.UriSchemeFile",
        "Uri.UriSchemeMailto",
        "Uri.UriSchemeNews",
        "Uri.UriSchemeGopher",
        // FileAccess constants
        "FileAccess.Read",
        "FileAccess.Write",
        "FileAccess.ReadWrite",
        // FileMode constants
        "FileMode.Create",
        "FileMode.CreateNew",
        "FileMode.Open",
        "FileMode.OpenOrCreate",
        "FileMode.Truncate",
        "FileMode.Append",
        // FileShare constants
        "FileShare.None",
        "FileShare.Read",
        "FileShare.Write",
        "FileShare.ReadWrite",
        "FileShare.Delete",
        // SeekOrigin constants
        "SeekOrigin.Begin",
        "SeekOrigin.Current",
        "SeekOrigin.End",
        // StringSplitOptions constants
        "StringSplitOptions.None",
        "StringSplitOptions.RemoveEmptyEntries",
        // StringComparison constants (already listed above)
        // StringComparison.CurrentCulture,
        // StringComparison.CurrentCultureIgnoreCase,
        // StringComparison.InvariantCulture,
        // StringComparison.InvariantCultureIgnoreCase,
        // StringComparison.Ordinal,
        // StringComparison.OrdinalIgnoreCase,
        // BindingFlags constants
        "BindingFlags.Default",
        "BindingFlags.IgnoreCase",
        "BindingFlags.DeclaredOnly",
        "BindingFlags.Instance",
        "BindingFlags.Static",
        "BindingFlags.Public",
        "BindingFlags.NonPublic",
        "BindingFlags.FlattenHierarchy",
        "BindingFlags.InvokeMethod",
        "BindingFlags.CreateInstance",
        "BindingFlags.GetField",
        "BindingFlags.SetField",
        "BindingFlags.GetProperty",
        "BindingFlags.SetProperty",
        "BindingFlags.PutDispProperty",
        "BindingFlags.PutRefDispProperty",
        "BindingFlags.ExactBinding",
        "BindingFlags.SuppressChangeType",
        "BindingFlags.OptionalParamBinding",
        "BindingFlags.IgnoreReturn",
        // ProcessorArchitecture constants
        "ProcessorArchitecture.None",
        "ProcessorArchitecture.MSIL",
        "ProcessorArchitecture.X86",
        "ProcessorArchitecture.Arm",
        "ProcessorArchitecture.Amd64",
        "ProcessorArchitecture.IA64",
        // PlatformID constants
        "PlatformID.Win32S",
        "PlatformID.Win32Windows",
        "PlatformID.Win32NT",
        "PlatformID.WinCE",
        "PlatformID.Unix",
        "PlatformID.Xbox",
        "PlatformID.MacOSX",
    ];
}

// Generate simple containment check functions
impl_list_checker!(
    CSharpStdlibDetector,
    [
        (DOTNET_NAMESPACES, is_dotnet_namespace),
        (BUILTIN_TYPES, is_builtin_type),
        (COMMON_METHODS, is_common_method),
        (STDLIB_CONSTANTS, is_stdlib_constant),
        (KEYWORDS, is_keyword),
        (CONTEXT_KEYWORDS, is_context_keyword),
    ]
);

impl CSharpStdlibDetector {
    pub fn is_system_type(name: &str) -> bool {
        // Direct match with full type name
        if Self::SYSTEM_TYPES.contains(&name) {
            return true;
        }

        // Check if it's a short name (e.g., "Console" for "System.Console")
        // by checking if any system type ends with ".{name}"
        let suffix = format!(".{}", name);
        Self::SYSTEM_TYPES.iter().any(|&t| t.ends_with(&suffix))
    }

    /// Check if a qualified path is from .NET
    pub fn is_dotnet_path(path: &str) -> bool {
        // C# uses . as namespace separator
        let first_component = path.split('.').next().unwrap_or("");
        Self::is_dotnet_namespace(first_component)
    }

    /// Check if a type name is a .NET builtin or system type
    pub fn is_stdlib_type(name: &str) -> bool {
        Self::is_builtin_type(name) || Self::is_system_type(name) || Self::is_dotnet_path(name)
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for builtin type
        if Self::is_builtin_type(call_name) {
            return true;
        }

        // Check for system type
        if Self::is_system_type(call_name) {
            return true;
        }

        // Check for stdlib constant
        if Self::is_stdlib_constant(call_name) {
            return true;
        }

        // Check for keyword (though keywords are not typically "called")
        if Self::is_keyword(call_name) || Self::is_context_keyword(call_name) {
            return true;
        }

        // Check for qualified path (e.g., System.Console.WriteLine, Console.WriteLine)
        if call_name.contains('.') {
            let parts: Vec<&str> = call_name.split('.').collect();
            if parts.len() >= 2 {
                // Check if the first part is a .NET namespace
                let first_part = parts[0];
                if Self::is_dotnet_namespace(first_part) {
                    return true;
                }

                // Check if it's a common method on a known type
                // For example: "String.IsNullOrEmpty", "Console.WriteLine"
                // We already checked for system types above, so this catches method calls
                if parts.len() >= 2 {
                    let type_part = parts[0];
                    let method_part = parts[1];

                    // Check if it's a method on a system type
                    if Self::is_system_type(type_part) && Self::is_common_method(method_part) {
                        return true;
                    }

                    // Check if it's a static method call like "System.Console.WriteLine"
                    let full_type = if parts.len() > 2 {
                        format!("{}.{}", parts[0], parts[1])
                    } else {
                        type_part.to_string()
                    };

                    if Self::is_system_type(&full_type)
                        && Self::is_common_method(parts.last().unwrap_or(&""))
                    {
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
            // Most call types in C# use the same detection logic
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::CallbackCall
            | RelationType::GenericCall => {
                // For C#, use the legacy detection logic
                Self::is_stdlib_call(call_name)
            }

            // Other relation types are not relevant for stdlib detection
            _ => false,
        }
    }
}

// Generate get_category using macro
impl_stdlib_categorizer!(
    CSharpStdlibDetector,
    [
        (
            StdlibCategory::Collection,
            [
                "System.Collections",
                "System.Collections.Generic",
                "System.Collections.Concurrent",
                "System.Collections.ObjectModel",
                "System.Collections.Specialized"
            ]
        ),
        (
            StdlibCategory::Io,
            [
                "System.IO",
                "System.IO.Compression",
                "System.IO.Pipes",
                "System.IO.Ports",
                "System.IO.MemoryMappedFiles",
                "System.IO.IsolatedStorage"
            ]
        ),
        (
            StdlibCategory::Concurrency,
            [
                "System.Threading",
                "System.Threading.Tasks",
                "System.Threading.Channels",
                "System.Threading.Timer",
                "System.Threading.Tasks.Dataflow"
            ]
        ),
        (
            StdlibCategory::Utility,
            [
                "System",
                "System.Text",
                "System.Text.RegularExpressions",
                "System.Text.Encoding",
                "System.Text.Json",
                "System.Text.Json.Serialization",
                "System.ComponentModel",
                "System.ComponentModel.DataAnnotations",
                "System.ComponentModel.Composition",
                "System.Globalization",
                "System.Numerics",
                "System.Numerics.Vectors"
            ]
        ),
        (
            StdlibCategory::String,
            [
                "System.Text",
                "System.Text.RegularExpressions",
                "System.Text.Encoding"
            ]
        ),
        (
            StdlibCategory::Numeric,
            ["System.Numerics", "System.Numerics.Vectors"]
        ),
        (StdlibCategory::Error, []),
        (StdlibCategory::Trait, []),
        (StdlibCategory::Macro, []),
    ]
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_dotnet_namespace() {
        assert!(CSharpStdlibDetector::is_dotnet_namespace("System"));
        assert!(CSharpStdlibDetector::is_dotnet_namespace(
            "System.Collections"
        ));
        assert!(CSharpStdlibDetector::is_dotnet_namespace("Microsoft"));
        assert!(!CSharpStdlibDetector::is_dotnet_namespace("MyNamespace"));
    }

    #[test]
    fn test_is_builtin_type() {
        assert!(CSharpStdlibDetector::is_builtin_type("string"));
        assert!(CSharpStdlibDetector::is_builtin_type("int"));
        assert!(CSharpStdlibDetector::is_builtin_type("bool"));
        assert!(!CSharpStdlibDetector::is_builtin_type("MyClass"));
    }

    #[test]
    fn test_is_system_type() {
        assert!(CSharpStdlibDetector::is_system_type("System.String"));
        assert!(CSharpStdlibDetector::is_system_type("System.Int32"));
        assert!(CSharpStdlibDetector::is_system_type(
            "System.Collections.Generic.List`1"
        ));
        assert!(!CSharpStdlibDetector::is_system_type("MyNamespace.MyClass"));
    }

    #[test]
    fn test_is_keyword() {
        assert!(CSharpStdlibDetector::is_keyword("class"));
        assert!(CSharpStdlibDetector::is_keyword("public"));
        assert!(CSharpStdlibDetector::is_keyword("if"));
        assert!(!CSharpStdlibDetector::is_keyword("MyKeyword"));
    }

    #[test]
    fn test_is_context_keyword() {
        assert!(CSharpStdlibDetector::is_context_keyword("var"));
        assert!(CSharpStdlibDetector::is_context_keyword("async"));
        assert!(CSharpStdlibDetector::is_context_keyword("await"));
        assert!(!CSharpStdlibDetector::is_context_keyword("mykeyword"));
    }

    #[test]
    fn test_is_dotnet_path() {
        assert!(CSharpStdlibDetector::is_dotnet_path("System"));
        assert!(CSharpStdlibDetector::is_dotnet_path("System.Console"));
        assert!(CSharpStdlibDetector::is_dotnet_path("Microsoft.Extensions"));
        assert!(!CSharpStdlibDetector::is_dotnet_path("MyNamespace.MyClass"));
    }

    #[test]
    fn test_is_stdlib_call() {
        // Builtin types
        assert!(CSharpStdlibDetector::is_stdlib_call("string"));
        assert!(CSharpStdlibDetector::is_stdlib_call("int"));

        // System types
        assert!(CSharpStdlibDetector::is_stdlib_call("System.String"));
        assert!(CSharpStdlibDetector::is_stdlib_call(
            "System.Collections.Generic.List`1"
        ));

        // Method calls on system types
        assert!(CSharpStdlibDetector::is_stdlib_call("Console.WriteLine"));
        assert!(CSharpStdlibDetector::is_stdlib_call("String.IsNullOrEmpty"));
        assert!(CSharpStdlibDetector::is_stdlib_call(
            "System.Console.WriteLine"
        ));

        // LINQ methods
        assert!(CSharpStdlibDetector::is_stdlib_call("Enumerable.Where"));

        // Keywords (though not typically "called")
        assert!(CSharpStdlibDetector::is_stdlib_call("class"));
        assert!(CSharpStdlibDetector::is_stdlib_call("var"));

        // Negative cases
        assert!(!CSharpStdlibDetector::is_stdlib_call("MyClass"));
        assert!(!CSharpStdlibDetector::is_stdlib_call("MyNamespace.MyClass"));
        assert!(!CSharpStdlibDetector::is_stdlib_call("MyClass.MyMethod"));
    }

    #[test]
    fn test_is_stdlib_constant() {
        // Math constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("Math.PI"));
        assert!(CSharpStdlibDetector::is_stdlib_constant("Math.E"));
        assert!(CSharpStdlibDetector::is_stdlib_constant("Math.Tau"));

        // String constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("String.Empty"));
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "StringComparison.Ordinal"
        ));

        // Environment constants
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "Environment.NewLine"
        ));
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "Environment.CurrentDirectory"
        ));

        // TimeSpan constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("TimeSpan.Zero"));
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "TimeSpan.MaxValue"
        ));

        // DateTime constants
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "DateTime.MinValue"
        ));
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "DateTime.MaxValue"
        ));

        // Guid constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("Guid.Empty"));

        // ConsoleKey constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("ConsoleKey.Enter"));
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "ConsoleKey.Escape"
        ));

        // ConsoleColor constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("ConsoleColor.Red"));
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "ConsoleColor.Blue"
        ));

        // DateTimeKind constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("DateTimeKind.Utc"));
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "DateTimeKind.Local"
        ));

        // DayOfWeek constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("DayOfWeek.Monday"));
        assert!(CSharpStdlibDetector::is_stdlib_constant("DayOfWeek.Friday"));

        // UriScheme constants
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "Uri.UriSchemeHttp"
        ));
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "Uri.UriSchemeHttps"
        ));

        // FileAccess constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("FileAccess.Read"));
        assert!(CSharpStdlibDetector::is_stdlib_constant("FileAccess.Write"));

        // FileMode constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("FileMode.Create"));
        assert!(CSharpStdlibDetector::is_stdlib_constant("FileMode.Open"));

        // FileShare constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("FileShare.Read"));
        assert!(CSharpStdlibDetector::is_stdlib_constant("FileShare.None"));

        // SeekOrigin constants
        assert!(CSharpStdlibDetector::is_stdlib_constant("SeekOrigin.Begin"));
        assert!(CSharpStdlibDetector::is_stdlib_constant(
            "SeekOrigin.Current"
        ));

        // Negative cases
        assert!(!CSharpStdlibDetector::is_stdlib_constant("MyConstant"));
        assert!(!CSharpStdlibDetector::is_stdlib_constant(
            "MyNamespace.MyConstant"
        ));
    }
}
