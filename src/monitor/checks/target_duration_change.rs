use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct TargetDurationChangeCheck;

impl Check for TargetDurationChangeCheck {
    fn name(&self) -> &'static str {
        "TargetDurationChange"
    }

    fn check(
        &self,
        prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        if prev.media_sequence == 0 {
            return vec![];
        }

        if (prev.target_duration - curr.target_duration).abs() > f64::EPSILON {
            return vec![MonitorError::new(
                ErrorType::TargetDurationChange,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "EXT-X-TARGETDURATION changed from {}s to {}s",
                    prev.target_duration, curr.target_duration
                ),
                &ctx.stream_url,
                &ctx.stream_id,
            )];
        }

        vec![]
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

    fn make_prev() -> VariantState {
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: 100,
            segment_uris: vec!["a.ts".into()],
            discontinuity_sequence: 0,
            next_is_discontinuity: false,
            prev_segments: vec![],
            duration: 10.0,
            cue_out_count: 0,
            cue_in_count: 0,
            in_cue_out: false,
            cue_out_duration: None,
            version: None,
            target_duration: 10.0,
            playlist_type: None,
            has_endlist: false,
        }
    }

    fn make_segment(uri: &str, duration: f64) -> SegmentSnapshot {
        SegmentSnapshot {
            uri: uri.into(),
            duration,
            discontinuity: false,
            cue_out: false,
            cue_in: false,
            cue_out_cont: None,
            gap: false,
            program_date_time: None,
            daterange: None,
        }
    }

    fn make_snap(target_duration: f64, segments: Vec<SegmentSnapshot>) -> PlaylistSnapshot {
        let duration: f64 = segments.iter().map(|s| s.duration).sum();
        PlaylistSnapshot {
            media_sequence: 100,
            discontinuity_sequence: 0,
            segments,
            duration,
            cue_out_count: 0,
            cue_in_count: 0,
            has_cue_out: false,
            cue_out_duration: None,
            target_duration,
            playlist_type: None,
            version: None,
            has_gaps: false,
            has_endlist: false,
            i_frames_only: false,
            has_byte_range: false,
            has_map: false,
            has_key_iv: false,
            has_key_format: false,
            keys: vec![],
        }
    }

    #[test]
    fn no_error_same_target_duration() {
        let check = TargetDurationChangeCheck;
        let prev = make_prev();
        let snap = make_snap(10.0, vec![make_segment("a.ts", 10.0)]);
        let errors = check.check(&prev, &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_target_duration_changed() {
        let check = TargetDurationChangeCheck;
        let prev = make_prev();
        let snap = make_snap(6.0, vec![make_segment("a.ts", 6.0)]);
        let errors = check.check(&prev, &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::TargetDurationChange);
        assert!(errors[0].details.contains("10"));
        assert!(errors[0].details.contains("6"));
    }

    #[test]
    fn no_error_first_poll() {
        let check = TargetDurationChangeCheck;
        let mut prev = make_prev();
        prev.media_sequence = 0;
        prev.target_duration = 10.0;
        let snap = make_snap(6.0, vec![make_segment("a.ts", 6.0)]);
        let errors = check.check(&prev, &snap, &ctx());
        assert!(errors.is_empty());
    }
}
