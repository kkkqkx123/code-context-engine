use std::path::Path;

use cce_utils::text::floor_char_boundary;
use cce_utils::token_estimation::TokenEstimator;

const _DEFAULT_MAX_TOKENS: usize = 512;

#[derive(Debug, Clone)]
pub struct CleanedChunk {
    pub chunk_id: String,
    pub file_path: String,
    pub original_text: String,
    pub cleaned_text: String,
    pub start_line: usize,
    pub end_line: usize,
    pub entity_name: String,
}

pub struct SymbolCleaner {
    max_tokens: usize,
    estimator: TokenEstimator,
}

impl SymbolCleaner {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            estimator: TokenEstimator::default(),
        }
    }

    /// Remove line comments (//) and block comments (/* ... */) from Rust code.
    /// Keeps doc comments (///, //!) as they contain useful symbol info.
    /// Preserves all code structure, identifiers, and string literals.
    pub fn clean_rust_code(&self, content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let chars: Vec<char> = content.chars().collect();
        let len = chars.len();
        let mut i = 0;

        let mut in_block_comment = false;

        while i < len {
            if in_block_comment {
                if i + 1 < len && chars[i] == '*' && chars[i + 1] == '/' {
                    in_block_comment = false;
                    i += 2;
                    // add space to preserve token boundary
                    result.push(' ');
                    continue;
                }
                i += 1;
                continue;
            }

            // String literal - copy as-is
            if chars[i] == '"' {
                result.push('"');
                i += 1;
                while i < len && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < len {
                        result.push(chars[i]);
                        i += 1;
                        result.push(chars[i]);
                        i += 1;
                    } else {
                        result.push(chars[i]);
                        i += 1;
                    }
                }
                if i < len {
                    result.push('"');
                    i += 1;
                }
                continue;
            }

            // Char literal - copy as-is
            if chars[i] == '\'' && i + 2 < len {
                result.push('\'');
                i += 1;
                if chars[i] == '\\' && i + 1 < len {
                    result.push(chars[i]);
                    i += 1;
                    result.push(chars[i]);
                    i += 1;
                } else {
                    result.push(chars[i]);
                    i += 1;
                }
                if i < len && chars[i] == '\'' {
                    result.push('\'');
                    i += 1;
                }
                continue;
            }

            // Line comment (//, ///, //!) - handle differently
            if chars[i] == '/' && i + 1 < len && chars[i + 1] == '/' {
                let is_doc = i + 2 < len && (chars[i + 2] == '/' || chars[i + 2] == '!');
                if is_doc {
                    // Keep doc comments - they contain useful semantic info
                    // But strip the leading ///
                    result.push(' ');
                    i += 3;
                    while i < len && chars[i] != '\n' {
                        result.push(chars[i]);
                        i += 1;
                    }
                    // Keep the newline
                    if i < len {
                        result.push('\n');
                        i += 1;
                    }
                    continue;
                }
                // Regular comment - skip to end of line
                i += 2;
                while i < len && chars[i] != '\n' {
                    i += 1;
                }
                if i < len {
                    // replace with space to preserve line structure
                    result.push('\n');
                    i += 1;
                }
                continue;
            }

            // Block comment - check if doc comment (/**)
            if chars[i] == '/' && i + 1 < len && chars[i + 1] == '*' {
                let is_doc = i + 2 < len && chars[i + 2] == '*';
                if is_doc {
                    // Keep doc comments as cleaned text
                    result.push(' ');
                    i += 3;
                    while i + 1 < len {
                        if chars[i] == '*' && chars[i + 1] == '/' {
                            i += 2;
                            break;
                        }
                        result.push(chars[i]);
                        i += 1;
                    }
                    result.push(' ');
                    continue;
                }
                // Skip block comment
                i += 2;
                in_block_comment = true;
                continue;
            }

            result.push(chars[i]);
            i += 1;
        }

        // Collapse multiple blank lines into one
        let mut cleaned = String::with_capacity(result.len());
        let mut prev_blank = false;
        for line in result.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !prev_blank {
                    cleaned.push('\n');
                    prev_blank = true;
                }
            } else {
                cleaned.push_str(trimmed);
                cleaned.push('\n');
                prev_blank = false;
            }
        }

        cleaned
    }

    pub fn clean_and_chunk_file(&self, file_path: &str, content: &str) -> Vec<CleanedChunk> {
        let cleaned = self.clean_rust_code(content);
        let file_name = Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        let mut chunks = Vec::new();
        let mut remaining = &cleaned[..];
        let mut idx = 0usize;
        let mut global_offset = 0usize;

        while !remaining.is_empty() {
            let tokens = self.estimator.estimate_text(remaining);
            if tokens <= self.max_tokens {
                let start_line = global_offset + 1;
                let lines = remaining.lines().count();
                chunks.push(CleanedChunk {
                    chunk_id: format!("sc_{}_{}", file_name, idx),
                    file_path: file_path.to_string(),
                    original_text: String::new(),
                    cleaned_text: remaining.to_string(),
                    start_line,
                    end_line: start_line + lines - 1,
                    entity_name: format!("{}:chunk{}", file_name, idx),
                });
                break;
            }

            let avg = remaining.len().div_ceil(tokens);
            let cut_len = self.max_tokens * avg;
            let cut_len = cut_len.min(remaining.len());
            let safe_cut = floor_char_boundary(remaining, cut_len);
            let chunk_text = &remaining[..safe_cut];

            let consumed = chunk_text.lines().count();
            let start_line = global_offset + 1;

            chunks.push(CleanedChunk {
                chunk_id: format!("sc_{}_{}", file_name, idx),
                file_path: file_path.to_string(),
                original_text: String::new(),
                cleaned_text: chunk_text.to_string(),
                start_line,
                end_line: global_offset + consumed,
                entity_name: format!("{}:chunk{}", file_name, idx),
            });

            remaining = &remaining[safe_cut..];
            global_offset += consumed;
            idx += 1;
        }

        chunks
    }

    pub fn clean_and_chunk_files(&self, files: &[(String, String)]) -> Vec<CleanedChunk> {
        let mut all = Vec::new();
        for (file_path, content) in files {
            let chunks = self.clean_and_chunk_file(file_path, content);
            all.extend(chunks);
        }
        all
    }
}
