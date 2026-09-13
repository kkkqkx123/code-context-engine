//! Shared call-site parsing utilities.
//!
//! Functions here parse comma-separated argument lists and stored call
//! targets. They are consumed by the generics, overload, and cross-file
//! modules.

/// Split a comma-separated argument list while respecting nesting.
///
/// Tracks `()`, `[]`, `{}` and `<>` depth plus string quotes so call-site
/// argument expressions such as `f(a, g(1, 2), [x, y])` split into exactly
/// three items. Unbalanced input yields a single item (the whole string).
pub fn split_call_args(args_text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth_paren = 0usize;
    let mut depth_bracket = 0usize;
    let mut depth_brace = 0usize;
    let mut depth_angle = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut current = String::new();
    for ch in args_text.chars() {
        if let Some(q) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' => {
                depth_paren += 1;
                current.push(ch);
            }
            ')' => {
                depth_paren = depth_paren.saturating_sub(1);
                current.push(ch);
            }
            '[' => {
                depth_bracket += 1;
                current.push(ch);
            }
            ']' => {
                depth_bracket = depth_bracket.saturating_sub(1);
                current.push(ch);
            }
            '{' => {
                depth_brace += 1;
                current.push(ch);
            }
            '}' => {
                depth_brace = depth_brace.saturating_sub(1);
                current.push(ch);
            }
            '<' => {
                depth_angle += 1;
                current.push(ch);
            }
            '>' => {
                depth_angle = depth_angle.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth_paren == 0
                && depth_bracket == 0
                && depth_brace == 0
                && depth_angle == 0 =>
            {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() || !args.is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

/// Reduce a callee path to its simple name.
///
/// Strips receiver qualification across languages: `.` (most languages),
/// `::` (Rust/C++), `/` (qualified paths), `->` (PHP `$this->m`, C++
/// `ptr->m`), `?.`/`?->` (Kotlin safe-call, PHP nullsafe) and a leading
/// `$` (PHP variables). `foo`, `module.func`, `A::new`, `$this->combine`
/// and `user?.getName` all reduce to their trailing identifier so name
/// lookups resolve uniformly.
pub fn simple_callee_name(name: &str) -> &str {
    let mut rest = name.trim();
    // PHP variable prefix: `$this->m` / `$svc->m`.
    rest = rest.strip_prefix('$').unwrap_or(rest);
    // Receiver separators, longest first so `->` / `?.` / `?->` are not
    // split as bare `.` / `?` fragments.
    for sep in ["?->", "?.", "->", "::", ".", ":", "/"] {
        if let Some(pos) = rest.rfind(sep) {
            rest = rest[pos + sep.len()..].trim();
            rest = rest.strip_prefix('$').unwrap_or(rest);
        }
    }
    rest.trim()
}

/// Split a callee path into its receiver and trailing method.
///
/// Finds the last receiver separator (`?->`, `?.`, `->`, `::`, `.`, `:`,
/// `/`) and returns both sides. Returns `None` for bare names so callers
/// can distinguish `foo()` from `obj.foo()`.
pub fn split_receiver_method(name: &str) -> Option<(&str, &str)> {
    let trimmed = name.trim();
    // Rightmost separator wins; ties on end position prefer the longer
    // separator so `?.` beats a bare `.` in `user?.getName`.
    let mut best: Option<(usize, usize)> = None;
    for sep in ["?->", "?.", "->", "::", ".", ":", "/"] {
        if let Some(pos) = trimmed.rfind(sep) {
            let end = pos + sep.len();
            let better = match best {
                None => true,
                Some((best_pos, best_len)) => (end, sep.len()) > (best_pos + best_len, best_len),
            };
            if better {
                best = Some((pos, sep.len()));
            }
        }
    }
    let (pos, len) = best?;
    let receiver = trimmed[..pos].trim();
    let method = trimmed[pos + len..].trim();
    if receiver.is_empty() || method.is_empty() {
        return None;
    }
    Some((receiver, method))
}

/// Split a stored call target into its callee name and argument expressions.
///
/// Stored targets look like `foo`, `module.func(a, b)` or `obj.m(x)`.
/// Returns the full callee path (qualification is stripped by callers via
/// [`simple_callee_name`]) and the raw argument texts (possibly empty).
/// Malformed input yields the whole string as the name with no arguments.
pub fn split_call_target(target: &str) -> (String, Vec<String>) {
    let trimmed = target.trim();
    let Some(paren_pos) = trimmed.find('(') else {
        return (trimmed.to_string(), Vec::new());
    };
    let name = trimmed[..paren_pos].trim().to_string();
    let rest = &trimmed[paren_pos + 1..];
    let Some(close_pos) = rest.rfind(')') else {
        return (trimmed.to_string(), Vec::new());
    };
    let args = split_call_args(rest[..close_pos].trim());
    (name, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_call_args_simple() {
        assert_eq!(split_call_args(""), Vec::<String>::new());
        assert_eq!(split_call_args("42"), vec!["42".to_string()]);
        assert_eq!(
            split_call_args("42, \"answer\", x"),
            vec!["42".to_string(), "\"answer\"".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn test_split_call_args_nested() {
        assert_eq!(
            split_call_args("a, g(1, 2), [x, y]"),
            vec!["a".to_string(), "g(1, 2)".to_string(), "[x, y]".to_string()]
        );
        assert_eq!(
            split_call_args("f(\"a,b\"), {k: 1}"),
            vec!["f(\"a,b\")".to_string(), "{k: 1}".to_string()]
        );
    }

    #[test]
    fn test_split_receiver_method() {
        assert_eq!(
            super::split_receiver_method("obj.method"),
            Some(("obj", "method"))
        );
        assert_eq!(
            super::split_receiver_method("$this->combineInts"),
            Some(("$this", "combineInts"))
        );
        assert_eq!(
            super::split_receiver_method("user?.getName"),
            Some(("user", "getName"))
        );
        assert_eq!(super::split_receiver_method("foo"), None);
        assert_eq!(super::split_receiver_method(""), None);
    }

    #[test]
    fn test_simple_callee_name() {
        assert_eq!(super::simple_callee_name("makePair"), "makePair");
        assert_eq!(super::simple_callee_name("module.func"), "func");
        assert_eq!(super::simple_callee_name("module::func"), "func");
        assert_eq!(
            super::simple_callee_name("$this->combineInts"),
            "combineInts"
        );
        assert_eq!(super::simple_callee_name("$svc->loadUser"), "loadUser");
        assert_eq!(super::simple_callee_name("ptr->method"), "method");
        assert_eq!(super::simple_callee_name("user?.getName"), "getName");
        assert_eq!(super::simple_callee_name("a/b/c"), "c");
        assert_eq!(
            super::simple_callee_name("  $this->combineInts  "),
            "combineInts"
        );
    }

    #[test]
    fn test_split_call_target() {
        let (name, args) = split_call_target("makePair");
        assert_eq!(name, "makePair");
        assert!(args.is_empty());
        let (name, args) = split_call_target("makePair(42, \"answer\")");
        assert_eq!(name, "makePair");
        assert_eq!(args, vec!["42".to_string(), "\"answer\"".to_string()]);
        let (name, args) = split_call_target("obj.method(x)");
        assert_eq!(name, "obj.method");
        assert_eq!(args, vec!["x".to_string()]);
    }
}
