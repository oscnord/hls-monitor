use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct MseqGapCheck {
    threshold: u64,
}

impl MseqGapCheck {
    pub fn new(threshold: u64) -> Self {
        Self { threshold }
    }
}

impl Check for MseqGapCheck {
    fn name(&self) -> &'static str {
        "MseqGap"
    }

    fn check(
        &self,
        prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        if curr.media_sequence <= prev.media_sequence {
            return vec![];
        }

        let diff = curr.media_sequence - prev.media_sequence;
        let window = prev.segment_uris.len() as u64;

        if diff > window && diff >= self.threshold {
            vec![MonitorError::new(
                ErrorType::MediaSequenceGap,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "Media sequence jumped forward by {} (from {} to {}), exceeding playlist window of {} segments",
                    diff, prev.media_sequence, curr.media_sequence, window
                ),
                &ctx.stream_url,
                &ctx.stream_id,
            )]
        } else {
            vec![]
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

    fn make_snap(mseq: u64) -> PlaylistSnapshot {
        PlaylistSnapshot {
            media_sequence: mseq,
            discontinuity_sequence: 0,
            segments: vec![
                SegmentSnapshot {
                    uri: "a.ts".into(),
                    duration: 10.0,
                    discontinuity: false,
                    cue_out: false,
                    cue_in: false,
                    cue_out_cont: None,
                    gap: false,
                    program_date_time: None,
                    daterange: None,
                },
            ],
            duration: 10.0,
            cue_out_count: 0,
            cue_in_count: 0,
            has_cue_out: false,
            cue_out_duration: None,
            target_duration: 10.0,
            playlist_type: None,
            version: None,
            has_gaps: false,
        }
    }

    #[test]
    fn no_error_on_normal_advance() {
        let check = MseqGapCheck::new(5);
        let errors = check.check(&make_prev(10, 5), &make_snap(11), &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_within_window() {
        let check = MseqGapCheck::new(5);
        let errors = check.check(&make_prev(10, 5), &make_snap(15), &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_on_regression() {
        let check = MseqGapCheck::new(5);
        let errors = check.check(&make_prev(10, 5), &make_snap(8), &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_on_large_jump() {
        let check = MseqGapCheck::new(5);
        let errors = check.check(&make_prev(10, 5), &make_snap(60), &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::MediaSequenceGap);
        assert!(errors[0].details.contains("jumped forward by 50"));
        assert!(errors[0].details.contains("from 10 to 60"));
        assert!(errors[0].details.contains("window of 5 segments"));
    }

    #[test]
    fn no_error_below_threshold() {
        let check = MseqGapCheck::new(50);
        let errors = check.check(&make_prev(10, 5), &make_snap(30), &ctx());
        assert!(errors.is_empty());
    }
}
