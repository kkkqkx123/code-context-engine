//! Range-based relevance judgments for the Java Spring Boot minimal demo.
//!
//! The fixture `fixtures/java/review/springboot-minimal-demo` is a small REST
//! demo, so the judgment set is scaled down relative to the Flask benchmark:
//! seven qualified symbol queries, six semantic behavior queries and five
//! fuzzy perturbations of the qualified set, all anchored to concrete method
//! bodies in the fixture.

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

pub fn spring_boot_relevance_judgments() -> Vec<RelevanceJudgment> {
    use QueryType::{Qualified, Semantic};

    let c = "src/main/java/com/example/demo/controller/UserApiController.java";
    let s = "src/main/java/com/example/demo/service/UserService.java";
    let r = "src/main/java/com/example/demo/repository/UserRepository.java";
    let e = "src/main/java/com/example/demo/entity/User.java";
    let d = "src/main/java/com/example/demo/dto/UserResponse.java";
    let a = "src/main/java/com/example/demo/Application.java";

    vec![
        // Qualified symbol queries: symbol tokens preserved in natural-language
        // structure (e.g. `getAllUsers method in UserApiController`).
        strong(
            "G1Q1",
            "getAllUsers method in UserApiController",
            Qualified,
            c,
            31,
            38,
        ),
        strong(
            "G1Q2",
            "createUser endpoint in UserApiController",
            Qualified,
            c,
            48,
            52,
        ),
        strong(
            "G1Q3",
            "searchByKeyword method in UserService",
            Qualified,
            s,
            57,
            59,
        ),
        strong(
            "G1Q4",
            "updateUser transactional method in UserService",
            Qualified,
            s,
            41,
            50,
        ),
        strong(
            "G1Q5",
            "searchByName in UserRepository",
            Qualified,
            r,
            21,
            21,
        ),
        strong(
            "G1Q6",
            "setName method on User entity",
            Qualified,
            e,
            45,
            47,
        ),
        strong(
            "G1Q7",
            "fromEntity static method on UserResponse",
            Qualified,
            d,
            22,
            29,
        ),
        // Semantic behavior queries.
        RelevanceJudgment {
            id: "G2Q1".into(),
            query_text: "list every stored user and map each entity into a response DTO".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: c.into(),
                        start_line: 31,
                        end_line: 38,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: s.into(),
                        start_line: 22,
                        end_line: 24,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q2".into(),
            query_text: "find a single user by id and answer with 404 when missing".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: c.into(),
                        start_line: 41,
                        end_line: 45,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: s.into(),
                        start_line: 26,
                        end_line: 28,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q3".into(),
            query_text: "persist a new user from the request payload and return a created status"
                .into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: s.into(),
                        start_line: 35,
                        end_line: 38,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: c.into(),
                        start_line: 48,
                        end_line: 52,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q4".into(),
            query_text: "expand a JPQL keyword into a SQL LIKE filter over the name column".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: r.into(),
                        start_line: 20,
                        end_line: 21,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: s.into(),
                        start_line: 57,
                        end_line: 59,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q5".into(),
            query_text: "delete a user by primary key and complete the transaction".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: s.into(),
                        start_line: 53,
                        end_line: 55,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: c.into(),
                        start_line: 55,
                        end_line: 58,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        strong(
            "G2Q6",
            "copy the entity fields into an immutable response DTO",
            Semantic,
            d,
            22,
            29,
        ),
        // Fuzzy queries: lexical perturbations of the qualified set, same ranges.
        // FZ-G1Q1: UserApiController::getAllUsers
        fuzzy_strong(
            "FZ-G1Q1-naming_affix",
            "run_all_users method in UserApiController",
            "naming_affix",
            c,
            31,
            38,
        ),
        // FZ-G1Q2: UserService::searchByKeyword
        fuzzy_strong(
            "FZ-G1Q2-synonym",
            "lookupByKeyword method in UserService",
            "synonym",
            s,
            57,
            59,
        ),
        // FZ-G1Q3: UserRepository::searchByName
        fuzzy_strong(
            "FZ-G1Q3-paraphrase",
            "query users whose name matches a partial text",
            "paraphrase",
            r,
            20,
            21,
        ),
        // FZ-G1Q4: User entity setters
        fuzzy_strong(
            "FZ-G1Q4-abbrev_expand",
            "setters method on user entity",
            "abbrev_expand",
            e,
            37,
            63,
        ),
        // FZ-G1Q5: Application::main
        fuzzy_strong(
            "FZ-G1Q5-naming_case",
            "main() in Application",
            "naming_case",
            a,
            9,
            11,
        ),
    ]
}
