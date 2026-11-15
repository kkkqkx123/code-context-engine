//! Static frontend hosting for the single-host bundle.
//!
//! Disabled by default, so a pure-API deployment never serves files. When
//! `[server.frontend]` is enabled, this module serves the frontend static
//! directory (the `frontend/build` output) on the same port as the API:
//! one executable plus one static directory, no reverse proxy required.
//!
//! Routing precedence is API first, static second, SPA fallback last. The
//! fallback only rewrites extension-less non-API paths to the index page;
//! fingerprinted assets that are missing stay a 404 so broken builds fail
//! loudly instead of masquerading as a working page.

use axum::{
    Json, Router,
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use cce_api::models::{ErrorResponse, error_codes};
use cce_server_shared::context::SharedContext;
use std::path::PathBuf;
use std::sync::Arc;
use tower::service_fn;
use tower_http::services::ServeDir;

/// Resolved static hosting state shared by the file service.
#[derive(Debug, Clone)]
struct StaticHosting {
    dir: PathBuf,
    index: Arc<PathBuf>,
    fallback: bool,
    immutable_max_age_secs: u64,
}

/// Build the static router when frontend hosting is enabled.
///
/// Returns `Ok(None)` when disabled so the caller keeps the pure-API
/// router. Returns `Err` when enabled but the directory is unusable, so
/// the server refuses to start instead of serving a half-broken page.
/// Build the static router from an explicit frontend config.
///
/// The Settings-backed entry point below delegates here; integration tests
/// use this form directly so no global configuration is touched.
pub fn static_router_from_config(
    frontend: &cce_config::FrontendConfig,
) -> anyhow::Result<Option<Router<SharedContext>>> {
    if !frontend.enabled {
        return Ok(None);
    }
    match frontend.resolve_validated() {
        Ok(dir) => {
            tracing::info!(dir = %dir.display(), "Static frontend hosting enabled");
            Ok(Some(static_router_with(StaticHosting {
                index: Arc::new(dir.join(frontend.index.trim())),
                dir,
                fallback: frontend.fallback,
                immutable_max_age_secs: frontend.immutable_max_age_secs,
            })))
        }
        Err(reason) => Err(anyhow::anyhow!("{reason}")),
    }
}

/// Assemble the fallback-only router for any router state.
///
/// The file service never needs handler state, so the same builder serves
/// both the production router and state-free test routers.
fn static_router_with<S>(hosting: StaticHosting) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let max_age = hosting.immutable_max_age_secs;
    let dir = hosting.dir.clone();
    let missing = service_fn(move |req: Request| {
        let hosting = hosting.clone();
        async move { Ok::<_, std::convert::Infallible>(serve_missing(&hosting, req).await) }
    });
    let files = ServeDir::new(&dir)
        .append_index_html_on_directories(false)
        .fallback(missing);
    Router::new()
        .fallback_service(files)
        .layer(middleware::from_fn(
            move |request: Request, next: Next| async move {
                let immutable = request.uri().path().starts_with("/_app/immutable/");
                let mut response = next.run(request).await;
                if immutable && response.status().is_success() {
                    let value = format!("public, max-age={max_age}, immutable");
                    if let Ok(header_value) = HeaderValue::from_str(&value) {
                        response
                            .headers_mut()
                            .insert(header::CACHE_CONTROL, header_value);
                    }
                }
                response
            },
        ))
}

/// What an unmatched path resolves to once the file service misses.
#[derive(Debug, PartialEq, Eq)]
enum MissingKind {
    /// Unknown API path: stays a JSON 404, never the index page.
    UnknownApi,
    /// Missing fingerprinted asset: stays a 404 with the request path.
    MissingAsset,
    /// Extension-less page path: rewrites to the index page.
    IndexPage,
    /// SPA fallback disabled: stays a 404 with the request path.
    NoFallback,
}

/// Classify an unmatched path without touching the filesystem.
fn classify_missing(path: &str, fallback: bool) -> MissingKind {
    if path == "/api" || path.starts_with("/api/") {
        return MissingKind::UnknownApi;
    }
    let leaf = path.rsplit('/').next().unwrap_or(path);
    if leaf.contains('.') {
        return MissingKind::MissingAsset;
    }
    if fallback {
        MissingKind::IndexPage
    } else {
        MissingKind::NoFallback
    }
}

/// Answer a request the file service could not satisfy.
async fn serve_missing(hosting: &StaticHosting, req: Request) -> Response {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(ErrorResponse::new(
                error_codes::INVALID_REQUEST,
                format!(
                    "Method {} is not served for static path {}",
                    req.method(),
                    req.uri().path()
                ),
            )),
        )
            .into_response();
    }
    match classify_missing(req.uri().path(), hosting.fallback) {
        MissingKind::UnknownApi => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                error_codes::ENTITY_NOT_FOUND,
                format!("Unknown API path: {}", req.uri().path()),
            )),
        )
            .into_response(),
        MissingKind::MissingAsset => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                error_codes::ENTITY_NOT_FOUND,
                format!("Static asset not found: {}", req.uri().path()),
            )),
        )
            .into_response(),
        MissingKind::NoFallback => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                error_codes::ENTITY_NOT_FOUND,
                format!("No static route for path: {}", req.uri().path()),
            )),
        )
            .into_response(),
        MissingKind::IndexPage => match tokio::fs::read(hosting.index.as_ref()).await {
            Ok(body) => (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                body,
            )
                .into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    error_codes::INTERNAL_ERROR,
                    format!("Fallback page is missing: {}", hosting.index.display()),
                )),
            )
                .into_response(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    fn test_hosting(dir: &std::path::Path, fallback: bool) -> StaticHosting {
        StaticHosting {
            dir: dir.to_path_buf(),
            index: Arc::new(dir.join("index.html")),
            fallback,
            immutable_max_age_secs: 60,
        }
    }

    fn write_fixture(dir: &std::path::Path) {
        std::fs::create_dir_all(dir.join("_app/immutable")).expect("fixture dirs");
        std::fs::write(dir.join("index.html"), "<html>app</html>").expect("index fixture");
        std::fs::write(dir.join("_app/immutable/app.js"), "console.log(1)").expect("asset fixture");
    }

    fn get(uri: &str) -> Request {
        Request::builder()
            .uri(uri)
            .body(Body::from(""))
            .expect("test request")
    }

    #[test]
    fn missing_path_classification_prefers_api_and_assets() {
        assert_eq!(
            classify_missing("/api/search", true),
            MissingKind::UnknownApi
        );
        assert_eq!(classify_missing("/api", true), MissingKind::UnknownApi);
        assert_eq!(
            classify_missing("/favicon.png", true),
            MissingKind::MissingAsset
        );
        assert_eq!(
            classify_missing("/_app/immutable/app.js", true),
            MissingKind::MissingAsset
        );
        assert_eq!(classify_missing("/dashboard", true), MissingKind::IndexPage);
        assert_eq!(classify_missing("/", true), MissingKind::IndexPage);
        assert_eq!(
            classify_missing("/dashboard", false),
            MissingKind::NoFallback
        );
        assert_eq!(
            classify_missing("/api/search", false),
            MissingKind::UnknownApi
        );
    }

    #[tokio::test]
    async fn serves_files_and_falls_back_to_index() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let router = static_router_with::<()>(test_hosting(dir.path(), true));

        let response = router
            .clone()
            .oneshot(get("/_app/immutable/app.js"))
            .await
            .expect("static asset");
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .is_some_and(|v| v.to_str().unwrap_or_default().contains("immutable"))
        );

        let response = router
            .clone()
            .oneshot(get("/dashboard"))
            .await
            .expect("spa path");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .map(|v| v.to_str().unwrap_or_default().to_string())
                .as_deref(),
            Some("text/html; charset=utf-8"),
        );

        let response = router
            .oneshot(get("/missing.js"))
            .await
            .expect("missing asset");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rejects_non_read_methods_and_unknown_api() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let router = static_router_with::<()>(test_hosting(dir.path(), true));

        let post = Request::builder()
            .method(Method::POST)
            .uri("/dashboard")
            .body(Body::from(""))
            .expect("test request");
        let response = router.clone().oneshot(post).await.expect("post");
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

        let response = router.oneshot(get("/api/search")).await.expect("api");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn disabled_fallback_keeps_page_paths_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let router = static_router_with::<()>(test_hosting(dir.path(), false));

        let response = router.oneshot(get("/dashboard")).await.expect("page");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
