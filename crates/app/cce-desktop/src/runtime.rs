//! Tokio runtime driving `cce-client` requests.
//!
//! The client stack (reqwest) expects a tokio reactor, while GPUI runs its
//! own executor, so client futures are polled here and forwarded back through
//! a plain oneshot channel.

use std::future::Future;

use cce_client::ClientError;
use gpui_kit::{App, Global, Task};

/// Process-wide tokio runtime handle.
pub struct RuntimeGlobal(Option<tokio::runtime::Runtime>);

impl Global for RuntimeGlobal {}

pub fn init(runtime: tokio::runtime::Runtime, cx: &mut App) {
    cx.set_global(RuntimeGlobal(Some(runtime)));
}

/// Drive `future` on the tokio runtime and expose the result as a gpui task.
///
/// Dropping the returned task cancels only the local await; the tokio side
/// still runs to completion and its result is discarded (abandon semantics).
pub fn spawn<R, F>(cx: &App, future: F) -> Task<Result<R, ClientError>>
where
    F: Future<Output = Result<R, ClientError>> + Send + 'static,
    R: Send + 'static,
{
    let runtime = cx
        .try_global::<RuntimeGlobal>()
        .and_then(|global| global.0.as_ref());
    let Some(runtime) = runtime else {
        return Task::ready(Err(ClientError::Config {
            detail: "tokio runtime is not initialized".to_string(),
        }));
    };

    let (tx, rx) = futures::channel::oneshot::channel();
    runtime.spawn(async move {
        let result = future.await;
        _ = tx.send(result);
    });

    cx.background_executor().spawn(async move {
        rx.await.unwrap_or_else(|_| {
            Err(ClientError::Config {
                detail: "background request dropped".to_string(),
            })
        })
    })
}
