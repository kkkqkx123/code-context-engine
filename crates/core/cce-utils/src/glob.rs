//! A lightweight glob matching implementation inspired by ripgrep's globset.
//!
//! This module provides efficient glob pattern matching for file paths,
//! supporting standard wildcards (`*`, `?`) and recursive globs (`**`).

use std::path::Path;

/// A compiled glob pattern.
#[derive(Debug, Clone)]
pub struct Glob {
    tokens: Vec<Token>,
    is_recursive: bool,
    is_anchored: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Literal(String),
    Wildcard,   // *
    Recursive,  // **
    SingleChar, // ?
}

impl Glob {
    /// Parse a glob pattern string.
    pub fn new(pattern: &str) -> Result<Self, String> {
        let is_anchored = pattern.starts_with('/');
        let clean_pattern = if is_anchored { &pattern[1..] } else { pattern };

        let tokens = tokenize(clean_pattern)?;
        let is_recursive = tokens.iter().any(|t| matches!(t, Token::Recursive));

        Ok(Self {
            tokens,
            is_recursive,
            is_anchored,
        })
    }

    /// Check if a path matches this glob pattern.
    pub fn is_match(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().replace('\\', "/");

        // If the pattern is anchored (started with /), it must match from the beginning of the path
        if self.is_anchored {
            return match_tokens(&self.tokens, &path_str, 0, 0);
        }

        // For unanchored patterns:
        // 1. Try matching the full path
        if match_tokens(&self.tokens, &path_str, 0, 0) {
            return true;
        }

        // 2. If the pattern doesn't contain a slash, match against the filename only
        // This is standard gitignore behavior: "*.rs" matches "src/test.rs"
        if !self.is_recursive && !path_str.contains('/') {
            return match_tokens(&self.tokens, &path_str, 0, 0);
        }

        // 3. Check if pattern looks like it's meant for a filename (e.g. "*.rs")
        // We check if any component of the path matches
        if !self
            .tokens
            .iter()
            .any(|t| matches!(t, Token::Literal(s) if s.contains('/')))
        {
            if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
                if match_tokens(&self.tokens, file_name, 0, 0) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if this glob contains a recursive wildcard (**).
    pub fn is_recursive(&self) -> bool {
        self.is_recursive
    }
}

fn tokenize(pattern: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = pattern.chars().peekable();
    let mut current_literal = String::new();

    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if !current_literal.is_empty() {
                    tokens.push(Token::Literal(current_literal.clone()));
                    current_literal.clear();
                }
                if chars.peek() == Some(&'*') {
                    chars.next(); // consume second *
                    // Handle /**/ or leading/trailing **
                    if chars.peek() == Some(&'/') || chars.peek().is_none() {
                        if chars.peek() == Some(&'/') {
                            chars.next(); // consume /
                        }
                        tokens.push(Token::Recursive);
                    } else {
                        return Err("Invalid use of **".to_string());
                    }
                } else {
                    tokens.push(Token::Wildcard);
                }
            }
            '?' => {
                if !current_literal.is_empty() {
                    tokens.push(Token::Literal(current_literal.clone()));
                    current_literal.clear();
                }
                tokens.push(Token::SingleChar);
            }
            _ => {
                current_literal.push(c);
            }
        }
    }

    if !current_literal.is_empty() {
        tokens.push(Token::Literal(current_literal));
    }

    Ok(tokens)
}

fn match_tokens(tokens: &[Token], path: &str, ti: usize, pi: usize) -> bool {
    if ti == tokens.len() {
        return pi == path.len();
    }

    match &tokens[ti] {
        Token::Literal(lit) => {
            if path[pi..].starts_with(lit.as_str()) {
                match_tokens(tokens, path, ti + 1, pi + lit.len())
            } else {
                false
            }
        }
        Token::Wildcard => {
            // * matches anything except /
            for i in pi..path.len() {
                if path.as_bytes()[i] == b'/' {
                    break;
                }
                if match_tokens(tokens, path, ti + 1, i + 1) {
                    return true;
                }
            }
            // Also match empty string
            match_tokens(tokens, path, ti + 1, pi)
        }
        Token::Recursive => {
            // ** matches everything including /
            // Try matching the rest of the tokens at every position
            for i in pi..=path.len() {
                if match_tokens(tokens, path, ti + 1, i) {
                    return true;
                }
            }
            false
        }
        Token::SingleChar => {
            if pi < path.len() && path.as_bytes()[pi] != b'/' {
                match_tokens(tokens, path, ti + 1, pi + 1)
            } else {
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_glob() {
        let g = Glob::new("*.rs").unwrap();
        assert!(g.is_match(Path::new("test.rs")));
        assert!(!g.is_match(Path::new("test.txt")));
    }

    #[test]
    fn test_recursive_glob() {
        let g = Glob::new("**/*.rs").unwrap();
        assert!(g.is_match(Path::new("test.rs")));
        assert!(g.is_match(Path::new("src/test.rs")));
        assert!(g.is_match(Path::new("src/deep/nested/test.rs")));
    }

    #[test]
    fn test_literal_match() {
        let g = Glob::new("src/main.rs").unwrap();
        assert!(g.is_match(Path::new("src/main.rs")));
        assert!(!g.is_match(Path::new("src/lib.rs")));
    }
}
