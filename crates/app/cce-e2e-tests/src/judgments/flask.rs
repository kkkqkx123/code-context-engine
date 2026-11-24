//! Range-based relevance judgments for the Flask Python codebase.
//!
//! The query split mirrors the ripgrep parameter benchmark: ten qualified
//! symbol queries, twenty semantic behavior queries and fifteen fuzzy
//! perturbations of the qualified set. Every range is anchored to a concrete
//! implementation in `fixtures/python/review/flask`.

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

pub fn flask_relevance_judgments() -> Vec<RelevanceJudgment> {
    use QueryType::{Qualified, Semantic};

    vec![
        // Qualified symbol queries: symbol tokens preserved in natural-language
        // structure (e.g. `wsgi_app method in Flask`).
        strong(
            "G1Q1",
            "wsgi_app method in Flask",
            Qualified,
            "src/flask/app.py",
            1566,
            1616,
        ),
        strong(
            "G1Q2",
            "dispatch_request in Flask",
            Qualified,
            "src/flask/app.py",
            966,
            990,
        ),
        strong(
            "G1Q3",
            "make_response method on Flask",
            Qualified,
            "src/flask/app.py",
            1224,
            1364,
        ),
        strong(
            "G1Q4",
            "url_for in Flask",
            Qualified,
            "src/flask/app.py",
            1102,
            1222,
        ),
        strong(
            "G1Q5",
            "add_url_rule in App",
            Qualified,
            "src/flask/sansio/app.py",
            605,
            661,
        ),
        strong(
            "G1Q6",
            "register_blueprint method on App",
            Qualified,
            "src/flask/sansio/app.py",
            570,
            595,
        ),
        strong(
            "G1Q7",
            "register in Blueprint",
            Qualified,
            "src/flask/sansio/blueprints.py",
            273,
            377,
        ),
        strong(
            "G1Q8",
            "push method on AppContext",
            Qualified,
            "src/flask/ctx.py",
            416,
            444,
        ),
        strong(
            "G1Q9",
            "session_transaction in FlaskClient",
            Qualified,
            "src/flask/testing.py",
            136,
            183,
        ),
        strong(
            "G1Q10",
            "dumps method on DefaultJSONProvider",
            Qualified,
            "src/flask/json/provider.py",
            166,
            179,
        ),
        // Semantic behavior queries.
        RelevanceJudgment {
            id: "G2Q1".into(),
            query_text: "dispatch an incoming request to its matching view function".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 966,
                        end_line: 990,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 945,
                        end_line: 964,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q2".into(),
            query_text: "run request preprocessing routing and response finalization".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 992,
                        end_line: 1019,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 966,
                        end_line: 990,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q3".into(),
            query_text: "convert a view return value into an HTTP response".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 1224,
                        end_line: 1364,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 1190,
                        end_line: 1222,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q4".into(),
            query_text: "build a URL for endpoint values and query parameters".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 1102,
                        end_line: 1222,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 1050,
                        end_line: 1100,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q5".into(),
            query_text: "run before request handlers and allow an early response".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 1366,
                        end_line: 1392,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/flask/app.py".into(),
                        start_line: 1340,
                        end_line: 1364,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        strong(
            "G2Q6",
            "run after request callbacks and save the session",
            Semantic,
            "src/flask/app.py",
            1394,
            1418,
        ),
        strong(
            "G2Q7",
            "handle an unhandled application exception as a 500 response",
            Semantic,
            "src/flask/app.py",
            897,
            948,
        ),
        strong(
            "G2Q8",
            "create and clean up the WSGI request context",
            Semantic,
            "src/flask/app.py",
            1566,
            1616,
        ),
        strong(
            "G2Q9",
            "register URL routing methods and view functions",
            Semantic,
            "src/flask/sansio/app.py",
            605,
            661,
        ),
        strong(
            "G2Q10",
            "register a blueprint with application route defaults",
            Semantic,
            "src/flask/sansio/app.py",
            570,
            595,
        ),
        strong(
            "G2Q11",
            "merge a blueprint routes callbacks and nested blueprints into an app",
            Semantic,
            "src/flask/sansio/blueprints.py",
            273,
            377,
        ),
        strong(
            "G2Q12",
            "find the most specific application or blueprint error handler",
            Semantic,
            "src/flask/sansio/app.py",
            868,
            891,
        ),
        strong(
            "G2Q13",
            "decide whether an HTTP exception should be trapped for debugging",
            Semantic,
            "src/flask/sansio/app.py",
            893,
            926,
        ),
        strong(
            "G2Q14",
            "serialize Python data as a JSON response with Flask defaults",
            Semantic,
            "src/flask/json/provider.py",
            189,
            215,
        ),
        strong(
            "G2Q15",
            "open modify and save a test client session transaction",
            Semantic,
            "src/flask/testing.py",
            136,
            183,
        ),
        RelevanceJudgment {
            id: "G2Q16".into(),
            query_text: "push a request context and match the incoming URL".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/flask/ctx.py".into(),
                        start_line: 405,
                        end_line: 414,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/flask/ctx.py".into(),
                        start_line: 416,
                        end_line: 444,
                    },
                    RelevanceLevel::Strong,
                ),
            ],
        },
        strong(
            "G2Q17",
            "serve a trusted file download using conditional response headers",
            Semantic,
            "src/flask/helpers.py",
            417,
            540,
        ),
        strong(
            "G2Q18",
            "store a message in the session for the next request",
            Semantic,
            "src/flask/helpers.py",
            326,
            357,
        ),
        strong(
            "G2Q19",
            "return flashed session messages filtered by category",
            Semantic,
            "src/flask/helpers.py",
            360,
            399,
        ),
        strong(
            "G2Q20",
            "turn malformed JSON loading errors into bad request responses",
            Semantic,
            "src/flask/wrappers.py",
            212,
            219,
        ),
        // Fuzzy queries: lexical perturbations of the qualified set, same ranges.
        // FZ-G1Q1: Flask::wsgi_app
        fuzzy_strong(
            "FZ-G1Q1-naming_affix",
            "wsgi_app_entry method in Flask",
            "naming_affix",
            "src/flask/app.py",
            1566,
            1616,
        ),
        fuzzy_strong(
            "FZ-G1Q1-paraphrase",
            "handle an incoming WSGI request in Flask",
            "paraphrase",
            "src/flask/app.py",
            1566,
            1616,
        ),
        // FZ-G1Q2: Flask::dispatch_request
        fuzzy_strong(
            "FZ-G1Q2-naming_case",
            "dispatchRequest in Flask",
            "naming_case",
            "src/flask/app.py",
            966,
            990,
        ),
        fuzzy_strong(
            "FZ-G1Q2-synonym",
            "route_request in Flask",
            "synonym",
            "src/flask/app.py",
            966,
            990,
        ),
        // FZ-G1Q3: Flask::make_response
        fuzzy_strong(
            "FZ-G1Q3-naming_case",
            "makeResponse method on Flask",
            "naming_case",
            "src/flask/app.py",
            1224,
            1364,
        ),
        // FZ-G1Q4: Flask::url_for
        fuzzy_strong(
            "FZ-G1Q4-paraphrase",
            "generate URL strings for endpoints in Flask",
            "paraphrase",
            "src/flask/app.py",
            1102,
            1222,
        ),
        // FZ-G1Q5: App::add_url_rule
        fuzzy_strong(
            "FZ-G1Q5-naming_case",
            "addUrlRule in App",
            "naming_case",
            "src/flask/sansio/app.py",
            605,
            661,
        ),
        // FZ-G1Q6: App::register_blueprint
        fuzzy_strong(
            "FZ-G1Q6-synonym",
            "attach_blueprint method on App",
            "synonym",
            "src/flask/sansio/app.py",
            570,
            595,
        ),
        fuzzy_strong(
            "FZ-G1Q6-abbrev_expand",
            "register blueprint in App",
            "abbrev_expand",
            "src/flask/sansio/app.py",
            570,
            595,
        ),
        // FZ-G1Q7: Blueprint::register
        fuzzy_strong(
            "FZ-G1Q7-synonym",
            "attach in Blueprint",
            "synonym",
            "src/flask/sansio/blueprints.py",
            273,
            377,
        ),
        // FZ-G1Q8: AppContext::push
        fuzzy_strong(
            "FZ-G1Q8-naming_affix",
            "push_context method on AppContext",
            "naming_affix",
            "src/flask/ctx.py",
            416,
            444,
        ),
        // FZ-G1Q9: FlaskClient::session_transaction
        fuzzy_strong(
            "FZ-G1Q9-paraphrase",
            "create a transactional session in FlaskClient",
            "paraphrase",
            "src/flask/testing.py",
            136,
            183,
        ),
        fuzzy_strong(
            "FZ-G1Q9-abbrev_expand",
            "session transaction in FlaskClient",
            "abbrev_expand",
            "src/flask/testing.py",
            136,
            183,
        ),
        // FZ-G1Q10: DefaultJSONProvider::dumps
        fuzzy_strong(
            "FZ-G1Q10-naming_affix",
            "dump_json method on DefaultJSONProvider",
            "naming_affix",
            "src/flask/json/provider.py",
            166,
            179,
        ),
        fuzzy_strong(
            "FZ-G1Q10-abbrev_expand",
            "dumps method on default JSON provider",
            "abbrev_expand",
            "src/flask/json/provider.py",
            166,
            179,
        ),
    ]
}
