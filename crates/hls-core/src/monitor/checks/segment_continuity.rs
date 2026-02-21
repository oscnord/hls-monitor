use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

/// Validates segment continuity when mseq advances.
pub struct SegmentContinuityCheck;

impl SegmentContinuityCheck {
    fn normalize_uri(uri: &str) -> &str {
        uri.split('?').next().unwrap_or(uri)
    }
}

impl Check for SegmentContinuityCheck {
    fn name(&self) -> &'static str {
        "SegmentContinuity"
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

        if curr.segments.is_empty() || prev.segment_uris.is_empty() {
            return vec![];
        }

        let mseq_diff = (curr.media_sequence - prev.media_sequence) as usize;

        if mseq_diff >= prev.segment_uris.len() {
            return vec![];
        }

        let expected_uri = &prev.segment_uris[mseq_diff];
        let actual_uri = &curr.segments[0].uri;

        let expected_normalized = Self::normalize_uri(expected_uri);
        let actual_normalized = Self::normalize_uri(actual_uri);

        if expected_normalized != actual_normalized {
            vec![MonitorError::new(
                ErrorType::SegmentContinuity,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "Faulty Segment Continuity! Expected first item-uri in mseq({}) to be: '{}'. Got: '{}'",
                    curr.media_sequence, expected_uri, actual_uri
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

    fn seg(uri: &str) -> SegmentSnapshot {
        SegmentSnapshot {
            uri: uri.to_string(),
            duration: 10.0,
            discontinuity: false,
            cue_out: false,
            cue_in: false,
            cue_out_cont: None,
        }
    }

    fn make_prev(mseq: u64, uris: &[&str]) -> VariantState {
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: mseq,
            segment_uris: uris.iter().map(|s| s.to_string()).collect(),
            discontinuity_sequence: 0,
            next_is_discontinuity: false,
            prev_segments: vec![],
            duration: uris.len() as f64 * 10.0,
            cue_out_count: 0,
            cue_in_count: 0,
            in_cue_out: false,
            cue_out_duration: None,
        }
    }

    fn make_snap(mseq: u64, uris: &[&str]) -> PlaylistSnapshot {
        PlaylistSnapshot {
            media_sequence: mseq,
            discontinuity_sequence: 0,
            segments: uris.iter().map(|u| seg(u)).collect(),
            duration: uris.len() as f64 * 10.0,
            cue_out_count: 0,
            cue_in_count: 0,
            has_cue_out: false,
            cue_out_duration: None,
        }
    }

    #[test]
    fn detects_wrong_first_segment() {
        let check = SegmentContinuityCheck;
        let prev = make_prev(0, &["a.ts", "b.ts"]);
        let curr = make_snap(1, &["c.ts", "d.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].details.contains("b.ts"));
        assert!(errors[0].details.contains("c.ts"));
    }

    #[test]
    fn no_error_on_correct_sliding_window() {
        let check = SegmentContinuityCheck;
        let prev = make_prev(0, &["a.ts", "b.ts"]);
        let curr = make_snap(1, &["b.ts", "c.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn skip_when_mseq_jump_too_large() {
        let check = SegmentContinuityCheck;
        let prev = make_prev(0, &["a.ts", "b.ts"]);
        let curr = make_snap(5, &["x.ts", "y.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn skip_when_mseq_equal() {
        let check = SegmentContinuityCheck;
        let prev = make_prev(5, &["a.ts", "b.ts"]);
        let curr = make_snap(5, &["c.ts", "d.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }
}
