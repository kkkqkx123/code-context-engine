use cce_types::ConversionResult;

use super::boundary::cost;
use super::config::ChunkingConfig;
use super::result::ChunkPath;

pub struct HeaderHelper<'a> {
    config: &'a ChunkingConfig,
}

impl<'a> HeaderHelper<'a> {
    pub fn new(config: &'a ChunkingConfig) -> Self {
        Self { config }
    }

    pub fn path_limit(&self, path: ChunkPath) -> usize {
        match path {
            ChunkPath::Bm25 => self.config.max_bm25_words,
            ChunkPath::Embedding => self.config.max_tokens,
        }
    }

    pub fn unit_count(&self, text: &str, path: ChunkPath) -> usize {
        cost(text, path)
    }

    /// Limit a repeated header to one third of the hard chunk limit.
    pub fn compact_header(&self, full: &str, brief: &str, path: ChunkPath) -> String {
        let limit = self.path_limit(path);
        if limit == 0 {
            return full.to_string();
        }
        let header_limit = (limit / 3).max(1);
        let preferred = if self.unit_count(full, path) <= header_limit {
            full
        } else if !brief.is_empty() {
            brief
        } else {
            full
        };
        if self.unit_count(preferred, path) <= header_limit {
            return preferred.to_string();
        }

        // Prefer cutting at the nearest sentence end within budget so the
        // summary stays semantically complete; fall back to word-level cuts
        // only when no sentence boundary fits inside the budget.
        if let Some(truncated) = self.truncate_at_sentence_end(preferred, header_limit, path) {
            return truncated;
        }

        let mut result = String::new();
        for word in preferred.split_whitespace() {
            let mut candidate = result.clone();
            if !candidate.is_empty() {
                candidate.push(' ');
            }
            candidate.push_str(word);
            if self.unit_count(&candidate, path) > header_limit {
                break;
            }
            result = candidate;
        }
        if result.is_empty() {
            preferred.chars().take(header_limit).collect()
        } else {
            result
        }
    }

    /// Truncate at the last sentence end (`.`, `?`, `!` followed by whitespace
    /// or end of string, plus newlines) whose prefix still fits the budget.
    /// Returns `None` when no in-budget sentence boundary exists.
    fn truncate_at_sentence_end(
        &self,
        text: &str,
        limit: usize,
        path: ChunkPath,
    ) -> Option<String> {
        let mut best_end = None;
        for (i, ch) in text.char_indices() {
            let end = i + ch.len_utf8();
            let is_sentence_end = match ch {
                '\n' => true,
                '.' | '?' | '!' => text[end..].chars().next().is_none_or(|c| c.is_whitespace()),
                _ => false,
            };
            if !is_sentence_end {
                continue;
            }
            if self.unit_count(&text[..end], path) > limit {
                break;
            }
            best_end = Some(end);
        }
        best_end.map(|end| text[..end].trim_end().to_string())
    }

    pub fn header_budget(&self, header: &str, path: ChunkPath) -> usize {
        let limit = self.path_limit(path);
        if limit == 0 {
            usize::MAX
        } else {
            limit.saturating_sub(self.unit_count(header, path))
        }
    }

    /// Group members into header-budget-sized groups.
    ///
    /// On the Embedding path, members flagged `self_contained` (they carry
    /// their own docstring/behavior description) are always placed in a
    /// single-member group, so their topic is never diluted by adjacent
    /// members. The BM25 path is unaffected.
    pub fn group_members_by_header_budget(
        &self,
        members: &[ConversionResult],
        first_budget: usize,
        continuation_budget: usize,
        path: ChunkPath,
        member_self_contained: &[bool],
    ) -> Vec<Vec<ConversionResult>> {
        if first_budget == usize::MAX {
            return vec![members.to_vec()];
        }

        let min_cost = match path {
            ChunkPath::Bm25 => self.config.min_chunk_bm25_words,
            ChunkPath::Embedding => self.config.min_chunk_tokens,
        };

        let max_limit = match path {
            ChunkPath::Bm25 => self.config.max_bm25_words,
            ChunkPath::Embedding => self.config.max_tokens,
        };

        let mut groups = Vec::new();
        let mut current_group = Vec::new();
        let mut current_cost = 0;
        for (idx, member) in members.iter().enumerate() {
            let member_text = match path {
                ChunkPath::Bm25 => member.bm25_text.as_deref().unwrap_or(""),
                ChunkPath::Embedding => member.embedding_text.as_deref().unwrap_or(""),
            };
            let member_cost = self.unit_count(member_text, path);

            if path == ChunkPath::Embedding
                && member_self_contained.get(idx).copied().unwrap_or(false)
            {
                if !current_group.is_empty() {
                    groups.push(std::mem::take(&mut current_group));
                    current_cost = 0;
                }
                groups.push(vec![member.clone()]);
                continue;
            }

            let budget = if groups.is_empty() {
                first_budget
            } else {
                continuation_budget
            };
            let would_exceed_max = current_cost + member_cost > max_limit;
            let would_exceed_budget = current_cost + member_cost > budget;
            let below_min = current_cost < min_cost;
            if !current_group.is_empty()
                && (would_exceed_max || (!below_min && would_exceed_budget))
            {
                groups.push(std::mem::take(&mut current_group));
                current_cost = 0;
            }
            current_cost += member_cost;
            current_group.push(member.clone());
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_helper<R>(max_bm25_words: usize, f: impl FnOnce(&HeaderHelper) -> R) -> R {
        let cfg = ChunkingConfig {
            max_bm25_words,
            max_tokens: 512,
            ..Default::default()
        };
        let helper = HeaderHelper::new(&cfg);
        f(&helper)
    }

    #[test]
    fn test_compact_header_within_limit_unchanged() {
        with_helper(300, |helper| {
            assert_eq!(
                helper.compact_header("short header", "", ChunkPath::Bm25),
                "short header"
            );
        });
    }

    #[test]
    fn test_compact_header_truncates_at_sentence_end() {
        with_helper(18, |helper| {
            let long =
                "First sentence with content here. Second sentence keeps going on and on and on.";
            let result = helper.compact_header(long, "", ChunkPath::Bm25);
            assert_eq!(result, "First sentence with content here.");
            assert!(result.ends_with('.'));
            assert!(helper.unit_count(&result, ChunkPath::Bm25) <= 6);
        });
    }

    #[test]
    fn test_compact_header_sentence_end_falls_back_to_word_level() {
        with_helper(12, |helper| {
            let long = "one two three four five six seven eight nine ten";
            let result = helper.compact_header(long, "", ChunkPath::Bm25);
            assert_eq!(result, "one two three four");
        });
    }

    #[test]
    fn test_compact_header_newline_is_sentence_end() {
        with_helper(18, |helper| {
            let long = "overview line with several words\nsecond line that would blow the budget easily if kept";
            let result = helper.compact_header(long, "", ChunkPath::Bm25);
            assert_eq!(result, "overview line with several words");
        });
    }

    #[test]
    fn test_compact_header_dot_inside_identifier_is_not_sentence_end() {
        with_helper(15, |helper| {
            let long = "foo.bar baz qux quux corge grault garply";
            let result = helper.compact_header(long, "", ChunkPath::Bm25);
            assert_eq!(result, "foo.bar baz qux quux corge");
        });
    }
}
