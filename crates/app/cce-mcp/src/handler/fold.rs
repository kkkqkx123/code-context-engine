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

/// Run the single fold tool call.
pub async fn fold(args: FoldArgs) -> Result<CallToolResult, rmcp::ErrorData> {
    use cce_orchestrator::{FileFoldMode, FileFoldRequest, FileFoldTool};
    use cce_types::language::Language;

    let mut request = FileFoldRequest::new(args.text);
    if let Some(name) = args.language.as_deref()
        && let Some(language) = Language::from_name(name)
    {
        request = request.with_language(language);
    }
    if let Some(file_name) = args.file_name.clone() {
        request = request.with_file_name(file_name);
    }
    if let Some(max_tokens) = args.max_tokens {
        request = request.with_max_tokens(max_tokens);
    }
    if let Some(mode) = args.mode.as_deref() {
        request = request.with_mode(FileFoldMode::parse(Some(mode)));
    }

    let mut coordinator = cce_parser::parser::ParseCoordinator::new();
    let folded = FileFoldTool::fold(&mut coordinator, request);
    Ok(CallToolResult::success(vec![ContentBlock::text(
        fold_payload(None, &folded).to_string(),
    )]))
}

/// Run the batch fold tool call.
pub async fn fold_batch(args: FoldBatchArgs) -> Result<CallToolResult, rmcp::ErrorData> {
    use cce_orchestrator::{FileFoldMode, FileFoldRequest, FileFoldTool};
    use cce_types::language::Language;

    let mut coordinator = cce_parser::parser::ParseCoordinator::new();
    let mut results = Vec::with_capacity(args.items.len());
    for item in &args.items {
        let mut request = FileFoldRequest::new(item.text.clone());
        let language = item.language.as_deref().or(args.language.as_deref());
        if let Some(name) = language
            && let Some(parsed) = Language::from_name(name)
        {
            request = request.with_language(parsed);
        }
        if let Some(file_name) = item.file_name.clone() {
            request = request.with_file_name(file_name);
        }
        if let Some(max_tokens) = item.max_tokens.or(args.max_tokens) {
            request = request.with_max_tokens(max_tokens);
        }
        let mode = item.mode.as_deref().or(args.mode.as_deref());
        if mode.is_some() {
            request = request.with_mode(FileFoldMode::parse(mode));
        }
        let folded = FileFoldTool::fold(&mut coordinator, request);
        results.push(fold_payload(Some(&item.id), &folded));
    }
    Ok(CallToolResult::success(vec![ContentBlock::text(
        json!({ "results": results }).to_string(),
    )]))
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
