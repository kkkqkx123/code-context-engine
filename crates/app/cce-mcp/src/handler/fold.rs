//! Stateless file folding tools for the MCP surface.
//!
//! Fold tools carry no project context: raw text goes straight to the parser
//! coordinator, degrading to truncated text with `structure_known=false` for
//! unknown languages or parse failures.

use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

/// Arguments for stateless file folding (no project context).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FoldArgs {
    /// Raw source text to fold.
    pub text: String,
    /// Optional language hint (e.g. "rust", "python").
    #[serde(default)]
    pub language: Option<String>,
    /// Optional file name hint for suffix inference.
    #[serde(default)]
    pub file_name: Option<String>,
    /// Optional token budget for the folded text.
    #[serde(default)]
    pub max_tokens: Option<usize>,
    /// Optional fold mode ("detailed" or "minimal").
    #[serde(default)]
    pub mode: Option<String>,
}

/// One batch fold entry mirroring the HTTP batch fold item shape.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FoldBatchItemArgs {
    /// Caller-provided stable identifier echoed in the paired result.
    pub id: String,
    /// Raw source text to fold.
    pub text: String,
    /// Optional language hint.
    #[serde(default)]
    pub language: Option<String>,
    /// Optional file name hint.
    #[serde(default)]
    pub file_name: Option<String>,
    /// Optional token budget for the folded text.
    #[serde(default)]
    pub max_tokens: Option<usize>,
    /// Optional fold mode (detailed or minimal).
    #[serde(default)]
    pub mode: Option<String>,
}

/// Arguments for stateless batch file folding (no project context).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FoldBatchArgs {
    /// Entries to fold.
    pub items: Vec<FoldBatchItemArgs>,
    /// Global language default for items without an explicit hint.
    #[serde(default)]
    pub language: Option<String>,
    /// Global token budget default.
    #[serde(default)]
    pub max_tokens: Option<usize>,
    /// Global fold mode default.
    #[serde(default)]
    pub mode: Option<String>,
}

/// Serialize a fold outcome with the shared payload field names.
fn fold_payload(id: Option<&str>, folded: &cce_orchestrator::FileFoldResponse) -> Value {
    let mut payload = json!({
        "folded_text": folded.folded_text,
        "language": folded.language,
        "structure_known": folded.structure_known,
        "original_tokens": folded.original_tokens,
        "folded_tokens": folded.folded_tokens,
        "kept_sections": folded.kept_sections,
        "dropped_sections": folded.dropped_sections,
    });
    if let Some(id) = id {
        payload["id"] = json!(id);
    }
    payload
}

/// Serialize one batch fold outcome with the shared payload field names.
fn batch_payload(id: &str, folded: &cce_orchestrator::FileFoldBatchItemResponse) -> Value {
    json!({
        "id": id,
        "folded_text": folded.folded_text,
        "language": folded.language,
        "structure_known": folded.structure_known,
        "original_tokens": folded.original_tokens,
        "folded_tokens": folded.folded_tokens,
        "kept_sections": folded.kept_sections,
        "dropped_sections": folded.dropped_sections,
    })
}

/// Run the single fold tool call.
pub async fn fold(args: FoldArgs) -> Result<CallToolResult, rmcp::ErrorData> {
    use cce_server_shared::services::tools::{self, FoldSpec};

    let folded = tools::fold(FoldSpec {
        text: args.text,
        language: args.language,
        file_name: args.file_name,
        max_tokens: args.max_tokens,
        mode: args.mode,
    });
    Ok(CallToolResult::success(vec![ContentBlock::text(
        fold_payload(None, &folded).to_string(),
    )]))
}

/// Run the batch fold tool call.
pub async fn fold_batch(args: FoldBatchArgs) -> Result<CallToolResult, rmcp::ErrorData> {
    use cce_server_shared::services::tools::{self, FoldBatchItemSpec, FoldBatchSpec};

    let spec = FoldBatchSpec {
        items: args
            .items
            .iter()
            .map(|item| FoldBatchItemSpec {
                id: item.id.clone(),
                text: item.text.clone(),
                language: item.language.clone(),
                file_name: item.file_name.clone(),
                max_tokens: item.max_tokens,
                mode: item.mode.clone(),
            })
            .collect(),
        language: args.language.clone(),
        max_tokens: args.max_tokens,
        mode: args.mode.clone(),
        max_concurrency: None,
    };
    match tools::fold_batch(spec) {
        Ok(folded) => Ok(CallToolResult::success(vec![ContentBlock::text(
            json!({
                "results": folded
                    .results
                    .iter()
                    .map(|entry| batch_payload(&entry.id, entry))
                    .collect::<Vec<_>>(),
                "stats": {
                    "total_original_tokens": folded.stats.total_original_tokens,
                    "total_folded_tokens": folded.stats.total_folded_tokens,
                    "structure_known_count": folded.stats.structure_known_count,
                },
            })
            .to_string(),
        )])),
        Err(error) => Ok(super::support::tool_failure(format!(
            "fold_batch failed: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_args_default_to_none() {
        let args: FoldArgs = serde_json::from_value(serde_json::json!({"text": "fn main() {}"}))
            .expect("minimal fold args deserialize");
        assert!(args.language.is_none());
        assert!(args.file_name.is_none());
        assert!(args.max_tokens.is_none());
        assert!(args.mode.is_none());
        let batch: FoldBatchArgs = serde_json::from_value(
            serde_json::json!({"items": [{"id": "a", "text": "fn a() {}"}]}),
        )
        .expect("minimal fold batch args deserialize");
        assert_eq!(batch.items.len(), 1);
        assert!(batch.language.is_none());
    }
}
