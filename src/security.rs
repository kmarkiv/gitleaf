//! Local browser requests must come from the editor's own origin.
use crate::AppState;
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

fn allowed(headers: &axum::http::HeaderMap, port: u16) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let Ok(authority) = host.parse::<axum::http::uri::Authority>() else {
        return false;
    };
    if !matches!(authority.host(), "localhost" | "127.0.0.1")
        || authority.port_u16().unwrap_or(80) != port
    {
        return false;
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        if origin.to_str().ok() != Some(format!("http://{host}").as_str()) {
            return false;
        }
    }
    !matches!(
        headers.get("sec-fetch-site").and_then(|h| h.to_str().ok()),
        Some("cross-site" | "same-site")
    )
}

pub async fn protect(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !allowed(request.headers(), state.port) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Open GitLeaf directly at its localhost address; cross-origin requests are blocked"}))).into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    if !headers.contains_key(header::CONTENT_SECURITY_POLICY) {
        headers.insert(header::CONTENT_SECURITY_POLICY, "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' https: data: blob:; worker-src 'self' blob:; object-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    fn headers(host: &str, origin: Option<&str>, site: Option<&str>) -> axum::http::HeaderMap {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::HOST, host.parse().unwrap());
        if let Some(origin) = origin {
            headers.insert(header::ORIGIN, origin.parse().unwrap());
        }
        if let Some(site) = site {
            headers.insert("sec-fetch-site", site.parse().unwrap());
        }
        headers
    }
    #[test]
    fn local_editor_and_command_line_requests_work() {
        assert!(allowed(&headers("127.0.0.1:8077", None, None), 8077));
        assert!(allowed(
            &headers(
                "localhost:8077",
                Some("http://localhost:8077"),
                Some("same-origin")
            ),
            8077
        ));
    }
    #[test]
    fn cross_origin_and_rebinding_requests_are_blocked() {
        for host in [
            "evil.example:8077",
            "localhost.evil.example:8077",
            "localhost:9000",
            "localhost",
        ] {
            assert!(!allowed(&headers(host, None, None), 8077));
        }
        for origin in [
            "null",
            "https://evil.example",
            "http://localhost:9000",
            "http://localhost:8077/",
        ] {
            assert!(!allowed(
                &headers("localhost:8077", Some(origin), None),
                8077
            ));
        }
        assert!(!allowed(
            &headers("localhost:8077", None, Some("cross-site")),
            8077
        ));
        assert!(!allowed(
            &headers("localhost:8077", None, Some("same-site")),
            8077
        ));
    }
}
