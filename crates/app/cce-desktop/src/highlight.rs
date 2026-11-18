//! Small, deterministic lexer for code previews.
//!
//! The search panel only shows a dozen lines of a hit, and the web console
//! uses a similarly lightweight tokenizer. Pulling in a full syntax engine
//! would make the desktop binary much larger for no extra information at that
//! size, so this module colours the four categories a reader actually scans
//! for: comments, strings, numbers and keywords.

/// Category of one preview token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
}

/// One coloured run of source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub kind: Kind,
}

/// Keywords common to the languages this project indexes.
const KEYWORDS: [&str; 28] = [
    "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "def",
    "default", "else", "enum", "export", "false", "fn", "for", "from", "func", "function", "if",
    "impl", "import", "in", "let", "match", "mut", "new",
];
const CONTROL_KEYWORDS: [&str; 12] = [
    "null", "None", "pub", "return", "self", "static", "struct", "switch", "this", "trait", "true",
    "while",
];

fn is_keyword(word: &str) -> bool {
    KEYWORDS.contains(&word) || CONTROL_KEYWORDS.contains(&word)
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Split `code` into coloured runs without losing a single byte.
///
/// Line comments end at a newline, block comments are not carried across
/// runs, and an unterminated string simply runs to the end of the slice: a
/// preview is truncated by design, so there may be no closing quote.
pub fn tokenize(code: &str) -> Vec<Token> {
    let bytes = code.as_bytes();
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut cursor = 0;

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'/' && bytes.get(cursor + 1) == Some(&b'/') {
            push_plain(&mut tokens, bytes, start, cursor);
            let end = bytes[cursor..]
                .iter()
                .position(|value| *value == b'\n')
                .map(|offset| cursor + offset)
                .unwrap_or(bytes.len());
            push(&mut tokens, &bytes[cursor..end], Kind::Comment);
            cursor = end;
            start = end;
            continue;
        }
        if byte == b'#' {
            push_plain(&mut tokens, bytes, start, cursor);
            let end = bytes[cursor..]
                .iter()
                .position(|value| *value == b'\n')
                .map(|offset| cursor + offset)
                .unwrap_or(bytes.len());
            push(&mut tokens, &bytes[cursor..end], Kind::Comment);
            cursor = end;
            start = end;
            continue;
        }
        if matches!(byte, b'"' | b'\'' | b'`') {
            push_plain(&mut tokens, bytes, start, cursor);
            let mut end = cursor + 1;
            while end < bytes.len() {
                if bytes[end] == b'\\' {
                    end += 2;
                    continue;
                }
                if bytes[end] == byte {
                    end += 1;
                    break;
                }
                end += 1;
            }
            push(
                &mut tokens,
                &bytes[cursor..end.min(bytes.len())],
                Kind::String,
            );
            cursor = end.min(bytes.len());
            start = cursor;
            continue;
        }
        if byte.is_ascii_digit() {
            push_plain(&mut tokens, bytes, start, cursor);
            let mut end = cursor + 1;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'.') {
                end += 1;
            }
            push(&mut tokens, &bytes[cursor..end], Kind::Number);
            cursor = end;
            start = end;
            continue;
        }
        if is_word(byte) {
            let mut end = cursor + 1;
            while end < bytes.len() && is_word(bytes[end]) {
                end += 1;
            }
            let word = std::str::from_utf8(&bytes[cursor..end]).unwrap_or("");
            if is_keyword(word) {
                push_plain(&mut tokens, bytes, start, cursor);
                push(&mut tokens, &bytes[cursor..end], Kind::Keyword);
                start = end;
            }
            cursor = end;
            continue;
        }
        cursor += 1;
    }
    push_plain(&mut tokens, bytes, start, bytes.len());
    tokens
}

fn push_plain(tokens: &mut Vec<Token>, bytes: &[u8], start: usize, end: usize) {
    if end > start {
        push(tokens, &bytes[start..end], Kind::Plain);
    }
}

fn push(tokens: &mut Vec<Token>, bytes: &[u8], kind: Kind) {
    if bytes.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(bytes).into_owned();
    if let Some(last) = tokens.last_mut()
        && last.kind == kind
    {
        last.text.push_str(&text);
    } else {
        tokens.push(Token { text, kind });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_cover_the_whole_input() {
        for code in [
            "",
            "let x = 1; // note\n",
            "fn run() -> String { \"hi\".to_string() }",
            "# comment\nprint('x')\n",
        ] {
            let tokens = tokenize(code);
            let joined: String = tokens.iter().map(|token| token.text.as_str()).collect();
            assert_eq!(joined, code);
        }
    }

    #[test]
    fn comments_strings_numbers_and_keywords_are_classified() {
        let tokens = tokenize("let n = 42; // note\n");
        assert_eq!(
            tokens[0],
            Token {
                text: "let".into(),
                kind: Kind::Keyword
            }
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.text == "42" && token.kind == Kind::Number)
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.text == "// note" && token.kind == Kind::Comment)
        );
    }

    #[test]
    fn an_unterminated_string_still_colours_the_rest_of_the_line() {
        let tokens = tokenize("let s = \"open");
        assert_eq!(tokens.last().map(|token| token.kind), Some(Kind::String));
    }
}
