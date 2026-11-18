//! Shared plumbing for panel requests.
//!
//! Every panel follows the same shape: assemble a request, drive it on the
//! tokio runtime through [`crate::runtime`], and render whichever of idle,
//! loading, result or failure the outcome produced. This module owns that
//! state machine and the failure classification, so a panel only decides what
//! to send and how a successful payload looks.

use cce_api::models::SymbolCandidate;
use cce_client::ClientError;
use gpui_kit::{Context, SharedString, Task};

/// Why a panel has no result yet or no result at all.
///
/// The message key keeps the rendered text localisable after the fact: a
/// language switch re-renders the same failure in the new language instead of
/// freezing the text captured when the call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// Catalogue key describing the failure class.
    pub key: &'static str,
    /// Untranslated transport detail, empty when there is nothing to add.
    pub detail: SharedString,
    /// Candidate entities when the server could not resolve an ambiguous
    /// seed. Non-empty makes the failure actionable instead of terminal.
    pub candidates: Vec<SymbolCandidate>,
}

impl Failure {
    /// Classify a client error into the class a panel renders.
    pub fn from_error(error: &ClientError) -> Self {
        let key = match error {
            ClientError::Config { .. } => "error.config",
            ClientError::Unavailable { .. } => "error.unavailable",
            ClientError::Unauthorized { .. } => "error.unauthorized",
            ClientError::Rejected { .. } => "error.rejected",
            ClientError::Protocol { .. } => "error.protocol",
        };
        Self {
            key,
            detail: SharedString::from(error.to_string()),
            candidates: error.symbol_candidates(),
        }
    }

    /// Failure raised when a panel is used before any client exists.
    pub fn disconnected() -> Self {
        Self {
            key: "error.notConnected",
            detail: SharedString::default(),
            candidates: Vec::new(),
        }
    }
}

/// One request slot: what the panel shows while a call is in flight and after.
pub enum Load<T> {
    /// Nothing requested yet, or the request was cleared.
    Idle,
    Loading,
    Ready(T),
    Failed(Failure),
}

impl<T> Load<T> {
    pub fn idle() -> Self {
        Self::Idle
    }

    /// Mark the slot as in flight. The previous result is dropped so a stale
    /// payload can never be mistaken for the answer to the new request.
    pub fn begin(&mut self) {
        *self = Self::Loading;
    }

    /// Build a slot from a finished call.
    pub fn settle(outcome: Result<T, ClientError>) -> Self {
        match outcome {
            Ok(value) => Self::Ready(value),
            Err(error) => Self::Failed(Failure::from_error(&error)),
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
}

/// Hand a finished client call back to the view that started it.
///
/// Dropping the returned task cancels only the local await; the tokio side
/// still runs to completion and its result is discarded.
pub fn spawn_into<V, T, F>(cx: &mut Context<V>, task: Task<Result<T, ClientError>>, apply: F)
where
    V: 'static,
    T: 'static,
    F: FnOnce(&mut V, Result<T, ClientError>, &mut Context<V>) + 'static,
{
    cx.spawn(async move |this, cx| {
        let outcome = task.await;
        this.update(cx, |view, cx| apply(view, outcome, cx)).ok();
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_client::ApiMethod;

    #[test]
    fn failures_are_classified_by_cause() {
        let unavailable = ClientError::Unavailable {
            method: ApiMethod::Get,
            path: "/api/health".to_string(),
            status: None,
            detail: "connection refused".to_string(),
        };
        assert_eq!(Failure::from_error(&unavailable).key, "error.unavailable");

        let rejected = ClientError::Rejected {
            method: ApiMethod::Post,
            path: "/api/search".to_string(),
            status: 400,
            code: Some("bad_request".to_string()),
            message: "query is empty".to_string(),
            details: None,
        };
        assert_eq!(Failure::from_error(&rejected).key, "error.rejected");

        let unauthorized = ClientError::Unauthorized {
            method: ApiMethod::Get,
            path: "/api/project".to_string(),
            status: 401,
            code: None,
            message: "invalid token".to_string(),
        };
        assert_eq!(Failure::from_error(&unauthorized).key, "error.unauthorized");
    }

    #[test]
    fn slots_track_the_request_lifecycle() {
        let mut slot: Load<u8> = Load::idle();
        slot.begin();
        assert!(slot.is_loading());
        assert!(slot.ready().is_none());

        let slot = Load::settle(Ok(7u8));
        assert_eq!(slot.ready(), Some(&7));
        assert!(!slot.is_loading());
    }

    #[test]
    fn ambiguous_rejections_carry_their_candidates() {
        let rejected = ClientError::Rejected {
            method: ApiMethod::Post,
            path: "/api/graph/ego".to_string(),
            status: 400,
            code: Some("AMBIGUOUS_SYMBOL".to_string()),
            message: "2 entities match".to_string(),
            details: Some(
                r#"[{"stable_id":"sym_a","file_path":"a.rs","scoped_name":"a::run","kind":"function"}]"#
                    .to_string(),
            ),
        };
        let failure = Failure::from_error(&rejected);
        assert_eq!(failure.key, "error.rejected");
        assert_eq!(failure.candidates.len(), 1);
        assert_eq!(failure.candidates[0].stable_id, "sym_a");
    }

    #[test]
    fn a_plain_rejection_has_no_candidates() {
        let rejected = ClientError::Rejected {
            method: ApiMethod::Post,
            path: "/api/search".to_string(),
            status: 400,
            code: Some("INVALID_INPUT".to_string()),
            message: "query is empty".to_string(),
            details: None,
        };
        assert!(Failure::from_error(&rejected).candidates.is_empty());
    }

    #[test]
    fn error_outcome_keeps_the_transport_detail() {
        let slot: Load<u8> = Load::settle(Err(ClientError::Config {
            detail: "invalid base url".to_string(),
        }));
        match slot {
            Load::Failed(failure) => {
                assert_eq!(failure.key, "error.config");
                assert!(failure.detail.contains("invalid base url"));
            }
            _ => panic!("expected a failure slot"),
        }
    }
}
