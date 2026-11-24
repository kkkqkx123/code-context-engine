//! Range-based relevance judgments for the ripgrep codebase
//!
//! Defines ground truth for 45 benchmark queries:
//! - G1: Qualified symbol queries (10) - symbol tokens in natural-language
//!   structure (e.g. `find_at in RegexMatcher`), Strong only
//! - G2: Semantic queries (20) - Strong + Related, single-entity ranges
//! - FZ: Fuzzy queries (15) - lexical perturbations of G1 sources
//!
//! All line ranges verified against fixtures/rust/review/ripgrep/crates/.
//! Each range is confined to a single function/struct/trait definition to avoid
//! chunking boundary issues.
//!
//! Path remap notes (fixture `src/` → `crates/` layout, commit `eb1cd003`):
//! - Identical-content moves keep original line numbers.
//! - `searcher/src/searcher/mod.rs` ranges shifted by -2, `searcher/src/{sink.rs,
//!   searcher/core.rs, searcher/glue.rs}` by -1 (import block shrink).
//! - G2Q12 re-judged to `printer/src/standard.rs` (write_prelude/write_line_number).
//! - G2Q15 re-judged to `searcher/mod.rs` `ConfigError`; pre-remap metrics for
//!   these two queries are not comparable with post-remap ones.

use crate::bench_data::{QueryType, RelevanceJudgment, RelevanceLevel, SourceRange};

pub fn ripgrep_relevance_judgments() -> Vec<RelevanceJudgment> {
    use RelevanceLevel as RL;

    vec![
        // ==================== G1: Qualified Symbol Queries (10 total) ====================

        // G1Q1: RegexMatcher::find_at - core matching method
        RelevanceJudgment {
            id: "G1Q1".into(),
            query_text: "find_at in RegexMatcher".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/regex/src/matcher.rs".into(),
                    start_line: 409,
                    end_line: 421,
                },
                RL::Strong,
            )],
        },
        // G1Q2: WalkBuilder::build - directory iterator builder
        RelevanceJudgment {
            id: "G1Q2".into(),
            query_text: "build method on WalkBuilder".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/ignore/src/walk.rs".into(),
                    start_line: 565,
                    end_line: 602,
                },
                RL::Strong,
            )],
        },
        // G1Q3: GlobSet::is_match - glob pattern matching
        RelevanceJudgment {
            id: "G1Q3".into(),
            query_text: "is_match in GlobSet".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/globset/src/lib.rs".into(),
                    start_line: 342,
                    end_line: 344,
                },
                RL::Strong,
            )],
        },
        // G1Q4: Matcher trait - core matching interface
        RelevanceJudgment {
            id: "G1Q4".into(),
            query_text: "Matcher trait with find_at method".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/matcher/src/lib.rs".into(),
                        start_line: 546,
                        end_line: 559,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/matcher/src/lib.rs".into(),
                        start_line: 571,
                        end_line: 575,
                    },
                    RL::Strong,
                ),
            ],
        },
        // G1Q5: Searcher::search_path - main search entry point
        RelevanceJudgment {
            id: "G1Q5".into(),
            query_text: "search_path in Searcher".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/searcher/src/searcher/mod.rs".into(),
                    start_line: 643,
                    end_line: 657,
                },
                RL::Strong,
            )],
        },
        // G1Q6: Sink::matched - match handling callback
        RelevanceJudgment {
            id: "G1Q6".into(),
            query_text: "matched method on Sink".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/searcher/src/sink.rs".into(),
                    start_line: 124,
                    end_line: 128,
                },
                RL::Strong,
            )],
        },
        // G1Q7: TypesBuilder::new - file type matcher builder
        RelevanceJudgment {
            id: "G1Q7".into(),
            query_text: "new constructor in TypesBuilder".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/ignore/src/types.rs".into(),
                    start_line: 316,
                    end_line: 318,
                },
                RL::Strong,
            )],
        },
        // G1Q8: GitignoreBuilder::new - gitignore parser builder
        RelevanceJudgment {
            id: "G1Q8".into(),
            query_text: "new constructor in GitignoreBuilder".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/ignore/src/gitignore.rs".into(),
                    start_line: 327,
                    end_line: 333,
                },
                RL::Strong,
            )],
        },
        // G1Q9: Pcre2Matcher::build - PCRE2 regex compiler
        RelevanceJudgment {
            id: "G1Q9".into(),
            query_text: "build method on Pcre2Matcher".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/pcre2/src/matcher.rs".into(),
                    start_line: 37,
                    end_line: 39,
                },
                RL::Strong,
            )],
        },
        // G1Q10: Match::new - match range constructor
        RelevanceJudgment {
            id: "G1Q10".into(),
            query_text: "new constructor in Match".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/matcher/src/lib.rs".into(),
                    start_line: 80,
                    end_line: 93,
                },
                RL::Strong,
            )],
        },
        // ==================== G2: Semantic Queries (20 total) ====================

        // G2Q1: search with regex - core regex matching
        RelevanceJudgment {
            id: "G2Q1".into(),
            query_text: "search for text pattern in files using regular expression".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/regex/src/matcher.rs".into(),
                        start_line: 409,
                        end_line: 421,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/regex/src/matcher.rs".into(),
                        start_line: 53,
                        end_line: 85,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/mod.rs".into(),
                        start_line: 645,
                        end_line: 659,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q2: walk directory tree - recursive file traversal
        RelevanceJudgment {
            id: "G2Q2".into(),
            query_text: "recursively walk directory tree respecting gitignore rules".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/ignore/src/walk.rs".into(),
                        start_line: 565,
                        end_line: 602,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/ignore/src/walk.rs".into(),
                        start_line: 1037,
                        end_line: 1045,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/ignore/src/gitignore.rs".into(),
                        start_line: 90,
                        end_line: 118,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q3: glob pattern match - file path glob matching
        RelevanceJudgment {
            id: "G2Q3".into(),
            query_text: "check if file path matches glob pattern".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/globset/src/lib.rs".into(),
                        start_line: 342,
                        end_line: 344,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/globset/src/lib.rs".into(),
                        start_line: 350,
                        end_line: 360,
                    },
                    RL::Strong,
                ),
            ],
        },
        // G2Q4: extract context lines - surrounding line extraction
        RelevanceJudgment {
            id: "G2Q4".into(),
            query_text: "extract matched results with surrounding context lines".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/searcher/src/sink.rs".into(),
                        start_line: 142,
                        end_line: 149,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/mod.rs".into(),
                        start_line: 399,
                        end_line: 419,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/core.rs".into(),
                        start_line: 240,
                        end_line: 274,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q5: compile regex with flags - case insensitive compilation
        RelevanceJudgment {
            id: "G2Q5".into(),
            query_text: "compile regular expression with case insensitive flag".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/regex/src/matcher.rs".into(),
                        start_line: 103,
                        end_line: 106,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/regex/src/config.rs".into(),
                        start_line: 25,
                        end_line: 43,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q6: multi-line pattern matching - patterns spanning lines
        RelevanceJudgment {
            id: "G2Q6".into(),
            query_text: "match patterns that span across multiple lines".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/mod.rs".into(),
                        start_line: 373,
                        end_line: 391,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/glue.rs".into(),
                        start_line: 150,
                        end_line: 165,
                    },
                    RL::Strong,
                ),
            ],
        },
        // G2Q7: memory-mapped file search - mmap optimization
        RelevanceJudgment {
            id: "G2Q7".into(),
            query_text: "use memory mapped file for efficient search performance".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/mmap.rs".into(),
                        start_line: 13,
                        end_line: 25,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/mmap.rs".into(),
                        start_line: 49,
                        end_line: 56,
                    },
                    RL::Strong,
                ),
            ],
        },
        // G2Q8: line buffer reading - incremental file reading
        RelevanceJudgment {
            id: "G2Q8".into(),
            query_text: "read file incrementally with bounded memory line buffer".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/searcher/src/line_buffer.rs".into(),
                        start_line: 294,
                        end_line: 323,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/glue.rs".into(),
                        start_line: 11,
                        end_line: 36,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q9: binary file detection - heuristic binary detection
        RelevanceJudgment {
            id: "G2Q9".into(),
            query_text: "detect and handle binary files during search".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/searcher/src/line_buffer.rs".into(),
                        start_line: 42,
                        end_line: 64,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/mod.rs".into(),
                        start_line: 34,
                        end_line: 42,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q10: filter by file type - extension/type based filtering
        RelevanceJudgment {
            id: "G2Q10".into(),
            query_text: "filter files by extension or type like rust or python".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/ignore/src/types.rs".into(),
                        start_line: 248,
                        end_line: 289,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/ignore/src/types.rs".into(),
                        start_line: 378,
                        end_line: 390,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q11: PCRE2 pattern compilation - advanced regex features
        RelevanceJudgment {
            id: "G2Q11".into(),
            query_text: "compile PCRE2 pattern with advanced flags for lookaround".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/pcre2/src/matcher.rs".into(),
                        start_line: 37,
                        end_line: 85,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/pcre2/src/matcher.rs".into(),
                        start_line: 94,
                        end_line: 117,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q12: format output with line numbers - grep-style output
        RelevanceJudgment {
            id: "G2Q12".into(),
            query_text: "format search output with line numbers and file paths".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/printer/src/standard.rs".into(),
                        start_line: 1172,
                        end_line: 1189,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/printer/src/standard.rs".into(),
                        start_line: 1679,
                        end_line: 1688,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q13: parallel directory traversal - concurrent file walking
        RelevanceJudgment {
            id: "G2Q13".into(),
            query_text: "parallel directory traversal using multiple threads".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/ignore/src/walk.rs".into(),
                        start_line: 1306,
                        end_line: 1325,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/ignore/src/walk.rs".into(),
                        start_line: 1355,
                        end_line: 1365,
                    },
                    RL::Strong,
                ),
            ],
        },
        // G2Q14: parse gitignore format - gitignore file parsing
        RelevanceJudgment {
            id: "G2Q14".into(),
            query_text: "parse gitignore file format with comments and negation".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/ignore/src/gitignore.rs".into(),
                    start_line: 385,
                    end_line: 421,
                },
                RL::Strong,
            )],
        },
        // G2Q15: handle search errors - unified error handling
        RelevanceJudgment {
            id: "G2Q15".into(),
            query_text: "handle search errors gracefully with unified error type".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/searcher/src/searcher/mod.rs".into(),
                        start_line: 238,
                        end_line: 262,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/regex/src/error.rs".into(),
                        start_line: 39,
                        end_line: 61,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q16: extract literals from regex - regex optimization
        RelevanceJudgment {
            id: "G2Q16".into(),
            query_text: "extract literal strings from regex for optimization".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/regex/src/literal.rs".into(),
                    start_line: 54,
                    end_line: 70,
                },
                RL::Strong,
            )],
        },
        // G2Q17: detect hidden files - dotfile detection
        RelevanceJudgment {
            id: "G2Q17".into(),
            query_text: "detect hidden files and directories starting with dot".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/ignore/src/pathutil.rs".into(),
                        start_line: 10,
                        end_line: 19,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/ignore/src/pathutil.rs".into(),
                        start_line: 27,
                        end_line: 45,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q18: override ignore rules - include exclude globs
        RelevanceJudgment {
            id: "G2Q18".into(),
            query_text: "override ignore rules with include or exclude globs".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/ignore/src/overrides.rs".into(),
                        start_line: 119,
                        end_line: 134,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/ignore/src/overrides.rs".into(),
                        start_line: 142,
                        end_line: 145,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q19: count matches and bytes - search statistics
        RelevanceJudgment {
            id: "G2Q19".into(),
            query_text: "count total matches and bytes processed in search".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/printer/src/stats.rs".into(),
                    start_line: 13,
                    end_line: 21,
                },
                RL::Strong,
            )],
        },
        // G2Q20: search stdin stream - stream search support
        RelevanceJudgment {
            id: "G2Q20".into(),
            query_text: "search stdin stream without file path".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/searcher/src/searcher/mod.rs".into(),
                    start_line: 727,
                    end_line: 765,
                },
                RL::Strong,
            )],
        },
        // ==================== FZ: Fuzzy Queries (15 total) ====================
        // Lexical perturbations of G1 sources; same relevant_ranges.

        // FZ-G1Q1: RegexMatcher::find_at
        RelevanceJudgment {
            id: "FZ-G1Q1-naming_case".into(),
            query_text: "findAt in RegexMatcher".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_case".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/regex/src/matcher.rs".into(),
                    start_line: 409,
                    end_line: 421,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q1-abbrev_expand".into(),
            query_text: "find_at in regex matcher".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("abbrev_expand".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/regex/src/matcher.rs".into(),
                    start_line: 409,
                    end_line: 421,
                },
                RL::Strong,
            )],
        },
        // FZ-G1Q2: WalkBuilder::build
        RelevanceJudgment {
            id: "FZ-G1Q2-synonym".into(),
            query_text: "create method on WalkBuilder".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("synonym".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/ignore/src/walk.rs".into(),
                    start_line: 565,
                    end_line: 602,
                },
                RL::Strong,
            )],
        },
        // FZ-G1Q3: GlobSet::is_match
        RelevanceJudgment {
            id: "FZ-G1Q3-naming_case".into(),
            query_text: "isMatch in GlobSet".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_case".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/globset/src/lib.rs".into(),
                    start_line: 342,
                    end_line: 344,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q3-paraphrase".into(),
            query_text: "test whether a path matches in GlobSet".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("paraphrase".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/globset/src/lib.rs".into(),
                    start_line: 342,
                    end_line: 344,
                },
                RL::Strong,
            )],
        },
        // FZ-G1Q4: Matcher trait
        RelevanceJudgment {
            id: "FZ-G1Q4-abbrev_expand".into(),
            query_text: "matcher trait with find_at method".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("abbrev_expand".into()),
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "crates/matcher/src/lib.rs".into(),
                        start_line: 546,
                        end_line: 559,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "crates/matcher/src/lib.rs".into(),
                        start_line: 571,
                        end_line: 575,
                    },
                    RL::Strong,
                ),
            ],
        },
        // FZ-G1Q5: Searcher::search_path
        RelevanceJudgment {
            id: "FZ-G1Q5-naming_case".into(),
            query_text: "searchPath in Searcher".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_case".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/searcher/src/searcher/mod.rs".into(),
                    start_line: 643,
                    end_line: 657,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q5-paraphrase".into(),
            query_text: "search a file path with Searcher".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("paraphrase".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/searcher/src/searcher/mod.rs".into(),
                    start_line: 643,
                    end_line: 657,
                },
                RL::Strong,
            )],
        },
        // FZ-G1Q6: Sink::matched
        RelevanceJudgment {
            id: "FZ-G1Q6-naming_affix".into(),
            query_text: "matched_handler method on Sink".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_affix".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/searcher/src/sink.rs".into(),
                    start_line: 124,
                    end_line: 128,
                },
                RL::Strong,
            )],
        },
        // FZ-G1Q7: TypesBuilder::new
        RelevanceJudgment {
            id: "FZ-G1Q7-naming_affix".into(),
            query_text: "new_builder constructor in TypesBuilder".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_affix".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/ignore/src/types.rs".into(),
                    start_line: 316,
                    end_line: 318,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q7-paraphrase".into(),
            query_text: "construct a fresh TypesBuilder".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("paraphrase".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/ignore/src/types.rs".into(),
                    start_line: 316,
                    end_line: 318,
                },
                RL::Strong,
            )],
        },
        // FZ-G1Q8: GitignoreBuilder::new
        RelevanceJudgment {
            id: "FZ-G1Q8-naming_affix".into(),
            query_text: "new_gitignore constructor in GitignoreBuilder".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_affix".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/ignore/src/gitignore.rs".into(),
                    start_line: 327,
                    end_line: 333,
                },
                RL::Strong,
            )],
        },
        // FZ-G1Q9: Pcre2Matcher::build
        RelevanceJudgment {
            id: "FZ-G1Q9-synonym".into(),
            query_text: "compile method on Pcre2Matcher".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("synonym".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/pcre2/src/matcher.rs".into(),
                    start_line: 37,
                    end_line: 39,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q9-abbrev_expand".into(),
            query_text: "build method on pcre2 matcher".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("abbrev_expand".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/pcre2/src/matcher.rs".into(),
                    start_line: 37,
                    end_line: 39,
                },
                RL::Strong,
            )],
        },
        // FZ-G1Q10: Match::new
        RelevanceJudgment {
            id: "FZ-G1Q10-synonym".into(),
            query_text: "make constructor in Match".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("synonym".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "crates/matcher/src/lib.rs".into(),
                    start_line: 80,
                    end_line: 93,
                },
                RL::Strong,
            )],
        },
    ]
}
