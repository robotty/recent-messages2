use crate::web::error::ApiError;
use crate::web::WebAppData;
use axum::middleware::Next;
use axum::response::IntoResponse;
use http::Request;
use lazy_static::lazy_static;
use prometheus::register_int_counter;
use prometheus::IntCounter;

lazy_static! {
    static ref HTTP_REQUEST_TIMEOUTS: IntCounter = register_int_counter!(
        "http_request_timeouts",
        "Total number of HTTP requests that timed out"
    )
    .unwrap();
}

pub async fn timeout<B>(req: Request<B>, next: Next<B>) -> impl IntoResponse {
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
        _ = timer => {
            HTTP_REQUEST_TIMEOUTS.inc();
            ApiError::RequestTimeout.into_response()
        },
        response = response_fut => {
            response
        }
    }
}
