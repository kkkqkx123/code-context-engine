//! Range-based relevance judgments for the Express JavaScript codebase.
//!
//! The query split mirrors the flask benchmark: ten qualified symbol
//! queries, twenty semantic behavior queries and fifteen fuzzy
//! perturbations of the qualified set. Every range is anchored to a
//! concrete implementation in `fixtures/javascript/review/express/lib`.

use crate::bench_data::{QueryType, RelevanceJudgment, RelevanceLevel, SourceRange};

fn strong(
    id: &str,
    query_text: &str,
    query_type: QueryType,
    file: &str,
    start_line: usize,
    end_line: usize,
) -> RelevanceJudgment {
    RelevanceJudgment {
        id: id.into(),
        query_text: query_text.into(),
        query_type,
        fuzzy_subtype: None,
        relevant_ranges: vec![(
            SourceRange {
                file: file.into(),
                start_line,
                end_line,
            },
            RelevanceLevel::Strong,
        )],
    }
}

fn fuzzy_strong(
    id: &str,
    query_text: &str,
    subtype: &str,
    file: &str,
    start_line: usize,
    end_line: usize,
) -> RelevanceJudgment {
    RelevanceJudgment {
        id: id.into(),
        query_text: query_text.into(),
        query_type: QueryType::Fuzzy,
        fuzzy_subtype: Some(subtype.into()),
        relevant_ranges: vec![(
            SourceRange {
                file: file.into(),
                start_line,
                end_line,
            },
            RelevanceLevel::Strong,
        )],
    }
}

pub fn express_relevance_judgments() -> Vec<RelevanceJudgment> {
    use QueryType::{Qualified, Semantic};

    vec![
        // Qualified symbol queries: symbol tokens preserved in
        // natural-language structure (e.g. `app.use method in Express`).
        strong(
            "G1Q1",
            "app.use method in Express",
            Qualified,
            "lib/application.js",
            190,
            244,
        ),
        strong(
            "G1Q2",
            "handle method in Express app",
            Qualified,
            "lib/application.js",
            152,
            178,
        ),
        strong(
            "G1Q3",
            "send method on Express response",
            Qualified,
            "lib/response.js",
            126,
            220,
        ),
        strong(
            "G1Q4",
            "json method on Express response",
            Qualified,
            "lib/response.js",
            234,
            248,
        ),
        strong(
            "G1Q5",
            "get method on Express request",
            Qualified,
            "lib/request.js",
            63,
            83,
        ),
        strong(
            "G1Q6",
            "createApplication factory in express",
            Qualified,
            "lib/express.js",
            36,
            56,
        ),
        strong(
            "G1Q7",
            "render method on Express app",
            Qualified,
            "lib/application.js",
            522,
            575,
        ),
        strong(
            "G1Q8",
            "View constructor in Express",
            Qualified,
            "lib/view.js",
            52,
            95,
        ),
        strong(
            "G1Q9",
            "cookie method on Express response",
            Qualified,
            "lib/response.js",
            745,
            778,
        ),
        strong(
            "G1Q10",
            "listen method on Express app",
            Qualified,
            "lib/application.js",
            598,
            606,
        ),
        // Semantic behavior queries.
        RelevanceJudgment {
            id: "G2Q1".into(),
            query_text: "dispatch an incoming req and res pair through the middleware pipeline"
                .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 152,
                        end_line: 178,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 190,
                        end_line: 244,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q2".into(),
            query_text: "attach a middleware function to the application router at a mount path"
                .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 190,
                        end_line: 244,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 256,
                        end_line: 258,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q3".into(),
            query_text: "create a route for a path and register chained HTTP method handlers"
                .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 256,
                        end_line: 258,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 471,
                        end_line: 482,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q4".into(),
            query_text:
                "assign and read back application settings and triggers compiled option callbacks"
                    .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 351,
                        end_line: 383,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 420,
                        end_line: 422,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q5".into(),
            query_text:
                "register a callback invoked for a route parameter, supporting arrays of names"
                    .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "lib/application.js".into(),
                    start_line: 322,
                    end_line: 334,
                },
                RelevanceLevel::Strong,
            )],
        },
        strong(
            "G2Q6",
            "register a template engine callback for a file extension",
            Semantic,
            "lib/application.js",
            294,
            308,
        ),
        RelevanceJudgment {
            id: "G2Q7".into(),
            query_text: "turn a view name and options into a rendered template string".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 522,
                        end_line: 575,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 625,
                        end_line: 631,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q8".into(),
            query_text:
                "write the HTTP response body applying content type length and etag handling".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "lib/response.js".into(),
                    start_line: 126,
                    end_line: 220,
                },
                RelevanceLevel::Strong,
            )],
        },
        strong(
            "G2Q9",
            "send a JavaScript object to the client as a JSON response",
            Semantic,
            "lib/response.js",
            234,
            248,
        ),
        strong(
            "G2Q10",
            "send a JSON response wrapped in a JSONP callback name from the query",
            Semantic,
            "lib/response.js",
            262,
            306,
        ),
        RelevanceJudgment {
            id: "G2Q11".into(),
            query_text: "stream a file from disk while setting content-type and cache headers"
                .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/response.js".into(),
                        start_line: 373,
                        end_line: 415,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/response.js".into(),
                        start_line: 921,
                        end_line: 1009,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q12".into(),
            query_text: "set or clear an http cookie with signing and expiry options".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/response.js".into(),
                        start_line: 745,
                        end_line: 778,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/response.js".into(),
                        start_line: 712,
                        end_line: 719,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        strong(
            "G2Q13",
            "redirect the client to a location using a configurable status code",
            Semantic,
            "lib/response.js",
            812,
            864,
        ),
        strong(
            "G2Q14",
            "return an incoming request header, special-casing referrer and referer",
            Semantic,
            "lib/request.js",
            63,
            83,
        ),
        strong(
            "G2Q15",
            "match the request accept headers against the supported content types",
            Semantic,
            "lib/request.js",
            127,
            130,
        ),
        RelevanceJudgment {
            id: "G2Q16".into(),
            query_text:
                "parse the raw query string of the url using the configured value for query parser"
                    .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/request.js".into(),
                        start_line: 230,
                        end_line: 241,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/utils.js".into(),
                        start_line: 162,
                        end_line: 184,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q17".into(),
            query_text:
                "determine the originating client ip address when running behind a trusted proxy"
                    .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/request.js".into(),
                        start_line: 340,
                        end_line: 343,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/request.js".into(),
                        start_line: 357,
                        end_line: 366,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q18".into(),
            query_text:
                "look up and resolve a template view file by name across its root directories"
                    .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/view.js".into(),
                        start_line: 104,
                        end_line: 123,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/view.js".into(),
                        start_line: 169,
                        end_line: 187,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q19".into(),
            query_text:
                "initialize the application with default settings, locals and a lazy router".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 59,
                        end_line: 83,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "lib/application.js".into(),
                        start_line: 90,
                        end_line: 141,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        strong(
            "G2Q20",
            "compile the etag setting value into a response tag generating function",
            Semantic,
            "lib/utils.js",
            130,
            152,
        ),
        // Fuzzy queries: lexical perturbations of the qualified set, same
        // ranges. FZ-G1Q<n> corresponds to qualified query G1Q<n>.
        // FZ-G1Q1: app.use
        fuzzy_strong(
            "FZ-G1Q1-naming_affix",
            "use_handler method on Express app",
            "naming_affix",
            "lib/application.js",
            190,
            244,
        ),
        fuzzy_strong(
            "FZ-G1Q1-paraphrase",
            "add middleware functions to the app router",
            "paraphrase",
            "lib/application.js",
            190,
            244,
        ),
        // FZ-G1Q2: app.handle
        fuzzy_strong(
            "FZ-G1Q2-naming_case",
            "appHandle in Express",
            "naming_case",
            "lib/application.js",
            152,
            178,
        ),
        fuzzy_strong(
            "FZ-G1Q2-synonym",
            "process_request in Express",
            "synonym",
            "lib/application.js",
            152,
            178,
        ),
        // FZ-G1Q3: res.send
        fuzzy_strong(
            "FZ-G1Q3-naming_case",
            "sendResponse method on the response",
            "naming_case",
            "lib/response.js",
            126,
            220,
        ),
        fuzzy_strong(
            "FZ-G1Q3-paraphrase",
            "write the response body back to the client",
            "paraphrase",
            "lib/response.js",
            126,
            220,
        ),
        // FZ-G1Q4: res.json
        fuzzy_strong(
            "FZ-G1Q4-naming_affix",
            "jsonify method on Express response",
            "naming_affix",
            "lib/response.js",
            234,
            248,
        ),
        // FZ-G1Q5: req.get
        fuzzy_strong(
            "FZ-G1Q5-naming_case",
            "reqGet in Express",
            "naming_case",
            "lib/request.js",
            63,
            83,
        ),
        fuzzy_strong(
            "FZ-G1Q5-paraphrase",
            "fetch a request header by name",
            "paraphrase",
            "lib/request.js",
            63,
            83,
        ),
        // FZ-G1Q6: createApplication
        fuzzy_strong(
            "FZ-G1Q6-naming_affix",
            "create_application factory in express",
            "naming_affix",
            "lib/express.js",
            36,
            56,
        ),
        fuzzy_strong(
            "FZ-G1Q6-paraphrase",
            "build a new express application",
            "paraphrase",
            "lib/express.js",
            36,
            56,
        ),
        // FZ-G1Q7: app.render
        fuzzy_strong(
            "FZ-G1Q7-abbrev_expand",
            "renders a template view in the Express app",
            "abbrev_expand",
            "lib/application.js",
            522,
            575,
        ),
        // FZ-G1Q8: View constructor
        fuzzy_strong(
            "FZ-G1Q8-naming_case",
            "View class constructor in Express",
            "naming_case",
            "lib/view.js",
            52,
            95,
        ),
        // FZ-G1Q9: res.cookie
        fuzzy_strong(
            "FZ-G1Q9-synonym",
            "set_cookie in Express",
            "synonym",
            "lib/response.js",
            745,
            778,
        ),
        // FZ-G1Q10: app.listen
        fuzzy_strong(
            "FZ-G1Q10-abbrev_expand",
            "start listening for connections on the http server",
            "abbrev_expand",
            "lib/application.js",
            598,
            606,
        ),
    ]
}
