use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::time::Duration;
use tracing::info;

pub fn setup_metrics(settings: &crate::config::Settings) -> Result<(), anyhow::Error> {
    if !settings.metrics.enabled {
        info!("Metrics disabled");
        return Ok(());
    }

    let builder = PrometheusBuilder::new()
        .with_http_listener(std::net::SocketAddr::from(([0, 0, 0, 0], settings.metrics.port.unwrap_or(9090))))
        .add_global_label("service", "twitch-music-bot")
        .add_global_label("version", env!("CARGO_PKG_VERSION"));

    builder.install()?;
    info!("Metrics server started on port {}", settings.metrics.port.unwrap_or(9090));
    Ok(())
}

pub fn record_twitch_message(command: &str, success: bool) {
    counter!("twitch_messages_total", "command" => command.to_string(), "success" => success.to_string()).increment(1);
}

pub fn record_queue_operation(operation: &str, success: bool) {
    counter!("queue_operations_total", "operation" => operation.to_string(), "success" => success.to_string()).increment(1);
}

pub fn record_music_search(source: &str, results: usize, duration: Duration) {
    counter!("music_searches_total", "source" => source.to_string()).increment(1);
    histogram!("music_search_duration_seconds", "source" => source.to_string()).record(duration.as_secs_f64());
    gauge!("music_search_results", "source" => source.to_string()).set(results as f64);
}

pub fn record_stream_url_fetch(source: &str, success: bool, duration: Duration) {
    counter!("stream_url_fetches_total", "source" => source.to_string(), "success" => success.to_string()).increment(1);
    if success {
        histogram!("stream_url_fetch_duration_seconds", "source" => source.to_string()).record(duration.as_secs_f64());
    }
}

pub fn record_music_error(source: &str) {
    counter!("music_errors_total", "source" => source.to_string()).increment(1);
}

pub fn record_active_connections(count: usize) {
    gauge!("overlay_connections_active").set(count as f64);
}

pub fn record_queue_size(streamer_id: &str, size: usize) {
    gauge!("queue_size", "streamer_id" => streamer_id.to_string()).set(size as f64);
}

pub fn record_current_song_duration(streamer_id: &str, duration: f64) {
    gauge!("current_song_duration_seconds", "streamer_id" => streamer_id.to_string()).set(duration);
}

pub fn record_memory_usage() {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_memory();
    gauge!("memory_usage_bytes").set(sys.used_memory() as f64);
    gauge!("memory_total_bytes").set(sys.total_memory() as f64);
}

pub fn start_metrics_collection() {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            record_memory_usage();
        }
    });
}