use super::super::call_utils::split_call_args;

/// A step in a call chain like `foo().bar()` or `module.func`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallStep {
    pub receiver: Option<String>,
    pub method_name: String,
    pub args: Vec<String>,
}

/// Parse call targets like "module.func", "obj.method", "foo().bar()"
/// into a chain of calls.
///
/// Step separators are `.` (most languages), `->` (PHP `$this->m`, C++
/// `ptr->m`), `?.` (Kotlin safe-call) and `?->` (PHP nullsafe), all only
/// at top level (parentheses depth zero). `::` and `/` stay qualification
/// inside one step and are stripped later via [`simple_callee_name`].
pub fn parse_call_chain(call_target: &str) -> Vec<CallStep> {
    use super::super::call_utils::simple_callee_name;
    let mut steps: Vec<String> = Vec::new();
    let trimmed = call_target.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // Split by '.', '->', '?.', '?->' but respect parentheses:
    // `foo().bar` -> ["foo()", "bar"], `$this->m()` -> ["$this", "m()"].
    let mut current = String::new();
    let mut depth = 0;
    let chars: Vec<char> = trimmed.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
                i += 1;
            }
            ')' => {
                depth -= 1;
                current.push(ch);
                i += 1;
            }
            _ if depth == 0
                && (chars[i..].starts_with(&['?', '-', '>'])
                    || chars[i..].starts_with(&['?', '.'])
                    || (ch == '-' && chars.get(i + 1) == Some(&'>'))) =>
            {
                if !current.trim().is_empty() {
                    steps.push(current.trim().to_string());
                }
                current.clear();
                if chars[i..].starts_with(&['?', '-', '>']) {
                    i += 3;
                } else {
                    i += 2;
                }
            }
            '.' if depth == 0 => {
                if !current.trim().is_empty() {
                    steps.push(current.trim().to_string());
                }
                current.clear();
                i += 1;
            }
            _ => {
                current.push(ch);
                i += 1;
            }
        }
    }
    if !current.trim().is_empty() {
        steps.push(current.trim().to_string());
    }
    // Convert raw step strings into CallStep structs
    let mut result: Vec<CallStep> = Vec::new();
    for (idx, raw) in steps.iter().enumerate() {
        let (name_part, args) = if let Some(paren_start) = raw.find('(') {
            let name = raw[..paren_start].trim().to_string();
            let arg_str = raw[paren_start + 1..raw.rfind(')').unwrap_or(raw.len() - 1)].trim();
            let args = if arg_str.is_empty() {
                vec![]
            } else {
                split_call_args(arg_str)
            };
            (name, args)
        } else {
            (raw.clone(), vec![])
        };
        // Strip qualification like `module::func` / `$this->m` -> `func` / `m`
        let simple = simple_callee_name(&name_part).to_string();
        result.push(CallStep {
            receiver: if idx == 0 {
                None
            } else {
                Some(steps[idx - 1].clone())
            },
            method_name: simple,
            args,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_call_chain_empty() {
        let result = parse_call_chain("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_call_chain_single_name() {
        let result = parse_call_chain("foo");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].method_name, "foo");
        assert!(result[0].receiver.is_none());
    }

    #[test]
    fn test_parse_call_chain_simple_call() {
        let result = parse_call_chain("foo()");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].method_name, "foo");
        assert!(result[0].args.is_empty());
    }

    #[test]
    fn test_parse_call_chain_call_with_args() {
        let result = parse_call_chain("foo(x, y)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].method_name, "foo");
        assert_eq!(result[0].args, vec!["x", "y"]);
    }

    #[test]
    fn test_parse_call_chain_chain_calls() {
        let result = parse_call_chain("foo().bar()");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].method_name, "foo");
        assert!(result[0].receiver.is_none());
        assert_eq!(result[1].method_name, "bar");
        assert_eq!(result[1].receiver, Some("foo()".to_string()));
    }

    #[test]
    fn test_parse_call_chain_module_qualified() {
        let result = parse_call_chain("module.func");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].method_name, "module");
        assert_eq!(result[1].method_name, "func");
    }

    #[test]
    fn test_parse_call_chain_nested_parens() {
        let result = parse_call_chain("foo(a)");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].args, vec!["a"]);
    }

    #[test]
    fn test_parse_call_chain_strip_qualification() {
        let result = parse_call_chain("module::func");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].method_name, "func");
    }

    #[test]
    fn test_parse_call_chain_php_arrow() {
        let result = parse_call_chain("$this->combineInts(1, 2)");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].method_name, "this");
        assert_eq!(result[1].method_name, "combineInts");
        assert_eq!(result[1].args, vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn test_parse_call_chain_safe_call() {
        let result = parse_call_chain("user?.getName()");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].method_name, "user");
        assert_eq!(result[1].method_name, "getName");
    }

    #[test]
    fn test_parse_call_chain_cpp_arrow() {
        let result = parse_call_chain("ptr->method(arg)");
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].method_name, "method");
    }
}
