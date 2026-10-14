//! HTTP regressions exercise the same router shipped in the binary.
use super::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

struct Fixture {
    state: AppState,
    directory: PathBuf,
}
impl Fixture {
    fn new(name: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("gitleaf-http-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let root = directory.join("projects");
        std::fs::create_dir_all(root.join("paper/.git")).unwrap();
        let state = AppState {
            root: root.canonicalize().unwrap(),
            cache: directory.join("cache"),
            port: 8077,
            update: Default::default(),
            shutdown: Default::default(),
            restart: Default::default(),
            locks: Default::default(),
            gallery_cache: Default::default(),
            presence: Default::default(),
        };
        Self { state, directory }
    }
    async fn request(
        &self,
        method: &str,
        path: &str,
        body: &str,
    ) -> (StatusCode, serde_json::Value) {
        let response = router(self.state.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("host", "localhost:8077")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[tokio::test]
async fn files_save_read_and_delete_through_the_real_routes() {
    let fixture = Fixture::new("files");
    let path = "/api/projects/paper/file?path=sections/intro.tex";
    let (status, _) = fixture
        .request("PUT", path, r#"{"content":"hello world"}"#)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, response) = fixture.request("GET", path, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["content"], "hello world");
    let (status, _) = fixture.request("DELETE", path, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        fixture.request("GET", path, "").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn new_file_never_overwrites_an_existing_document() {
    let fixture = Fixture::new("create");
    let path = "/api/projects/paper/file?path=main.tex&create=true";
    assert_eq!(
        fixture
            .request("PUT", path, r#"{"content":"original"}"#)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        fixture
            .request("PUT", path, r#"{"content":"replaced"}"#)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        std::fs::read_to_string(fixture.state.root.join("paper/main.tex")).unwrap(),
        "original"
    );
}

#[tokio::test]
async fn api_paths_cannot_access_git_metadata_or_parent_directories() {
    let fixture = Fixture::new("paths");
    for path in [
        ".git/config",
        "nested/.git/hooks/pre-commit",
        "../outside",
        "%2Fetc%2Fpasswd",
        "a%5Cb.tex",
    ] {
        let endpoint = format!("/api/projects/paper/file?path={path}");
        assert_eq!(
            fixture
                .request("PUT", &endpoint, r#"{"content":"blocked"}"#)
                .await
                .0,
            StatusCode::BAD_REQUEST,
            "accepted {path}"
        );
    }
}

#[tokio::test]
async fn cross_origin_mutations_are_rejected_before_writing() {
    let fixture = Fixture::new("origin");
    let response = router(fixture.state.clone())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/projects/paper/file?path=main.tex")
                .header("host", "localhost:8077")
                .header("origin", "https://unrelated.example")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"content":"blocked"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!fixture.state.root.join("paper/main.tex").exists());
}

#[tokio::test]
async fn active_uploaded_content_is_sandboxed() {
    let fixture = Fixture::new("svg");
    std::fs::write(fixture.state.root.join("paper/plot.svg"), "<svg></svg>").unwrap();
    let response = router(fixture.state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/projects/paper/raw?path=plot.svg")
                .header("host", "localhost:8077")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-security-policy"],
        "sandbox; default-src 'none'"
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
}

#[tokio::test]
async fn bundled_assets_and_browser_security_headers_are_served() {
    let fixture = Fixture::new("assets");
    for path in [
        "/",
        "/assets/app.js",
        "/assets/app.css",
        "/assets/pdf.worker.min.mjs",
    ] {
        let response = router(fixture.state.clone())
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("host", "localhost:8077")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "missing {path}");
        assert_eq!(response.headers()["x-frame-options"], "DENY");
    }
    assert_eq!(
        fixture.request("GET", "/assets/missing.js", "").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn oversized_json_requests_are_refused() {
    let fixture = Fixture::new("limits");
    let body = serde_json::json!({"content": "x".repeat(8 * 1024 * 1024)}).to_string();
    assert_eq!(
        fixture
            .request("PUT", "/api/projects/paper/file?path=main.tex", &body)
            .await
            .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert!(!fixture.state.root.join("paper/main.tex").exists());
}

#[tokio::test]
async fn missing_projects_cannot_serve_or_probe_cached_pdfs() {
    let fixture = Fixture::new("missing");
    assert_eq!(
        fixture
            .request("GET", "/api/projects/missing/pdf", "")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request("GET", "/api/projects/paper/pdf?main=../secret.tex", "")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn source_builds_refuse_self_replacement() {
    let fixture = Fixture::new("update-apply");
    let (status, body) = fixture.request("POST", "/api/update/apply", "").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("build directory"));
    assert!(!fixture.state.cache.exists());
}

#[tokio::test]
async fn update_checks_use_cached_release_information() {
    let fixture = Fixture::new("update-cache");
    let mut cache = fixture.state.update.lock().await;
    cache.when = Some(std::time::Instant::now());
    cache.info = Some(update::UpdateInfo {
        current: env!("CARGO_PKG_VERSION").into(),
        latest: Some(env!("CARGO_PKG_VERSION").into()),
        update_available: false,
        url: None,
        notes: None,
        asset: None,
        can_self_update: false,
        manual_update_instructions: None,
        checked_at: Some(1),
        error: None,
    });
    drop(cache);
    let (status, body) = fixture.request("GET", "/api/update/check", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["checked_at"], 1);
    assert_eq!(body["update_available"], false);
}
