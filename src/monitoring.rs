use crate::shutdown::ShutdownNoticeReceiver;
use chrono::Utc;
use prometheus::{register_gauge, register_int_gauge};
use simple_process_stats::ProcessStats;
use tokio::time::Duration;

/// Provides metrics for CPU and memory usage.
pub async fn run_process_monitoring(mut shutdown_receiver: ShutdownNoticeReceiver) {
    let start_time_seconds = register_gauge!(
        "process_start_time_seconds",
        "UTC timestamp (in seconds) of when the process started."
    )
    .unwrap();
    let cpu_user_seconds_total = register_gauge!(
        "process_cpu_user_seconds_total",
        "Cumulative number of seconds spent executing in user mode"
    )
    .unwrap();
    let cpu_system_seconds_total = register_gauge!(
        "process_cpu_system_seconds_total",
        "Cumulative number of seconds spent executing in kernel mode"
    )
    .unwrap();
    let resident_memory_bytes = register_int_gauge!(
        "process_resident_memory_bytes",
        "Resident memory usage size as reported by the kernel, in bytes"
    )
    .unwrap();
    start_time_seconds.set(Utc::now().timestamp() as f64);

    let mut interval = tokio::time::interval(Duration::from_secs(10));
    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown_receiver.next_shutdown_notice(), if shutdown_receiver.may_have_more_notices() => {
                break;
            }
        }

        let system_stats = ProcessStats::get().await;
        let system_stats = match system_stats {
            Ok(system_stats) => system_stats,
            Err(e) => {
                tracing::error!("Monitoring: Failed to get CPU and Memory statistics: {}", e);
                continue;
            }
        };

        let user_seconds = system_stats.cpu_time_user.as_secs_f64();
        let kernel_seconds = system_stats.cpu_time_kernel.as_secs_f64();
        cpu_user_seconds_total.set(user_seconds);
        cpu_system_seconds_total.set(kernel_seconds);
        resident_memory_bytes.set(system_stats.memory_usage_bytes as i64);
    }
}
