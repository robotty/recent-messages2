use metrics_core::Builder;
use metrics_core::Drain;
use metrics_core::Observe;
use metrics_runtime::observers::PrometheusBuilder;
use metrics_runtime::Controller;

// GET /api/v2/metrics
pub fn get_metrics(metrics_controller: &Controller) -> String {
    let mut prom_observer = PrometheusBuilder::new().build();
    metrics_controller.observe(&mut prom_observer);
    let response_text = prom_observer.drain();
    response_text
}
