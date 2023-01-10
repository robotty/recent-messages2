use prometheus::TextEncoder;

pub async fn get_metrics() -> String {
    TextEncoder.encode_to_string(&prometheus::gather()).unwrap()
}
