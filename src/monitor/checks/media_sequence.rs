use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

/// Detects media sequence regressions (current mseq < previous mseq).
pub struct MediaSequenceCheck;

impl Check for MediaSequenceCheck {
    fn name(&self) -> &'static str {
        "MediaSequence"
    }

    fn check(
        &self,
        prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        if curr.media_sequence < prev.media_sequence {
            vec![MonitorError::new(
                ErrorType::MediaSequence,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "Expected mediaSequence >= {}. Got: {}",
                    prev.media_sequence, curr.media_sequence
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

    fn make_prev(mseq: u64) -> VariantState {
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: mseq,
            segment_uris: vec!["a.ts".into(), "b.ts".into()],
            discontinuity_sequence: 0,
            next_is_discontinuity: false,
            prev_segments: vec![],
            duration: 20.0,
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
                    uri: "b.ts".into(),
                    duration: 10.0,
                    discontinuity: false,
                    cue_out: false,
                    cue_in: false,
                    cue_out_cont: None,
                    gap: false,
                    program_date_time: None,
                    daterange: None,
                },
                SegmentSnapshot {
                    uri: "c.ts".into(),
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
            duration: 20.0,
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
    fn detects_regression() {
        let check = MediaSequenceCheck;
        let errors = check.check(&make_prev(5), &make_snap(3), &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::MediaSequence);
        assert!(errors[0]
            .details
            .contains("Expected mediaSequence >= 5. Got: 3"));
    }

    #[test]
    fn no_error_on_equal_mseq() {
        let check = MediaSequenceCheck;
        let errors = check.check(&make_prev(5), &make_snap(5), &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_on_forward_mseq() {
        let check = MediaSequenceCheck;
        let errors = check.check(&make_prev(5), &make_snap(7), &ctx());
        assert!(errors.is_empty());
    }
}
