//! Entity detail, call relations, class relations and classification queries.

use cce_api::models::{
    CallChainDirectionParams, CallChainQueryParams, CallChainResponse, CallPathQuery,
    CallPathResponse, ClassImplementationsResponse, ClassInheritanceResponse,
    ClassificationQueryParams, ClassificationStatsResponse, FunctionCallersResponse,
    FunctionCallsResponse, FunctionDetailResponse,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET, id_scoped};

impl Client {
    /// Details of one function or method.
    pub async fn function_detail(
        &self,
        project_id: i64,
        function_id: &str,
    ) -> Result<FunctionDetailResponse, ClientError> {
        self.get_json(
            id_scoped(paths::FUNCTION_DETAIL_TEMPLATE, project_id, function_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Functions called by one function.
    pub async fn function_calls(
        &self,
        project_id: i64,
        function_id: &str,
        params: &CallChainQueryParams,
    ) -> Result<FunctionCallsResponse, ClientError> {
        self.get_json_query(
            id_scoped(paths::FUNCTION_CALLS_TEMPLATE, project_id, function_id),
            params,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Functions calling one function.
    pub async fn function_callers(
        &self,
        project_id: i64,
        function_id: &str,
        params: &CallChainQueryParams,
    ) -> Result<FunctionCallersResponse, ClientError> {
        self.get_json_query(
            id_scoped(paths::FUNCTION_CALLERS_TEMPLATE, project_id, function_id),
            params,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Call chain above or below one function.
    pub async fn call_chain(
        &self,
        project_id: i64,
        function_id: &str,
        params: &CallChainDirectionParams,
    ) -> Result<CallChainResponse, ClientError> {
        self.get_json_query(
            id_scoped(paths::CALL_CHAIN_TEMPLATE, project_id, function_id),
            params,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Call path between two functions.
    pub async fn call_path(
        &self,
        project_id: i64,
        query: &CallPathQuery,
    ) -> Result<CallPathResponse, ClientError> {
        self.get_json_query(
            paths::project_scoped(paths::CALL_PATH_TEMPLATE, project_id),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Inheritance relations of one class.
    pub async fn class_inheritance(
        &self,
        project_id: i64,
        class_id: &str,
    ) -> Result<ClassInheritanceResponse, ClientError> {
        self.get_json(
            id_scoped(paths::CLASS_INHERITANCE_TEMPLATE, project_id, class_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Interfaces implemented by one class.
    pub async fn class_implementations(
        &self,
        project_id: i64,
        class_id: &str,
    ) -> Result<ClassImplementationsResponse, ClientError> {
        self.get_json(
            id_scoped(paths::CLASS_IMPLEMENTATIONS_TEMPLATE, project_id, class_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Relation counts per classification.
    pub async fn classification_stats(
        &self,
        project_id: i64,
    ) -> Result<ClassificationStatsResponse, ClientError> {
        self.get_json(
            paths::project_scoped(paths::CLASSIFICATION_STATS_TEMPLATE, project_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Relations belonging to one classification.
    pub async fn classification_relations(
        &self,
        project_id: i64,
        classification: &str,
        params: &ClassificationQueryParams,
    ) -> Result<Vec<serde_json::Value>, ClientError> {
        self.get_json_query(
            id_scoped(
                paths::CLASSIFICATION_DETAIL_TEMPLATE,
                project_id,
                classification,
            ),
            params,
            INTERACTIVE_BUDGET,
        )
        .await
    }
}
