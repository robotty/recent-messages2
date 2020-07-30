use chrono::Utc;
use simple_process_stats::ProcessStats;
use tokio::time::Duration;

/// Provides metrics for CPU and memory usage.
pub fn spawn_system_monitoring() {
    metrics::gauge!("process_start_time_seconds", Utc::now().timestamp());
    tokio::spawn(run_system_monitoring());
}

async fn run_system_monitoring() {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    let mut last_seconds_user: u64 = 0;
    let mut last_seconds_kernel: u64 = 0;
    let mut last_seconds_total: u64 = 0;
    loop {
        interval.tick().await;

        let system_stats = ProcessStats::get().await;
        let system_stats = match system_stats {
            Ok(system_stats) => system_stats,
            Err(e) => {
                log::error!("Monitoring: Failed to get CPU and Memory statistics: {}", e);
                continue;
            }
        };

        // we do this retarded delta calculation because the `metrics` crate only has functionality
        // to _increment_ a counter. Additionally, it only supports whole numbers (i64).
        // For this reason, we simply calculate how many seconds to add since the last run.
        let user_seconds = system_stats.cpu_time_user.as_secs() - last_seconds_user;
        let kernel_seconds = system_stats.cpu_time_kernel.as_secs() - last_seconds_kernel;
        let total_seconds = (system_stats.cpu_time_user + system_stats.cpu_time_kernel).as_secs()
            - last_seconds_total;
        metrics::counter!("process_cpu_user_seconds_total", user_seconds);
        metrics::counter!("process_cpu_system_seconds_total", kernel_seconds);
        metrics::counter!("process_cpu_seconds_total", total_seconds);
        last_seconds_user += user_seconds;
        last_seconds_kernel += kernel_seconds;
        last_seconds_total += total_seconds;

        metrics::gauge!(
            "process_resident_memory_bytes",
            system_stats.memory_usage_bytes as i64
        );
    }
}
