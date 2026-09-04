use super::Comment;

/// Comment channel decided by marker shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentClass {
    /// `///` — outer doc line comment.
    OuterDoc,
    /// `//!` / `#!` / `/*!` — inner doc line or block comment.
    InnerDoc,
    /// `"""` / `'''` — Python docstring.
    Docstring,
    /// `/* */` / `/** */` / `/*!` / `<!-- -->` — block comment.
    DocBlock,
    /// `//` (no `/` marker) / `#` (no `!` marker) — plain line comment.
    Plain,
}

/// Doc line comment marker family, used for merging consecutive lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocLineMarker {
    /// `///`
    Outer,
    /// `//!`
    InnerSlash,
    /// `#!`
    InnerHash,
}

/// Classify a comment into its channel by marker shape, falling back to the
/// capture name when the text carries no marker.
pub(crate) fn classify_comment(comment: &Comment) -> CommentClass {
    let trimmed = comment.text.trim_start();
    if trimmed.starts_with("///") {
        CommentClass::OuterDoc
    } else if is_yard_doc_line(trimmed) {
        // `# @return [T]` style API doc lines (Ruby YARD and similar `@tag`
        // conventions) read as documentation, not plain remarks.
        CommentClass::OuterDoc
    } else if trimmed.starts_with("//!") || trimmed.starts_with("#!") || trimmed.starts_with("/*!")
    {
        CommentClass::InnerDoc
    } else if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
        CommentClass::Docstring
    } else if trimmed.starts_with("/*") || trimmed.starts_with("<!--") {
        CommentClass::DocBlock
    } else if trimmed.starts_with("//") || trimmed.starts_with('#') {
        CommentClass::Plain
    } else if comment.capture_name.contains("docstring") {
        CommentClass::Docstring
    } else if comment.capture_name.contains("doc") {
        CommentClass::OuterDoc
    } else if comment.capture_name.contains("block") {
        CommentClass::DocBlock
    } else {
        CommentClass::Plain
    }
}

/// Whether a `#` line comment carries an API doc tag (`@return`, `@param`,
/// `@type`, ...). Plain `#` remarks (including `# type:` comments and
/// shebangs, handled by earlier arms) stay on the plain channel.
fn is_yard_doc_line(trimmed: &str) -> bool {
    let Some(body) = trimmed.strip_prefix('#') else {
        return false;
    };
    let body = body.trim_start();
    if !body.starts_with('@') {
        return false;
    }
    let tag: String = body[1..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    matches!(
        tag.as_str(),
        "return" | "param" | "type" | "option" | "raise"
    )
}

/// Detect the doc line marker family of a comment, if any.
pub(crate) fn doc_line_marker(comment: &Comment) -> Option<DocLineMarker> {
    let trimmed = comment.text.trim_start();
    if trimmed.starts_with("///") {
        Some(DocLineMarker::Outer)
    } else if trimmed.starts_with("//!") {
        Some(DocLineMarker::InnerSlash)
    } else if trimmed.starts_with("#!") {
        Some(DocLineMarker::InnerHash)
    } else {
        None
    }
}

/// Merge consecutive doc-marked line comments of the same family into a single
/// logical block. Plain line comments pass through unmerged; they are merged
/// later in `merge_plain_comment_blocks`.
pub(crate) fn merge_top_level_line_comments(comments: Vec<Comment>) -> Vec<Comment> {
    let mut merged: Vec<Comment> = Vec::with_capacity(comments.len());

    for comment in comments {
        let marker = doc_line_marker(&comment);

        if let Some(marker) = marker {
            if let Some(last) = merged.last_mut() {
                let last_marker = doc_line_marker(last);

                let consecutive = comment.span.start_position.row == last.span.end_position.row
                    || comment.span.start_position.row == last.span.end_position.row + 1;

                if last_marker == Some(marker) && consecutive {
                    let is_whitespace_only = comment.text.trim().is_empty();

                    if is_whitespace_only {
                        if !last.text.ends_with("\n\n") {
                            last.text.push('\n');
                        }
                    } else {
                        if !last.text.ends_with('\n') {
                            last.text.push('\n');
                        }
                        last.text.push_str(&comment.text);
                    }
                    last.span.end_byte = comment.span.end_byte;
                    last.span.end_position = comment.span.end_position;
                    continue;
                }
            }
        }

        merged.push(comment);
    }

    merged
}

/// Deduplicate captures that point to the same source row.
///
/// Rust/Java tree-sitter queries may return both the raw line-comment node and
/// a nested doc-comment/content node for the same source line. We keep the
/// more informative capture so downstream normalization sees a single line.
pub(crate) fn dedup_same_row_comments(comments: Vec<Comment>) -> Vec<Comment> {
    let mut deduped: Vec<Comment> = Vec::with_capacity(comments.len());

    for comment in comments {
        if let Some(last) = deduped.last_mut() {
            if last.span.start_position.row == comment.span.start_position.row {
                if should_replace_comment(last, &comment) {
                    *last = comment;
                }
                continue;
            }
        }

        deduped.push(comment);
    }

    deduped
}

fn should_replace_comment(existing: &Comment, candidate: &Comment) -> bool {
    let existing_score = comment_score(existing);
    let candidate_score = comment_score(candidate);
    candidate_score > existing_score
}

fn comment_score(comment: &Comment) -> (bool, usize, usize) {
    let text = comment.text.trim_start();
    let has_marker = text.starts_with("//")
        || text.starts_with("/*")
        || text.starts_with("/**")
        || text.starts_with("<!--")
        || text.starts_with('#');

    // Prefer raw marker-bearing captures, then longer captures, then narrower
    // indentation (more likely to be the actual comment node rather than a
    // nested content node).
    (
        has_marker,
        comment.text.len(),
        usize::MAX.saturating_sub(comment.span.start_position.column),
    )
}
