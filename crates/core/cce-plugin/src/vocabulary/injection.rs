//! Injection of domain terminology into indexed and exported text.
//!
//! The terminology a `Vocabulary` plugin contributes is added to the text
//! that is actually retrieved. The injection never replaces an identifier:
//! the expansion is placed next to the term it explains, so the original
//! spelling stays searchable and the meaning travels with it.
//!
//! Two shapes are rendered, because the two retrieval paths read the text
//! differently:
//!
//! - the embedding text carries the expansion inline, right after the
//!   identifier run it belongs to. A vector built from that text then
//!   carries the meaning of the symbol itself, not merely of the paragraph
//!   the symbol happens to sit in.
//! - the keyword text carries the expansion as a trailing block at the end
//!   of the line the identifier sits on. The keyword index is a bag of
//!   words, so the position of the expansion does not change what it
//!   matches, while every adjacency the line originally had stays intact.
//!
//! The injection is bounded: a chunk carries at most a fixed number of
//! expansions, and the final chunk text may not exceed the path's hard
//! limit. The reserve is a planning target for the splitter, not a spending
//! permission: every candidate is measured against the actual final text.
//! The terms that do not fit are reported as truncated instead of being
//! silently dropped.

use std::collections::{HashMap, HashSet};

use cce_text::ChunkUnit;
use cce_types::plugin::VocabularyMatch;
use cce_types::{ChunkPath, ChunkedResult};

use super::split_identifier_segments;

/// The terminology a run of injections already expanded.
///
/// The chunk scope expands every term in every chunk, so the set stays
/// empty. The entity scope expands every term once per entity, so the set
/// carries the terms the entity already carries.
#[derive(Debug, Default)]
pub struct ExpandedTerms {
    /// The match keys that were expanded.
    keys: HashSet<String>,
}

/// The shape of an injected terminology expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionStyle {
    /// An inline parenthesis right after the matched identifier run.
    Embedding,
    /// A trailing terminology block at the end of the matched line.
    Bm25,
}

/// The budget one chunk offers to the terminology injection.
#[derive(Debug, Clone, Copy)]
pub struct InjectionBudget {
    /// The maximum number of terminology entries one chunk may carry.
    pub max_entries: usize,
    /// The absolute limit of the final chunk text, in `unit`.
    /// Zero disables the limit.
    pub max_total: usize,
    /// The unit the limit is expressed in.
    pub unit: ChunkUnit,
}

/// The absolute limits of the two chunk paths.
#[derive(Debug, Clone, Copy)]
pub struct InjectionLimits {
    /// The BM25 term limit.
    pub bm25: usize,
    /// The embedding token limit.
    pub embedding: usize,
}

impl InjectionLimits {
    fn for_path(self, path: ChunkPath) -> usize {
        match path {
            ChunkPath::Bm25 => self.bm25,
            ChunkPath::Embedding => self.embedding,
        }
    }
}

/// The result of one terminology injection.
#[derive(Debug, Clone)]
pub struct Injection {
    /// The text with the expansions injected.
    pub text: String,
    /// The number of terminology entries that were injected.
    pub injected: usize,
    /// The number of matched terminology entries that did not fit into the
    /// budget.
    pub truncated: usize,
}

/// The counters of one terminology injection run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VocabularyStats {
    /// The number of terminology entries that were injected.
    pub injected: usize,
    /// The number of matched terminology entries that did not fit into the
    /// budget of their chunk.
    pub truncated: usize,
}

impl VocabularyStats {
    /// Merge the counters of another run into this one.
    pub fn absorb(&mut self, other: VocabularyStats) {
        self.injected += other.injected;
        self.truncated += other.truncated;
    }
}

/// The terminology entries prepared for injection.
///
/// The lookup is built once per file and reused for every chunk of that
/// file, so the cost of an injection depends on the number of identifier
/// runs in the chunk, not on the size of the terminology table.
#[derive(Debug, Default, Clone)]
pub struct TerminologyIndex {
    /// The entries, in table order.
    entries: Vec<VocabularyMatch>,
    /// The index of the entry every match key resolves to.
    by_key: HashMap<String, usize>,
}

impl TerminologyIndex {
    /// Build the index from the terminology entries.
    ///
    /// The first entry of a match key wins, so every key resolves to
    /// exactly one expansion.
    pub fn new(entries: &[VocabularyMatch]) -> Self {
        let mut index = Self::default();
        for entry in entries {
            let key = entry.match_key();
            if key.is_empty() || index.by_key.contains_key(&key) {
                continue;
            }
            index.by_key.insert(key, index.entries.len());
            index.entries.push(entry.clone());
        }
        index
    }

    /// Whether the index carries no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The number of entries the index holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The entries, in table order.
    pub fn entries(&self) -> &[VocabularyMatch] {
        &self.entries
    }

    /// The entry a match key resolves to.
    fn get(&self, key: &str) -> Option<&VocabularyMatch> {
        self.by_key
            .get(key)
            .and_then(|index| self.entries.get(*index))
    }
}

/// Inject the terminology expansions into `text`.
///
/// The expansion of a term is placed at the first occurrence of the term:
/// inline after the identifier run for the embedding text, and as a trailing
/// block at the end of the line for the keyword text. The identifier itself
/// is never modified, and the injected text never takes part in the
/// matching, so an expansion that mentions another term cannot trigger a
/// second injection.
///
/// A term that `expanded` already carries is left alone, so a run of
/// injections over the chunks of one entity can expand every term only
/// once. The injection stops when the budget is exhausted; the terms that
/// did not fit are reported as truncated.
pub fn inject(
    text: &str,
    index: &TerminologyIndex,
    style: InjectionStyle,
    budget: &InjectionBudget,
    expanded: &mut ExpandedTerms,
) -> Injection {
    if index.is_empty() || text.is_empty() {
        return Injection {
            text: text.to_string(),
            injected: 0,
            truncated: 0,
        };
    }

    let mut insertions: Vec<(usize, String)> = Vec::new();
    let mut injected = 0usize;
    let mut truncated = 0usize;
    // The terms this text already mentions; the first occurrence of a term
    // is the only one that carries the expansion.
    let mut mentioned: HashSet<String> = HashSet::new();

    for anchor in anchors(text, style) {
        if !text.is_char_boundary(anchor.offset) {
            truncated += anchor.keys.len();
            continue;
        }
        // The terms this anchor mentions, in the order they appear. A term
        // is considered once per text: repeating it at a later anchor cannot
        // change whether its expansion fits.
        let mut fresh: Vec<&VocabularyMatch> = Vec::new();
        for key in &anchor.keys {
            let Some(entry) = index.get(key) else {
                continue;
            };
            let match_key = entry.match_key();
            if expanded.keys.contains(&match_key) || !mentioned.insert(match_key) {
                continue;
            }
            fresh.push(entry);
        }
        if fresh.is_empty() {
            continue;
        }

        let separator = separator_before(text, anchor.offset);
        // Measure the absolute final text, not the growth of the block.
        // The block may start with a separator and may be inserted in the
        // middle of a tokenizer run; only the final text is relevant.
        let mut fitted = 0usize;
        for count in 1..=fresh.len() {
            if injected + count > budget.max_entries {
                break;
            }
            let mut candidate_insertions = insertions.clone();
            candidate_insertions.push((
                anchor.offset,
                format!("{separator}{}", render(&fresh[..count], style)),
            ));
            let candidate = apply(text, &candidate_insertions);
            if budget.max_total != 0 && budget.unit.cost(&candidate) > budget.max_total {
                break;
            }
            fitted = count;
        }
        if fitted == 0 {
            truncated += fresh.len();
            continue;
        }
        truncated += fresh.len() - fitted;
        injected += fitted;
        for entry in &fresh[..fitted] {
            expanded.keys.insert(entry.match_key());
        }
        insertions.push((
            anchor.offset,
            format!("{separator}{}", render(&fresh[..fitted], style)),
        ));
    }

    Injection {
        text: apply(text, &insertions),
        injected,
        truncated,
    }
}

/// Inject terminology into every chunk of a file.
///
/// This is the single chunk-level loop shared by the AST and document
/// pipelines. The injected text is checked against the chunk's absolute
/// hard limit, so an expansion can never push a chunk over the budget even
/// when the body was split without a reserve.
pub fn inject_chunks(
    chunks: &mut [ChunkedResult],
    vocabulary: &super::FileVocabulary,
    per_entity: bool,
    max_entries: usize,
    limits: InjectionLimits,
) -> VocabularyStats {
    let mut stats = VocabularyStats::default();
    let index = TerminologyIndex::new(&vocabulary.matches());
    if index.is_empty() {
        return stats;
    }

    let mut expanded = ExpandedTerms::default();
    let mut entity = String::new();
    for chunk in chunks.iter_mut() {
        if per_entity {
            if chunk.source_group_id != entity {
                entity = chunk.source_group_id.clone();
                expanded = ExpandedTerms::default();
            }
        } else {
            expanded = ExpandedTerms::default();
        }

        let (style, unit) = match chunk.path {
            ChunkPath::Bm25 => (InjectionStyle::Bm25, ChunkUnit::Terms),
            ChunkPath::Embedding => (InjectionStyle::Embedding, ChunkUnit::Tokens),
        };
        let budget = InjectionBudget {
            max_entries,
            max_total: limits.for_path(chunk.path),
            unit,
        };
        let injection = inject(&chunk.text, &index, style, &budget, &mut expanded);
        stats.injected += injection.injected;
        stats.truncated += injection.truncated;
        if injection.injected == 0 {
            continue;
        }
        chunk.text = injection.text;
        chunk.token_count = unit.cost(&chunk.text);
        if chunk.metadata.bm25_word_count.is_some() {
            chunk.metadata.bm25_word_count = Some(ChunkUnit::Terms.cost(&chunk.text));
        }
    }
    stats
}

/// The expansion block of `entries`.
fn render(entries: &[&VocabularyMatch], style: InjectionStyle) -> String {
    match style {
        InjectionStyle::Embedding => {
            let expansions: Vec<&str> = entries
                .iter()
                .map(|entry| entry.expansion.as_str())
                .collect();
            format!("({})", expansions.join("; "))
        }
        InjectionStyle::Bm25 => {
            let parts: Vec<String> = entries
                .iter()
                .map(|entry| format!("{} = {}", entry.token, entry.expansion))
                .collect();
            format!("[{}]", parts.join("; "))
        }
    }
}

/// The separator the injected block needs in front of it.
///
/// A block that follows a whitespace needs no separator of its own; one
/// that follows a word needs a space so it does not glue itself to that
/// word.
fn separator_before(text: &str, offset: usize) -> &'static str {
    match text[..offset].chars().next_back() {
        None => "",
        Some(character) if character.is_whitespace() => "",
        Some(_) => " ",
    }
}

/// The text with the injected blocks placed at their offsets.
///
/// The offsets grow, so the text is copied in one pass and every block
/// lands at the place it was measured for.
fn apply(text: &str, insertions: &[(usize, String)]) -> String {
    if insertions.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 64);
    let mut cursor = 0usize;
    for (offset, block) in insertions {
        out.push_str(&text[cursor..*offset]);
        out.push_str(block);
        cursor = *offset;
    }
    out.push_str(&text[cursor..]);
    out
}

/// One place where an expansion can be placed.
struct Anchor {
    /// The byte offset the expansion is inserted at.
    offset: usize,
    /// The match keys the anchor offers, in the order they appear.
    keys: Vec<String>,
}

/// The anchors of `text`, in the order they appear.
fn anchors(text: &str, style: InjectionStyle) -> Vec<Anchor> {
    let runs = identifier_runs(text);
    match style {
        InjectionStyle::Embedding => runs
            .into_iter()
            .map(|run| Anchor {
                offset: run.end,
                keys: run.keys,
            })
            .collect(),
        InjectionStyle::Bm25 => line_anchors(text, runs),
    }
}

/// The anchors of the keyword text: one per line, holding every run of
/// that line.
///
/// The block of a line carries every term the line mentions, so a line
/// that names two terms explains both of them in one place.
fn line_anchors(text: &str, runs: Vec<TokenRun>) -> Vec<Anchor> {
    let lines = line_ranges(text);
    let mut anchors: Vec<Anchor> = Vec::with_capacity(lines.len());
    let mut line = 0usize;
    for run in runs {
        while line + 1 < lines.len() && lines[line].1 < run.end {
            line += 1;
        }
        let end = lines.get(line).map_or(text.len(), |(_, end)| *end);
        match anchors.last_mut() {
            Some(anchor) if anchor.offset == end => anchor.keys.extend(run.keys),
            _ => anchors.push(Anchor {
                offset: end,
                keys: run.keys,
            }),
        }
    }
    anchors
}

/// The byte ranges of the lines of `text`, without their terminators.
fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                ranges.push((start, index));
                index += 1;
                start = index;
            }
            b'\r' => {
                ranges.push((start, index));
                index += if bytes.get(index + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                start = index;
            }
            _ => index += 1,
        }
    }
    if start < bytes.len() {
        ranges.push((start, bytes.len()));
    }
    ranges
}

/// One identifier run of a text, with the byte range it occupies.
struct TokenRun {
    /// The byte offset of the run end.
    end: usize,
    /// The match keys the run offers: the lowercased run and the lowercased
    /// segments of its underscore and camelCase boundaries.
    keys: Vec<String>,
}

/// The identifier tokens of `text`, lowercased.
///
/// A token is a maximal sequence of alphanumeric characters and underscores,
/// plus each of its underscore and camelCase separated segments. Matching a
/// term against `parse_stts` therefore hits `stts`, while `ts` never matches
/// `stts`: a segment boundary is a real identifier boundary.
pub(super) fn identifier_tokens(text: &str) -> impl Iterator<Item = String> {
    identifier_runs(text)
        .into_iter()
        .flat_map(|run| run.keys.into_iter())
}

/// The identifier runs of `text`.
///
/// A run is a maximal sequence of alphanumeric characters and underscores.
/// A run offers its own lowercased text and the lowercased segments of its
/// boundaries as match keys, so the term `stts` matches `parse_stts` while
/// the term `ts` never matches `stts`.
fn identifier_runs(text: &str) -> Vec<TokenRun> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (index, character) in text.char_indices() {
        if character.is_alphanumeric() || character == '_' {
            if start.is_none() {
                start = Some(index);
            }
            continue;
        }
        if let Some(run_start) = start.take() {
            runs.push(token_run(text, run_start, index));
        }
    }
    if let Some(run_start) = start {
        runs.push(token_run(text, run_start, text.len()));
    }
    runs
}

/// The run of `text` that spans `[start, end)`.
fn token_run(text: &str, start: usize, end: usize) -> TokenRun {
    let run = &text[start..end];
    let mut keys = Vec::with_capacity(2);
    keys.push(run.to_lowercase());
    keys.extend(
        split_identifier_segments(run)
            .into_iter()
            .map(str::to_lowercase),
    );
    TokenRun { end, keys }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(entries: &[(&str, &str)]) -> Vec<VocabularyMatch> {
        entries
            .iter()
            .map(|(token, expansion)| VocabularyMatch::new(*token, *expansion))
            .collect()
    }

    fn index(entries: &[(&str, &str)]) -> TerminologyIndex {
        TerminologyIndex::new(&matches(entries))
    }

    /// Inject with a fresh expansion state, as the chunk scope does.
    fn inject_once(
        text: &str,
        index: &TerminologyIndex,
        style: InjectionStyle,
        budget: &InjectionBudget,
    ) -> Injection {
        inject(text, index, style, budget, &mut ExpandedTerms::default())
    }

    fn budget(max_entries: usize, max_total: usize) -> InjectionBudget {
        InjectionBudget {
            max_entries,
            max_total,
            unit: ChunkUnit::Tokens,
        }
    }

    #[test]
    fn the_expansion_lands_after_the_identifier_run() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let injection = inject_once(
            "parse_stts reads the table.",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 100),
        );
        assert_eq!(
            injection.text,
            "parse_stts (decoding time to sample box) reads the table."
        );
        assert_eq!(injection.injected, 1);
    }

    #[test]
    fn the_run_end_anchors_the_expansion_not_the_segment() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let injection = inject_once(
            "the sttsCount field grows",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 100),
        );
        assert_eq!(
            injection.text, "the sttsCount (decoding time to sample box) field grows",
            "the expansion must not split the identifier"
        );
    }

    #[test]
    fn only_the_first_occurrence_is_expanded() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let injection = inject_once(
            "stts then stts again",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 100),
        );
        assert_eq!(
            injection.text,
            "stts (decoding time to sample box) then stts again"
        );
        assert_eq!(injection.injected, 1);
    }

    #[test]
    fn several_terms_of_one_run_share_one_block() {
        let index = index(&[
            ("stts", "decoding time to sample box"),
            ("sdp", "session description protocol"),
        ]);
        let injection = inject_once(
            "parse_stts_sdp opens the session",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 100),
        );
        assert_eq!(
            injection.text,
            "parse_stts_sdp (decoding time to sample box; session description protocol) opens the session"
        );
        assert_eq!(injection.injected, 2);
    }

    #[test]
    fn the_budget_stops_the_injection_and_reports_the_rest() {
        let index = index(&[
            ("stts", "decoding time to sample box"),
            ("sdp", "session description protocol"),
        ]);
        let injection = inject_once(
            "stts and sdp in one line",
            &index,
            InjectionStyle::Embedding,
            &budget(1, 100),
        );
        assert_eq!(injection.injected, 1);
        assert_eq!(injection.truncated, 1);
        assert!(!injection.text.contains("session description protocol"));
    }

    #[test]
    fn the_cost_budget_stops_the_injection() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let injection = inject_once(
            "stts opens the table",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 1),
        );
        assert_eq!(injection.injected, 0);
        assert_eq!(injection.truncated, 1);
        assert_eq!(injection.text, "stts opens the table");
    }

    #[test]
    fn the_budget_is_the_absolute_final_text_not_the_growth() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let text = "stts opens the table and keeps the original spelling";
        let limit = ChunkUnit::Tokens.cost(text) + 2;
        let injection = inject_once(text, &index, InjectionStyle::Embedding, &budget(4, limit));
        assert_eq!(injection.injected, 0);
        assert_eq!(injection.truncated, 1);
        assert!(ChunkUnit::Tokens.cost(&injection.text) <= limit);
    }

    #[test]
    fn an_expansion_never_triggers_a_second_injection() {
        let index = index(&[
            ("stts", "decoding time to sample box"),
            ("box", "container of the samples"),
        ]);
        let injection = inject_once(
            "stts opens the table",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 100),
        );
        assert_eq!(injection.injected, 1);
        assert_eq!(
            injection.text,
            "stts (decoding time to sample box) opens the table"
        );
    }

    #[test]
    fn the_keyword_text_carries_the_block_at_the_end_of_the_line() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let injection = inject_once(
            "fn parse_stts(count: u32) -> Vec<u32>\nfn other() {}\n",
            &index,
            InjectionStyle::Bm25,
            &budget(4, 100),
        );
        assert_eq!(
            injection.text,
            "fn parse_stts(count: u32) -> Vec<u32> [stts = decoding time to sample box]\nfn other() {}\n"
        );
    }

    #[test]
    fn several_terms_of_one_line_share_one_block() {
        let index = index(&[
            ("stts", "decoding time to sample box"),
            ("ctts", "composition time to sample box"),
        ]);
        let injection = inject_once(
            "int stts = 0; int ctts = 0;\n",
            &index,
            InjectionStyle::Bm25,
            &budget(4, 100),
        );
        assert_eq!(
            injection.text,
            "int stts = 0; int ctts = 0; [stts = decoding time to sample box; ctts = composition time to sample box]\n"
        );
        assert_eq!(injection.injected, 2);
    }

    #[test]
    fn the_keyword_text_keeps_the_adjacency_of_the_line() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let injection = inject_once(
            "parse_stts sample_count\n",
            &index,
            InjectionStyle::Bm25,
            &budget(4, 100),
        );
        assert!(
            injection.text.starts_with("parse_stts sample_count"),
            "the original adjacency must survive the injection, got: {}",
            injection.text
        );
    }

    #[test]
    fn a_carriage_return_terminated_line_keeps_its_terminator() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let injection = inject_once(
            "int stts = 0;\r\nint other = 1;\r\n",
            &index,
            InjectionStyle::Bm25,
            &budget(4, 100),
        );
        assert_eq!(
            injection.text,
            "int stts = 0; [stts = decoding time to sample box]\r\nint other = 1;\r\n"
        );
    }

    #[test]
    fn a_multi_byte_text_keeps_its_characters_intact() {
        let index = index(&[("stts", "解码时间到样本盒")]);
        let injection = inject_once(
            "解析 stts 表格",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 100),
        );
        assert_eq!(injection.text, "解析 stts (解码时间到样本盒) 表格");
        assert!(injection.text.is_char_boundary(injection.text.len()));
    }

    #[test]
    fn a_text_without_a_term_stays_untouched() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let injection = inject_once(
            "nothing to explain here",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 100),
        );
        assert_eq!(injection.text, "nothing to explain here");
        assert_eq!(injection.injected, 0);
        assert_eq!(injection.truncated, 0);
    }

    #[test]
    fn an_empty_budget_injects_nothing() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let injection = inject_once(
            "stts opens the table",
            &index,
            InjectionStyle::Embedding,
            &budget(0, 100),
        );
        assert_eq!(injection.text, "stts opens the table");
        assert_eq!(injection.injected, 0);
        assert_eq!(injection.truncated, 1);
    }

    #[test]
    fn the_term_of_a_later_line_does_not_leak_into_an_earlier_one() {
        let index = index(&[("sdp", "session description protocol")]);
        let injection = inject_once(
            "first line\nsecond sdp line\n",
            &index,
            InjectionStyle::Bm25,
            &budget(4, 100),
        );
        assert_eq!(
            injection.text,
            "first line\nsecond sdp line [sdp = session description protocol]\n"
        );
    }

    #[test]
    fn an_already_expanded_term_is_left_alone() {
        let index = index(&[("stts", "decoding time to sample box")]);
        let mut expanded = ExpandedTerms::default();
        let first = inject(
            "stts opens the table",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 100),
            &mut expanded,
        );
        let second = inject(
            "stts opens the table",
            &index,
            InjectionStyle::Embedding,
            &budget(4, 100),
            &mut expanded,
        );
        assert_eq!(first.injected, 1);
        assert_eq!(second.injected, 0);
        assert_eq!(second.text, "stts opens the table");
    }
}
