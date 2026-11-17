//! Entity commands: detail, call relations, class relations and classification.
//!
//! The entity panel works on stable symbol ids it obtains from the entity
//! search, from the graph panel, or by pasting one directly.

use cce_api::models::{
    CallChainDirectionParams, CallChainQueryParams, CallChainResponse, CallPathQuery,
    CallPathResponse, ClassImplementationsResponse, ClassInheritanceResponse,
    ClassificationStatsResponse, FunctionCallersResponse, FunctionCallsResponse,
    FunctionDetailResponse,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

/// Call chain directions the panel offers.
pub const CHAIN_DIRECTIONS: [&str; 2] = ["down", "up"];

pub fn detail(
    cx: &App,
    client: Client,
    project_id: i64,
    function_id: String,
) -> Task<Result<FunctionDetailResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.function_detail(project_id, &function_id).await
    })
}

pub fn calls(
    cx: &App,
    client: Client,
    project_id: i64,
    function_id: String,
    max_depth: usize,
    limit: usize,
) -> Task<Result<FunctionCallsResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let params = chain_params(max_depth, limit);
        client
            .function_calls(project_id, &function_id, &params)
            .await
    })
}

pub fn callers(
    cx: &App,
    client: Client,
    project_id: i64,
    function_id: String,
    max_depth: usize,
    limit: usize,
) -> Task<Result<FunctionCallersResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let params = chain_params(max_depth, limit);
        client
            .function_callers(project_id, &function_id, &params)
            .await
    })
}

pub fn chain(
    cx: &App,
    client: Client,
    project_id: i64,
    function_id: String,
    direction: String,
    max_depth: usize,
    limit: usize,
) -> Task<Result<CallChainResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let params = CallChainDirectionParams {
            direction,
            max_depth,
            offset: None,
            limit,
            exclude_tests: None,
            directory_prefix: None,
            excluded_files: None,
            domains: None,
            include_external: true,
        };
        client.call_chain(project_id, &function_id, &params).await
    })
}

pub fn call_path(
    cx: &App,
    client: Client,
    project_id: i64,
    start: String,
    end: String,
    max_depth: usize,
) -> Task<Result<CallPathResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let query = CallPathQuery {
            start_id: start,
            end_id: end,
            max_depth,
        };
        client.call_path(project_id, &query).await
    })
}

pub fn inheritance(
    cx: &App,
    client: Client,
    project_id: i64,
    class_id: String,
) -> Task<Result<ClassInheritanceResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.class_inheritance(project_id, &class_id).await
    })
}

pub fn implementations(
    cx: &App,
    client: Client,
    project_id: i64,
    class_id: String,
) -> Task<Result<ClassImplementationsResponse, ClientError>> {
    runtime::spawn(cx, async move {
        client.class_implementations(project_id, &class_id).await
    })
}

pub fn classification_stats(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<ClassificationStatsResponse, ClientError>> {
    runtime::spawn(
        cx,
        async move { client.classification_stats(project_id).await },
    )
}

fn chain_params(max_depth: usize, limit: usize) -> CallChainQueryParams {
    CallChainQueryParams {
        max_depth,
        offset: None,
        limit,
        exclude_tests: None,
        directory_prefix: None,
        excluded_files: None,
        domains: None,
        include_external: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_params_carry_the_page_and_depth() {
        let params = chain_params(4, 50);
        assert_eq!(params.max_depth, 4);
        assert_eq!(params.limit, 50);
        assert!(params.include_external);
    }

    #[test]
    fn directions_match_the_contract() {
        assert_eq!(CHAIN_DIRECTIONS, ["down", "up"]);
    }
}
