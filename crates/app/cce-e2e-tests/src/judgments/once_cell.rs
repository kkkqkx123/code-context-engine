use crate::bench_data::{QueryType, RelevanceJudgment, RelevanceLevel, SourceRange};

pub fn once_cell_relevance_judgments() -> Vec<RelevanceJudgment> {
    use RelevanceLevel as RL;

    vec![
        // G1: Qualified symbol queries - symbol tokens preserved, wrapped in
        // natural-language structure (e.g. "new constructor in unsync OnceCell").
        // G1Q1: unsync::OnceCell::new constructor
        RelevanceJudgment {
            id: "G1Q1".into(),
            query_text: "new constructor in unsync OnceCell".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 480,
                    end_line: 482,
                },
                RL::Strong,
            )],
        },
        // G1Q2: unsync::OnceCell::get method
        RelevanceJudgment {
            id: "G1Q2".into(),
            query_text: "get method on unsync OnceCell".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 493,
                    end_line: 499,
                },
                RL::Strong,
            )],
        },
        // G1Q3: unsync::OnceCell::set method
        RelevanceJudgment {
            id: "G1Q3".into(),
            query_text: "set method on unsync OnceCell".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 540,
                    end_line: 545,
                },
                RL::Strong,
            )],
        },
        // G1Q4: unsync::OnceCell::get_or_init method
        RelevanceJudgment {
            id: "G1Q4".into(),
            query_text: "get_or_init in unsync OnceCell".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 596,
                    end_line: 605,
                },
                RL::Strong,
            )],
        },
        // G1Q5: sync::OnceCell::new constructor
        RelevanceJudgment {
            id: "G1Q5".into(),
            query_text: "new constructor in sync OnceCell".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 960,
                    end_line: 962,
                },
                RL::Strong,
            )],
        },
        // G1Q6: sync::OnceCell::get_or_try_init method
        RelevanceJudgment {
            id: "G1Q6".into(),
            query_text: "get_or_try_init method on sync OnceCell".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 1162,
                    end_line: 1176,
                },
                RL::Strong,
            )],
        },
        // G1Q7: sync::Lazy::force method
        RelevanceJudgment {
            id: "G1Q7".into(),
            query_text: "force method on sync Lazy".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 1318,
                    end_line: 1323,
                },
                RL::Strong,
            )],
        },
        // G1Q8: race::OnceNonZeroUsize::new constructor
        RelevanceJudgment {
            id: "G1Q8".into(),
            query_text: "new constructor in race OnceNonZeroUsize".into(),
            query_type: QueryType::Qualified,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/race.rs".into(),
                    start_line: 50,
                    end_line: 52,
                },
                RL::Strong,
            )],
        },
        // G2: Semantic queries - Strong (core) + Related (context), single-entity ranges
        // G2Q1: initialize a value only once
        RelevanceJudgment {
            id: "G2Q1".into(),
            query_text: "initialize a value only once".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 596,
                        end_line: 605,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 540,
                        end_line: 545,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1125,
                        end_line: 1134,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1068,
                        end_line: 1073,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 561,
                        end_line: 573,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q2: read stored value from cell
        RelevanceJudgment {
            id: "G2Q2".into(),
            query_text: "read the stored value from cell".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 493,
                        end_line: 499,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 518,
                        end_line: 521,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 973,
                        end_line: 980,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1029,
                        end_line: 1031,
                    },
                    RL::Strong,
                ),
            ],
        },
        // G2Q3: lazy evaluation delayed initialization
        RelevanceJudgment {
            id: "G2Q3".into(),
            query_text: "lazy evaluation delayed initialization".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 724,
                        end_line: 727,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 752,
                        end_line: 754,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1265,
                        end_line: 1268,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1288,
                        end_line: 1290,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 783,
                        end_line: 788,
                    },
                    RL::Related,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1318,
                        end_line: 1323,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q4: consume and extract inner value
        RelevanceJudgment {
            id: "G2Q4".into(),
            query_text: "consume and extract inner value".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 697,
                        end_line: 701,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 677,
                        end_line: 679,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1227,
                        end_line: 1229,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1207,
                        end_line: 1209,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 759,
                        end_line: 765,
                    },
                    RL::Related,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1295,
                        end_line: 1301,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q5: thread synchronization blocking once cell (imp_std)
        RelevanceJudgment {
            id: "G2Q5".into(),
            query_text: "thread synchronization blocking once cell".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/imp_std.rs".into(),
                        start_line: 61,
                        end_line: 85,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_std.rs".into(),
                        start_line: 177,
                        end_line: 208,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_std.rs".into(),
                        start_line: 210,
                        end_line: 239,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_std.rs".into(),
                        start_line: 145,
                        end_line: 168,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q6: parking lot based once cell implementation
        RelevanceJudgment {
            id: "G2Q6".into(),
            query_text: "parking lot based once cell implementation".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/imp_pl.rs".into(),
                        start_line: 45,
                        end_line: 77,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_pl.rs".into(),
                        start_line: 141,
                        end_line: 169,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_pl.rs".into(),
                        start_line: 124,
                        end_line: 137,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q7: critical section no_std once cell
        RelevanceJudgment {
            id: "G2Q7".into(),
            query_text: "no standard library once cell critical section".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/imp_cs.rs".into(),
                        start_line: 44,
                        end_line: 54,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_cs.rs".into(),
                        start_line: 27,
                        end_line: 29,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_cs.rs".into(),
                        start_line: 63,
                        end_line: 67,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_cs.rs".into(),
                        start_line: 8,
                        end_line: 13,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q8: lock-free non-blocking race once cell
        RelevanceJudgment {
            id: "G2Q8".into(),
            query_text: "lock-free non-blocking once cell atomic".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 150,
                        end_line: 157,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 120,
                        end_line: 129,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 199,
                        end_line: 204,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 311,
                        end_line: 317,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 43,
                        end_line: 45,
                    },
                    RL::Related,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 167,
                        end_line: 169,
                    },
                    RL::Related,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 232,
                        end_line: 235,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q9: mutable access get_mut methods
        RelevanceJudgment {
            id: "G2Q9".into(),
            query_text: "mutable reference access once cell".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 518,
                        end_line: 521,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1029,
                        end_line: 1031,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1337,
                        end_line: 1346,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 677,
                        end_line: 679,
                    },
                    RL::Related,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1207,
                        end_line: 1209,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q10: blocking wait for initialization
        RelevanceJudgment {
            id: "G2Q10".into(),
            query_text: "blocking wait for once cell initialization".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1003,
                        end_line: 1011,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_std.rs".into(),
                        start_line: 88,
                        end_line: 90,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/imp_pl.rs".into(),
                        start_line: 80,
                        end_line: 94,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1162,
                        end_line: 1176,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q11: heap allocated once box
        RelevanceJudgment {
            id: "G2Q11".into(),
            query_text: "heap allocated thread safe once box".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 389,
                        end_line: 391,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 399,
                        end_line: 405,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 411,
                        end_line: 424,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 432,
                        end_line: 441,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 361,
                        end_line: 364,
                    },
                    RL::Related,
                ),
            ],
        },
        // G2Q12: pre-initialized with_value constructor
        RelevanceJudgment {
            id: "G2Q12".into(),
            query_text: "create once cell with pre existing value".into(),
            query_type: QueryType::Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 485,
                        end_line: 487,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 965,
                        end_line: 967,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 433,
                        end_line: 437,
                    },
                    RL::Related,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 913,
                        end_line: 917,
                    },
                    RL::Related,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 394,
                        end_line: 396,
                    },
                    RL::Related,
                ),
            ],
        },
        // FZ: Fuzzy queries - artificially perturbed lexical forms of the G1
        // sources. Same relevant_ranges as their source query. Subtype in
        // fuzzy_subtype drives per-perturbation-type analysis.
        RelevanceJudgment {
            id: "FZ-G1Q1-naming_case".into(),
            query_text: "new constructor in unsync once_cell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_case".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 480,
                    end_line: 482,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q2-naming_affix".into(),
            query_text: "get_value method on unsync OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_affix".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 493,
                    end_line: 499,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q2-synonym".into(),
            query_text: "retrieve method on unsync OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("synonym".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 493,
                    end_line: 499,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q3-naming_affix".into(),
            query_text: "set_value method on unsync OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_affix".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 540,
                    end_line: 545,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q3-paraphrase".into(),
            query_text: "write a value into unsync OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("paraphrase".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 540,
                    end_line: 545,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q4-naming_case".into(),
            query_text: "getOrInit in unsync OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_case".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 596,
                    end_line: 605,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q4-paraphrase".into(),
            query_text: "fetch or create the stored value in unsync OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("paraphrase".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 596,
                    end_line: 605,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q5-synonym".into(),
            query_text: "make constructor in sync OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("synonym".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 960,
                    end_line: 962,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q5-abbrev_expand".into(),
            query_text: "new constructor in synchronous OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("abbrev_expand".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 960,
                    end_line: 962,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q6-naming_case".into(),
            query_text: "getOrTryInit method on sync OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_case".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 1162,
                    end_line: 1176,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q6-abbrev_expand".into(),
            query_text: "get_or_try_initialize method on sync OnceCell".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("abbrev_expand".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 1162,
                    end_line: 1176,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q7-synonym".into(),
            query_text: "evaluate method on sync Lazy".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("synonym".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 1318,
                    end_line: 1323,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q7-paraphrase".into(),
            query_text: "trigger computation of sync Lazy".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("paraphrase".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/lib.rs".into(),
                    start_line: 1318,
                    end_line: 1323,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q8-naming_affix".into(),
            query_text: "new_cell constructor in race OnceNonZeroUsize".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("naming_affix".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/race.rs".into(),
                    start_line: 50,
                    end_line: 52,
                },
                RL::Strong,
            )],
        },
        RelevanceJudgment {
            id: "FZ-G1Q8-abbrev_expand".into(),
            query_text: "new constructor in race once non zero usize".into(),
            query_type: QueryType::Fuzzy,
            fuzzy_subtype: Some("abbrev_expand".into()),
            relevant_ranges: vec![(
                SourceRange {
                    file: "src/race.rs".into(),
                    start_line: 50,
                    end_line: 52,
                },
                RL::Strong,
            )],
        },
        // G4: Cross-lang verification queries (kept in once_cell only; real
        // queries are AI-generated and English-primary). Chinese descriptions
        // of the same entities, same ranges as G1/G2 anchors.
        RelevanceJudgment {
            id: "G4Q1".into(),
            query_text: "懒加载的全局变量".into(),
            query_type: QueryType::CrossLang,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 724,
                        end_line: 727,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1265,
                        end_line: 1268,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1318,
                        end_line: 1323,
                    },
                    RL::Strong,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G4Q2".into(),
            query_text: "线程安全的一次初始化".into(),
            query_type: QueryType::CrossLang,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 960,
                        end_line: 962,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1125,
                        end_line: 1134,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1162,
                        end_line: 1176,
                    },
                    RL::Strong,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G4Q3".into(),
            query_text: "获取可能未初始化的值".into(),
            query_type: QueryType::CrossLang,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 493,
                        end_line: 499,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 973,
                        end_line: 980,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/lib.rs".into(),
                        start_line: 1029,
                        end_line: 1031,
                    },
                    RL::Strong,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G4Q4".into(),
            query_text: "延迟初始化的高性能无锁实现".into(),
            query_type: QueryType::CrossLang,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 50,
                        end_line: 52,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 389,
                        end_line: 391,
                    },
                    RL::Strong,
                ),
                (
                    SourceRange {
                        file: "src/race.rs".into(),
                        start_line: 432,
                        end_line: 441,
                    },
                    RL::Strong,
                ),
            ],
        },
    ]
}
