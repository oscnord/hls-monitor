use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct SegmentDurationAnomalyCheck {
    ratio: f64,
}

impl SegmentDurationAnomalyCheck {
    pub fn new(ratio: f64) -> Self {
        Self { ratio }
    }
}

impl Check for SegmentDurationAnomalyCheck {
    fn name(&self) -> &'static str {
        "SegmentDurationAnomaly"
    }

    fn check(
        &self,
        _prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        if curr.segments.len() < 2 {
            return vec![];
        }

        let threshold = curr.target_duration * self.ratio;
        let mut errors = Vec::new();

        for (i, seg) in curr.segments[..curr.segments.len() - 1].iter().enumerate() {
            if seg.duration < threshold {
                let mseq = curr.media_sequence + i as u64;
                errors.push(MonitorError::new(
                    ErrorType::SegmentDurationAnomaly,
                    &ctx.media_type,
                    &ctx.variant_key,
                    format!(
                        "Abnormally short segment {:.3}s (target: {}s, threshold: {:.1}s) at index({}) in mseq({})",
                        seg.duration, curr.target_duration, threshold, i, mseq
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
    fn no_error_normal_durations() {
        let check = SegmentDurationAnomalyCheck::new(0.5);
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 9.5),
            make_segment("b.ts", 10.0),
            make_segment("c.ts", 9.8),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_short_segment() {
        let check = SegmentDurationAnomalyCheck::new(0.5);
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 10.0),
            make_segment("b.ts", 2.0),
            make_segment("c.ts", 10.0),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::SegmentDurationAnomaly);
        assert!(errors[0].details.contains("2.000s"));
        assert!(errors[0].details.contains("index(1)"));
        assert!(errors[0].details.contains("mseq(101)"));
    }

    #[test]
    fn ignores_short_last_segment() {
        let check = SegmentDurationAnomalyCheck::new(0.5);
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 10.0),
            make_segment("b.ts", 10.0),
            make_segment("c.ts", 2.0),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn skips_single_segment_playlist() {
        let check = SegmentDurationAnomalyCheck::new(0.5);
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 2.0),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }
}
