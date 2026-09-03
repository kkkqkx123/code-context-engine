// Lua Standard Library Detector
// Handles detection of Lua standard library functions and modules

pub struct LuaStdlibDetector;

impl LuaStdlibDetector {
    // Lua standard library modules and functions (MUST be sorted)
    pub const STDLIB_MODULES: &[&str] = &[
        "_G",
        "_VERSION",
        "assert",
        "bit",
        "bit32",
        "bit32.arshift",
        "bit32.band",
        "bit32.bnot",
        "bit32.bor",
        "bit32.btest",
        "bit32.bxor",
        "bit32.extract",
        "bit32.lrotate",
        "bit32.lshift",
        "bit32.replace",
        "bit32.rrotate",
        "bit32.rshift",
        "coroutine",
        "coroutine.create",
        "coroutine.isyieldable",
        "coroutine.resume",
        "coroutine.running",
        "coroutine.status",
        "coroutine.wrap",
        "coroutine.yield",
        "debug",
        "debug.debug",
        "debug.gethook",
        "debug.getinfo",
        "debug.getlocal",
        "debug.getmetatable",
        "debug.getregistry",
        "debug.getupvalue",
        "debug.setcstraceback",
        "debug.sethook",
        "debug.setlocal",
        "debug.setmetatable",
        "debug.setupvalue",
        "debug.traceback",
        "debug.upvalueid",
        "debug.upvaluejoin",
        "error",
        "getmetatable",
        "io",
        "io.close",
        "io.flush",
        "io.input",
        "io.lines",
        "io.open",
        "io.output",
        "io.popen",
        "io.read",
        "io.stderr",
        "io.stdin",
        "io.stdout",
        "io.tmpfile",
        "io.type",
        "io.write",
        "ipairs",
        "load",
        "loadstring",
        "math",
        "math.abs",
        "math.acos",
        "math.asin",
        "math.atan",
        "math.atan2",
        "math.ceil",
        "math.cos",
        "math.cosh",
        "math.deg",
        "math.exp",
        "math.floor",
        "math.fmod",
        "math.frexp",
        "math.huge",
        "math.ldexp",
        "math.log",
        "math.log10",
        "math.max",
        "math.min",
        "math.modf",
        "math.pi",
        "math.pow",
        "math.rad",
        "math.random",
        "math.randomseed",
        "math.sin",
        "math.sinh",
        "math.sqrt",
        "math.tan",
        "math.tanh",
        "module",
        "next",
        "os",
        "os.clock",
        "os.date",
        "os.difftime",
        "os.execute",
        "os.exit",
        "os.getenv",
        "os.remove",
        "os.rename",
        "os.setlocale",
        "os.time",
        "os.tmpname",
        "package",
        "package.config",
        "package.cpath",
        "package.loaded",
        "package.loadlib",
        "package.path",
        "package.preload",
        "package.searchers",
        "package.searchpath",
        "package.seeall",
        "pairs",
        "pcall",
        "print",
        "rawget",
        "rawlen",
        "rawset",
        "require",
        "select",
        "setmetatable",
        "string",
        "string.byte",
        "string.char",
        "string.dump",
        "string.find",
        "string.format",
        "string.gmatch",
        "string.gsub",
        "string.len",
        "string.lower",
        "string.match",
        "string.rep",
        "string.reverse",
        "string.sub",
        "string.upper",
        "table",
        "table.concat",
        "table.copy",
        "table.insert",
        "table.move",
        "table.pack",
        "table.remove",
        "table.sort",
        "table.unpack",
        "tonumber",
        "tostring",
        "type",
        "unpack",
        "utf8",
        "utf8.char",
        "utf8.codes",
        "utf8.codepoint",
        "utf8.len",
        "utf8.offset",
        "xpcall",
    ];

    // Built-in types (MUST be sorted)
    pub const BUILTIN_TYPES: &[&str] = &[
        "&",
        "<<",
        ">>",
        "boolean",
        "function",
        "lightuserdata",
        "nil",
        "number",
        "string",
        "table",
        "thread",
        "userdata",
        "|",
        "~",
    ];

    /// Check if a name is a Lua standard library module or function
    pub fn is_stdlib_module(name: &str) -> bool {
        Self::STDLIB_MODULES.binary_search(&name).is_ok()
    }

    /// Check if a name is a built-in Lua type
    pub fn is_builtin_type(name: &str) -> bool {
        Self::BUILTIN_TYPES.binary_search(&name).is_ok()
    }

    /// Check if a call is to stdlib
    pub fn is_stdlib_call(call_name: &str) -> bool {
        // Check for built-in types
        if Self::is_builtin_type(call_name) {
            return true;
        }

        // Check for direct stdlib function
        if Self::is_stdlib_module(call_name) {
            return true;
        }

        // Check for qualified module call (e.g., string.format, math.sqrt)
        if call_name.contains('.') {
            let parts: Vec<&str> = call_name.split('.').collect();
            if parts.len() >= 2 {
                // Check if module prefix exists in stdlib
                if Self::STDLIB_MODULES
                    .iter()
                    .position(|&m| {
                        m.starts_with(parts[0])
                            && (m == parts[0] || (m.chars().nth(parts[0].len()) == Some('.')))
                    })
                    .is_some()
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a call is to stdlib using relation type (optimized)
    pub fn is_stdlib_by_type(
        call_name: &str,
        relation_type: &cce_types::relation::RelationType,
    ) -> bool {
        use cce_types::relation::RelationType;

        match relation_type {
            // Most call types use the same detection logic
            RelationType::DirectCall
            | RelationType::InstanceMethodCall
            | RelationType::StaticMethodCall
            | RelationType::ChainedMethodCall
            | RelationType::ConstructorCall
            | RelationType::CallbackCall
            | RelationType::GenericCall => Self::is_stdlib_call(call_name),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stdlib_module() {
        assert!(LuaStdlibDetector::is_stdlib_module("print"));
        assert!(LuaStdlibDetector::is_stdlib_module("string"));
        assert!(LuaStdlibDetector::is_stdlib_module("math"));
        assert!(LuaStdlibDetector::is_stdlib_module("table"));
        assert!(!LuaStdlibDetector::is_stdlib_module("my_module"));
    }

    #[test]
    fn test_is_builtin_type() {
        assert!(LuaStdlibDetector::is_builtin_type("string"));
        assert!(LuaStdlibDetector::is_builtin_type("number"));
        assert!(LuaStdlibDetector::is_builtin_type("table"));
        assert!(LuaStdlibDetector::is_builtin_type("function"));
        assert!(!LuaStdlibDetector::is_builtin_type("MyClass"));
    }

    #[test]
    fn test_is_stdlib_call() {
        assert!(LuaStdlibDetector::is_stdlib_call("print"));
        assert!(LuaStdlibDetector::is_stdlib_call("string.format"));
        assert!(LuaStdlibDetector::is_stdlib_call("math.sqrt"));
        assert!(LuaStdlibDetector::is_stdlib_call("table.insert"));
        assert!(!LuaStdlibDetector::is_stdlib_call("my_function"));
    }
}
