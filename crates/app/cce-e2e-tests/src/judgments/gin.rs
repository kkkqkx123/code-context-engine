//! Range-based relevance judgments for the Go gin web framework.
//!
//! The query split mirrors the Flask benchmark: ten qualified symbol queries,
//! twenty semantic behavior queries and fifteen fuzzy perturbations of the
//! qualified set. Every range is anchored to a concrete implementation in
//! `fixtures/go/review/gin` (the gin HTTP framework), with source files laid
//! out directly under the fixture root (e.g. `context.go`, `gin.go`,
//! `routergroup.go`, `tree.go`).

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

pub fn gin_relevance_judgments() -> Vec<RelevanceJudgment> {
    use QueryType::{Qualified, Semantic};

    vec![
        // Qualified symbol queries: gin symbol tokens preserved in
        // natural-language structure (e.g. `JSON method on Context`).
        strong(
            "G1Q1",
            "JSON method on Context",
            Qualified,
            "context.go",
            1255,
            1257,
        ),
        strong(
            "G1Q2",
            "handleHTTPRequest in Engine",
            Qualified,
            "gin.go",
            690,
            759,
        ),
        strong("G1Q3", "ServeHTTP in Engine", Qualified, "gin.go", 662, 672),
        strong(
            "G1Q4",
            "addRoute method on node",
            Qualified,
            "tree.go",
            135,
            247,
        ),
        strong(
            "G1Q5",
            "GET in RouterGroup",
            Qualified,
            "routergroup.go",
            116,
            118,
        ),
        strong(
            "G1Q6",
            "Use method in RouterGroup",
            Qualified,
            "routergroup.go",
            65,
            68,
        ),
        strong(
            "G1Q7",
            "Group in RouterGroup",
            Qualified,
            "routergroup.go",
            72,
            78,
        ),
        strong(
            "G1Q8",
            "Render method on Context",
            Qualified,
            "context.go",
            1202,
            1216,
        ),
        strong(
            "G1Q9",
            "ShouldBindJSON in Context",
            Qualified,
            "context.go",
            890,
            892,
        ),
        strong(
            "G1Q10",
            "Param method on Context",
            Qualified,
            "context.go",
            513,
            515,
        ),
        // Semantic behavior queries.
        RelevanceJudgment {
            id: "G2Q1".into(),
            query_text: "dispatch an incoming HTTP request to its matching handler".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "gin.go".into(),
                        start_line: 690,
                        end_line: 759,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "gin.go".into(),
                        start_line: 662,
                        end_line: 672,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q2".into(),
            query_text: "find the registered handler for a URL path in the route tree".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "tree.go".into(),
                        start_line: 418,
                        end_line: 668,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "tree.go".into(),
                        start_line: 135,
                        end_line: 247,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q3".into(),
            query_text: "insert a new route and its handler into the radix tree".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "tree.go".into(),
                    start_line: 135,
                    end_line: 247,
                },
                RelevanceLevel::Strong,
            )],
        },
        RelevanceJudgment {
            id: "G2Q4".into(),
            query_text: "serialize a struct as a JSON response body".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "render/json.go".into(),
                    start_line: 57,
                    end_line: 75,
                },
                RelevanceLevel::Strong,
            )],
        },
        RelevanceJudgment {
            id: "G2Q5".into(),
            query_text: "set the JSON content type on the response writer".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![(
                SourceRange {
                    file: "render/json.go".into(),
                    start_line: 62,
                    end_line: 64,
                },
                RelevanceLevel::Strong,
            )],
        },
        RelevanceJudgment {
            id: "G2Q6".into(),
            query_text: "decode an incoming request body into a bound struct".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "context.go".into(),
                        start_line: 942,
                        end_line: 944,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "context.go".into(),
                        start_line: 861,
                        end_line: 864,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        strong(
            "G2Q7",
            "read a query string value with a fallback default",
            Semantic,
            "context.go",
            548,
            553,
        ),
        RelevanceJudgment {
            id: "G2Q8".into(),
            query_text: "run the pending handlers in a middleware chain".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "context.go".into(),
                        start_line: 198,
                        end_line: 206,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "context.go".into(),
                        start_line: 217,
                        end_line: 222,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        strong(
            "G2Q9",
            "stop further handlers from being executed for a request",
            Semantic,
            "context.go",
            217,
            219,
        ),
        RelevanceJudgment {
            id: "G2Q10".into(),
            query_text: "attach middleware handlers to a router group".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "routergroup.go".into(),
                        start_line: 65,
                        end_line: 71,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "routergroup.go".into(),
                        start_line: 241,
                        end_line: 248,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        strong(
            "G2Q11",
            "create a nested router group sharing a path prefix",
            Semantic,
            "routergroup.go",
            72,
            78,
        ),
        RelevanceJudgment {
            id: "G2Q12".into(),
            query_text: "register a handler for HTTP GET requests".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "routergroup.go".into(),
                        start_line: 116,
                        end_line: 118,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "routergroup.go".into(),
                        start_line: 86,
                        end_line: 97,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q13".into(),
            query_text: "register a route and its handlers on the engine route trees".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "gin.go".into(),
                        start_line: 364,
                        end_line: 386,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "tree.go".into(),
                        start_line: 135,
                        end_line: 247,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        strong(
            "G2Q14",
            "execute an HTML template and render it into the response",
            Semantic,
            "context.go",
            1221,
            1224,
        ),
        strong(
            "G2Q15",
            "write a formatted string into the response body",
            Semantic,
            "context.go",
            1304,
            1306,
        ),
        strong(
            "G2Q16",
            "set the HTTP status code for the response",
            Semantic,
            "context.go",
            1123,
            1125,
        ),
        strong(
            "G2Q17",
            "serialize accumulated request errors as JSON",
            Semantic,
            "errors.go",
            77,
            79,
        ),
        strong(
            "G2Q18",
            "pick the right binding engine from the HTTP method and content type",
            Semantic,
            "binding/binding.go",
            95,
            120,
        ),
        strong(
            "G2Q19",
            "write response headers and body bytes through the response writer",
            Semantic,
            "response_writer.go",
            84,
            89,
        ),
        strong(
            "G2Q20",
            "write a plain text server error response for unmatched routes",
            Semantic,
            "gin.go",
            764,
            779,
        ),
        // Fuzzy queries: lexical perturbations of the qualified set, same ranges.
        // FZ-G1Q1: Context::JSON
        fuzzy_strong(
            "G3Q1",
            "Json method in Context",
            "naming_case",
            "context.go",
            1255,
            1257,
        ),
        // FZ-G1Q1: Context::JSON
        fuzzy_strong(
            "G3Q2",
            "render response on Context",
            "synonym",
            "context.go",
            1202,
            1216,
        ),
        // FZ-G1Q2: Engine::handleHTTPRequest
        fuzzy_strong(
            "G3Q3",
            "dispatch_request helper in Engine",
            "naming_affix",
            "gin.go",
            690,
            759,
        ),
        fuzzy_strong(
            "G3Q4",
            "route an incoming HTTP request to a handler function",
            "paraphrase",
            "gin.go",
            690,
            759,
        ),
        // FZ-G1Q3: Engine::ServeHTTP
        fuzzy_strong(
            "G3Q5",
            "serveHTTP in Engine",
            "naming_case",
            "gin.go",
            662,
            672,
        ),
        // FZ-G1Q4: node::addRoute
        fuzzy_strong(
            "G3Q6",
            "add_route method on node",
            "naming_affix",
            "tree.go",
            135,
            247,
        ),
        fuzzy_strong(
            "G3Q7",
            "tree lookup for a URL path",
            "synonym",
            "tree.go",
            418,
            668,
        ),
        fuzzy_strong(
            "G3Q8",
            "find the handler registered for a path in the routing tree",
            "paraphrase",
            "tree.go",
            418,
            668,
        ),
        // FZ-G1Q5: RouterGroup::GET
        fuzzy_strong(
            "G3Q9",
            "get method on RouterGroup",
            "naming_case",
            "routergroup.go",
            116,
            118,
        ),
        fuzzy_strong(
            "G3Q10",
            "register a GET route in RouterGroup",
            "abbrev_expand",
            "routergroup.go",
            116,
            118,
        ),
        // FZ-G1Q9: Context::ShouldBindJSON
        fuzzy_strong(
            "G3Q11",
            "ShouldBindJSON helper in Context",
            "naming_case",
            "context.go",
            890,
            892,
        ),
        fuzzy_strong(
            "G3Q12",
            "bind_json function on Context",
            "naming_affix",
            "context.go",
            890,
            892,
        ),
        fuzzy_strong(
            "G3Q13",
            "decode a JSON request body into a struct",
            "paraphrase",
            "context.go",
            890,
            892,
        ),
        // FZ-G1Q10: Context::Param
        fuzzy_strong(
            "G3Q14",
            "look up a URL path parameter by name",
            "synonym",
            "context.go",
            513,
            515,
        ),
        fuzzy_strong(
            "G3Q15",
            "param_value accessor on Context",
            "naming_affix",
            "context.go",
            513,
            515,
        ),
    ]
}
