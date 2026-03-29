use axum::extract::MatchedPath;
use axum::middleware::Next;
use axum::response::IntoResponse;
use http::Request;
use humantime::format_duration;
use prometheus::{register_histogram_vec, register_int_counter_vec};
use prometheus::{HistogramVec, IntCounterVec};
use std::sync::LazyLock;
use std::time::Instant;

static HTTP_REQUESTS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "http_requests_total",
        "Total number of HTTP requests",
        &["endpoint", "method", "status_code"]
    )
    .unwrap()
});
static HTTP_REQUESTS_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        "http_request_duration_seconds",
        "Histogram of time taken to fulfill HTTP requests",
        &["endpoint", "method", "status_code"]
    )
    .unwrap()
});

pub async fn record_metrics<B>(req: Request<B>, next: Next<B>) -> impl IntoResponse {
    let start = Instant::now();
    let path = if let Some(matched_path) = req.extensions().get::<MatchedPath>() {
        matched_path.as_str().to_owned()
    } else {
        // req.uri().path().to_owned()
        "other".to_owned()
    };
    let method = req.method().clone();

    let response = next.run(req).await;

    let latency = start.elapsed();
    let status = response.status().as_u16().to_string();

    tracing::trace!(
        "Observed {} {} {} @ {}",
        method.as_str(),
        &status,
        &path,
        format_duration(latency)
    );

    HTTP_REQUESTS_TOTAL
        .with_label_values(&[&path, method.as_str(), &status])
        .inc();
    HTTP_REQUESTS_DURATION_SECONDS
        .with_label_values(&[&path, method.as_str(), &status])
        .observe(latency.as_secs_f64());

    response
}
