use chrono::Utc;
use simple_process_stats::ProcessStats;
use tokio::time::Duration;

/// Provides metrics for CPU and memory usage.
pub fn spawn_system_monitoring() {
    // TODO gauges -> counters
    metrics::describe_gauge!(
        "process_start_time_seconds",
        "UTC timestamp (in seconds) of when the process started."
    );
    metrics::gauge!("process_start_time_seconds", Utc::now().timestamp() as f64);

    metrics::describe_gauge!(
        "process_cpu_user_seconds_total",
        metrics::Unit::Seconds,
        "Cumulative number of seconds spent executing in user mode"
    );
    metrics::describe_gauge!(
        "process_cpu_system_seconds_total",
        metrics::Unit::Seconds,
        "Cumulative number of seconds spent executing in kernel mode"
    );
    metrics::describe_gauge!(
        "process_cpu_seconds_total",
        metrics::Unit::Seconds,
        "Cumulative number of seconds spent executing in either kernel or user mode"
    );
    metrics::describe_gauge!(
        "process_resident_memory_bytes",
        metrics::Unit::Bytes,
        "Resident memory usage size as reported by the kernel, in bytes"
    );
    tokio::spawn(run_system_monitoring());
}

async fn run_system_monitoring() {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
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

        let user_seconds = system_stats.cpu_time_user.as_secs_f64();
        let kernel_seconds = system_stats.cpu_time_kernel.as_secs_f64();
        let total_seconds =
            (system_stats.cpu_time_user + system_stats.cpu_time_kernel).as_secs_f64();
        metrics::gauge!("process_cpu_user_seconds_total", user_seconds);
        metrics::gauge!("process_cpu_system_seconds_total", kernel_seconds);
        metrics::gauge!("process_cpu_seconds_total", total_seconds);
        metrics::gauge!(
            "process_resident_memory_bytes",
            system_stats.memory_usage_bytes as f64
        );
    }
}
