//! Entity commands: detail, call relations, class relations and classification.
//!
//! The entity panel works on stable symbol ids it obtains from the entity
//! search, from the graph panel, or by pasting one directly.

use cce_api::models::{
    CallChainDirectionParams, CallChainQueryParams, CallChainResponse, CallPathQuery,
    CallPathResponse, ClassImplementationsResponse, ClassInheritanceResponse,
    ClassificationQueryParams, ClassificationRelation, ClassificationStatsResponse,
    FunctionCallersResponse, FunctionCallsResponse, FunctionDetailResponse,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

/// Call chain directions the panel offers.
pub const CHAIN_DIRECTIONS: [&str; 2] = ["down", "up"];

/// Relation filters shared by the calls, callers and chain queries.
///
/// The panel used to pin every one of these fields, which silently changed
/// what "callers" means: results always included test files and always
/// crossed domain boundaries. The filter is now an explicit input, and the
/// default keeps the previous answer (`include_external: true`) so nothing
/// narrows until the user asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainFilter {
    pub exclude_tests: bool,
    pub directory_prefix: Option<String>,
    pub domains: Vec<String>,
    pub include_external: bool,
}

impl Default for ChainFilter {
    fn default() -> Self {
        Self {
            exclude_tests: false,
            directory_prefix: None,
            domains: Vec::new(),
            include_external: true,
        }
    }
}

impl ChainFilter {
    /// Comma-separated domain selector; `None` keeps every domain.
    fn domains(&self) -> Option<String> {
        if self.domains.is_empty() {
            None
        } else {
            Some(self.domains.join(","))
        }
    }

    fn apply(&self, params: CallChainQueryParams) -> CallChainQueryParams {
        CallChainQueryParams {
            exclude_tests: Some(self.exclude_tests),
            directory_prefix: self.directory_prefix.clone(),
            domains: self.domains(),
            include_external: self.include_external,
            ..params
        }
    }

    fn apply_direction(&self, params: CallChainDirectionParams) -> CallChainDirectionParams {
        CallChainDirectionParams {
            exclude_tests: Some(self.exclude_tests),
            directory_prefix: self.directory_prefix.clone(),
            domains: self.domains(),
            include_external: self.include_external,
            ..params
        }
    }
}

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
    filter: ChainFilter,
) -> Task<Result<FunctionCallsResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let params = filter.apply(chain_params(max_depth, limit));
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
    filter: ChainFilter,
) -> Task<Result<FunctionCallersResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let params = filter.apply(chain_params(max_depth, limit));
        client
            .function_callers(project_id, &function_id, &params)
            .await
    })
}

/// One call-chain query: the symbol, the direction, the page and the filter.
///
/// Grouped so the chain entry point keeps the same argument shape as the
/// calls/callers ones instead of growing an eighth positional parameter.
pub struct ChainRequest {
    pub function_id: String,
    pub direction: String,
    pub max_depth: usize,
    pub limit: usize,
    pub filter: ChainFilter,
}

pub fn chain(
    cx: &App,
    client: Client,
    project_id: i64,
    request: ChainRequest,
) -> Task<Result<CallChainResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let params = request.filter.apply_direction(CallChainDirectionParams {
            direction: request.direction,
            max_depth: request.max_depth,
            offset: None,
            limit: request.limit,
            exclude_tests: None,
            directory_prefix: None,
            excluded_files: None,
            domains: None,
            include_external: true,
        });
        client
            .call_chain(project_id, &request.function_id, &params)
            .await
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

/// Typed detail rows of one external-call classification.
pub fn classification_relations(
    cx: &App,
    client: Client,
    project_id: i64,
    classification: String,
    limit: usize,
) -> Task<Result<Vec<ClassificationRelation>, ClientError>> {
    runtime::spawn(cx, async move {
        client
            .classification_relations(
                project_id,
                &classification,
                &ClassificationQueryParams { limit: Some(limit) },
            )
            .await
    })
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
    fn the_default_filter_matches_the_previous_answer() {
        let params = ChainFilter::default().apply(chain_params(3, 20));
        assert_eq!(params.exclude_tests, Some(false));
        assert!(params.include_external);
        assert!(params.domains.is_none(), "an empty domain list means all");
        assert!(params.directory_prefix.is_none());
    }

    #[test]
    fn a_narrowed_filter_reaches_the_wire() {
        let filter = ChainFilter {
            exclude_tests: true,
            directory_prefix: Some("src/core".to_string()),
            domains: vec!["runtime".to_string()],
            include_external: false,
        };
        let params = filter.apply(chain_params(2, 10));
        assert_eq!(params.exclude_tests, Some(true));
        assert_eq!(params.directory_prefix.as_deref(), Some("src/core"));
        assert_eq!(params.domains.as_deref(), Some("runtime"));
        assert!(!params.include_external);
    }

    #[test]
    fn directions_match_the_contract() {
        assert_eq!(CHAIN_DIRECTIONS, ["down", "up"]);
    }
}
