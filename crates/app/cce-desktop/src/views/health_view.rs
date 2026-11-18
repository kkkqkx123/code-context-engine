//! Health panel: the unified probe plus the three backend diagnostics.

use cce_api::models::{
    AdmissionStatsResponse, Bm25HealthResponse, EmbeddingHealthResponse, HealthStatus,
    QdrantHealthResponse,
};
use gpui_kit::component::{
    Sizable as _,
    button::{Button, ButtonVariants as _},
    v_flex,
};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window,
};

use crate::components::{
    actions, failure_block, hint, loading_row, page_body, panel_header, stack, stat_line, surface,
};
use crate::connection::ClientStore;
use crate::i18n::{locale, t};
use crate::services::ops_service;
use crate::services::refresh;
use crate::services::request::{self, Load};
use crate::state::AppState;

/// The four health payloads the panel shows.
struct Answer {
    health: HealthStatus,
    qdrant: Option<QdrantHealthResponse>,
    embedding: Option<EmbeddingHealthResponse>,
    bm25: Option<Bm25HealthResponse>,
    admission: Option<AdmissionStatsResponse>,
}

pub struct HealthView {
    answer: Load<Answer>,
}

impl HealthView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            answer: Load::idle(),
        }
    }

    /// Probe every health endpoint in one pass.
    ///
    /// The unified probe decides the overall verdict; the three diagnostics are
    /// best-effort and a failure there leaves their block empty instead of
    /// hiding the verdict that did succeed.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let Some(client) = ClientStore::client(cx) else {
            self.answer = Load::Failed(request::Failure::disconnected());
            cx.notify();
            return;
        };
        self.answer.begin();
        cx.notify();
        let unified = ops_service::health(cx, client.clone());
        let qdrant = ops_service::health_qdrant(cx, client.clone());
        let embedding = ops_service::health_embedding(cx, client.clone());
        let bm25 = ops_service::health_bm25(cx, client.clone());
        let admission = ops_service::admission_stats(cx, client);
        cx.spawn(async move |this, cx| {
            let health = unified.await;
            let qdrant = qdrant.await.ok();
            let embedding = embedding.await.ok();
            let bm25 = bm25.await.ok();
            let admission = admission.await.ok();
            this.update(cx, |this, cx| {
                this.answer = match health {
                    Ok(health) => Load::Ready(Answer {
                        health,
                        qdrant,
                        embedding,
                        bm25,
                        admission,
                    }),
                    Err(error) => Load::Failed(request::Failure::from_error(&error)),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Re-probe on the next refresh tick.
    pub fn replay(&mut self, cx: &mut Context<Self>) {
        refresh::touch(crate::views::PanelKind::Health, cx);
        self.load(cx);
    }

    fn render_answer(&self, answer: &Answer, cx: &Context<Self>) -> AnyElement {
        let locale = locale(cx);
        let dense = AppState::global(cx).read(cx).density.is_dense();

        let mut block = surface(dense, cx).child(stat_line(
            t("health.overall", locale),
            if answer.health.healthy {
                t("health.healthy", locale).to_string()
            } else {
                t("health.degraded", locale).to_string()
            },
            cx,
        ));
        for (label, status) in [
            ("vector", &answer.health.qdrant),
            ("bm25", &answer.health.bm25),
            ("embedding", &answer.health.embedding),
        ] {
            block = block.child(stat_line(
                label,
                format!(
                    "{} · {}",
                    if status.reachable {
                        t("health.reachable", locale)
                    } else {
                        t("health.unreachable", locale)
                    },
                    status.message
                ),
                cx,
            ));
        }

        let mut diagnostics = v_flex().gap_2().w_full();
        if let Some(qdrant) = &answer.qdrant {
            diagnostics = diagnostics.child(hint(
                SharedString::from(format!(
                    "qdrant · circuit={} · points={} · collection={}",
                    qdrant.circuit_breaker,
                    qdrant.diagnostic.points_count,
                    qdrant.diagnostic.collection_exists
                )),
                cx,
            ));
        }
        if let Some(embedding) = &answer.embedding {
            diagnostics = diagnostics.child(hint(
                SharedString::from(format!(
                    "embedding · model={} · {}",
                    embedding.model_name.clone().unwrap_or_default(),
                    embedding.message
                )),
                cx,
            ));
        }
        if let Some(bm25) = &answer.bm25 {
            diagnostics = diagnostics.child(hint(
                SharedString::from(format!(
                    "bm25 · enabled={} · connected={}",
                    bm25.enabled, bm25.connected
                )),
                cx,
            ));
        }
        if let Some(admission) = &answer.admission {
            diagnostics = diagnostics.child(hint(
                SharedString::from(format!(
                    "admission · {} admitted · {} rejected (auth {})",
                    admission.global.admitted,
                    admission.global.auth_rejections
                        + admission.global.scope_rejections
                        + admission.global.rate_rejections
                        + admission.global.body_rejections
                        + admission.global.quota_rejections,
                    admission.global.auth_rejections
                )),
                cx,
            ));
        }

        block.child(diagnostics).into_any_element()
    }
}

impl Render for HealthView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = locale(cx);
        let header_actions = actions().child(
            Button::new("health-probe")
                .small()
                .primary()
                .label(t("health.probe", locale))
                .loading(self.answer.is_loading())
                .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
        );

        let body = match &self.answer {
            Load::Idle => hint(t("health.idle", locale), cx).into_any_element(),
            Load::Loading => loading_row(t("health.loading", locale), cx).into_any_element(),
            Load::Failed(failure) => failure_block(failure, locale, cx).into_any_element(),
            Load::Ready(answer) => self.render_answer(answer, cx),
        };

        page_body("health")
            .child(panel_header(
                t("health.title", locale),
                Some(SharedString::from(t("health.subtitle", locale))),
                Some(header_actions.into_any_element()),
                cx,
            ))
            .child(stack().child(body))
    }
}
