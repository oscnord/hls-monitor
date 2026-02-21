use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use futures::stream::{self, StreamExt};
use m3u8_rs::Playlist;
use rand::Rng;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::MonitorConfig;
use crate::loader::ManifestLoader;
use crate::monitor::checks::stale_manifest::check_stale;
use crate::monitor::checks::stream_check;
use crate::monitor::checks::{default_checks, default_stream_checks, Check};
use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::event::{EventKind, MonitorEvent};
use crate::monitor::state::*;
use crate::webhook::Notification;

pub struct Monitor {
    id: Uuid,
    monitor_id: String,
    config: MonitorConfig,
    streams: Arc<RwLock<Vec<StreamItem>>>,
    stream_data: Arc<RwLock<HashMap<String, StreamData>>>,
    state: Arc<RwLock<MonitorState>>,
    loader: Arc<dyn ManifestLoader>,
    checks: Arc<Vec<Box<dyn Check>>>,
    stream_checks: Arc<Vec<Box<dyn stream_check::StreamCheck>>>,
    created_at: chrono::DateTime<Utc>,
    last_checked: Arc<RwLock<Option<chrono::DateTime<Utc>>>>,
    total_errors_per_stream: Arc<RwLock<HashMap<String, u64>>>,
    last_error_time_per_stream: Arc<RwLock<HashMap<String, chrono::DateTime<Utc>>>>,
    manifest_error_count: Arc<RwLock<u64>>,
    notification_tx: Option<UnboundedSender<Notification>>,
}

impl Monitor {
    pub fn new(
        streams: Vec<StreamItem>,
        config: MonitorConfig,
        loader: Arc<dyn ManifestLoader>,
        notification_tx: Option<UnboundedSender<Notification>>,
    ) -> Self {
        let checks = default_checks(&config);
        let stream_checks = default_stream_checks(&config);
        let id = Uuid::new_v4();
        Self {
            monitor_id: id.to_string(),
            id,
            config,
            streams: Arc::new(RwLock::new(streams)),
            stream_data: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(RwLock::new(MonitorState::Idle)),
            loader,
            checks: Arc::new(checks),
            stream_checks: Arc::new(stream_checks),
            created_at: Utc::now(),
            last_checked: Arc::new(RwLock::new(None)),
            total_errors_per_stream: Arc::new(RwLock::new(HashMap::new())),
            last_error_time_per_stream: Arc::new(RwLock::new(HashMap::new())),
            manifest_error_count: Arc::new(RwLock::new(0)),
            notification_tx,
        }
    }

    pub fn with_monitor_id(mut self, monitor_id: impl Into<String>) -> Self {
        self.monitor_id = monitor_id.into();
        self
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn monitor_id(&self) -> &str {
        &self.monitor_id
    }

    pub fn created_at(&self) -> chrono::DateTime<Utc> {
        self.created_at
    }

    pub fn config(&self) -> &MonitorConfig {
        &self.config
    }

    pub async fn state(&self) -> MonitorState {
        *self.state.read().await
    }

    pub async fn last_checked(&self) -> Option<chrono::DateTime<Utc>> {
        *self.last_checked.read().await
    }

    pub async fn streams(&self) -> Vec<StreamItem> {
        self.streams.read().await.clone()
    }

    pub async fn get_errors(&self) -> Vec<MonitorError> {
        let data = self.stream_data.read().await;
        let mut all_errors = Vec::new();
        for stream_data in data.values() {
            all_errors.extend(stream_data.errors.list());
        }
        // Sort newest first
        all_errors.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all_errors
    }

    pub async fn clear_errors(&self) {
        let mut data = self.stream_data.write().await;
        for stream_data in data.values_mut() {
            stream_data.errors.clear();
        }
    }

    pub async fn total_errors_per_stream(&self) -> HashMap<String, u64> {
        self.total_errors_per_stream.read().await.clone()
    }

    pub async fn last_error_time_per_stream(&self) -> HashMap<String, chrono::DateTime<Utc>> {
        self.last_error_time_per_stream.read().await.clone()
    }

    pub async fn manifest_error_count(&self) -> u64 {
        *self.manifest_error_count.read().await
    }

    pub async fn get_stream_status(&self) -> Vec<StreamStatus> {
        let streams = self.streams.read().await;
        let data = self.stream_data.read().await;
        let mut result = Vec::with_capacity(streams.len());

        for stream in streams.iter() {
            let base_url = get_base_url(&stream.url);
            if let Some(sd) = data.get(&base_url) {
                let mut variants: Vec<VariantStatus> = sd
                    .variants
                    .iter()
                    .map(|(key, vs)| VariantStatus {
                        variant_key: key.clone(),
                        media_type: vs.media_type.clone(),
                        media_sequence: vs.media_sequence,
                        discontinuity_sequence: vs.discontinuity_sequence,
                        segment_count: vs.segment_uris.len(),
                        playlist_duration_secs: vs.duration,
                        in_cue_out: vs.in_cue_out,
                        cue_out_duration: vs.cue_out_duration,
                        cue_out_count: vs.cue_out_count,
                        cue_in_count: vs.cue_in_count,
                        consecutive_failures: sd
                            .variant_failures
                            .get(key)
                            .copied()
                            .unwrap_or(0),
                    })
                    .collect();

                for (key, media_type) in &sd.known_variants {
                    if !sd.variants.contains_key(key) {
                        let failures = sd.variant_failures.get(key).copied().unwrap_or(0);
                        variants.push(VariantStatus {
                            variant_key: key.clone(),
                            media_type: media_type.clone(),
                            media_sequence: 0,
                            discontinuity_sequence: 0,
                            segment_count: 0,
                            playlist_duration_secs: 0.0,
                            in_cue_out: false,
                            cue_out_duration: None,
                            cue_out_count: 0,
                            cue_in_count: 0,
                            consecutive_failures: failures,
                        });
                    }
                }

                result.push(StreamStatus {
                    stream_id: stream.id.clone(),
                    stream_url: stream.url.clone(),
                    last_fetch: sd.last_fetch,
                    last_content_change: sd.last_content_change,
                    error_count: sd.errors.len(),
                    variants,
                });
            }
        }
        result
    }

    pub async fn get_events(&self) -> Vec<MonitorEvent> {
        let data = self.stream_data.read().await;
        let mut all_events = Vec::new();
        for sd in data.values() {
            all_events.extend(sd.events.list());
        }
        // Sort newest first
        all_events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all_events
    }

    pub async fn add_streams(&self, new_streams: Vec<StreamItem>) {
        let mut streams = self.streams.write().await;
        streams.extend(new_streams);
    }

    pub async fn remove_stream(&self, stream_id: &str) -> Result<(), String> {
        let mut streams = self.streams.write().await;
        let idx = streams.iter().position(|s| s.id == stream_id);
        match idx {
            Some(i) => {
                let removed = streams.remove(i);
                drop(streams);

                let base_url = get_base_url(&removed.url);
                self.stream_data.write().await.remove(&base_url);
                self.total_errors_per_stream.write().await.remove(stream_id);
                self.last_error_time_per_stream.write().await.remove(stream_id);
                Ok(())
            }
            None => Err(format!("Stream with ID '{}' not found", stream_id)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        {
            let mut state = self.state.write().await;
            if *state == MonitorState::Active {
                return Ok(());
            }
            *state = MonitorState::Active;
        }

        info!(monitor_id = %self.id, "Starting monitor");

        let state = Arc::clone(&self.state);
        let streams = Arc::clone(&self.streams);
        let stream_data = Arc::clone(&self.stream_data);
        let loader = Arc::clone(&self.loader);
        let checks = Arc::clone(&self.checks);
        let stream_checks = Arc::clone(&self.stream_checks);
        let config = self.config.clone();
        let last_checked = Arc::clone(&self.last_checked);
        let total_errors = Arc::clone(&self.total_errors_per_stream);
        let last_error_times = Arc::clone(&self.last_error_time_per_stream);
        let manifest_err_count = Arc::clone(&self.manifest_error_count);
        let notification_tx = self.notification_tx.clone();
        let monitor_id = self.monitor_id.clone();

        tokio::spawn(async move {
            loop {
                {
                    let current_state = *state.read().await;
                    if current_state != MonitorState::Active {
                        let mut s = state.write().await;
                        *s = MonitorState::Stopped;
                        info!("Monitor stopped");
                        break;
                    }
                }

                let current_streams = streams.read().await.clone();
                *last_checked.write().await = Some(Utc::now());

                for stream in &current_streams {
                    let errors = poll_stream(
                        stream,
                        &loader,
                        &checks,
                        &stream_checks,
                        &stream_data,
                        &config,
                        &notification_tx,
                        &monitor_id,
                    )
                    .await;

                    if !errors.is_empty() {
                        let now = Utc::now();
                        let mut totals = total_errors.write().await;
                        let mut times = last_error_times.write().await;
                        let mut mfcount = manifest_err_count.write().await;

                        for e in &errors {
                            *totals.entry(e.stream_id.clone()).or_insert(0) += 1;
                            times.insert(e.stream_id.clone(), now);
                            if e.error_type == ErrorType::ManifestRetrieval {
                                *mfcount += 1;
                            }
                        }
                    }
                }

                let base_ms = config.poll_interval.as_millis() as u64;
                let jitter_range = base_ms / 7;
                let jitter = if jitter_range > 0 {
                    rand::thread_rng().gen_range(0..jitter_range * 2) as i64 - jitter_range as i64
                } else {
                    0
                };
                let sleep_ms = (base_ms as i64 + jitter).max(1) as u64;
                tokio::time::sleep(tokio::time::Duration::from_millis(sleep_ms)).await;
            }
        });

        Ok(())
    }

    pub async fn stop(&self) {
        let mut state = self.state.write().await;
        if *state == MonitorState::Active {
            *state = MonitorState::Stopping;
            info!(monitor_id = %self.id, "Stopping monitor");
        }
    }

    pub async fn poll_once(&self) {
        let current_streams = self.streams.read().await.clone();
        *self.last_checked.write().await = Some(Utc::now());

        for stream in &current_streams {
            let errors = poll_stream(
                stream,
                &self.loader,
                &self.checks,
                &self.stream_checks,
                &self.stream_data,
                &self.config,
                &self.notification_tx,
                &self.monitor_id,
            )
            .await;

            if !errors.is_empty() {
                let now = Utc::now();
                let mut totals = self.total_errors_per_stream.write().await;
                let mut times = self.last_error_time_per_stream.write().await;
                let mut mfcount = self.manifest_error_count.write().await;

                for e in &errors {
                    *totals.entry(e.stream_id.clone()).or_insert(0) += 1;
                    times.insert(e.stream_id.clone(), now);
                    if e.error_type == ErrorType::ManifestRetrieval {
                        *mfcount += 1;
                    }
                }
            }
        }
    }
}

pub fn get_base_url(url: &str) -> String {
    match url.rfind('/') {
        Some(idx) => format!("{}/", &url[..idx]),
        None => url.to_string(),
    }
}

fn build_playlist_url(base_url: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    format!("{}{}", base_url, path)
}

fn record_error(
    sd: &mut StreamData,
    all_errors: &mut Vec<MonitorError>,
    tx: &Option<UnboundedSender<Notification>>,
    monitor_id: &str,
    error: MonitorError,
) {
    sd.errors.push(error.clone());
    if let Some(tx) = tx {
        let _ = tx.send(Notification::Error {
            monitor_id: monitor_id.to_string(),
            error: error.clone(),
        });
    }
    all_errors.push(error);
}

fn record_event(
    sd: &mut StreamData,
    tx: &Option<UnboundedSender<Notification>>,
    monitor_id: &str,
    event: MonitorEvent,
) {
    sd.events.push(event.clone());
    if let Some(tx) = tx {
        let _ = tx.send(Notification::Event {
            monitor_id: monitor_id.to_string(),
            event,
        });
    }
}

fn segment_to_snapshot(seg: &m3u8_rs::MediaSegment) -> SegmentSnapshot {
    let cue_out = seg.unknown_tags.iter().any(|t| {
        t.tag.contains("X-CUE-OUT") && !t.tag.contains("X-CUE-OUT-CONT")
    });
    let cue_in = seg.unknown_tags.iter().any(|t| t.tag.contains("X-CUE-IN"));
    let cue_out_cont = seg.unknown_tags.iter().find_map(|t| {
        if t.tag.contains("X-CUE-OUT-CONT") {
            t.rest.clone()
        } else {
            None
        }
    });
    let gap = seg.unknown_tags.iter().any(|t| t.tag == "X-GAP");
    let daterange = seg.daterange.as_ref().map(|dr| DateRangeSnapshot {
        id: dr.id.clone(),
        class: dr.class.clone(),
        start_date: dr.start_date,
        end_date: dr.end_date,
        duration: dr.duration,
        end_on_next: dr.end_on_next,
    });

    SegmentSnapshot {
        uri: seg.uri.clone(),
        duration: seg.duration as f64,
        discontinuity: seg.discontinuity,
        cue_out,
        cue_in,
        cue_out_cont,
        gap,
        program_date_time: seg.program_date_time,
        daterange,
    }
}

fn segment_to_info(seg: &m3u8_rs::MediaSegment) -> SegmentInfo {
    SegmentInfo {
        uri: seg.uri.clone(),
        discontinuity: seg.discontinuity,
    }
}

fn playlist_to_snapshot(pl: &m3u8_rs::MediaPlaylist) -> PlaylistSnapshot {
    let segments: Vec<SegmentSnapshot> = pl.segments.iter().map(segment_to_snapshot).collect();
    let duration: f64 = segments.iter().map(|s| s.duration).sum();
    let cue_out_count = segments.iter().filter(|s| s.cue_out).count();
    let cue_in_count = segments.iter().filter(|s| s.cue_in).count();
    let has_cue_out = cue_out_count > 0;
    let has_gaps = segments.iter().any(|s| s.gap);

    PlaylistSnapshot {
        media_sequence: pl.media_sequence,
        discontinuity_sequence: pl.discontinuity_sequence,
        segments,
        duration,
        cue_out_count,
        cue_in_count,
        has_cue_out,
        cue_out_duration: None,
        target_duration: pl.target_duration as f64,
        version: pl.version.and_then(|v| u16::try_from(v).ok()),
        playlist_type: pl.playlist_type.as_ref().map(|pt| pt.to_string()),
        has_gaps,
    }
}

fn variant_key(variant: &m3u8_rs::VariantStream) -> String {
    if variant.is_i_frame {
        format!("iframe_{}", variant.bandwidth)
    } else {
        variant.bandwidth.to_string()
    }
}

fn media_key(media: &m3u8_rs::AlternativeMedia) -> String {
    let group = &media.group_id;
    let lang = media.language.as_deref().unwrap_or(&media.name);
    format!("{};{}", group, lang)
}

#[allow(clippy::too_many_arguments)]
async fn poll_stream(
    stream: &StreamItem,
    loader: &Arc<dyn ManifestLoader>,
    checks: &Arc<Vec<Box<dyn Check>>>,
    stream_checks: &Arc<Vec<Box<dyn stream_check::StreamCheck>>>,
    stream_data: &Arc<RwLock<HashMap<String, StreamData>>>,
    config: &MonitorConfig,
    notification_tx: &Option<UnboundedSender<Notification>>,
    monitor_id: &str,
) -> Vec<MonitorError> {
    let base_url = get_base_url(&stream.url);
    let mut all_errors = Vec::new();

    let master_body = match loader.load(&stream.url).await {
        Ok(body) => body,
        Err(e) => {
            if e.is_last_retry() {
                let error = MonitorError::new(
                    ErrorType::ManifestRetrieval,
                    "MASTER",
                    "master",
                    format!("Failed to fetch master manifest: {}", e),
                    &stream.url,
                    &stream.id,
                )
                .with_status_code(e.status_code().unwrap_or(0));

                let mut data = stream_data.write().await;
                let sd = data
                    .entry(base_url.clone())
                    .or_insert_with(|| StreamData::new(config.error_limit, config.event_limit));
                record_error(sd, &mut all_errors, notification_tx, monitor_id, error);
            } else {
                warn!(stream_url = %stream.url, error = %e, "Transient master manifest error");
            }
            return all_errors;
        }
    };

    let master = match m3u8_rs::parse_playlist(master_body.as_bytes()) {
        Ok((_, Playlist::MasterPlaylist(pl))) => pl,
        Ok((_, Playlist::MediaPlaylist(_))) => {
            debug!(stream_url = %stream.url, "URL points to media playlist, not master");
            return all_errors;
        }
        Err(e) => {
            let error = MonitorError::new(
                ErrorType::ManifestRetrieval,
                "MASTER",
                "master",
                format!("Failed to parse master manifest: {}", e),
                &stream.url,
                &stream.id,
            );
            let mut data = stream_data.write().await;
            let sd = data
                .entry(base_url.clone())
                .or_insert_with(|| StreamData::new(config.error_limit, config.event_limit));
            record_error(sd, &mut all_errors, notification_tx, monitor_id, error);
            return all_errors;
        }
    };

    let mut variant_targets: Vec<(String, String, String)> = Vec::new();

    for variant in &master.variants {
        let url = build_playlist_url(&base_url, &variant.uri);
        let key = variant_key(variant);
        variant_targets.push((url, key, "VIDEO".to_string()));
    }

    for media in &master.alternatives {
        if let Some(ref uri) = media.uri {
            let url = build_playlist_url(&base_url, uri);
            let key = media_key(media);
            let mt = media.media_type.to_string();
            variant_targets.push((url, key, mt));
        }
    }

    let concurrency = config.max_concurrent_fetches.max(1);
    let fetch_futures: Vec<_> = variant_targets
        .iter()
        .enumerate()
        .map(|(i, (url, _, _))| {
            let loader = Arc::clone(loader);
            let url = url.clone();
            async move { (i, loader.load(&url).await) }
        })
        .collect();
    let results: Vec<(usize, Result<String, crate::loader::LoadError>)> =
        stream::iter(fetch_futures)
            .buffer_unordered(concurrency)
            .collect()
            .await;

    let mut content_changed = false;

    {
        let mut data = stream_data.write().await;
        let sd = data
            .entry(base_url.clone())
            .or_insert_with(|| StreamData::new(config.error_limit, config.event_limit));

        for (_, key, media_type) in &variant_targets {
            sd.known_variants
                .entry(key.clone())
                .or_insert_with(|| media_type.clone());
        }

        for (i, result) in results.into_iter() {
            let (variant_url, variant_key_str, media_type) = &variant_targets[i];

            let variant_body = match result {
                Ok(body) => body,
                Err(e) => {
                    let error = MonitorError::new(
                        ErrorType::ManifestRetrieval,
                        media_type.as_str(),
                        variant_key_str.as_str(),
                        format!("Failed to fetch variant manifest: {}", e),
                        base_url.as_str(),
                        stream.id.as_str(),
                    )
                    .with_status_code(e.status_code().unwrap_or(0));
                    record_error(sd, &mut all_errors, notification_tx, monitor_id, error);
                    *sd.variant_failures.entry(variant_key_str.clone()).or_insert(0) += 1;
                    continue;
                }
            };

            let media_playlist = match m3u8_rs::parse_media_playlist_res(variant_body.as_bytes()) {
                Ok(pl) => pl,
                Err(e) => {
                    let error = MonitorError::new(
                        ErrorType::ManifestRetrieval,
                        media_type.as_str(),
                        variant_key_str.as_str(),
                        format!("Failed to parse variant manifest {}: {}", variant_url, e),
                        base_url.as_str(),
                        stream.id.as_str(),
                    );
                    record_error(sd, &mut all_errors, notification_tx, monitor_id, error);
                    *sd.variant_failures.entry(variant_key_str.clone()).or_insert(0) += 1;
                    continue;
                }
            };

            sd.variant_failures.remove(variant_key_str);

            let snapshot = playlist_to_snapshot(&media_playlist);

            if let Some(prev_state) = sd.variants.get(variant_key_str.as_str()) {
                if snapshot.media_sequence != prev_state.media_sequence
                    || snapshot.segments.len() != prev_state.segment_uris.len()
                {
                    content_changed = true;
                }

                let ctx = CheckContext {
                    stream_url: base_url.clone(),
                    stream_id: stream.id.clone(),
                    media_type: media_type.clone(),
                    variant_key: variant_key_str.clone(),
                };

                let mut check_errors_batch = Vec::new();
                for check in checks.iter() {
                    check_errors_batch.extend(check.check(prev_state, &snapshot, &ctx));
                }

                // FIX: TS version always set next_is_discontinuity to false (missing else-block)
                let next_is_disc = if let Some(first) = snapshot.segments.first() {
                    if first.discontinuity {
                        true
                    } else {
                        snapshot.segments.get(1).is_some_and(|s| s.discontinuity)
                    }
                } else {
                    false
                };

                let has_cue_out = snapshot.has_cue_out;
                let has_cue_in = snapshot.cue_in_count > 0;
                let was_in_cue_out = prev_state.in_cue_out;
                let prev_mseq = prev_state.media_sequence;
                let prev_dseq = prev_state.discontinuity_sequence;

                let new_in_cue_out = if has_cue_out {
                    !has_cue_in
                } else if has_cue_in {
                    false
                } else {
                    was_in_cue_out
                };

                let new_state = VariantState {
                    media_type: media_type.clone(),
                    media_sequence: snapshot.media_sequence,
                    segment_uris: snapshot.segments.iter().map(|s| s.uri.clone()).collect(),
                    discontinuity_sequence: snapshot.discontinuity_sequence,
                    next_is_discontinuity: next_is_disc,
                    prev_segments: media_playlist.segments.iter().map(segment_to_info).collect(),
                    duration: snapshot.duration,
                    cue_out_count: snapshot.cue_out_count,
                    cue_in_count: snapshot.cue_in_count,
                    in_cue_out: new_in_cue_out,
                    cue_out_duration: snapshot.cue_out_duration,
                    version: snapshot.version,
                };
                sd.variants.insert(variant_key_str.clone(), new_state);

                for e in check_errors_batch {
                    record_error(sd, &mut all_errors, notification_tx, monitor_id, e);
                }

                if snapshot.media_sequence != prev_mseq {
                    record_event(sd, notification_tx, monitor_id, MonitorEvent::new(
                        EventKind::ManifestUpdated,
                        media_type.as_str(),
                        variant_key_str.as_str(),
                        format!(
                            "mseq {} -> {}, {} segments, {:.1}s",
                            prev_mseq,
                            snapshot.media_sequence,
                            snapshot.segments.len(),
                            snapshot.duration,
                        ),
                        stream.id.as_str(),
                    ));
                }

                if !was_in_cue_out && has_cue_out {
                    let dur_detail = snapshot
                        .cue_out_duration
                        .map(|d| format!(" duration={:.1}s", d))
                        .unwrap_or_default();
                    record_event(sd, notification_tx, monitor_id, MonitorEvent::new(
                        EventKind::CueOutStarted,
                        media_type.as_str(),
                        variant_key_str.as_str(),
                        format!("Ad break started at mseq {}{}", snapshot.media_sequence, dur_detail),
                        stream.id.as_str(),
                    ));
                }

                if was_in_cue_out && has_cue_in {
                    record_event(sd, notification_tx, monitor_id, MonitorEvent::new(
                        EventKind::CueInReturned,
                        media_type.as_str(),
                        variant_key_str.as_str(),
                        format!("Ad break ended at mseq {}", snapshot.media_sequence),
                        stream.id.as_str(),
                    ));
                }

                for seg in &snapshot.segments {
                    if let Some(ref cont_val) = seg.cue_out_cont {
                        record_event(sd, notification_tx, monitor_id, MonitorEvent::new(
                            EventKind::CueOutCont,
                            media_type.as_str(),
                            variant_key_str.as_str(),
                            format!("CUE-OUT-CONT: {}", cont_val),
                            stream.id.as_str(),
                        ));
                        break;
                    }
                }

                if snapshot.discontinuity_sequence != prev_dseq {
                    record_event(sd, notification_tx, monitor_id, MonitorEvent::new(
                        EventKind::DiscontinuityChanged,
                        media_type.as_str(),
                        variant_key_str.as_str(),
                        format!(
                            "dseq {} -> {}",
                            prev_dseq,
                            snapshot.discontinuity_sequence,
                        ),
                        stream.id.as_str(),
                    ));
                }
            } else {
                content_changed = true;
                let has_cue_out = snapshot.has_cue_out;
                let has_cue_in = snapshot.cue_in_count > 0;

                let next_is_disc = if let Some(first) = snapshot.segments.first() {
                    if first.discontinuity {
                        true
                    } else {
                        snapshot.segments.get(1).is_some_and(|s| s.discontinuity)
                    }
                } else {
                    false
                };

                let initial_state = VariantState {
                    media_type: media_type.clone(),
                    media_sequence: snapshot.media_sequence,
                    segment_uris: snapshot.segments.iter().map(|s| s.uri.clone()).collect(),
                    discontinuity_sequence: snapshot.discontinuity_sequence,
                    next_is_discontinuity: next_is_disc,
                    prev_segments: media_playlist.segments.iter().map(segment_to_info).collect(),
                    duration: snapshot.duration,
                    cue_out_count: snapshot.cue_out_count,
                    cue_in_count: snapshot.cue_in_count,
                    in_cue_out: has_cue_out && !has_cue_in,
                    cue_out_duration: snapshot.cue_out_duration,
                    version: snapshot.version,
                };
                sd.variants.insert(variant_key_str.clone(), initial_state);
            }
        }

        let stream_check_ctx = stream_check::StreamCheckContext {
            stream_url: stream.url.clone(),
            stream_id: stream.id.clone(),
            variant_failures: sd.variant_failures.clone(),
        };
        for sc in stream_checks.iter() {
            for e in sc.check(&sd.variants, &stream_check_ctx) {
                record_error(sd, &mut all_errors, notification_tx, monitor_id, e);
            }
        }

        if content_changed {
            sd.last_content_change = Utc::now();
        }
        sd.last_fetch = Utc::now();

        let time_since_change = (Utc::now() - sd.last_content_change)
            .num_milliseconds()
            .max(0) as u128;
        let is_stale = check_stale(
            time_since_change,
            config.stale_limit,
            &stream.url,
            &stream.id,
        );
        if let Some(stale_err) = is_stale {
            record_error(sd, &mut all_errors, notification_tx, monitor_id, stale_err);
            sd.was_stale = true;
        } else if sd.was_stale && content_changed {
            record_event(sd, notification_tx, monitor_id, MonitorEvent::new(
                EventKind::StaleRecovered,
                "MASTER",
                "all",
                format!("Stream recovered after stale period at {}", Utc::now().format("%H:%M:%S")),
                stream.id.as_str(),
            ));
            sd.was_stale = false;
        }
    }

    all_errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_base_url_extracts_directory() {
        assert_eq!(
            get_base_url("https://example.com/path/to/master.m3u8"),
            "https://example.com/path/to/"
        );
    }

    #[test]
    fn get_base_url_handles_root() {
        assert_eq!(
            get_base_url("https://example.com/master.m3u8"),
            "https://example.com/"
        );
    }

    #[test]
    fn build_playlist_url_absolute() {
        assert_eq!(
            build_playlist_url("https://a.com/", "https://b.com/foo.m3u8"),
            "https://b.com/foo.m3u8"
        );
    }

    #[test]
    fn build_playlist_url_relative() {
        assert_eq!(
            build_playlist_url("https://a.com/path/", "level_0.m3u8"),
            "https://a.com/path/level_0.m3u8"
        );
    }
}
