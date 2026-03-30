use std::sync::LazyLock;

use crate::web::WebAppData;
use crate::web::error::ApiError;
use axum::{body::Body, middleware::Next};
use axum::response::IntoResponse;
use http::Request;
use prometheus::IntCounter;
use prometheus::register_int_counter;

static HTTP_REQUEST_TIMEOUTS: LazyLock<IntCounter> = LazyLock::new(|| {
    register_int_counter!(
        "http_request_timeouts",
        "Total number of HTTP requests that timed out"
    )
    .unwrap()
});

pub async fn timeout(req: Request<Body>, next: Next) -> impl IntoResponse {
    let request_timeout = req
        .extensions()
        .get::<WebAppData>()
        .unwrap()
        .config
        .web
        .request_timeout;
    let timer = tokio::time::sleep(request_timeout);
    let response_fut = next.run(req);

    tokio::select! {
        () = timer => {
            HTTP_REQUEST_TIMEOUTS.inc();
            ApiError::RequestTimeout.into_response()
        },
        response = response_fut => {
            response
        }
    }
}
