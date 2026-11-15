//! Single-host bundle contract: the API router and the static file
//! router merge side by side on one port.
//!
//! Pages and fingerprinted assets come from the static directory, API
//! routes keep their precedence and envelopes, and unknown paths resolve
//! per the SPA fallback rules instead of leaking the index page for
//! missing assets.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use cce_config::FrontendConfig;
use cce_server::api::router;
use tower::ServiceExt;

fn fixture_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("static fixture dir");
    std::fs::create_dir_all(dir.path().join("_app/immutable")).expect("fixture dirs");
    std::fs::write(dir.path().join("index.html"), "<html>single</html>").expect("index");
    std::fs::write(
        dir.path().join("_app/immutable/app.abc123.js"),
        "console.log(1)",
    )
    .expect("asset");
    dir
}

fn enabled_frontend(dir: &tempfile::TempDir) -> FrontendConfig {
    FrontendConfig {
        enabled: true,
        dir: dir.path().to_string_lossy().into_owned(),
        ..FrontendConfig::default()
    }
}

fn request(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::from(""))
        .expect("test request")
}

fn get(uri: &str) -> Request<Body> {
    request(Method::GET, uri)
}

#[tokio::test]
async fn single_host_serves_pages_beside_api() {
    let env = common::test_env().await;
    let dir = fixture_dir();
    let app = router::create_router_with_frontend(env.state, &enabled_frontend(&dir))
        .expect("fixture dir is servable");

    let response = app.clone().oneshot(get("/")).await.expect("root page");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(get("/dashboard"))
        .await
        .expect("spa path");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(get("/_app/immutable/app.abc123.js"))
        .await
        .expect("fingerprinted asset");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(get("/missing.css"))
        .await
        .expect("missing asset");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .clone()
        .oneshot(get("/api/no-such-route"))
        .await
        .expect("unknown api");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .clone()
        .oneshot(request(Method::POST, "/dashboard"))
        .await
        .expect("non-read method");
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let response = app.oneshot(get("/api/health")).await.expect("health");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn disabled_frontend_keeps_pure_api_router() {
    let env = common::test_env().await;
    let app = router::create_router_with_frontend(env.state, &FrontendConfig::default())
        .expect("disabled frontend builds");

    let response = app.oneshot(get("/dashboard")).await.expect("page");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn enabled_frontend_with_missing_dir_refuses_router() {
    let env = common::test_env().await;
    let frontend = FrontendConfig {
        enabled: true,
        dir: "/definitely/missing/cce-web-dir".to_string(),
        ..FrontendConfig::default()
    };
    assert!(
        router::create_router_with_frontend(env.state, &frontend).is_err(),
        "unusable static dir must fail router construction"
    );
}
