//! Serves Tauri's embedded frontend assets through the local HTTP server.

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, Response, StatusCode, Uri},
    routing::get,
    Router,
};
use tauri::AppHandle;

pub(crate) fn router(app: AppHandle) -> Router {
    Router::new()
        .route("/__byok-api__/", get(asset))
        .route("/__byok-api__/{*path}", get(asset))
        .with_state(app)
}

async fn asset(State(app): State<AppHandle>, uri: Uri) -> Response<Body> {
    let path = uri
        .path()
        .strip_prefix("/__byok-api__/")
        .filter(|path| !path.is_empty())
        .unwrap_or("index.html");
    let Some(asset) = app.asset_resolver().get(path.to_string()) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .expect("static not-found response");
    };
    let mut response = Response::new(Body::from(asset.bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&asset.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    if let Some(csp) = asset.csp_header.and_then(|value| value.parse().ok()) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, csp);
    }
    response
}
