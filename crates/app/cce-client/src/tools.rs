//! Source-level tool operations: folding, compression, diagnostics and symbol
//! lookup.

use cce_api::models::{
    BatchCompressRequest, BatchCompressResponse, BatchFoldRequest, BatchFoldResponse,
    CompressApiResponse, CompressRequest, DiagnoseApiResponse, DiagnoseRequest,
    FindReferencesRequest, FindReferencesResponse, FoldRequest, FoldResponse, GetSymbolsRequest,
    GetSymbolsResponse, GotoDefinitionRequest, GotoDefinitionResponse, KeywordSearchApiResponse,
    KeywordSearchRequest,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Semantic text of one file.
    pub async fn compress(
        &self,
        request: &CompressRequest,
    ) -> Result<CompressApiResponse, ClientError> {
        self.post_json(
            paths::TOOLS_COMPRESS.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Semantic text of several files in one request.
    pub async fn compress_batch(
        &self,
        request: &BatchCompressRequest,
    ) -> Result<BatchCompressResponse, ClientError> {
        self.post_json(
            paths::TOOLS_COMPRESS_BATCH.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Syntax and semantic diagnostics for a code snippet.
    pub async fn diagnose(
        &self,
        request: &DiagnoseRequest,
    ) -> Result<DiagnoseApiResponse, ClientError> {
        self.post_json(
            paths::TOOLS_DIAGNOSE.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Fold a code snippet to a structural skeleton.
    pub async fn fold(&self, request: &FoldRequest) -> Result<FoldResponse, ClientError> {
        self.post_json(paths::TOOLS_FOLD.to_string(), request, INTERACTIVE_BUDGET)
            .await
    }

    /// Fold several code snippets in one request.
    pub async fn fold_batch(
        &self,
        request: &BatchFoldRequest,
    ) -> Result<BatchFoldResponse, ClientError> {
        self.post_json(
            paths::TOOLS_FOLD_BATCH.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Keyword search over indexed text.
    pub async fn keyword_search(
        &self,
        request: &KeywordSearchRequest,
    ) -> Result<KeywordSearchApiResponse, ClientError> {
        self.post_json(
            paths::TOOLS_KEYWORD_SEARCH.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Symbols defined in the given files.
    pub async fn symbols(
        &self,
        request: &GetSymbolsRequest,
    ) -> Result<GetSymbolsResponse, ClientError> {
        self.post_json(
            paths::TOOLS_SYMBOLS.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// References to a symbol at a source position.
    pub async fn references(
        &self,
        request: &FindReferencesRequest,
    ) -> Result<FindReferencesResponse, ClientError> {
        self.post_json(
            paths::TOOLS_REFERENCES.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Definition of the symbol at a source position.
    pub async fn definition(
        &self,
        request: &GotoDefinitionRequest,
    ) -> Result<GotoDefinitionResponse, ClientError> {
        self.post_json(
            paths::TOOLS_DEFINITION.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }
}
