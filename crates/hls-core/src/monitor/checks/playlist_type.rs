use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct PlaylistTypeCheck;

impl Check for PlaylistTypeCheck {
    fn name(&self) -> &'static str {
        "PlaylistType"
    }

    fn check(
        &self,
        prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        let playlist_type = match curr.playlist_type.as_deref() {
            Some(t) => t,
            None => return vec![],
        };

        match playlist_type {
            "EVENT" => {
                if curr.media_sequence > prev.media_sequence {
                    vec![MonitorError::new(
                        ErrorType::PlaylistTypeViolation,
                        &ctx.media_type,
                        &ctx.variant_key,
                        format!(
                            "EVENT playlist removed segments \u{2014} mseq advanced from {} to {}",
                            prev.media_sequence, curr.media_sequence
                        ),
                        &ctx.stream_url,
                        &ctx.stream_id,
                    )]
                } else {
                    vec![]
                }
            }
            "VOD" => {
                let mseq_changed = curr.media_sequence != prev.media_sequence;
                let seg_count_changed = curr.segments.len() != prev.segment_uris.len();

                if mseq_changed || seg_count_changed {
                    vec![MonitorError::new(
                        ErrorType::PlaylistTypeViolation,
                        &ctx.media_type,
                        &ctx.variant_key,
                        format!(
                            "VOD playlist changed \u{2014} mseq: {} -> {}, segments: {} -> {}",
                            prev.media_sequence,
                            curr.media_sequence,
                            prev.segment_uris.len(),
                            curr.segments.len()
                        ),
                        &ctx.stream_url,
                        &ctx.stream_id,
                    )]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::state::SegmentSnapshot;

    fn ctx() -> CheckContext {
        CheckContext {
            stream_url: "http://example.com/".to_string(),
            stream_id: "stream_1".to_string(),
            media_type: "VIDEO".to_string(),
            variant_key: "1200000".to_string(),
        }
    }

    fn make_prev(mseq: u64, segment_count: usize) -> VariantState {
        let segment_uris: Vec<String> = (0..segment_count)
            .map(|i| format!("seg{}.ts", i))
            .collect();
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: mseq,
            segment_uris,
            discontinuity_sequence: 0,
            next_is_discontinuity: false,
            prev_segments: vec![],
            duration: 10.0 * segment_count as f64,
            cue_out_count: 0,
            cue_in_count: 0,
            in_cue_out: false,
            cue_out_duration: None,
            version: None,
        }
    }

    fn make_segment(uri: &str) -> SegmentSnapshot {
        SegmentSnapshot {
            uri: uri.into(),
            duration: 10.0,
            discontinuity: false,
            cue_out: false,
            cue_in: false,
            cue_out_cont: None,
            gap: false,
            program_date_time: None,
            daterange: None,
        }
    }

    fn make_snap(mseq: u64, segments: Vec<SegmentSnapshot>, playlist_type: Option<&str>) -> PlaylistSnapshot {
        let duration: f64 = segments.iter().map(|s| s.duration).sum();
        PlaylistSnapshot {
            media_sequence: mseq,
            discontinuity_sequence: 0,
            segments,
            duration,
            cue_out_count: 0,
            cue_in_count: 0,
            has_cue_out: false,
            cue_out_duration: None,
            target_duration: 10.0,
            playlist_type: playlist_type.map(String::from),
            version: None,
            has_gaps: false,
        }
    }

    #[test]
    fn no_error_without_playlist_type() {
        let check = PlaylistTypeCheck;
        let prev = make_prev(100, 3);
        let snap = make_snap(101, vec![make_segment("a.ts")], None);
        let errors = check.check(&prev, &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn event_allows_append() {
        let check = PlaylistTypeCheck;
        let prev = make_prev(100, 3);
        let snap = make_snap(
            100,
            vec![
                make_segment("a.ts"),
                make_segment("b.ts"),
                make_segment("c.ts"),
                make_segment("d.ts"),
            ],
            Some("EVENT"),
        );
        let errors = check.check(&prev, &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn event_rejects_removal() {
        let check = PlaylistTypeCheck;
        let prev = make_prev(100, 3);
        let snap = make_snap(101, vec![make_segment("b.ts"), make_segment("c.ts")], Some("EVENT"));
        let errors = check.check(&prev, &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::PlaylistTypeViolation);
        assert!(errors[0].details.contains("EVENT playlist removed segments"));
        assert!(errors[0].details.contains("from 100 to 101"));
    }

    #[test]
    fn vod_rejects_any_change() {
        let check = PlaylistTypeCheck;
        let prev = make_prev(100, 3);
        let snap = make_snap(
            100,
            vec![make_segment("a.ts"), make_segment("b.ts")],
            Some("VOD"),
        );
        let errors = check.check(&prev, &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::PlaylistTypeViolation);
        assert!(errors[0].details.contains("VOD playlist changed"));
        assert!(errors[0].details.contains("segments: 3 -> 2"));
    }

    #[test]
    fn vod_no_error_when_unchanged() {
        let check = PlaylistTypeCheck;
        let prev = make_prev(100, 3);
        let snap = make_snap(
            100,
            vec![make_segment("a.ts"), make_segment("b.ts"), make_segment("c.ts")],
            Some("VOD"),
        );
        let errors = check.check(&prev, &snap, &ctx());
        assert!(errors.is_empty());
    }
}
