use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct TargetDurationCheck {
    tolerance: f64,
}

impl TargetDurationCheck {
    pub fn new(tolerance: f64) -> Self {
        Self { tolerance }
    }
}

impl Check for TargetDurationCheck {
    fn name(&self) -> &'static str {
        "TargetDuration"
    }

    fn check(
        &self,
        _prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        let limit = curr.target_duration + self.tolerance;
        let mut errors = Vec::new();

        for (i, seg) in curr.segments.iter().enumerate() {
            if seg.duration > limit {
                let mseq = curr.media_sequence + i as u64;
                errors.push(MonitorError::new(
                    ErrorType::TargetDurationExceeded,
                    &ctx.media_type,
                    &ctx.variant_key,
                    format!(
                        "Segment duration {:.3}s exceeds EXT-X-TARGETDURATION {}s (tolerance {:.1}s) at index({}) in mseq({})",
                        seg.duration, curr.target_duration, self.tolerance, i, mseq
                    ),
                    &ctx.stream_url,
                    &ctx.stream_id,
                ));
            }
        }

        errors
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
            media_sequence: 0,
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
        }
    }

    #[test]
    fn no_error_within_target() {
        let check = TargetDurationCheck::new(0.5);
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 9.5),
            make_segment("b.ts", 10.0),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_within_tolerance() {
        let check = TargetDurationCheck::new(0.5);
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 10.3),
            make_segment("b.ts", 10.5),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_when_exceeding_tolerance() {
        let check = TargetDurationCheck::new(0.5);
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 10.6),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::TargetDurationExceeded);
        assert!(errors[0].details.contains("10.600s"));
        assert!(errors[0].details.contains("index(0)"));
        assert!(errors[0].details.contains("mseq(100)"));
    }

    #[test]
    fn multiple_violations_reported() {
        let check = TargetDurationCheck::new(0.5);
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 10.6),
            make_segment("b.ts", 9.0),
            make_segment("c.ts", 11.0),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 2);
        assert!(errors[0].details.contains("index(0)"));
        assert!(errors[1].details.contains("index(2)"));
    }
}
