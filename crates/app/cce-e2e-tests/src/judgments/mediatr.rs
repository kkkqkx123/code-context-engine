//! Range-based relevance judgments for the MediatR C# codebase.
//!
//! The query split mirrors the other review fixtures: ten qualified symbol
//! queries, twenty semantic behavior queries and fifteen fuzzy perturbations
//! of the qualified set. Every range is anchored to a concrete implementation
//! in `fixtures/csharp/review/MediatR/src`.

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

pub fn mediatr_relevance_judgments() -> Vec<RelevanceJudgment> {
    use QueryType::{Qualified, Semantic};

    vec![
        // Qualified symbol queries: symbol tokens preserved in natural-language
        // structure (e.g. `Send method on Mediator`).
        strong(
            "G1Q1",
            "Send method on Mediator",
            Qualified,
            "src/MediatR/Mediator.cs",
            50,
            65,
        ),
        strong(
            "G1Q2",
            "Publish method on Mediator",
            Qualified,
            "src/MediatR/Mediator.cs",
            121,
            130,
        ),
        strong(
            "G1Q3",
            "CreateStream in Mediator",
            Qualified,
            "src/MediatR/Mediator.cs",
            163,
            180,
        ),
        strong(
            "G1Q4",
            "Handle method on RequestHandlerWrapperImpl",
            Qualified,
            "src/MediatR/Wrappers/RequestHandlerWrapper.cs",
            34,
            45,
        ),
        strong(
            "G1Q5",
            "Handle in RequestPreProcessorBehavior",
            Qualified,
            "src/MediatR/Pipeline/RequestPreProcessorBehavior.cs",
            20,
            28,
        ),
        strong(
            "G1Q6",
            "Handle in RequestPostProcessorBehavior",
            Qualified,
            "src/MediatR/Pipeline/RequestPostProcessorBehavior.cs",
            20,
            30,
        ),
        strong(
            "G1Q7",
            "AddRequiredServices in ServiceRegistrar",
            Qualified,
            "src/MediatR/Registration/ServiceRegistrar.cs",
            464,
            548,
        ),
        strong(
            "G1Q8",
            "Handle in RequestExceptionProcessorBehavior",
            Qualified,
            "src/MediatR/Pipeline/RequestExceptionProcessorBehavior.cs",
            26,
            76,
        ),
        strong(
            "G1Q9",
            "Handle method on INotificationHandler",
            Qualified,
            "src/MediatR/INotificationHandler.cs",
            10,
            19,
        ),
        strong(
            "G1Q10",
            "Publish in ForeachAwaitPublisher",
            Qualified,
            "src/MediatR/NotificationPublishers/ForeachAwaitPublisher.cs",
            17,
            23,
        ),
        // Semantic behavior queries.
        RelevanceJudgment {
            id: "G2Q1".into(),
            query_text: "resolve the matching request handler and dispatch a request through the pipeline".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 50,
                        end_line: 65,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Wrappers/RequestHandlerWrapper.cs".into(),
                        start_line: 34,
                        end_line: 45,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q2".into(),
            query_text: "route handler invocation for a request that produces no response value".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 67,
                        end_line: 83,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Wrappers/RequestHandlerWrapper.cs".into(),
                        start_line: 55,
                        end_line: 71,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q3".into(),
            query_text: "send a request to its handler via dynamic dispatch without reflection".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 85,
                        end_line: 119,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 50,
                        end_line: 65,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q4".into(),
            query_text: "publish a notification to all registered notification handlers".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 121,
                        end_line: 130,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 150,
                        end_line: 160,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q5".into(),
            query_text: "await each notification handler sequentially one at a time".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/NotificationPublishers/ForeachAwaitPublisher.cs".into(),
                        start_line: 17,
                        end_line: 23,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 147,
                        end_line: 148,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q6".into(),
            query_text: "run notification handlers concurrently and await all of them".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/NotificationPublishers/TaskWhenAllPublisher.cs".into(),
                        start_line: 20,
                        end_line: 27,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 147,
                        end_line: 148,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q7".into(),
            query_text: "create a lazy async stream response from a stream request handler".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 163,
                        end_line: 180,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/IStreamRequestHandler.cs".into(),
                        start_line: 11,
                        end_line: 21,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q8".into(),
            query_text: "wrap the inner handler with pre-processors that run before it".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Pipeline/RequestPreProcessorBehavior.cs".into(),
                        start_line: 20,
                        end_line: 28,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Pipeline/IRequestPreProcessor.cs".into(),
                        start_line: 10,
                        end_line: 19,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q9".into(),
            query_text: "run post-processors after the handler has produced a response".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Pipeline/RequestPostProcessorBehavior.cs".into(),
                        start_line: 20,
                        end_line: 30,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Pipeline/IRequestPostProcessor.cs".into(),
                        start_line: 11,
                        end_line: 21,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q10".into(),
            query_text: "recover from handler exceptions using registered exception handlers".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Pipeline/RequestExceptionProcessorBehavior.cs".into(),
                        start_line: 26,
                        end_line: 76,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Pipeline/RequestExceptionProcessorBehavior.cs".into(),
                        start_line: 86,
                        end_line: 95,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q11".into(),
            query_text: "compose pipeline behaviors from the innermost handler outwards".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Wrappers/RequestHandlerWrapper.cs".into(),
                        start_line: 34,
                        end_line: 45,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Wrappers/RequestHandlerWrapper.cs".into(),
                        start_line: 55,
                        end_line: 71,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q12".into(),
            query_text: "resolve and de-duplicate notification handlers by published type".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Wrappers/NotificationHandlerWrapper.cs".into(),
                        start_line: 20,
                        end_line: 31,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 150,
                        end_line: 160,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q13".into(),
            query_text: "register mediator sender and publisher as the required core services".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Registration/ServiceRegistrar.cs".into(),
                        start_line: 464,
                        end_line: 548,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Registration/ServiceRegistrar.cs".into(),
                        start_line: 44,
                        end_line: 92,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q14".into(),
            query_text: "scan assemblies and register all request and notification handlers".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Registration/ServiceRegistrar.cs".into(),
                        start_line: 44,
                        end_line: 92,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Registration/ServiceRegistrar.cs".into(),
                        start_line: 94,
                        end_line: 170,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q15".into(),
            query_text: "register handlers and mediator types into the service collection".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/MicrosoftExtensionsDI/MediatRServiceCollectionExtensions.cs".into(),
                        start_line: 46,
                        end_line: 61,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Registration/ServiceRegistrar.cs".into(),
                        start_line: 44,
                        end_line: 92,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q16".into(),
            query_text: "resolve a single request handler instance from the service provider".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Wrappers/RequestHandlerWrapper.cs".into(),
                        start_line: 34,
                        end_line: 45,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/IRequestHandler.cs".into(),
                        start_line: 11,
                        end_line: 21,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q17".into(),
            query_text: "define the async continuation contract each pipeline behavior must call".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/IPipelineBehavior.cs".into(),
                        start_line: 12,
                        end_line: 30,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/Wrappers/RequestHandlerWrapper.cs".into(),
                        start_line: 40,
                        end_line: 44,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q18".into(),
            query_text: "represent a request marker that signals a void or typed response".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR.Contracts/IRequest.cs".into(),
                        start_line: 6,
                        end_line: 17,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/IRequestHandler.cs".into(),
                        start_line: 27,
                        end_line: 37,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q19".into(),
            query_text: "synchronously wrap a void notification handler into a task returning handler".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/INotificationHandler.cs".into(),
                        start_line: 25,
                        end_line: 39,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/INotificationHandler.cs".into(),
                        start_line: 10,
                        end_line: 19,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        RelevanceJudgment {
            id: "G2Q20".into(),
            query_text: "delegate the publish work to the configured notification publisher".into(),
            query_type: Semantic,
            fuzzy_subtype: None,
            relevant_ranges: vec![
                (
                    SourceRange {
                        file: "src/MediatR/Mediator.cs".into(),
                        start_line: 147,
                        end_line: 160,
                    },
                    RelevanceLevel::Strong,
                ),
                (
                    SourceRange {
                        file: "src/MediatR/NotificationPublishers/ForeachAwaitPublisher.cs".into(),
                        start_line: 17,
                        end_line: 23,
                    },
                    RelevanceLevel::Related,
                ),
            ],
        },
        // Fuzzy queries: lexical perturbations of the qualified set, same ranges.
        fuzzy_strong(
            "G3Q1",
            "SendRequest method in Mediator",
            "naming_affix",
            "src/MediatR/Mediator.cs",
            50,
            65,
        ),
        fuzzy_strong(
            "G3Q2",
            "SendHandler method on Mediator",
            "naming_case",
            "src/MediatR/Mediator.cs",
            50,
            65,
        ),
        fuzzy_strong(
            "G3Q3",
            "deliver a notification to handlers in Mediator",
            "paraphrase",
            "src/MediatR/Mediator.cs",
            121,
            130,
        ),
        fuzzy_strong(
            "G3Q4",
            "publish notification method in Mediator",
            "abbrev_expand",
            "src/MediatR/Mediator.cs",
            121,
            130,
        ),
        fuzzy_strong(
            "G3Q5",
            "CreateAsyncStream in Mediator",
            "synonym",
            "src/MediatR/Mediator.cs",
            163,
            180,
        ),
        fuzzy_strong(
            "G3Q6",
            "Invoke method on RequestHandlerWrapperImpl",
            "synonym",
            "src/MediatR/Wrappers/RequestHandlerWrapper.cs",
            34,
            45,
        ),
        fuzzy_strong(
            "G3Q7",
            "ProcessHandler in RequestHandlerWrapperImpl",
            "naming_affix",
            "src/MediatR/Wrappers/RequestHandlerWrapper.cs",
            34,
            45,
        ),
        fuzzy_strong(
            "G3Q8",
            "execute preprocessing hooks before handling a request",
            "paraphrase",
            "src/MediatR/Pipeline/RequestPreProcessorBehavior.cs",
            20,
            28,
        ),
        fuzzy_strong(
            "G3Q9",
            "PostProcess in RequestPostProcessorBehavior",
            "synonym",
            "src/MediatR/Pipeline/RequestPostProcessorBehavior.cs",
            20,
            30,
        ),
        fuzzy_strong(
            "G3Q10",
            "addRequiredServices in ServiceRegistrar",
            "naming_case",
            "src/MediatR/Registration/ServiceRegistrar.cs",
            464,
            548,
        ),
        fuzzy_strong(
            "G3Q11",
            "add the required mediator core services",
            "abbrev_expand",
            "src/MediatR/Registration/ServiceRegistrar.cs",
            464,
            548,
        ),
        fuzzy_strong(
            "G3Q12",
            "HandleRequest in RequestExceptionProcessorBehavior",
            "naming_case",
            "src/MediatR/Pipeline/RequestExceptionProcessorBehavior.cs",
            26,
            76,
        ),
        fuzzy_strong(
            "G3Q13",
            "Receive method on INotificationHandler",
            "naming_affix",
            "src/MediatR/INotificationHandler.cs",
            10,
            19,
        ),
        fuzzy_strong(
            "G3Q14",
            "publish notifications via the foreach await publisher",
            "abbrev_expand",
            "src/MediatR/NotificationPublishers/ForeachAwaitPublisher.cs",
            17,
            23,
        ),
        fuzzy_strong(
            "G3Q15",
            "stream request handler contract in MediatR",
            "naming_affix",
            "src/MediatR/IStreamRequestHandler.cs",
            11,
            21,
        ),
    ]
}
