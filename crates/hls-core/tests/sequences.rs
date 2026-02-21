use std::collections::HashMap;
use std::fmt::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use hls_core::{ErrorType, EventKind, LoadError, ManifestLoader, Monitor, MonitorConfig, MonitorEvent, StreamItem};

const MASTER_URL: &str = "https://mock.mock.com/channels/1xx/master.m3u8";
const LEVEL0_URL: &str = "https://mock.mock.com/channels/1xx/level_0.m3u8";
const LEVEL1_URL: &str = "https://mock.mock.com/channels/1xx/level_1.m3u8";

const MASTER_PLAYLIST: &str = "\
#EXTM3U
#EXT-X-VERSION:3
#EXT-X-STREAM-INF:BANDWIDTH=1212000,RESOLUTION=1280x720,FRAME-RATE=30.000
level_0.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2424000,RESOLUTION=1280x720,FRAME-RATE=30.000
level_1.m3u8
";

struct Seg {
    uri: &'static str,
    disc: bool,
    cue_out: Option<&'static str>,
    cue_in: bool,
}

fn s(uri: &'static str) -> Seg {
    Seg { uri, disc: false, cue_out: None, cue_in: false }
}

fn ds(uri: &'static str) -> Seg {
    Seg { uri, disc: true, cue_out: None, cue_in: false }
}

fn ds_co(uri: &'static str, dur: &'static str) -> Seg {
    Seg { uri, disc: true, cue_out: Some(dur), cue_in: false }
}

fn ds_ci(uri: &'static str) -> Seg {
    Seg { uri, disc: true, cue_out: None, cue_in: true }
}

fn s_ci(uri: &'static str) -> Seg {
    Seg { uri, disc: false, cue_out: None, cue_in: true }
}

fn mp(mseq: u64, dseq: Option<u64>, segs: &[Seg]) -> String {
    let mut out = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:10\n#EXT-X-MEDIA-SEQUENCE:{}\n",
        mseq
    );
    if let Some(d) = dseq {
        writeln!(out, "#EXT-X-DISCONTINUITY-SEQUENCE:{}", d).unwrap();
    }
    for seg in segs {
        if seg.disc {
            out.push_str("#EXT-X-DISCONTINUITY\n");
        }
        if let Some(dur) = seg.cue_out {
            writeln!(out, "#EXT-X-CUE-OUT:{}", dur).unwrap();
        }
        if seg.cue_in {
            out.push_str("#EXT-X-CUE-IN\n");
        }
        writeln!(out, "#EXTINF:10.000,\n{}", seg.uri).unwrap();
    }
    out
}

struct SequenceLoader {
    step: Arc<AtomicUsize>,
    responses: HashMap<String, Vec<String>>,
}

#[async_trait]
impl ManifestLoader for SequenceLoader {
    async fn load(&self, uri: &str) -> Result<String, LoadError> {
        let responses = self
            .responses
            .get(uri)
            .unwrap_or_else(|| panic!("SequenceLoader: unexpected URL: {}", uri));
        let step = self.step.load(Ordering::SeqCst);
        let idx = step.min(responses.len() - 1);
        Ok(responses[idx].clone())
    }
}

async fn run_sequence(
    level0_steps: Vec<String>,
    level1_steps: Vec<String>,
    num_polls: usize,
) -> Vec<hls_core::MonitorError> {
    let step = Arc::new(AtomicUsize::new(0));

    let mut responses = HashMap::new();
    responses.insert(MASTER_URL.to_string(), vec![MASTER_PLAYLIST.to_string()]);
    responses.insert(LEVEL0_URL.to_string(), level0_steps);
    responses.insert(LEVEL1_URL.to_string(), level1_steps);

    let loader = Arc::new(SequenceLoader {
        step: Arc::clone(&step),
        responses,
    });

    let config = MonitorConfig::default().with_stale_limit(8000);
    let stream = StreamItem {
        id: "stream_1".to_string(),
        url: MASTER_URL.to_string(),
    };

    let monitor = Monitor::new(vec![stream], config, loader, None);

    for poll in 0..num_polls {
        step.store(poll, Ordering::SeqCst);
        monitor.poll_once().await;
    }

    monitor.get_errors().await
}

async fn run_sequence_with_events(
    level0_steps: Vec<String>,
    level1_steps: Vec<String>,
    num_polls: usize,
) -> (Vec<hls_core::MonitorError>, Vec<MonitorEvent>) {
    let step = Arc::new(AtomicUsize::new(0));

    let mut responses = HashMap::new();
    responses.insert(MASTER_URL.to_string(), vec![MASTER_PLAYLIST.to_string()]);
    responses.insert(LEVEL0_URL.to_string(), level0_steps);
    responses.insert(LEVEL1_URL.to_string(), level1_steps);

    let loader = Arc::new(SequenceLoader {
        step: Arc::clone(&step),
        responses,
    });

    let config = MonitorConfig::default().with_stale_limit(8000);
    let stream = StreamItem {
        id: "stream_1".to_string(),
        url: MASTER_URL.to_string(),
    };

    let monitor = Monitor::new(vec![stream], config, loader, None);

    for poll in 0..num_polls {
        step.store(poll, Ordering::SeqCst);
        monitor.poll_once().await;
    }

    (monitor.get_errors().await, monitor.get_events().await)
}

async fn run_sequence_scte35_with_events(
    level0_steps: Vec<String>,
    level1_steps: Vec<String>,
    num_polls: usize,
) -> (Vec<hls_core::MonitorError>, Vec<MonitorEvent>) {
    let step = Arc::new(AtomicUsize::new(0));

    let mut responses = HashMap::new();
    responses.insert(MASTER_URL.to_string(), vec![MASTER_PLAYLIST.to_string()]);
    responses.insert(LEVEL0_URL.to_string(), level0_steps);
    responses.insert(LEVEL1_URL.to_string(), level1_steps);

    let loader = Arc::new(SequenceLoader {
        step: Arc::clone(&step),
        responses,
    });

    let config = MonitorConfig::default().with_stale_limit(8000).with_scte35(true);
    let stream = StreamItem {
        id: "stream_1".to_string(),
        url: MASTER_URL.to_string(),
    };

    let monitor = Monitor::new(vec![stream], config, loader, None);

    for poll in 0..num_polls {
        step.store(poll, Ordering::SeqCst);
        monitor.poll_once().await;
    }

    (monitor.get_errors().await, monitor.get_events().await)
}

fn assert_any_error_contains(errors: &[hls_core::MonitorError], needle: &str) {
    assert!(
        errors.iter().any(|e| e.details.contains(needle)),
        "Expected at least one error containing '{}', but got:\n{:#?}",
        needle,
        errors.iter().map(|e| &e.details).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_monitor_unique_ids_and_config() {
    let loader: Arc<dyn ManifestLoader> = Arc::new(SequenceLoader {
        step: Arc::new(AtomicUsize::new(0)),
        responses: HashMap::new(),
    });

    let config1 = MonitorConfig::default().with_stale_limit(8000);
    let config2 = MonitorConfig::default(); // default stale_limit = 6000

    let m1 = Monitor::new(vec![], config1.clone(), Arc::clone(&loader), None);
    let m2 = Monitor::new(vec![], config2.clone(), Arc::clone(&loader), None);

    assert_ne!(m1.id(), m2.id());
    assert_eq!(config1.poll_interval.as_millis(), 4000);
    assert_eq!(config2.poll_interval.as_millis(), 3000);
    assert_eq!(config1.stale_limit.as_millis(), 8000);
    assert_eq!(config2.stale_limit.as_millis(), 6000);
}

#[tokio::test]
async fn test_segment_continuity_wrong_first_segment() {
    let level0 = vec![
        mp(0, None, &[s("index_0_0.ts"), s("index_0_1.ts")]),
        mp(1, None, &[s("index_0_1.ts"), s("index_0_2.ts")]),
        mp(2, None, &[s("index_0_1.ts"), s("index_0_2.ts")]),
        mp(3, None, &[s("index_0_2.ts"), s("index_0_3.ts")]),
    ];
    let level1 = vec![
        mp(0, None, &[s("index_1_0.ts"), s("index_1_1.ts")]),
        mp(1, None, &[s("index_1_1.ts"), s("index_1_2.ts")]),
        mp(2, None, &[s("index_1_1.ts"), s("index_1_2.ts")]),
        mp(3, None, &[s("index_1_2.ts"), s("index_1_3.ts")]),
    ];

    let errors = run_sequence(level0, level1, 4).await;
    assert!(!errors.is_empty());
    assert_any_error_contains(
        &errors,
        "Expected first item-uri in mseq(2) to be: 'index_0_2.ts'. Got: 'index_0_1.ts'",
    );
}

#[tokio::test]
async fn test_playlist_content_wrong_segment_on_same_mseq() {
    let level0 = vec![
        mp(0, None, &[s("index_0_0.ts"), s("index_0_1.ts")]),
        mp(1, None, &[s("index_0_1.ts"), s("index_0_2.ts")]),
        mp(2, None, &[s("index_0_2.ts"), s("index_0_3.ts")]),
        mp(2, None, &[s("index_0_3.ts"), s("index_0_4.ts")]),
    ];
    let level1 = vec![
        mp(0, None, &[s("index_1_0.ts"), s("index_1_1.ts")]),
        mp(1, None, &[s("index_1_1.ts"), s("index_1_2.ts")]),
        mp(2, None, &[s("index_1_2.ts"), s("index_1_3.ts")]),
        mp(2, None, &[s("index_1_3.ts"), s("index_1_4.ts")]),
    ];

    let errors = run_sequence(level0, level1, 4).await;
    assert!(!errors.is_empty());
    assert_any_error_contains(
        &errors,
        "Expected playlist item-uri in mseq(2) at index(0) to be: 'index_0_2.ts'. Got: 'index_0_3.ts'",
    );
}

#[tokio::test]
async fn test_playlist_size_shrinkage() {
    let level0 = vec![
        mp(10, None, &[s("index_0_0.ts"), s("index_0_1.ts"), s("index_0_2.ts")]),
        mp(11, None, &[s("index_0_1.ts"), s("index_0_2.ts"), s("index_0_3.ts")]),
        mp(12, None, &[s("index_0_2.ts"), s("index_0_3.ts"), s("index_0_4.ts")]),
        mp(12, None, &[s("index_0_2.ts"), s("index_0_3.ts"), s("index_0_4.ts"), s("index_0_5.ts")]),
        mp(13, None, &[s("index_0_3.ts"), s("index_0_4.ts"), s("index_0_5.ts"), s("index_0_6.ts"), s("index_0_7.ts")]),
        mp(13, None, &[s("index_0_3.ts"), s("index_0_4.ts"), s("index_0_5.ts"), s("index_0_6.ts")]),
    ];
    let level1 = vec![
        mp(10, None, &[s("index_1_0.ts"), s("index_1_1.ts"), s("index_1_2.ts")]),
        mp(11, None, &[s("index_1_1.ts"), s("index_1_2.ts"), s("index_1_3.ts")]),
        mp(12, None, &[s("index_1_2.ts"), s("index_1_3.ts"), s("index_1_4.ts")]),
        mp(12, None, &[s("index_1_2.ts"), s("index_1_3.ts"), s("index_1_4.ts"), s("index_1_5.ts")]),
        mp(13, None, &[s("index_1_3.ts"), s("index_1_4.ts"), s("index_1_5.ts"), s("index_1_6.ts"), s("index_1_7.ts")]),
        mp(13, None, &[s("index_1_3.ts"), s("index_1_4.ts"), s("index_1_5.ts"), s("index_1_6.ts")]),
    ];

    let errors = run_sequence(level0, level1, 6).await;
    assert!(!errors.is_empty());
    assert_any_error_contains(&errors, "Expected playlist size in mseq(13) to be: 5. Got: 4");
}

#[tokio::test]
async fn test_media_sequence_regression() {
    let level0 = vec![
        mp(0, None, &[s("index_0_0.ts"), s("index_0_1.ts")]),
        mp(1, None, &[s("index_0_1.ts"), s("index_0_2.ts")]),
        mp(3, None, &[s("index_0_3.ts"), s("index_0_4.ts")]),
        mp(2, None, &[s("index_0_2.ts"), s("index_0_3.ts")]),
    ];
    let level1 = vec![
        mp(0, None, &[s("index_1_0.ts"), s("index_1_1.ts")]),
        mp(1, None, &[s("index_1_1.ts"), s("index_1_2.ts")]),
        mp(3, None, &[s("index_1_3.ts"), s("index_1_4.ts")]),
        mp(2, None, &[s("index_1_2.ts"), s("index_1_3.ts")]),
    ];

    let errors = run_sequence(level0, level1, 4).await;
    assert!(!errors.is_empty());
    assert_any_error_contains(&errors, "Expected mediaSequence >= 3. Got: 2");
}

#[tokio::test]
async fn test_discontinuity_too_big_increment() {
    let level0 = vec![
        mp(0, Some(10), &[s("index_0_0.ts"), s("index_0_1.ts")]),
        mp(1, Some(10), &[s("index_0_1.ts"), ds("other_0_1.ts")]),
        mp(2, Some(10), &[ds("other_0_1.ts"), s("other_0_2.ts")]),
        mp(3, Some(12), &[s("other_0_2.ts"), s("other_0_3.ts")]),
    ];
    let level1 = vec![
        mp(0, Some(10), &[s("index_1_0.ts"), s("index_1_1.ts")]),
        mp(1, Some(10), &[s("index_1_1.ts"), ds("other_1_1.ts")]),
        mp(2, Some(10), &[ds("other_1_1.ts"), s("other_1_2.ts")]),
        mp(3, Some(12), &[s("other_1_2.ts"), s("other_1_3.ts")]),
    ];

    let errors = run_sequence(level0, level1, 4).await;
    assert!(!errors.is_empty());
    assert_any_error_contains(
        &errors,
        "Wrong count increment in mseq(3) - Expected: 11. Got: 12",
    );
}

#[tokio::test]
async fn test_discontinuity_no_increment() {
    let level0 = vec![
        mp(0, Some(10), &[s("index_0_0.ts"), s("index_0_1.ts")]),
        mp(1, Some(10), &[s("index_0_1.ts"), ds("other_0_1.ts")]),
        mp(2, Some(10), &[ds("other_0_1.ts"), s("other_0_2.ts")]),
        mp(3, Some(10), &[s("other_0_2.ts"), s("other_0_3.ts")]),
    ];
    let level1 = vec![
        mp(0, Some(10), &[s("index_1_0.ts"), s("index_1_1.ts")]),
        mp(1, Some(10), &[s("index_1_1.ts"), ds("other_1_1.ts")]),
        mp(2, Some(10), &[ds("other_1_1.ts"), s("other_1_2.ts")]),
        mp(3, Some(10), &[s("other_1_2.ts"), s("other_1_3.ts")]),
    ];

    let errors = run_sequence(level0, level1, 4).await;
    assert!(!errors.is_empty());
    assert_any_error_contains(
        &errors,
        "Wrong count increment in mseq(3) - Expected: 11. Got: 10",
    );
}

#[tokio::test]
async fn test_discontinuity_early_increment_tag_at_top() {
    let level0 = vec![
        mp(20, Some(10), &[s("index_0_0.ts"), s("index_0_1.ts")]),
        mp(21, Some(10), &[s("index_0_1.ts"), ds("other_0_1.ts")]),
        mp(22, Some(11), &[ds("other_0_1.ts"), s("other_0_2.ts")]),
        mp(23, Some(11), &[s("other_0_2.ts"), s("other_0_3.ts")]),
    ];
    let level1 = vec![
        mp(20, Some(10), &[s("index_1_0.ts"), s("index_1_1.ts")]),
        mp(21, Some(10), &[s("index_1_1.ts"), ds("other_1_1.ts")]),
        mp(22, Some(11), &[ds("other_1_1.ts"), s("other_1_2.ts")]),
        mp(23, Some(11), &[s("other_1_2.ts"), s("other_1_3.ts")]),
    ];

    let errors = run_sequence(level0, level1, 4).await;
    assert!(!errors.is_empty());
    assert_any_error_contains(
        &errors,
        "Early count increment in mseq(22) - Expected: 10. Got: 11",
    );
}

#[tokio::test]
async fn test_discontinuity_early_increment_tag_under_top() {
    let level0 = vec![
        mp(20, Some(10), &[s("index_0_0.ts"), s("index_0_1.ts"), s("index_0_2.ts")]),
        mp(21, Some(11), &[s("index_0_1.ts"), s("index_0_2.ts"), ds_ci("other_0_1.ts")]),
        mp(24, Some(11), &[s_ci("other_0_1.ts"), s("other_0_2.ts")]),
        mp(25, Some(11), &[s("other_0_2.ts"), s("other_0_3.ts")]),
    ];
    let level1 = vec![
        mp(20, Some(10), &[s("index_1_0.ts"), s("index_1_1.ts"), s("index_1_2.ts")]),
        mp(21, Some(11), &[s("index_1_1.ts"), s("index_1_2.ts"), ds_ci("other_1_1.ts")]),
        mp(24, Some(11), &[s_ci("other_1_1.ts"), s("other_1_2.ts")]),
        mp(25, Some(11), &[s("other_1_2.ts"), s("other_1_3.ts")]),
    ];

    let errors = run_sequence(level0, level1, 4).await;
    assert!(!errors.is_empty());
    assert_any_error_contains(
        &errors,
        "Early count increment in mseq(21) - Expected: 10. Got: 11",
    );
}

#[tokio::test]
async fn test_discontinuity_passable_multi_disc() {
    let level0 = vec![
        mp(19, Some(10), &[
            s("index_0_00.ts"), s("index_0_0.ts"),
            ds("next_0_0.ts"), s("next_0_1.ts"),
            ds("other_0_0.ts"),
        ]),
        mp(20, Some(10), &[
            s("index_0_0.ts"),
            ds("next_0_0.ts"), s("next_0_1.ts"),
            ds("other_0_0.ts"), s("other_0_1.ts"),
        ]),
        mp(23, Some(11), &[
            ds("other_0_0.ts"), s("other_0_1.ts"), s("other_0_2.ts"),
            s("other_0_3.ts"), s("other_0_4.ts"),
        ]),
    ];
    let level1 = vec![
        mp(19, Some(10), &[
            s("index_1_00.ts"), s("index_1_0.ts"),
            ds("next_1_0.ts"), s("next_1_1.ts"),
            ds("other_1_0.ts"),
        ]),
        mp(20, Some(10), &[
            s("index_1_0.ts"),
            ds("next_1_0.ts"), s("next_1_1.ts"),
            ds("other_1_0.ts"), s("other_1_1.ts"),
        ]),
        mp(23, Some(11), &[
            ds("other_1_0.ts"), s("other_1_1.ts"), s("other_1_2.ts"),
            s("other_1_3.ts"), s("other_1_4.ts"),
        ]),
    ];

    let errors = run_sequence(level0, level1, 3).await;
    assert!(
        errors.is_empty(),
        "Expected no errors, got: {:#?}",
        errors.iter().map(|e| &e.details).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_discontinuity_passable_cue_out() {
    let level0 = vec![
        mp(19, Some(10), &[
            s("vod_0_0.ts"), s("vod_0_1.ts"),
            ds("slate_0_0.ts"),
            ds("nextvod_0_0.ts"),
            ds_co("live_0_0.ts", "DURATION=136"),
        ]),
        mp(20, Some(10), &[
            s("vod_0_1.ts"),
            ds("slate_0_0.ts"),
            ds("nextvod_0_0.ts"),
            ds_co("live_0_0.ts", "DURATION=136"),
            s("live_0_1.ts"),
        ]),
        mp(23, Some(12), &[
            ds_co("live_0_0.ts", "DURATION=136"),
            s("live_0_1.ts"), s("live_0_2.ts"), s("live_0_3.ts"),
        ]),
        mp(24, Some(13), &[
            s("live_0_1.ts"), s("live_0_2.ts"), s("live_0_3.ts"), s("live_0_4.ts"),
        ]),
    ];
    let level1 = vec![
        mp(19, Some(10), &[
            s("vod_1_0.ts"), s("vod_1_1.ts"),
            ds("slate_1_0.ts"),
            ds("nextvod_1_0.ts"),
            ds_co("live_1_0.ts", "DURATION=136"),
        ]),
        mp(20, Some(10), &[
            s("vod_1_1.ts"),
            ds("slate_1_0.ts"),
            ds("nextvod_1_0.ts"),
            ds_co("live_1_0.ts", "DURATION=136"),
            s("live_1_1.ts"),
        ]),
        mp(23, Some(12), &[
            ds_co("live_1_0.ts", "DURATION=136"),
            s("live_1_1.ts"), s("live_1_2.ts"), s("live_1_3.ts"),
        ]),
        mp(24, Some(13), &[
            s("live_1_1.ts"), s("live_1_2.ts"), s("live_1_3.ts"), s("live_1_4.ts"),
        ]),
    ];

    let errors = run_sequence(level0, level1, 4).await;
    assert!(
        errors.is_empty(),
        "Expected no errors, got: {:#?}",
        errors.iter().map(|e| &e.details).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_discontinuity_passable_large_mseq_jump() {
    let level0 = vec![
        mp(19, Some(10), &[
            s("vod_0_0.ts"), s("vod_0_1.ts"),
            ds("slate_0_0.ts"),
            ds("nextvod_0_0.ts"),
            ds_co("live_0_0.ts", "DURATION=136"),
        ]),
        mp(20, Some(10), &[
            s("vod_0_1.ts"),
            ds("slate_0_0.ts"),
            ds("nextvod_0_0.ts"),
            ds_co("live_0_0.ts", "DURATION=136"),
            s("live_0_1.ts"),
        ]),
        mp(123, Some(12), &[
            ds_co("live_0_0.ts", "DURATION=136"),
            s("live_0_1.ts"), s("live_0_2.ts"), s("live_0_3.ts"),
        ]),
        mp(124, Some(13), &[
            s("live_0_1.ts"), s("live_0_2.ts"), s("live_0_3.ts"), s("live_0_4.ts"),
        ]),
    ];
    let level1 = vec![
        mp(19, Some(10), &[
            s("vod_1_0.ts"), s("vod_1_1.ts"),
            ds("slate_1_0.ts"),
            ds("nextvod_1_0.ts"),
            ds_co("live_1_0.ts", "DURATION=136"),
        ]),
        mp(20, Some(10), &[
            s("vod_1_1.ts"),
            ds("slate_1_0.ts"),
            ds("nextvod_1_0.ts"),
            ds_co("live_1_0.ts", "DURATION=136"),
            s("live_1_1.ts"),
        ]),
        mp(123, Some(12), &[
            ds_co("live_1_0.ts", "DURATION=136"),
            s("live_1_1.ts"), s("live_1_2.ts"), s("live_1_3.ts"),
        ]),
        mp(124, Some(13), &[
            s("live_1_1.ts"), s("live_1_2.ts"), s("live_1_3.ts"), s("live_1_4.ts"),
        ]),
    ];

    let errors = run_sequence(level0, level1, 4).await;
    let disc_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.error_type == ErrorType::DiscontinuitySequence)
        .collect();
    assert!(
        disc_errors.is_empty(),
        "Expected no discontinuity errors, got: {:#?}",
        disc_errors.iter().map(|e| &e.details).collect::<Vec<_>>()
    );
}

fn count_events(events: &[MonitorEvent], kind: EventKind) -> usize {
    events.iter().filter(|e| e.kind == kind).count()
}

#[tokio::test]
async fn test_events_manifest_updated() {
    let level0 = vec![
        mp(0, None, &[s("a0.ts"), s("a1.ts")]),
        mp(1, None, &[s("a1.ts"), s("a2.ts")]),
        mp(2, None, &[s("a2.ts"), s("a3.ts")]),
    ];
    let level1 = vec![
        mp(0, None, &[s("b0.ts"), s("b1.ts")]),
        mp(1, None, &[s("b1.ts"), s("b2.ts")]),
        mp(2, None, &[s("b2.ts"), s("b3.ts")]),
    ];

    let (errors, events) = run_sequence_with_events(level0, level1, 3).await;
    assert!(errors.is_empty());

    let update_count = count_events(&events, EventKind::ManifestUpdated);
    assert_eq!(update_count, 4, "Expected 4 ManifestUpdated events (2 variants x 2 advances), got {}", update_count);
}

#[tokio::test]
async fn test_events_discontinuity_changed() {
    let level0 = vec![
        mp(19, Some(10), &[
            s("vod_0_0.ts"), s("vod_0_1.ts"),
            ds("slate_0_0.ts"),
            ds("nextvod_0_0.ts"),
            ds_co("live_0_0.ts", "DURATION=136"),
        ]),
        mp(20, Some(10), &[
            s("vod_0_1.ts"),
            ds("slate_0_0.ts"),
            ds("nextvod_0_0.ts"),
            ds_co("live_0_0.ts", "DURATION=136"),
            s("live_0_1.ts"),
        ]),
        mp(23, Some(12), &[
            ds_co("live_0_0.ts", "DURATION=136"),
            s("live_0_1.ts"), s("live_0_2.ts"), s("live_0_3.ts"),
        ]),
        mp(24, Some(13), &[
            s("live_0_1.ts"), s("live_0_2.ts"), s("live_0_3.ts"), s("live_0_4.ts"),
        ]),
    ];
    let level1 = vec![
        mp(19, Some(10), &[
            s("vod_1_0.ts"), s("vod_1_1.ts"),
            ds("slate_1_0.ts"),
            ds("nextvod_1_0.ts"),
            ds_co("live_1_0.ts", "DURATION=136"),
        ]),
        mp(20, Some(10), &[
            s("vod_1_1.ts"),
            ds("slate_1_0.ts"),
            ds("nextvod_1_0.ts"),
            ds_co("live_1_0.ts", "DURATION=136"),
            s("live_1_1.ts"),
        ]),
        mp(23, Some(12), &[
            ds_co("live_1_0.ts", "DURATION=136"),
            s("live_1_1.ts"), s("live_1_2.ts"), s("live_1_3.ts"),
        ]),
        mp(24, Some(13), &[
            s("live_1_1.ts"), s("live_1_2.ts"), s("live_1_3.ts"), s("live_1_4.ts"),
        ]),
    ];

    let (errors, events) = run_sequence_with_events(level0, level1, 4).await;
    assert!(errors.is_empty());

    let disc_count = count_events(&events, EventKind::DiscontinuityChanged);
    assert_eq!(disc_count, 4, "Expected 4 DiscontinuityChanged events, got {}", disc_count);
}

#[tokio::test]
async fn test_events_cue_out_and_cue_in() {
    let level0 = vec![
        mp(0, None, &[s("a0.ts"), s("a1.ts")]),
        mp(1, None, &[s("a1.ts"), ds_co("ad_0.ts", "30")]),
        mp(2, None, &[ds_co("ad_0.ts", "30"), s("ad_1.ts")]),
        mp(3, None, &[s("ad_1.ts"), ds_ci("a2.ts")]),
    ];
    let level1 = vec![
        mp(0, None, &[s("b0.ts"), s("b1.ts")]),
        mp(1, None, &[s("b1.ts"), ds_co("bad_0.ts", "30")]),
        mp(2, None, &[ds_co("bad_0.ts", "30"), s("bad_1.ts")]),
        mp(3, None, &[s("bad_1.ts"), ds_ci("b2.ts")]),
    ];

    let (_errors, events) = run_sequence_scte35_with_events(level0, level1, 4).await;

    let cue_out = count_events(&events, EventKind::CueOutStarted);
    assert_eq!(cue_out, 2, "Expected 2 CueOutStarted events, got {}", cue_out);

    let cue_in = count_events(&events, EventKind::CueInReturned);
    assert_eq!(cue_in, 2, "Expected 2 CueInReturned events, got {}", cue_in);
}

#[tokio::test]
async fn test_stream_status() {
    let step = Arc::new(AtomicUsize::new(0));

    let mut responses = HashMap::new();
    responses.insert(MASTER_URL.to_string(), vec![MASTER_PLAYLIST.to_string()]);
    responses.insert(
        LEVEL0_URL.to_string(),
        vec![mp(10, Some(5), &[s("a0.ts"), s("a1.ts"), s("a2.ts")])],
    );
    responses.insert(
        LEVEL1_URL.to_string(),
        vec![mp(10, Some(5), &[s("b0.ts"), s("b1.ts"), s("b2.ts")])],
    );

    let loader = Arc::new(SequenceLoader {
        step: Arc::clone(&step),
        responses,
    });

    let config = MonitorConfig::default().with_stale_limit(8000);
    let stream = StreamItem {
        id: "stream_1".to_string(),
        url: MASTER_URL.to_string(),
    };

    let monitor = Monitor::new(vec![stream], config, loader, None);
    monitor.poll_once().await;

    let statuses = monitor.get_stream_status().await;
    assert_eq!(statuses.len(), 1);

    let ss = &statuses[0];
    assert_eq!(ss.stream_id, "stream_1");
    assert_eq!(ss.variants.len(), 2);

    for v in &ss.variants {
        assert_eq!(v.media_sequence, 10);
        assert_eq!(v.discontinuity_sequence, 5);
        assert_eq!(v.segment_count, 3);
        assert!(!v.in_cue_out);
    }
}
