use cce_types::ConversionResult;

use super::boundary::cost;
use super::config::ChunkingConfig;
use super::result::ChunkPath;

pub struct HeaderHelper<'a> {
    config: &'a ChunkingConfig,
}

/// Per-member signals consumed by `HeaderHelper::group_members_by_header_budget`.
pub struct MemberSignals<'a> {
    pub self_contained: &'a [bool],
    pub multiline: &'a [bool],
    pub source_lines: &'a [usize],
}

/// Source lines from which an undocumented member keeps its own chunk.
///
/// A three-line method with a doc comment and a call body (e.g. a Gin
/// `String`/`Render` helper) carries an independent behavior even when its
/// description is short; a one-line delegation does not. The threshold sits
/// above one-line residues so viable short methods stand alone while pure
/// forwarders still rejoin their neighbor.
pub(crate) const VIABLE_SOURCE_LINES: usize = 3;

/// One standalone-eligible member plus the budgets that decide whether it
/// may rejoin the previous member group instead of standing alone.
struct PrevPack<'a> {
    member: &'a ConversionResult,
    member_text: &'a str,
    member_source_lines: usize,
    member_documented: bool,
    first_budget: usize,
    continuation_budget: usize,
    separator_cost: usize,
    min_cost: usize,
    path: ChunkPath,
}

impl<'a> HeaderHelper<'a> {
    pub fn new(config: &'a ChunkingConfig) -> Self {
        Self { config }
    }

    pub fn path_limit(&self, path: ChunkPath) -> usize {
        match path {
            ChunkPath::Bm25 => self.config.max_bm25_terms,
            ChunkPath::Embedding => self.config.max_tokens,
        }
    }

    pub fn unit_count(&self, text: &str, path: ChunkPath) -> usize {
        cost(text, path)
    }

    /// Limit a repeated header to half of the hard chunk limit.
    ///
    /// The header carries the group identity plus the full documentation
    /// of its own entity; its member tables only repeat names that the
    /// keyword field already holds, so under pressure the tables yield
    /// first. Group text lays identity and documentation before the
    /// tables, and truncation below keeps whole sentences, which is
    /// exactly the priority a recall header needs: a dropped doc
    /// sentence (for example a struct paragraph holding the only
    /// occurrence of a query word) can never be recovered downstream,
    /// while a dropped table row still matches through keywords.
    pub fn compact_header(&self, full: &str, brief: &str, path: ChunkPath) -> String {
        let limit = self.path_limit(path);
        if limit == 0 {
            return full.to_string();
        }
        let header_limit = (limit / 2).max(1);
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
            let truncated =
                cce_text::truncate_to_cost(preferred, super::boundary::unit(path), header_limit);
            if truncated.is_empty() {
                preferred
                    .chars()
                    .next()
                    .map(|character| character.to_string())
                    .unwrap_or_default()
            } else {
                truncated.to_string()
            }
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
    /// A member may become a chunk of its own only when it can stand alone
    /// as a chunk.
    ///
    /// On the Embedding path that means the member is flagged `self_contained`
    /// (it carries its own docstring/behavior description) or it owns a
    /// multi-statement implementation (`member_source_lines` at or above
    /// `VIABLE_SOURCE_LINES`), so its topic is never diluted by adjacent
    /// members. An undocumented ten-line method keeps its identity; a
    /// one-line delegation still rejoins its neighbor.
    ///
    /// On the BM25 path the member qualifies when it is `self_contained` or
    /// when its own source spans more than one line. A single declaration
    /// line - a macro, an enum constant, a struct field, a function-local
    /// variable - has no retrieval value of its own, and emitting it alone
    /// fills the corpus with one-line chunks. Such members are packed with
    /// their peers instead, which keeps the declaration searchable without
    /// spending a corpus slot on it.
    pub fn group_members_by_header_budget(
        &self,
        members: &[ConversionResult],
        first_budget: usize,
        continuation_budget: usize,
        path: ChunkPath,
        signals: MemberSignals<'_>,
    ) -> Vec<Vec<ConversionResult>> {
        if first_budget == usize::MAX {
            return vec![members.to_vec()];
        }

        let min_cost = match path {
            ChunkPath::Bm25 => self.config.min_chunk_bm25_terms,
            ChunkPath::Embedding => self.config.min_chunk_tokens,
        };

        // The header is fixed for the whole group; the member budget therefore
        // excludes the header cost AND the separator that
        // `assemble_combined_text` inserts between them. The separator is
        // measured in the path unit instead of using a fixed constant, so a
        // path whose tokenizer counts no term for blank lines keeps the full
        // budget. Keeping the two values distinct lets `below_min` pack up
        // to the member maximum without ever pushing the combined text past
        // the path limit.
        let separator_cost = cost("\n\n", path);
        let member_max_limit = first_budget.saturating_sub(separator_cost).max(1);

        let mut groups = Vec::new();
        let mut current_group = Vec::new();
        let mut current_text = String::new();
        let mut current_cost = 0;
        for (idx, member) in members.iter().enumerate() {
            let member_text = match path {
                ChunkPath::Bm25 => member.bm25_text.as_deref().unwrap_or(""),
                ChunkPath::Embedding => member.embedding_text.as_deref().unwrap_or(""),
            };

            let member_source_lines = signals.source_lines.get(idx).copied().unwrap_or(0);
            let may_stand_alone = match path {
                ChunkPath::Embedding => {
                    signals.self_contained.get(idx).copied().unwrap_or(false)
                        || member_source_lines >= VIABLE_SOURCE_LINES
                }
                ChunkPath::Bm25 => {
                    signals.self_contained.get(idx).copied().unwrap_or(false)
                        || signals.multiline.get(idx).copied().unwrap_or(false)
                }
            };
            if may_stand_alone {
                if !current_group.is_empty() {
                    groups.push(std::mem::take(&mut current_group));
                    current_text.clear();
                }
                // A standalone member still joins the previous member group
                // when it only continues that group's topic. Overloads share
                // one declaration name but are distinct entities; splitting
                // adjacent same-name overloads (e.g. two `Handle` methods in
                // `RequestHandlerWrapperImpl`) scatters the call body
                // (`Reverse`/`Aggregate`) a query must match in one unit. A
                // member below the min threshold is a residue with no
                // retrieval value of its own (e.g. the one-line `PublishCore`
                // delegation `=> _publisher.Publish(...)`); leaving it alone
                // spends a corpus slot on a fragment no query can match.
                // Either packs backward when the combined text still fits the
                // budget that governed the previous group.
                if Self::pack_with_previous(
                    self,
                    &mut groups,
                    PrevPack {
                        member,
                        member_text,
                        member_source_lines,
                        member_documented: signals
                            .self_contained
                            .get(idx)
                            .copied()
                            .unwrap_or(false),
                        first_budget,
                        continuation_budget,
                        separator_cost,
                        min_cost,
                        path,
                    },
                ) {
                    continue;
                }
                groups.push(vec![member.clone()]);
                continue;
            }

            let budget = if groups.is_empty() {
                first_budget
            } else {
                continuation_budget
            };
            let effective_max = if groups.is_empty() {
                member_max_limit
            } else {
                continuation_budget.saturating_sub(separator_cost).max(1)
            };
            if !current_group.is_empty() {
                let mut candidate = current_text.clone();
                candidate.push_str("\n\n");
                candidate.push_str(member_text);
                let candidate_cost = self.unit_count(&candidate, path);
                let below_min = current_cost < min_cost;
                if candidate_cost > effective_max || (!below_min && candidate_cost > budget) {
                    groups.push(std::mem::take(&mut current_group));
                    current_text.clear();
                }
            }
            if current_group.is_empty() {
                current_text.push_str(member_text);
            } else {
                current_text.push_str("\n\n");
                current_text.push_str(member_text);
            }
            current_cost = self.unit_count(&current_text, path);
            current_group.push(member.clone());
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }
        groups
    }

    /// Pack a standalone-eligible member backward into the previous member
    /// group when it only continues that group's topic.
    ///
    /// Returns true when the member was packed (the caller must skip the
    /// standalone push). Packing applies in two cases: adjacent same-name
    /// overloads, which share one declaration identity, and tiny residues
    /// below the min threshold, which have no retrieval value alone. On the
    /// embedding path a documented member never packs as a residue: its own
    /// description is a retrieval unit even when short, and merging it only
    /// dilutes both sides under dense averaging. The keyword path keeps the
    /// residue packing so one-line splinters do not fill the corpus.
    /// A residue
    /// is short on both axes: its natural-language cost is below the min
    /// threshold and its source spans fewer than `VIABLE_SOURCE_LINES` lines.
    /// A multi-statement implementation keeps its own chunk even when its
    /// undocumented description is short; only a one-line delegation rejoins
    /// its neighbor. The combined text must fit the budget that governed the
    /// previous group; multi-entity previous groups only absorb a same-name
    /// member when every member they hold already shares that name, so
    /// unrelated members never glue together through a shared neighbor.
    fn pack_with_previous(&self, groups: &mut [Vec<ConversionResult>], pack: PrevPack) -> bool {
        let PrevPack {
            member,
            member_text,
            member_source_lines,
            member_documented,
            first_budget,
            continuation_budget,
            separator_cost,
            min_cost,
            path,
        } = pack;
        if groups.is_empty() {
            return false;
        }
        let is_first = groups.len() == 1;
        let Some(prev) = groups.last_mut() else {
            return false;
        };
        if member.name.is_empty() || !prev.iter().all(|m| m.name == member.name) {
            if path == ChunkPath::Embedding && member_documented {
                return false;
            }
            let member_cost = self.unit_count(member_text, path);
            if member_cost >= min_cost || member_source_lines >= VIABLE_SOURCE_LINES {
                return false;
            }
        }
        let prev_budget = if is_first {
            first_budget
        } else {
            continuation_budget
        };
        let prev_max = prev_budget.saturating_sub(separator_cost).max(1);
        let mut candidate_cost = self.unit_count(member_text, path);
        for m in prev.iter() {
            let text = match path {
                ChunkPath::Bm25 => m.bm25_text.as_deref().unwrap_or(""),
                ChunkPath::Embedding => m.embedding_text.as_deref().unwrap_or(""),
            };
            candidate_cost = candidate_cost
                .saturating_add(separator_cost)
                .saturating_add(self.unit_count(text, path));
        }
        if candidate_cost > prev_max {
            return false;
        }
        prev.push(member.clone());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_helper<R>(max_bm25_terms: usize, f: impl FnOnce(&HeaderHelper) -> R) -> R {
        let cfg = ChunkingConfig {
            max_bm25_terms,
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
            assert_eq!(result, "one two three four five six");
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
        with_helper(9, |helper| {
            let long = "foo.bar baz qux quux corge grault garply";
            let result = helper.compact_header(long, "", ChunkPath::Bm25);
            assert_eq!(result, "foo.bar baz qux quux");
        });
    }

    fn member_conversions(n: usize) -> Vec<ConversionResult> {
        (0..n)
            .map(|i| ConversionResult {
                entity_id: cce_types::entity::EntityId(i as u64),
                bm25_text: Some(format!("fn member_{i}()")),
                embedding_text: Some(format!("member {i} does things")),
                ..Default::default()
            })
            .collect()
    }

    /// Members above the min threshold: distinct viable units stay apart.
    fn large_member_conversions(n: usize) -> Vec<ConversionResult> {
        (0..n)
            .map(|i| ConversionResult {
                entity_id: cce_types::entity::EntityId(i as u64),
                bm25_text: Some(format!("fn member_{i}() {}", "body ".repeat(60))),
                embedding_text: Some(format!("member {i} does things {}", "body ".repeat(60))),
                ..Default::default()
            })
            .collect()
    }

    fn default_helper<R>(f: impl FnOnce(&HeaderHelper) -> R) -> R {
        let cfg = ChunkingConfig::default();
        let helper = HeaderHelper::new(&cfg);
        f(&helper)
    }

    #[test]
    fn test_bm25_multiline_members_split_per_member() {
        default_helper(|helper| {
            // Viable multiline members (above the min threshold) keep one
            // chunk each: each is a retrieval unit of its own.
            let members = large_member_conversions(13);
            let self_contained = vec![false; members.len()];
            let multiline = vec![true; members.len()];
            let source_lines = vec![0; members.len()];
            let groups = helper.group_members_by_header_budget(
                &members,
                1000,
                1000,
                ChunkPath::Bm25,
                MemberSignals {
                    self_contained: &self_contained,
                    multiline: &multiline,
                    source_lines: &source_lines,
                },
            );
            assert_eq!(groups.len(), 13);
            assert!(groups.iter().all(|g| g.len() == 1));
        });
    }

    #[test]
    fn test_bm25_tiny_multiline_members_pack_together() {
        default_helper(|helper| {
            // A residue below the min threshold is no retrieval unit alone
            // (e.g. a one-line `PublishCore` delegation): tiny multiline
            // members rejoin their neighbors instead of standing alone.
            let members = member_conversions(13);
            let self_contained = vec![false; members.len()];
            let multiline = vec![true; members.len()];
            let source_lines = vec![2; members.len()];
            let groups = helper.group_members_by_header_budget(
                &members,
                1000,
                1000,
                ChunkPath::Bm25,
                MemberSignals {
                    self_contained: &self_contained,
                    multiline: &multiline,
                    source_lines: &source_lines,
                },
            );
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].len(), 13);
        });
    }

    #[test]
    fn test_bm25_same_name_overloads_pack_together() {
        default_helper(|helper| {
            // Adjacent same-name overloads share one declaration identity;
            // splitting them scatters the call body a query must match in
            // one unit, so they pack when the combined text fits.
            let members: Vec<ConversionResult> = (0..2)
                .map(|i| ConversionResult {
                    entity_id: cce_types::entity::EntityId(i),
                    name: "Handle".to_string(),
                    bm25_text: Some(format!("Task Handle overload {i} {}", "body ".repeat(20))),
                    embedding_text: Some(format!("Handle overload {i}")),
                    ..Default::default()
                })
                .collect();
            let self_contained = vec![false; members.len()];
            let multiline = vec![true; members.len()];
            let source_lines = vec![10; members.len()];
            let groups = helper.group_members_by_header_budget(
                &members,
                1000,
                1000,
                ChunkPath::Bm25,
                MemberSignals {
                    self_contained: &self_contained,
                    multiline: &multiline,
                    source_lines: &source_lines,
                },
            );
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].len(), 2);
        });
    }

    #[test]
    fn test_bm25_single_line_members_are_packed() {
        default_helper(|helper| {
            let members = member_conversions(13);
            let self_contained = vec![false; members.len()];
            let multiline = vec![false; members.len()];
            let source_lines = vec![1; members.len()];
            let groups = helper.group_members_by_header_budget(
                &members,
                1000,
                1000,
                ChunkPath::Bm25,
                MemberSignals {
                    self_contained: &self_contained,
                    multiline: &multiline,
                    source_lines: &source_lines,
                },
            );
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].len(), 13);
        });
    }

    #[test]
    fn test_bm25_self_contained_single_line_member_stands_alone() {
        default_helper(|helper| {
            // A viable self-contained member (above the min threshold)
            // stands alone so its topic is never diluted by neighbors.
            let members = large_member_conversions(3);
            let self_contained = vec![false, true, false];
            let multiline = vec![false; members.len()];
            let source_lines = vec![1; members.len()];
            let groups = helper.group_members_by_header_budget(
                &members,
                1000,
                1000,
                ChunkPath::Bm25,
                MemberSignals {
                    self_contained: &self_contained,
                    multiline: &multiline,
                    source_lines: &source_lines,
                },
            );
            assert_eq!(groups.len(), 3);
            assert!(groups.iter().all(|g| g.len() == 1));
        });
    }

    #[test]
    fn test_bm25_small_group_keeps_packing() {
        default_helper(|helper| {
            let members = member_conversions(3);
            let self_contained = vec![false; members.len()];
            let multiline = vec![false; members.len()];
            let source_lines = vec![1; members.len()];
            let groups = helper.group_members_by_header_budget(
                &members,
                1000,
                1000,
                ChunkPath::Bm25,
                MemberSignals {
                    self_contained: &self_contained,
                    multiline: &multiline,
                    source_lines: &source_lines,
                },
            );
            assert_eq!(groups.len(), 1);
        });
    }

    #[test]
    fn test_embedding_path_packs_members_without_own_description() {
        default_helper(|helper| {
            let members = member_conversions(13);
            let self_contained = vec![false; members.len()];
            let multiline = vec![true; members.len()];
            let source_lines = vec![2; members.len()];
            let groups = helper.group_members_by_header_budget(
                &members,
                1000,
                1000,
                ChunkPath::Embedding,
                MemberSignals {
                    self_contained: &self_contained,
                    multiline: &multiline,
                    source_lines: &source_lines,
                },
            );
            assert_eq!(groups.len(), 1);
        });
    }

    #[test]
    fn test_embedding_viable_source_lines_stand_alone() {
        default_helper(|helper| {
            // An undocumented ten-line implementation keeps its own chunk
            // even when its description is short; tiny residues stay in
            // the greedy tail instead of diluting the viable member.
            let members = member_conversions(3);
            let self_contained = vec![false; members.len()];
            let multiline = vec![true; members.len()];
            let source_lines = vec![10, 10, 1];
            let groups = helper.group_members_by_header_budget(
                &members,
                1000,
                1000,
                ChunkPath::Embedding,
                MemberSignals {
                    self_contained: &self_contained,
                    multiline: &multiline,
                    source_lines: &source_lines,
                },
            );
            assert_eq!(groups.len(), 3);
            assert_eq!(groups[0].len(), 1);
            assert_eq!(groups[1].len(), 1);
        });
    }

    #[test]
    fn test_embedding_documented_tiny_members_stand_alone() {
        default_helper(|helper| {
            // A documented one-line member is a retrieval unit of its own
            // on the embedding path: merging it only dilutes both sides
            // under dense averaging. The keyword path keeps packing such
            // residues so one-line splinters do not fill the corpus.
            let members = member_conversions(2);
            let self_contained = vec![true; members.len()];
            let multiline = vec![false; members.len()];
            let source_lines = vec![1; members.len()];
            let emb_groups = helper.group_members_by_header_budget(
                &members,
                1000,
                1000,
                ChunkPath::Embedding,
                MemberSignals {
                    self_contained: &self_contained,
                    multiline: &multiline,
                    source_lines: &source_lines,
                },
            );
            assert_eq!(emb_groups.len(), 2);
            assert!(emb_groups.iter().all(|g| g.len() == 1));
        });
    }
}
