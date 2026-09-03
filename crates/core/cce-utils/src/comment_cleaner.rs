use crate::text::normalize_whitespace;

pub fn strip_comment_markers(text: &str, preserve_newlines: bool) -> String {
    if text.is_empty() {
        return String::new();
    }

    let trimmed_start = text.trim_start();

    if trimmed_start.starts_with("\"\"\"") || trimmed_start.starts_with("'''") {
        return clean_python_docstring(text);
    }

    let cleaned = if trimmed_start.starts_with("///") || trimmed_start.starts_with("//!") {
        clean_comment_generic(
            text,
            |line| {
                let trimmed = line.trim_start();
                let result = if trimmed.starts_with("///") {
                    trimmed.strip_prefix("///").unwrap_or(trimmed)
                } else if trimmed.starts_with("//!") {
                    trimmed.strip_prefix("//!").unwrap_or(trimmed)
                } else {
                    trimmed
                };
                result.trim_start()
            },
            preserve_newlines,
        )
    } else if trimmed_start.starts_with("<!--") || trimmed_start.starts_with("-->") {
        clean_comment_generic(
            text,
            |line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with("<!--") {
                    let s = trimmed.strip_prefix("<!--").unwrap_or(trimmed);
                    s.strip_suffix("-->").unwrap_or(s).trim_end()
                } else if trimmed.ends_with("-->") {
                    trimmed.strip_suffix("-->").unwrap_or(trimmed).trim_end()
                } else {
                    trimmed
                }
            },
            preserve_newlines,
        )
    } else if trimmed_start.starts_with("/*")
        || trimmed_start.starts_with("*")
        || text.contains("/*")
    {
        clean_comment_generic(
            text,
            |line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with("/**") {
                    let s = trimmed.strip_prefix("/**").unwrap_or(trimmed);
                    s.strip_suffix("*/").unwrap_or(s).trim_end()
                } else if trimmed.starts_with("/*!") {
                    let s = trimmed.strip_prefix("/*!").unwrap_or(trimmed);
                    s.strip_suffix("*/").unwrap_or(s).trim_end()
                } else if trimmed.starts_with("/*") {
                    let s = trimmed.strip_prefix("/*").unwrap_or(trimmed);
                    s.strip_suffix("*/").unwrap_or(s).trim_end()
                } else if trimmed.starts_with("*/") {
                    trimmed.strip_prefix("*/").unwrap_or(trimmed)
                } else if trimmed.ends_with("*/") {
                    trimmed.strip_suffix("*/").unwrap_or(trimmed).trim_end()
                } else if trimmed.starts_with('*') {
                    trimmed.strip_prefix('*').unwrap_or(trimmed)
                } else {
                    trimmed
                }
            },
            preserve_newlines,
        )
    } else if trimmed_start.starts_with('#') {
        clean_comment_generic(
            text,
            |line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') {
                    trimmed.trim_start_matches('#').trim_start()
                } else {
                    trimmed
                }
            },
            preserve_newlines,
        )
    } else if trimmed_start.starts_with("//") {
        clean_comment_generic(
            text,
            |line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    trimmed.trim_start_matches('/').trim_start()
                } else {
                    trimmed
                }
            },
            preserve_newlines,
        )
    } else {
        clean_comment_generic(
            text,
            |line| {
                let trimmed = line.trim();
                if trimmed.starts_with('#') {
                    trimmed.trim_start_matches('#').trim_start()
                } else if trimmed.starts_with("//") {
                    trimmed.trim_start_matches('/').trim_start()
                } else {
                    trimmed
                }
            },
            preserve_newlines,
        )
    };

    if preserve_newlines {
        cleaned
    } else {
        normalize_whitespace(&cleaned)
    }
}

fn clean_comment_generic<F>(text: &str, clean_line: F, preserve_newlines: bool) -> String
where
    F: Fn(&str) -> &str,
{
    if preserve_newlines {
        clean_preserving_lines(text, clean_line)
    } else {
        text.lines()
            .map(clean_line)
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn clean_preserving_lines<F>(text: &str, mut clean_line: F) -> String
where
    F: FnMut(&str) -> &str,
{
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| clean_line(line).trim().to_string())
        .collect();

    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

fn clean_python_docstring(text: &str) -> String {
    let trimmed = text.trim();

    let without_quotes = if trimmed.starts_with("\"\"\"") {
        trimmed.strip_prefix("\"\"\"").unwrap_or(trimmed)
    } else if trimmed.starts_with("'''") {
        trimmed.strip_prefix("'''").unwrap_or(trimmed)
    } else {
        trimmed
    };

    let without_quotes = if without_quotes.ends_with("\"\"\"") {
        without_quotes
            .strip_suffix("\"\"\"")
            .unwrap_or(without_quotes)
    } else if without_quotes.ends_with("'''") {
        without_quotes.strip_suffix("'''").unwrap_or(without_quotes)
    } else {
        without_quotes
    };

    let lines: Vec<&str> = without_quotes.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line
            }
        })
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn clean_comment_markers(text: &str) -> String {
    strip_comment_markers(text, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_rust_doc_comment() {
        let doc = "/// Returns true if the device is ready, false if timed out.\n///\n/// # Arguments\n/// * `timeout` - The maximum time to wait";
        let cleaned = strip_comment_markers(doc, true);
        assert!(cleaned.contains("Returns true if the device is ready"));
        assert!(cleaned.contains("# Arguments"));
    }

    #[test]
    fn test_strip_rust_block_comment() {
        let doc = "/**\n * Calculates the total price.\n *\n * @param items The items to calculate\n * @return The total price\n */";
        let cleaned = strip_comment_markers(doc, true);
        assert!(cleaned.contains("Calculates the total price"));
    }

    #[test]
    fn test_strip_c_comment() {
        let doc =
            "// Initializes the connection pool.\n// Must be called before any other operations.";
        let cleaned = strip_comment_markers(doc, true);
        assert!(cleaned.contains("Initializes the connection pool"));
    }

    #[test]
    fn test_strip_javadoc() {
        let doc = "/**\n * Connects to the database.\n * @param url The database URL\n * @return Connection object\n */";
        let cleaned = strip_comment_markers(doc, true);
        assert!(cleaned.contains("Connects to the database"));
    }

    #[test]
    fn test_strip_empty() {
        assert_eq!(strip_comment_markers("", true), "");
        assert_eq!(strip_comment_markers("", false), "");
    }

    #[test]
    fn test_strip_single_line() {
        let doc = "/// Simple description.";
        let cleaned = strip_comment_markers(doc, false);
        assert_eq!(cleaned, "Simple description.");
    }

    #[test]
    fn test_strip_hash_comment_all_stripped() {
        let doc = "# This is a Python comment\n# Another line";
        let cleaned = strip_comment_markers(doc, true);
        assert!(cleaned.contains("This is a Python comment"));
        assert!(cleaned.contains("Another line"));
        assert!(!cleaned.contains('#'));
    }

    #[test]
    fn test_strip_hash_preserves_triple_slash_markdown_heading() {
        let doc = "/// Some doc\n/// # Arguments\n/// * arg1 - first arg";
        let cleaned = strip_comment_markers(doc, true);
        assert!(cleaned.contains("# Arguments"));
    }

    #[test]
    fn test_strip_python_comment() {
        let doc = "# This is a Python comment\n# Another line";
        let cleaned = strip_comment_markers(doc, true);
        assert!(cleaned.contains("This is a Python comment"));
        assert!(cleaned.contains("Another line"));
    }

    #[test]
    fn test_strip_rust_inner_block_doc_comment() {
        let doc = "/*! This is a module-level doc */";
        let cleaned = strip_comment_markers(doc, false);
        assert_eq!(cleaned, "This is a module-level doc");
        assert!(!cleaned.contains('!'));
    }

    #[test]
    fn test_strip_multi_line_inner_block() {
        let doc = "/*!\n * Module-level documentation\n * with multiple lines\n */";
        let cleaned = strip_comment_markers(doc, false);
        assert_eq!(cleaned, "Module-level documentation with multiple lines");
    }

    #[test]
    fn test_compact_mode_joins_lines() {
        let doc = "/// Line one\n/// Line two\n/// Line three";
        let cleaned = strip_comment_markers(doc, false);
        assert_eq!(cleaned, "Line one Line two Line three");
    }

    #[test]
    fn test_preserve_newlines_keeps_structure() {
        let doc = "/// Line one\n///\n/// Line three";
        let cleaned = strip_comment_markers(doc, true);
        assert!(cleaned.contains("Line one"));
        assert!(cleaned.contains("Line three"));
        assert!(cleaned.contains('\n'));
    }

    #[test]
    fn test_strip_block_comment_rust_doc_style() {
        let doc = "/** This is a single-line block doc */";
        let cleaned = strip_comment_markers(doc, false);
        assert_eq!(cleaned, "This is a single-line block doc");
    }

    #[test]
    fn test_strip_hash_preserving_trims_leading_whitespace() {
        let doc = "#   indented comment";
        let cleaned = strip_comment_markers(doc, true);
        assert_eq!(cleaned, "indented comment");
    }

    #[test]
    fn test_strip_python_preserving_trims_leading_whitespace() {
        let doc = "  #   trailing space comment   ";
        let cleaned = strip_comment_markers(doc, true);
        assert_eq!(cleaned, "trailing space comment");
    }

    #[test]
    fn test_clean_comment_markers_backward_compat() {
        let doc = "/// Returns true if ready.\n///\n/// # Arguments\n/// * `timeout` - max wait";
        let cleaned = clean_comment_markers(doc);
        assert!(cleaned.contains("Returns true if ready."));
        assert!(cleaned.contains("# Arguments"));
        assert!(cleaned.contains("max wait"));
    }
}
