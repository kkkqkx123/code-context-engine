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
use tower::{ServiceExt, service_fn};
use tower_http::services::{ServeDir, ServeFile};

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
    // Non-read methods also reach the fallback so unknown API paths stay
    // a 404 for every method and static paths keep the JSON 405 envelope.
    let files = ServeDir::new(&dir)
        .append_index_html_on_directories(false)
        .call_fallback_on_method_not_allowed(true)
        .fallback(missing);
    Router::new()
        .fallback_service(files)
        .layer(middleware::from_fn(
            move |request: Request, next: Next| async move {
                // API-looking paths keep their own missing semantics below;
                // the guard only hides dot segments on the static surface.
                let path = request.uri().path();
                let is_api = path == "/api"
                    || path.starts_with("/api/")
                    || path == "/api-docs"
                    || path.starts_with("/api-docs/");
                if !is_api && has_dot_segment(path) {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse::new(
                            error_codes::ENTITY_NOT_FOUND,
                            format!("Static asset not found: {}", request.uri().path()),
                        )),
                    )
                        .into_response();
                }
                let immutable = request.uri().path().starts_with("/_app/immutable/");
                let mut response = next.run(request).await;
                if let Ok(nosniff) = HeaderValue::from_str("nosniff") {
                    response
                        .headers_mut()
                        .insert(header::X_CONTENT_TYPE_OPTIONS, nosniff);
                }
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

/// Whether any path segment is hidden or a parent reference.
///
/// Hidden files are never served; the guard runs before the file service
/// so their existence stays invisible. Segments are checked after
/// percent-decoding, mirroring the file service itself: an encoded dot
/// (`/%2esecret`) must not bypass the guard. Undecodable paths are
/// treated as hidden, matching the file service rejecting them.
fn has_dot_segment(path: &str) -> bool {
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8();
    let Ok(decoded) = decoded else {
        return true;
    };
    decoded
        .split('/')
        .any(|seg| seg == ".." || seg.starts_with('.'))
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
    if let Some((stem, ext)) = leaf.rsplit_once('.')
        && !stem.is_empty()
        && !ext.is_empty()
    {
        return MissingKind::MissingAsset;
    }
    if fallback {
        MissingKind::IndexPage
    } else {
        MissingKind::NoFallback
    }
}

/// Whether the request only reads the target.
fn is_read(method: &Method) -> bool {
    *method == Method::GET || *method == Method::HEAD
}

/// Answer a request the file service could not satisfy.
///
/// Unknown API paths stay a JSON 404 for every method; only static page
/// and asset paths enforce the read-only constraint.
async fn serve_missing(hosting: &StaticHosting, req: Request) -> Response {
    let kind = classify_missing(req.uri().path(), hosting.fallback);
    if kind != MissingKind::UnknownApi && !is_read(req.method()) {
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
    match kind {
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
        // Serve the fallback page through the file service so
        // conditional requests, ranges and HEAD keep file semantics;
        // only the cache policy is forced to no-store.
        MissingKind::IndexPage => match ServeFile::new(hosting.index.as_ref()).oneshot(req).await {
            Ok(mut response) => {
                if response.status() == StatusCode::NOT_FOUND {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(
                            error_codes::INTERNAL_ERROR,
                            format!("Fallback page is missing: {}", hosting.index.display()),
                        )),
                    )
                        .into_response();
                }
                response
                    .headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/html; charset=utf-8"),
                );
                response.map(axum::body::Body::new)
            }
            Err(never) => match never {},
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
            classify_missing("/dashboard.", true),
            MissingKind::IndexPage
        );
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

        let response = router
            .clone()
            .oneshot(get("/api/search"))
            .await
            .expect("api");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let post_api = Request::builder()
            .method(Method::POST)
            .uri("/api/no-such-route")
            .body(Body::from(""))
            .expect("test request");
        let response = router.oneshot(post_api).await.expect("post api");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn hidden_segments_stay_invisible() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        std::fs::write(dir.path().join(".secret"), "hidden").expect("dotfile fixture");
        let router = static_router_with::<()>(test_hosting(dir.path(), true));

        let response = router
            .clone()
            .oneshot(get("/.secret"))
            .await
            .expect("dotfile");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = router
            .oneshot(get("/_app/../.secret"))
            .await
            .expect("traversal");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn encoded_dots_do_not_bypass_the_guard() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        std::fs::write(dir.path().join(".secret"), "hidden").expect("dotfile fixture");
        let router = static_router_with::<()>(test_hosting(dir.path(), true));

        for uri in ["/%2esecret", "/%2Esecret", "/%2e%2e/.secret"] {
            let response = router.clone().oneshot(get(uri)).await.expect("encoded");
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        }

        // Encoded non-dot segments keep working, including the fallback.
        let response = router
            .oneshot(get("/%64ashboard"))
            .await
            .expect("encoded page");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn fallback_page_keeps_file_semantics() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_fixture(dir.path());
        let router = static_router_with::<()>(test_hosting(dir.path(), true));

        let head = Request::builder()
            .method(Method::HEAD)
            .uri("/dashboard")
            .body(Body::from(""))
            .expect("test request");
        let response = router.clone().oneshot(head).await.expect("head");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("head body");
        assert!(body.is_empty());

        let response = router
            .clone()
            .oneshot(get("/dashboard"))
            .await
            .expect("first get");
        assert_eq!(response.status(), StatusCode::OK);
        let last_modified = response
            .headers()
            .get(header::LAST_MODIFIED)
            .expect("last-modified")
            .clone();

        let conditional = Request::builder()
            .uri("/dashboard")
            .header(header::IF_MODIFIED_SINCE, last_modified)
            .body(Body::from(""))
            .expect("test request");
        let response = router.oneshot(conditional).await.expect("conditional");
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
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
