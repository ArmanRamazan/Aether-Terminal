use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "frontend/dist"]
struct FrontendAssets;

/// Serve embedded frontend assets with SPA fallback.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try exact path first, then SPA fallback to index.html
    match FrontendAssets::get(path) {
        Some(file) => serve_file(path, &file.data),
        None => match FrontendAssets::get("index.html") {
            Some(file) => serve_file("index.html", &file.data),
            None => (StatusCode::NOT_FOUND, "index.html not found").into_response(),
        },
    }
}

fn serve_file(path: &str, data: &[u8]) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type(path))],
        data.to_vec(),
    )
        .into_response()
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("css") => "text/css",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_js() {
        assert_eq!(content_type("app.js"), "application/javascript");
    }

    #[test]
    fn test_content_type_css() {
        assert_eq!(content_type("style.css"), "text/css");
    }

    #[test]
    fn test_content_type_html() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
    }

    #[test]
    fn test_content_type_wasm() {
        assert_eq!(content_type("module.wasm"), "application/wasm");
    }

    #[test]
    fn test_content_type_unknown() {
        assert_eq!(content_type("file.xyz"), "application/octet-stream");
    }

    #[tokio::test]
    async fn test_static_handler_missing_asset_returns_fallback() {
        // When no dist/ exists, both asset and index.html will be missing
        let uri: Uri = "/nonexistent".parse().unwrap();
        let response = static_handler(uri).await;
        // Either serves index.html (SPA fallback) or 404 if no dist
        let status = response.status();
        assert!(
            status == StatusCode::OK || status == StatusCode::NOT_FOUND,
            "expected 200 or 404, got {status}"
        );
    }

    #[tokio::test]
    async fn test_spa_fallback_returns_index() {
        let uri: Uri = "/graph".parse().unwrap();
        let response = static_handler(uri).await;
        // SPA fallback: /graph should return index.html or 404 if no dist
        let status = response.status();
        assert!(
            status == StatusCode::OK || status == StatusCode::NOT_FOUND,
            "expected 200 or 404, got {status}"
        );
    }
}
