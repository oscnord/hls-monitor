#![forbid(unsafe_code)]

mod config;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tracing_subscriber::{fmt, EnvFilter};

use hls_core::{
    notification_channel, EventKind, HttpLoader, Monitor, MonitorConfig, MonitorError, StreamItem,
    WebhookDispatcher,
};

fn version_string() -> &'static str {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const GIT_HASH: &str = env!("GIT_HASH");

    if GIT_HASH.is_empty() {
        // Leak is fine — called once, lives for the program's lifetime.
        Box::leak(VERSION.to_string().into_boxed_str())
    } else {
        Box::leak(format!("{VERSION} ({GIT_HASH})").into_boxed_str())
    }
}

/// HLS stream monitor — detect playlist anomalies in real-time.
#[derive(Parser)]
#[command(name = "hls-monitor", version = version_string(), about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args)]
struct CheckArgs {
    /// Validate SCTE-35 CUE-OUT/CUE-IN ad break markers.
    #[arg(long, default_value_t = false)]
    scte35: bool,

    /// HTTP request timeout in milliseconds [default: 10000].
    #[arg(long)]
    request_timeout: Option<u64>,

    /// Max seconds a segment may exceed EXT-X-TARGETDURATION [default: 0.5].
    #[arg(long)]
    target_duration_tolerance: Option<f64>,

    /// Max allowed jump in EXT-X-MEDIA-SEQUENCE between polls [default: 5].
    #[arg(long)]
    mseq_gap_threshold: Option<u64>,

    /// Max media-sequence difference between variants before flagging drift [default: 3].
    #[arg(long)]
    variant_sync_drift_threshold: Option<u64>,

    /// Consecutive fetch failures before a variant is reported unavailable [default: 3].
    #[arg(long)]
    variant_failure_threshold: Option<u32>,

    /// Min ratio of a segment's duration to target duration before flagging [default: 0.5].
    #[arg(long)]
    segment_duration_anomaly_ratio: Option<f64>,

    /// Max concurrent variant playlist fetches [default: 4].
    #[arg(long)]
    max_concurrent_fetches: Option<usize>,
}

impl CheckArgs {
    fn to_monitor_config(&self) -> MonitorConfig {
        let mut config = MonitorConfig::default().with_scte35(self.scte35);
        if let Some(ms) = self.request_timeout {
            config.request_timeout = std::time::Duration::from_millis(ms);
        }
        if let Some(v) = self.target_duration_tolerance {
            config = config.with_target_duration_tolerance(v);
        }
        if let Some(v) = self.mseq_gap_threshold {
            config = config.with_mseq_gap_threshold(v);
        }
        if let Some(v) = self.variant_sync_drift_threshold {
            config = config.with_variant_sync_drift_threshold(v);
        }
        if let Some(v) = self.variant_failure_threshold {
            config = config.with_variant_failure_threshold(v);
        }
        if let Some(v) = self.segment_duration_anomaly_ratio {
            config = config.with_segment_duration_anomaly_ratio(v);
        }
        if let Some(v) = self.max_concurrent_fetches {
            config = config.with_max_concurrent_fetches(v);
        }
        config
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP API server.
    Serve {
        /// Listen address (e.g. 0.0.0.0:8080). Overrides config file.
        #[arg(short, long)]
        listen: Option<SocketAddr>,

        /// Path to TOML config file.
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Validate an HLS playlist tree once and exit with a report.
    Validate {
        /// Master playlist URL to validate.
        url: String,

        /// Output errors as JSON array.
        #[arg(long, default_value_t = false)]
        json: bool,

        #[command(flatten)]
        checks: CheckArgs,
    },
    /// Monitor a single stream from the command line (no API server).
    Watch {
        /// Master playlist URL to monitor.
        url: String,

        /// Stale limit in milliseconds.
        #[arg(long, default_value_t = 6000)]
        stale_limit: u64,

        /// Poll interval in milliseconds [default: stale_limit / 2].
        #[arg(long)]
        poll_interval: Option<u64>,

        /// Optional webhook URL to POST notifications to.
        #[arg(long)]
        webhook_url: Option<String>,

        #[command(flatten)]
        checks: CheckArgs,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { listen, config } => {
            run_serve(listen, config).await;
        }
        Commands::Validate { url, json, checks } => {
            fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
                )
                .init();
            run_validate(url, json, checks).await;
        }
        Commands::Watch {
            url,
            stale_limit,
            poll_interval,
            webhook_url,
            checks,
        } => {
            fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
                )
                .init();
            run_watch(url, stale_limit, poll_interval, webhook_url, checks).await;
        }
    }
}

async fn run_validate(url: String, json: bool, checks: CheckArgs) {
    let config = checks.to_monitor_config();
    let loader = Arc::new(HttpLoader::from_config(&config));
    let stream = StreamItem {
        id: "validate".to_string(),
        url: url.clone(),
    };

    let monitor = Monitor::new(vec![stream], config, loader, None);
    monitor.poll_once().await;
    let errors = monitor.get_errors().await;

    if json {
        print_errors_json(&errors);
    } else {
        print_errors_styled(&errors);
    }

    if errors.is_empty() {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}

fn print_errors_json(errors: &[MonitorError]) {
    let json = serde_json::to_string_pretty(errors).expect("MonitorError is Serialize");
    println!("{json}");
}

fn print_errors_styled(errors: &[MonitorError]) {
    if errors.is_empty() {
        eprintln!("{}", style("No violations found.").green().bold());
        return;
    }

    eprintln!(
        "{} {} violation(s) found:\n",
        style("!").red().bold(),
        errors.len()
    );

    for e in errors {
        eprintln!(
            "  {} {:<20} {} {}  {}",
            style(format!("{}", e.error_type)).red(),
            e.variant,
            style(&e.media_type).dim(),
            style(&e.stream_url).dim(),
            e.details,
        );
    }
}

async fn run_serve(listen_override: Option<SocketAddr>, config_path: Option<PathBuf>) {
    let app_config = if let Some(ref path) = config_path {
        match config::AppConfig::load(path) {
            Ok(c) => {
                init_tracing(&c.server.log_format);
                tracing::info!(path = %path.display(), "Loaded config file");
                Some(c)
            }
            Err(e) => {
                    init_tracing("pretty");
                tracing::error!("{}", e);
                std::process::exit(1);
            }
        }
    } else {
        init_tracing("pretty");
        None
    };

    let listen = listen_override
        .or(app_config.as_ref().map(|c| c.server.listen))
        .unwrap_or_else(|| "0.0.0.0:8080".parse().unwrap());

    let default_config = app_config
        .as_ref()
        .map(|c| c.defaults.to_monitor_config())
        .unwrap_or_default();

    let webhooks = app_config
        .as_ref()
        .map(|c| c.webhook.clone())
        .unwrap_or_default();

    let (notification_tx, notification_rx) = notification_channel();

    let state = hls_api::state::AppState::new()
        .with_default_config(default_config.clone())
        .with_notification_tx(notification_tx.clone());

    let shared_client = HttpLoader::build_client(default_config.request_timeout);

    let webhook_handle = if !webhooks.is_empty() {
        let dispatcher = WebhookDispatcher::new(notification_rx, webhooks, shared_client.clone());
        let handle = tokio::spawn(dispatcher.run());
        tracing::info!("Webhook dispatcher started");
        Some(handle)
    } else {
        let handle = tokio::spawn(async move {
            let mut rx = notification_rx;
            while rx.recv().await.is_some() {}
        });
        Some(handle)
    };

    if let Some(ref app_config) = app_config {
        for monitor_def in &app_config.monitor {
            let config = monitor_def.to_monitor_config(&app_config.defaults);
            let loader = Arc::new(HttpLoader::from_config_with_client(
                &config,
                shared_client.clone(),
            ));
            let streams = monitor_def.to_stream_items();
            let monitor = Monitor::new(streams, config, loader, Some(notification_tx.clone()))
                .with_monitor_id(&monitor_def.id);

            let monitor_id = monitor_def.id.clone();
            let uuid = monitor.id();

            if let Err(e) = monitor.start().await {
                tracing::error!(monitor_id = %monitor_id, error = %e, "Failed to start monitor");
                continue;
            }

            state.monitors.insert(uuid, Arc::new(monitor));
            tracing::info!(monitor_id = %monitor_id, uuid = %uuid, "Monitor started from config");
        }
    }

    let monitors = state.monitors.clone();

    tracing::info!(%listen, "Starting HLS Monitor API server");
    if let Err(e) = hls_api::serve_with_state(listen, state, hls_api::shutdown_signal()).await {
        tracing::error!(error = %e, "Server failed");
        std::process::exit(1);
    }

    tracing::info!("Shutdown signal received, stopping monitors...");

    let mut stop_count = 0u32;
    for entry in monitors.iter() {
        entry.value().stop().await;
        stop_count += 1;
    }
    tracing::info!(count = stop_count, "All monitors stopped");

    drop(notification_tx);

    if let Some(handle) = webhook_handle {
        match tokio::time::timeout(std::time::Duration::from_secs(5), handle).await {
            Ok(_) => tracing::info!("Webhook dispatcher shut down"),
            Err(_) => tracing::warn!("Webhook dispatcher did not shut down in time, aborting"),
        }
    }

    tracing::info!("Shutdown complete");
}

async fn run_watch(
    url: String,
    stale_limit: u64,
    poll_interval: Option<u64>,
    webhook_url: Option<String>,
    checks: CheckArgs,
) {
    let config = {
        let mut c = checks.to_monitor_config().with_stale_limit(stale_limit);
        if let Some(pi) = poll_interval {
            c = c.with_poll_interval(pi);
        }
        c
    };

    let poll_ms = config.poll_interval.as_millis();

    let notification_tx = if let Some(ref wh_url) = webhook_url {
        let (tx, rx) = notification_channel();
        let wh_config = hls_core::WebhookConfig {
            url: wh_url.clone(),
            events: vec![],
            timeout_ms: 5000,
            max_retries: 2,
            secret: None,
        };
        let client = HttpLoader::build_client(config.request_timeout);
        let dispatcher = WebhookDispatcher::new(rx, vec![wh_config], client);
        tokio::spawn(dispatcher.run());
        Some(tx)
    } else {
        None
    };

    let scte35_enabled = config.scte35_enabled;
    let loader = Arc::new(HttpLoader::from_config(&config));
    let stream = StreamItem {
        id: "stream_1".to_string(),
        url: url.clone(),
    };

    let monitor = Monitor::new(vec![stream], config, loader, notification_tx);

    let multi = MultiProgress::new();
    let msg_style = ProgressStyle::with_template("{msg}").expect("valid template");

    multi
        .println(format!(
            "{} {}",
            style("hls-monitor").bold(),
            style(env!("CARGO_PKG_VERSION")).dim()
        ))
        .ok();
    multi
        .println(format!(
            "  {} {}",
            style("url:   ").dim(),
            style(&url).bold()
        ))
        .ok();
    multi
        .println(format!("  {} {}ms", style("poll:  ").dim(), poll_ms))
        .ok();
    multi
        .println(format!("  {} {}ms", style("stale: ").dim(), stale_limit))
        .ok();
    multi
        .println(format!("  {} {}", style("scte35:").dim(), scte35_enabled))
        .ok();
    if let Some(ref wh) = webhook_url {
        multi
            .println(format!("  {} {}", style("webhook:").dim(), wh))
            .ok();
    }
    multi.println("").ok();
    multi
        .println(format!("{}", style("Press Ctrl+C to stop").dim()))
        .ok();
    multi.println("").ok();

    monitor.start().await.expect("Failed to start monitor");

    let status_bar = multi.add(ProgressBar::new_spinner().with_style(msg_style.clone()));
    status_bar.set_message(format!(
        "{}\n  {}",
        format_separator(0),
        style("Waiting for first manifest fetch...").dim()
    ));

    let mut last_error_count = 0usize;
    let mut last_event_count = 0usize;
    let mut poll_num = 0u64;

    let shutdown = hls_api::shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(poll_ms as u64)) => {}
            _ = &mut shutdown => {
                status_bar.finish_and_clear();
                multi.println(format!("\n{}", style("Monitor stopped.").dim())).ok();
                monitor.stop().await;
                return;
            }
        }

        poll_num += 1;

        let events = monitor.get_events().await;
        if events.len() > last_event_count {
            let new_count = events.len() - last_event_count;
            let new_events: Vec<_> = events[..new_count].iter().rev().collect();
            for ev in new_events {
                if ev.kind == EventKind::ManifestUpdated {
                    continue;
                }
                let ts = ev.timestamp.format("%H:%M:%S");
                let kind_str = format!("{:<12}", format!("{}", ev.kind));
                let colored_kind = match ev.kind {
                    EventKind::CueOutStarted
                    | EventKind::CueOutCont
                    | EventKind::DiscontinuityChanged => style(kind_str).yellow(),
                    _ => style(kind_str).green(),
                };
                multi
                    .println(format!(
                        "  {}  {} {} {}  {}",
                        style(ts).dim(),
                        colored_kind,
                        ev.variant_key,
                        style(&ev.media_type).dim(),
                        ev.details
                    ))
                    .ok();
            }
            last_event_count = events.len();
        }

        let errors = monitor.get_errors().await;
        if errors.len() > last_error_count {
            let new_count = errors.len() - last_error_count;
            let new_errors: Vec<_> = errors[..new_count].iter().rev().collect();
            for e in new_errors {
                let ts = e.timestamp.format("%H:%M:%S");
                multi
                    .println(format!(
                        "  {}  {} {:<20} {} {}  {}",
                        style(ts).dim(),
                        style("ERROR").red().bold(),
                        style(format!("{}", e.error_type)).red(),
                        e.variant,
                        style(&e.media_type).dim(),
                        e.details
                    ))
                    .ok();
            }
            last_error_count = errors.len();
        }

        let statuses = monitor.get_stream_status().await;
        let mut status_lines = vec![format_separator(poll_num)];

        if statuses.is_empty() {
            status_lines.push(format!(
                "  {}",
                style("Waiting for first manifest fetch...").dim()
            ));
        } else {
            for ss in &statuses {
                let mut variants: Vec<_> = ss.variants.iter().collect();
                variants.sort_by(|a, b| a.variant_key.cmp(&b.variant_key));
                for v in variants {
                    if v.consecutive_failures > 0 && v.segment_count == 0 {
                        status_lines.push(format!(
                            "  {:<16} {:<6} {}",
                            v.variant_key,
                            style(&v.media_type).dim(),
                            style(format!("FAILING ({}x)", v.consecutive_failures))
                                .red()
                                .bold(),
                        ));
                        continue;
                    }
                    let mut badges = String::new();
                    if v.in_cue_out {
                        badges.push_str(&format!("  {}", style("CUE-OUT").yellow().bold()));
                    }
                    if v.consecutive_failures > 0 {
                        badges.push_str(&format!(
                            "  {}",
                            style(format!("RETRY ({}x)", v.consecutive_failures))
                                .red()
                        ));
                    }
                    status_lines.push(format!(
                        "  {:<16} {:<6} mseq={:<10} segs={:<4} {:.1}s{}",
                        v.variant_key,
                        style(&v.media_type).dim(),
                        v.media_sequence,
                        v.segment_count,
                        v.playlist_duration_secs,
                        badges,
                    ));
                }
            }
        }

        status_bar.set_message(status_lines.join("\n"));
    }
}

fn format_separator(poll_num: u64) -> String {
    let label = if poll_num == 0 {
        String::new()
    } else {
        format!(" poll {} ", poll_num)
    };
    let width = 54usize.saturating_sub(label.len());
    format!(
        "{}{}{}",
        style("──").dim(),
        style(label).dim().bold(),
        style("─".repeat(width)).dim()
    )
}

fn init_tracing(log_format: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    match log_format {
        "json" => {
            fmt()
                .with_env_filter(filter)
                .json()
                .init();
        }
        _ => {
            fmt()
                .with_env_filter(filter)
                .init();
        }
    }
}
