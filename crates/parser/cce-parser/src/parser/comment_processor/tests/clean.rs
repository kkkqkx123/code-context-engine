use super::*;

#[test]
fn test_clean_doc_comment_rust() {
    let text = "/// This is a doc comment\n/// with multiple lines";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "This is a doc comment with multiple lines");
}

#[test]
fn test_clean_doc_comment_javadoc() {
    let text = "/**\n * This is a javadoc\n * comment\n */";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "This is a javadoc comment");
}

#[test]
fn test_clean_doc_comment_python_docstring() {
    // Single line docstring with double quotes
    let text = "\"\"\"This is a Python docstring\"\"\"";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "This is a Python docstring");

    // Multi-line docstring with double quotes
    let text = "\"\"\"This is a multi-line\ndocstring for Python\nwith multiple lines\"\"\"";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(
        cleaned,
        "This is a multi-line\ndocstring for Python\nwith multiple lines"
    );

    // Single quotes docstring
    let text = "'''Single quote docstring'''";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "Single quote docstring");
}

#[test]
fn test_clean_doc_comment_block() {
    // C-style block comment
    let text = "/* This is a block comment */";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "This is a block comment");

    // Multi-line block comment
    let text = "/*\n * Line 1\n * Line 2\n */";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "Line 1 Line 2");
}

#[test]
fn test_clean_rust_doc_comment_multiline_preserving() {
    // Test the actual case from OnceCell
    let text = "/// A cell which can be written to only once. It is not thread safe.\n\
                    ///\n\
                    /// Unlike [`std::cell::RefCell`], a `OnceCell` provides simple `&`\n\
                    /// references to the contents.\n\
                    ///\n\
                    /// # Example\n\
                    /// Some example code here";

    let cleaned = clean_doc_comment_impl(text, true);

    // Should preserve multiple lines with content
    assert!(
        cleaned.contains("A cell which"),
        "First line should be present"
    );
    assert!(
        cleaned.contains("Unlike"),
        "Second paragraph should be present"
    );
    assert!(
        cleaned.contains("Example"),
        "Section header should be present"
    );
}

#[test]
fn test_clean_doc_comment_generic_comment() {
    // Generic comment (from common query)
    let text = "/* Generic comment without specific markers */";
    let cleaned = clean_doc_comment_impl(text, false);
    assert_eq!(cleaned, "Generic comment without specific markers");
}
