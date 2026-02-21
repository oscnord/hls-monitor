use std::fmt::Write;
use std::sync::Arc;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;

use hls_core::ErrorType;

use crate::state::AppState;

pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut out = String::with_capacity(4096);

    let monitors: Vec<_> = state
        .monitors
        .iter()
        .map(|e| (*e.key(), Arc::clone(e.value())))
        .collect();

    writeln!(out, "# TYPE hls_monitor_info info").unwrap();
    writeln!(out, "# HELP hls_monitor_info Information about the HLS monitor").unwrap();
    for (id, m) in &monitors {
        writeln!(
            out,
            "hls_monitor_info{{monitor_id=\"{}\",created=\"{}\"}} 1",
            id,
            m.created_at().to_rfc3339()
        )
        .unwrap();
    }

    writeln!(out, "# TYPE hls_monitor_state stateset").unwrap();
    writeln!(out, "# HELP hls_monitor_state Current state of the HLS monitor").unwrap();
    for (id, m) in &monitors {
        let s = m.state().await.to_string();
        for variant in &["idle", "active", "stopping", "stopped"] {
            writeln!(
                out,
                "hls_monitor_state{{monitor_id=\"{}\",state=\"{}\"}} {}",
                id,
                variant,
                if s == *variant { 1 } else { 0 }
            )
            .unwrap();
        }
    }

    writeln!(out, "# TYPE hls_monitor_streams gauge").unwrap();
    writeln!(out, "# HELP hls_monitor_streams Number of streams being monitored").unwrap();
    for (id, m) in &monitors {
        let count = m.streams().await.len();
        writeln!(out, "hls_monitor_streams{{monitor_id=\"{}\"}} {}", id, count).unwrap();
    }

    writeln!(out, "# TYPE hls_monitor_total_errors gauge").unwrap();
    writeln!(out, "# HELP hls_monitor_total_errors Total number of errors in the buffer").unwrap();
    for (id, m) in &monitors {
        let count = m.get_errors().await.len();
        writeln!(out, "hls_monitor_total_errors{{monitor_id=\"{}\"}} {}", id, count).unwrap();
    }

    writeln!(out, "# TYPE hls_monitor_current_errors gauge").unwrap();
    writeln!(
        out,
        "# HELP hls_monitor_current_errors Current errors broken down by type and media type"
    )
    .unwrap();
    for (id, m) in &monitors {
        let errors = m.get_errors().await;
        let mut counts: std::collections::HashMap<(String, String, String), usize> =
            std::collections::HashMap::new();
        for e in &errors {
            *counts
                .entry((
                    format!("{}", e.error_type),
                    e.media_type.clone(),
                    e.stream_id.clone(),
                ))
                .or_default() += 1;
        }
        for ((et, mt, sid), count) in &counts {
            writeln!(
                out,
                "hls_monitor_current_errors{{monitor_id=\"{}\",error_type=\"{}\",media_type=\"{}\",stream_id=\"{}\"}} {}",
                id, et, mt, sid, count
            ).unwrap();
        }
    }

    writeln!(out, "# TYPE hls_monitor_last_check_timestamp_seconds gauge").unwrap();
    writeln!(
        out,
        "# HELP hls_monitor_last_check_timestamp_seconds Unix timestamp of the last check"
    )
    .unwrap();
    for (id, m) in &monitors {
        if let Some(t) = m.last_checked().await {
            let secs = t.timestamp() as f64 + (t.timestamp_subsec_millis() as f64 / 1000.0);
            writeln!(
                out,
                "hls_monitor_last_check_timestamp_seconds{{monitor_id=\"{}\"}} {:.3}",
                id, secs
            )
            .unwrap();
        }
    }

    writeln!(out, "# TYPE hls_monitor_uptime_seconds gauge").unwrap();
    writeln!(out, "# HELP hls_monitor_uptime_seconds Time since monitor was created").unwrap();
    for (id, m) in &monitors {
        let uptime = (chrono::Utc::now() - m.created_at()).num_milliseconds() as f64 / 1000.0;
        writeln!(
            out,
            "hls_monitor_uptime_seconds{{monitor_id=\"{}\"}} {:.3}",
            id, uptime
        )
        .unwrap();
    }

    writeln!(out, "# TYPE hls_monitor_stream_total_errors counter").unwrap();
    writeln!(
        out,
        "# HELP hls_monitor_stream_total_errors Total errors per stream since creation"
    )
    .unwrap();
    for (id, m) in &monitors {
        let totals = m.total_errors_per_stream().await;
        for (stream_id, count) in &totals {
            writeln!(
                out,
                "hls_monitor_stream_total_errors{{monitor_id=\"{}\",stream_id=\"{}\"}} {}",
                id, stream_id, count
            )
            .unwrap();
        }
    }

    writeln!(
        out,
        "# TYPE hls_monitor_stream_time_since_last_error_seconds gauge"
    )
    .unwrap();
    writeln!(
        out,
        "# HELP hls_monitor_stream_time_since_last_error_seconds Time since last error per stream"
    )
    .unwrap();
    for (id, m) in &monitors {
        let times = m.last_error_time_per_stream().await;
        for (stream_id, t) in &times {
            let since = (chrono::Utc::now() - *t).num_milliseconds() as f64 / 1000.0;
            writeln!(
                out,
                "hls_monitor_stream_time_since_last_error_seconds{{monitor_id=\"{}\",stream_id=\"{}\",last_error_time=\"{}\"}} {:.3}",
                id, stream_id, t.to_rfc3339(), since
            ).unwrap();
        }
    }

    writeln!(out, "# TYPE hls_monitor_manifest_errors counter").unwrap();
    writeln!(
        out,
        "# HELP hls_monitor_manifest_errors Total manifest fetch errors"
    )
    .unwrap();
    for (id, m) in &monitors {
        writeln!(
            out,
            "hls_monitor_manifest_errors{{monitor_id=\"{}\"}} {}",
            id,
            m.manifest_error_count().await
        )
        .unwrap();
    }

    writeln!(out, "# TYPE hls_monitor_manifest_error_types gauge").unwrap();
    writeln!(
        out,
        "# HELP hls_monitor_manifest_error_types Current manifest errors by media type"
    )
    .unwrap();
    for (id, m) in &monitors {
        let errors = m.get_errors().await;
        let mut type_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for e in errors
            .iter()
            .filter(|e| e.error_type == ErrorType::ManifestRetrieval)
        {
            *type_counts.entry(e.media_type.clone()).or_default() += 1;
        }
        for (mt, count) in &type_counts {
            writeln!(
                out,
                "hls_monitor_manifest_error_types{{monitor_id=\"{}\",media_type=\"{}\"}} {}",
                id, mt, count
            )
            .unwrap();
        }
    }

    writeln!(out, "# EOF").unwrap();

    (
        [(
            header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        out,
    )
}
