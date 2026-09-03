// C Standard Library Detector
// Handles detection of C standard library entities

pub struct CStdlibDetector;

impl CStdlibDetector {
    // Standard library headers (C99)
    pub const STDLIB_HEADERS: &[&str] = &[
        // C89/C90 headers
        "assert.h",
        "ctype.h",
        "errno.h",
        "float.h",
        "limits.h",
        "locale.h",
        "math.h",
        "setjmp.h",
        "signal.h",
        "stdarg.h",
        "stddef.h",
        "stdio.h",
        "stdlib.h",
        "string.h",
        "time.h",
        // C95 headers
        "iso646.h",
        "wchar.h",
        "wctype.h",
        // C99 headers
        "complex.h",
        "fenv.h",
        "inttypes.h",
        "stdbool.h",
        "stdint.h",
        "tgmath.h",
        // C11 headers
        "stdalign.h",
        "stdatomic.h",
        "stdnoreturn.h",
        "threads.h",
        "uchar.h",
        // POSIX headers (commonly available)
        "unistd.h",
        "fcntl.h",
        "sys/types.h",
        "sys/stat.h",
        "dirent.h",
        "fnmatch.h",
        "glob.h",
        "grp.h",
        "netdb.h",
        "pwd.h",
        "regex.h",
        "tar.h",
        "termios.h",
        "unistd.h",
        "utime.h",
        "wordexp.h",
        "arpa/inet.h",
        "net/if.h",
        "netinet/in.h",
        "netinet/tcp.h",
        "sys/ipc.h",
        "sys/mman.h",
        "sys/msg.h",
        "sys/resource.h",
        "sys/select.h",
        "sys/sem.h",
        "sys/shm.h",
        "sys/socket.h",
        "sys/stat.h",
        "sys/time.h",
        "sys/times.h",
        "sys/types.h",
        "sys/uio.h",
        "sys/un.h",
        "sys/utsname.h",
        "sys/wait.h",
    ];

    // Standard library functions (C99)
    pub const STDLIB_FUNCTIONS: &[&str] = &[
        // stdio.h functions
        "printf",
        "fprintf",
        "sprintf",
        "snprintf",
        "scanf",
        "fscanf",
        "sscanf",
        "fopen",
        "fclose",
        "fread",
        "fwrite",
        "fgets",
        "fputs",
        "fgetc",
        "fputc",
        "feof",
        "ferror",
        "fflush",
        "fseek",
        "ftell",
        "rewind",
        "clearerr",
        "remove",
        "rename",
        "tmpfile",
        "tmpnam",
        "setbuf",
        "setvbuf",
        "perror",
        "getchar",
        "putchar",
        "gets",
        "puts",
        // stdlib.h functions
        "malloc",
        "calloc",
        "realloc",
        "free",
        "abort",
        "exit",
        "atexit",
        "quick_exit",
        "at_quick_exit",
        "getenv",
        "system",
        "bsearch",
        "qsort",
        "abs",
        "labs",
        "llabs",
        "div",
        "ldiv",
        "lldiv",
        "mblen",
        "mbtowc",
        "wctomb",
        "mbstowcs",
        "wcstombs",
        "rand",
        "srand",
        "atoi",
        "atol",
        "atoll",
        "strtol",
        "strtoul",
        "strtoll",
        "strtoull",
        "strtof",
        "strtod",
        "strtold",
        "aligned_alloc",
        // string.h functions
        "memcpy",
        "memmove",
        "memcmp",
        "memchr",
        "memset",
        "strcpy",
        "strncpy",
        "strcat",
        "strncat",
        "strcmp",
        "strncmp",
        "strcoll",
        "strxfrm",
        "strchr",
        "strrchr",
        "strspn",
        "strcspn",
        "strpbrk",
        "strstr",
        "strtok",
        "strerror",
        "strlen",
        "strnlen",
        // ctype.h functions
        "isalnum",
        "isalpha",
        "isblank",
        "iscntrl",
        "isdigit",
        "isgraph",
        "islower",
        "isprint",
        "ispunct",
        "isspace",
        "isupper",
        "isxdigit",
        "tolower",
        "toupper",
        // math.h functions
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
        "exp",
        "log",
        "log10",
        "log2",
        "log1p",
        "exp2",
        "expm1",
        "pow",
        "sqrt",
        "cbrt",
        "hypot",
        "fabs",
        "ceil",
        "floor",
        "trunc",
        "round",
        "lround",
        "llround",
        "rint",
        "lrint",
        "llrint",
        "nearbyint",
        "remainder",
        "remquo",
        "copysign",
        "nan",
        "nextafter",
        "nexttoward",
        "fdim",
        "fmax",
        "fmin",
        "fma",
        "fpclassify",
        "isfinite",
        "isinf",
        "isnan",
        "isnormal",
        "signbit",
        "isgreater",
        "isgreaterequal",
        "isless",
        "islessequal",
        "islessgreater",
        "isunordered",
        // time.h functions
        "clock",
        "time",
        "difftime",
        "mktime",
        "asctime",
        "ctime",
        "gmtime",
        "localtime",
        "strftime",
        // setjmp.h functions
        "setjmp",
        "longjmp",
        // signal.h functions
        "signal",
        "raise",
        // stdarg.h macros/functions
        "va_start",
        "va_arg",
        "va_end",
        "va_copy",
        // complex.h functions
        "cabs",
        "carg",
        "cimag",
        "creal",
        "conj",
        "cproj",
        "csqrt",
        "cexp",
        "clog",
        "cpow",
        "csin",
        "ccos",
        "ctan",
        "casin",
        "cacos",
        "catan",
        "csinh",
        "ccosh",
        "ctanh",
        "casinh",
        "cacosh",
        "catanh",
        // fenv.h functions
        "feclearexcept",
        "fegetexceptflag",
        "feraiseexcept",
        "fesetexceptflag",
        "fetestexcept",
        "fegetround",
        "fesetround",
        "fegetenv",
        "feholdexcept",
        "fesetenv",
        "feupdateenv",
        // inttypes.h macros
        "PRId8",
        "PRId16",
        "PRId32",
        "PRId64",
        "PRIi8",
        "PRIi16",
        "PRIi32",
        "PRIi64",
        "PRIo8",
        "PRIo16",
        "PRIo32",
        "PRIo64",
        "PRIu8",
        "PRIu16",
        "PRIu32",
        "PRIu64",
        "PRIx8",
        "PRIx16",
        "PRIx32",
        "PRIx64",
        "PRIX8",
        "PRIX16",
        "PRIX32",
        "PRIX64",
        "SCNd8",
        "SCNd16",
        "SCNd32",
        "SCNd64",
        "SCNi8",
        "SCNi16",
        "SCNi32",
        "SCNi64",
        "SCNo8",
        "SCNo16",
        "SCNo32",
        "SCNo64",
        "SCNu8",
        "SCNu16",
        "SCNu32",
        "SCNu64",
        "SCNx8",
        "SCNx16",
        "SCNx32",
        "SCNx64",
        // stdbool.h macros
        "bool",
        "true",
        "false",
        // stdint.h types
        "int8_t",
        "int16_t",
        "int32_t",
        "int64_t",
        "uint8_t",
        "uint16_t",
        "uint32_t",
        "uint64_t",
        "int_least8_t",
        "int_least16_t",
        "int_least32_t",
        "int_least64_t",
        "uint_least8_t",
        "uint_least16_t",
        "uint_least32_t",
        "uint_least64_t",
        "int_fast8_t",
        "int_fast16_t",
        "int_fast32_t",
        "int_fast64_t",
        "uint_fast8_t",
        "uint_fast16_t",
        "uint_fast32_t",
        "uint_fast64_t",
        "intptr_t",
        "uintptr_t",
        "intmax_t",
        "uintmax_t",
        // stddef.h types/macros
        "ptrdiff_t",
        "size_t",
        "wchar_t",
        "NULL",
        "offsetof",
        // stdalign.h macros
        "alignas",
        "alignof",
        // stdnoreturn.h macros
        "noreturn",
        // threads.h functions (C11)
        "thrd_create",
        "thrd_join",
        "thrd_detach",
        "thrd_equal",
        "thrd_current",
        "thrd_sleep",
        "thrd_yield",
        "thrd_exit",
        "mtx_init",
        "mtx_lock",
        "mtx_trylock",
        "mtx_timedlock",
        "mtx_unlock",
        "mtx_destroy",
        "call_once",
        "cnd_init",
        "cnd_signal",
        "cnd_broadcast",
        "cnd_wait",
        "cnd_timedwait",
        "cnd_destroy",
        "tss_create",
        "tss_get",
        "tss_set",
        "tss_delete",
        // uchar.h functions (C11)
        "mbrtoc16",
        "c16rtomb",
        "mbrtoc32",
        "c32rtomb",
        // POSIX functions (commonly available)
        "open",
        "close",
        "read",
        "write",
        "lseek",
        "stat",
        "fstat",
        "lstat",
        "chmod",
        "fchmod",
        "chown",
        "fchown",
        "link",
        "unlink",
        "symlink",
        "readlink",
        "mkdir",
        "rmdir",
        "opendir",
        "readdir",
        "closedir",
        "getcwd",
        "chdir",
        "fork",
        "exec",
        "wait",
        "waitpid",
        "kill",
        "getpid",
        "getppid",
        "getuid",
        "geteuid",
        "getgid",
        "getegid",
        "setuid",
        "setgid",
        "getpwnam",
        "getpwuid",
        "getgrnam",
        "getgrgid",
        "gethostname",
        "gethostbyname",
        "gethostbyaddr",
        "getaddrinfo",
        "getnameinfo",
        "socket",
        "bind",
        "listen",
        "accept",
        "connect",
        "send",
        "recv",
        "sendto",
        "recvfrom",
        "shutdown",
        "setsockopt",
        "getsockopt",
        "getpeername",
        "getsockname",
        "select",
        "poll",
        "epoll_create",
        "epoll_ctl",
        "epoll_wait",
        "kqueue",
        "kevent",
        "ioctl",
        "fcntl",
        "mmap",
        "munmap",
        "mprotect",
        "msync",
        "mlock",
        "munlock",
        "mlockall",
        "munlockall",
        "shm_open",
        "shm_unlink",
        "sem_open",
        "sem_close",
        "sem_unlink",
        "sem_wait",
        "sem_trywait",
        "sem_post",
        "sem_getvalue",
        "msgget",
        "msgsnd",
        "msgrcv",
        "msgctl",
        "shmget",
        "shmat",
        "shmdt",
        "shmctl",
        "semget",
        "semop",
        "semctl",
    ];

    // Standard library types
    pub const STDLIB_TYPES: &[&str] = &[
        // stddef.h types
        "ptrdiff_t",
        "size_t",
        "wchar_t",
        // stdint.h types
        "int8_t",
        "int16_t",
        "int32_t",
        "int64_t",
        "uint8_t",
        "uint16_t",
        "uint32_t",
        "uint64_t",
        "int_least8_t",
        "int_least16_t",
        "int_least32_t",
        "int_least64_t",
        "uint_least8_t",
        "uint_least16_t",
        "uint_least32_t",
        "uint_least64_t",
        "int_fast8_t",
        "int_fast16_t",
        "int_fast32_t",
        "int_fast64_t",
        "uint_fast8_t",
        "uint_fast16_t",
        "uint_fast32_t",
        "uint_fast64_t",
        "intptr_t",
        "uintptr_t",
        "intmax_t",
        "uintmax_t",
        // stdbool.h type
        "bool",
        // time.h types
        "time_t",
        "clock_t",
        "struct tm",
        "struct timespec",
        "struct itimerspec",
        // Other common types
        "FILE",
        "fpos_t",
        "va_list",
        "jmp_buf",
        "sig_atomic_t",
        "div_t",
        "ldiv_t",
        "lldiv_t",
        // Complex types
        "float complex",
        "double complex",
        "long double complex",
        // Threads types (C11)
        "thrd_t",
        "mtx_t",
        "cnd_t",
        "tss_t",
        "once_flag",
        // POSIX types
        "pid_t",
        "uid_t",
        "gid_t",
        "off_t",
        "mode_t",
        "dev_t",
        "ino_t",
        "nlink_t",
        "blksize_t",
        "blkcnt_t",
        "socklen_t",
        "sa_family_t",
        "sockaddr",
        "sockaddr_in",
        "sockaddr_in6",
        "sockaddr_un",
        "sockaddr_storage",
        "addrinfo",
        "hostent",
        "servent",
        "protoent",
        "dirent",
        "stat",
        "timeval",
        "timezone",
        "itimerval",
        "rusage",
        "sigaction",
        "sigset_t",
        "sigval",
        "sigevent",
        "timer_t",
        "mqd_t",
        "pthread_t",
        "pthread_attr_t",
        "pthread_mutex_t",
        "pthread_mutexattr_t",
        "pthread_cond_t",
        "pthread_condattr_t",
        "pthread_rwlock_t",
        "pthread_rwlockattr_t",
        "pthread_spinlock_t",
        "pthread_barrier_t",
        "pthread_barrierattr_t",
        "pthread_once_t",
        "pthread_key_t",
    ];

    // Standard library macros
    pub const STDLIB_MACROS: &[&str] = &[
        // stdio.h macros
        "EOF",
        "BUFSIZ",
        "FOPEN_MAX",
        "FILENAME_MAX",
        "L_tmpnam",
        "SEEK_SET",
        "SEEK_CUR",
        "SEEK_END",
        "TMP_MAX",
        "stdin",
        "stdout",
        "stderr",
        // stdlib.h macros
        "EXIT_SUCCESS",
        "EXIT_FAILURE",
        "RAND_MAX",
        "MB_CUR_MAX",
        // limits.h macros
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
        // float.h macros
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
        // errno.h macros
        "errno",
        "EDOM",
        "ERANGE",
        "EILSEQ",
        // math.h macros
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
        // signal.h macros
        "SIG_DFL",
        "SIG_IGN",
        "SIG_ERR",
        "SIGABRT",
        "SIGFPE",
        "SIGILL",
        "SIGINT",
        "SIGSEGV",
        "SIGTERM",
        // stdarg.h macros
        "va_start",
        "va_arg",
        "va_end",
        "va_copy",
        // stdbool.h macros
        "true",
        "false",
        // stddef.h macros
        "NULL",
        "offsetof",
        // stdnoreturn.h macros
        "noreturn",
        // stdalign.h macros
        "alignas",
        "alignof",
        // POSIX macros
        "O_RDONLY",
        "O_WRONLY",
        "O_RDWR",
        "O_CREAT",
        "O_EXCL",
        "O_TRUNC",
        "O_APPEND",
        "O_NONBLOCK",
        "S_IRUSR",
        "S_IWUSR",
        "S_IXUSR",
        "S_IRGRP",
        "S_IWGRP",
        "S_IXGRP",
        "S_IROTH",
        "S_IWOTH",
        "S_IXOTH",
        "F_OK",
        "R_OK",
        "W_OK",
        "X_OK",
        "SEEK_SET",
        "SEEK_CUR",
        "SEEK_END",
        "STDIN_FILENO",
        "STDOUT_FILENO",
        "STDERR_FILENO",
    ];

    // Standard library constants (subset of macros that represent constant values)
    pub const STDLIB_CONSTANTS: &[&str] = &[
        // stdio.h constants
        "EOF",
        "BUFSIZ",
        "FOPEN_MAX",
        "FILENAME_MAX",
        "L_tmpnam",
        "SEEK_SET",
        "SEEK_CUR",
        "SEEK_END",
        "TMP_MAX",
        "stdin",
        "stdout",
        "stderr",
        // stdlib.h constants
        "EXIT_SUCCESS",
        "EXIT_FAILURE",
        "RAND_MAX",
        "MB_CUR_MAX",
        // limits.h constants
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
        // float.h constants
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
        // errno.h constants
        "EDOM",
        "ERANGE",
        "EILSEQ",
        // math.h constants
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
        // signal.h constants
        "SIG_DFL",
        "SIG_IGN",
        "SIG_ERR",
        "SIGABRT",
        "SIGFPE",
        "SIGILL",
        "SIGINT",
        "SIGSEGV",
        "SIGTERM",
        // stdbool.h constants
        "true",
        "false",
        // stddef.h constants
        "NULL",
        // POSIX constants
        "O_RDONLY",
        "O_WRONLY",
        "O_RDWR",
        "O_CREAT",
        "O_EXCL",
        "O_TRUNC",
        "O_APPEND",
        "O_NONBLOCK",
        "S_IRUSR",
        "S_IWUSR",
        "S_IXUSR",
        "S_IRGRP",
        "S_IWGRP",
        "S_IXGRP",
        "S_IROTH",
        "S_IWOTH",
        "S_IXOTH",
        "F_OK",
        "R_OK",
        "W_OK",
        "X_OK",
        "STDIN_FILENO",
        "STDOUT_FILENO",
        "STDERR_FILENO",
    ];

    pub fn is_stdlib_header(header: &str) -> bool {
        // Remove angle brackets or quotes
        let clean_header = header
            .trim_start_matches('<')
            .trim_start_matches('"')
            .trim_end_matches('>')
            .trim_end_matches('"');

        // Check both with and without .h suffix
        // STDLIB_HEADERS stores names with .h suffix (e.g., "stdio.h")
        if Self::STDLIB_HEADERS.contains(&clean_header) {
            return true;
        }

        // Also check with .h suffix appended (for input like "stdio")
        let with_suffix = format!("{}.h", clean_header);
        if Self::STDLIB_HEADERS.contains(&with_suffix.as_str()) {
            return true;
        }

        false
    }

    pub fn is_stdlib_function(name: &str) -> bool {
        Self::STDLIB_FUNCTIONS.contains(&name)
    }

    pub fn is_stdlib_type(name: &str) -> bool {
        Self::STDLIB_TYPES.contains(&name)
    }

    pub fn is_stdlib_macro(name: &str) -> bool {
        Self::STDLIB_MACROS.contains(&name)
    }

    pub fn is_stdlib_constant(name: &str) -> bool {
        Self::STDLIB_CONSTANTS.contains(&name)
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for direct function call
        if Self::is_stdlib_function(call_name) {
            return true;
        }

        // Check for macro
        if Self::is_stdlib_macro(call_name) {
            return true;
        }

        // Check for type (though types are not typically "called")
        if Self::is_stdlib_type(call_name) {
            return true;
        }

        // C doesn't have namespacing like C++, so we don't need to check for ::
        // However, we might have function-like macros or type casts
        // For simplicity, we just check the direct name

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
            // Most call types in C use the same detection logic
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::CallbackCall
            | RelationType::GenericCall => {
                // For C, use the legacy detection logic
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
    fn test_is_stdlib_header() {
        assert!(CStdlibDetector::is_stdlib_header("stdio.h"));
        assert!(CStdlibDetector::is_stdlib_header("<stdlib.h>"));
        assert!(CStdlibDetector::is_stdlib_header("\"string.h\""));
        assert!(!CStdlibDetector::is_stdlib_header("myheader.h"));
    }

    #[test]
    fn test_is_stdlib_function() {
        assert!(CStdlibDetector::is_stdlib_function("printf"));
        assert!(CStdlibDetector::is_stdlib_function("malloc"));
        assert!(CStdlibDetector::is_stdlib_function("strlen"));
        assert!(!CStdlibDetector::is_stdlib_function("my_function"));
    }

    #[test]
    fn test_is_stdlib_type() {
        assert!(CStdlibDetector::is_stdlib_type("FILE"));
        assert!(CStdlibDetector::is_stdlib_type("size_t"));
        assert!(CStdlibDetector::is_stdlib_type("int32_t"));
        assert!(!CStdlibDetector::is_stdlib_type("MyStruct"));
    }

    #[test]
    fn test_is_stdlib_macro() {
        assert!(CStdlibDetector::is_stdlib_macro("NULL"));
        assert!(CStdlibDetector::is_stdlib_macro("EOF"));
        assert!(CStdlibDetector::is_stdlib_macro("RAND_MAX"));
        assert!(!CStdlibDetector::is_stdlib_macro("MY_MACRO"));
    }

    #[test]
    fn test_is_stdlib_call() {
        assert!(CStdlibDetector::is_stdlib_call("printf"));
        assert!(CStdlibDetector::is_stdlib_call("malloc"));
        assert!(CStdlibDetector::is_stdlib_call("strlen"));
        assert!(CStdlibDetector::is_stdlib_call("NULL")); // macro
        assert!(!CStdlibDetector::is_stdlib_call("my_function"));
        assert!(!CStdlibDetector::is_stdlib_call("MyStruct"));
    }
}
