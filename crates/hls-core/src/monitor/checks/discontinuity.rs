use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

/// Validates that the discontinuity sequence counter increments correctly.
pub struct DiscontinuityCheck;

impl Check for DiscontinuityCheck {
    fn name(&self) -> &'static str {
        "Discontinuity"
    }

    fn check(
        &self,
        prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        let mut errors = Vec::new();

        if curr.media_sequence <= prev.media_sequence {
            return errors;
        }

        let mseq_diff = (curr.media_sequence - prev.media_sequence) as usize;
        let disc_on_top = curr.segments.first().is_some_and(|s| s.discontinuity);

        if !disc_on_top && prev.next_is_discontinuity {
            let expected_dseq = prev.discontinuity_sequence + 1;
            if mseq_diff == 1 && expected_dseq != curr.discontinuity_sequence {
                errors.push(MonitorError::new(
                    ErrorType::DiscontinuitySequence,
                    &ctx.media_type,
                    &ctx.variant_key,
                    format!(
                        "Wrong count increment in mseq({}) - Expected: {}. Got: {}",
                        curr.media_sequence, expected_dseq, curr.discontinuity_sequence
                    ),
                    &ctx.stream_url,
                    &ctx.stream_id,
                ));
            }
        } else if prev.discontinuity_sequence != curr.discontinuity_sequence {
            let dseq_diff = curr.discontinuity_sequence as i64 - prev.discontinuity_sequence as i64;
            let prev_playlist_size = prev.prev_segments.len();

            if mseq_diff < prev_playlist_size {
                let mut found_disc_count: i64 = if disc_on_top { -1 } else { 0 };
                let end = (mseq_diff + 1).min(prev_playlist_size);

                for seg in prev.prev_segments.iter().take(end) {
                    if seg.discontinuity {
                        found_disc_count += 1;
                    }
                }

                if dseq_diff != found_disc_count {
                    errors.push(MonitorError::new(
                        ErrorType::DiscontinuitySequence,
                        &ctx.media_type,
                        &ctx.variant_key,
                        format!(
                            "Early count increment in mseq({}) - Expected: {}. Got: {}",
                            curr.media_sequence,
                            prev.discontinuity_sequence,
                            curr.discontinuity_sequence
                        ),
                        &ctx.stream_url,
                        &ctx.stream_id,
                    ));
                }
            }
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::state::{SegmentInfo, SegmentSnapshot};

    fn ctx() -> CheckContext {
        CheckContext {
            stream_url: "http://example.com/".to_string(),
            stream_id: "stream_1".to_string(),
            media_type: "VIDEO".to_string(),
            variant_key: "1200000".to_string(),
        }
    }

    fn seg_snap(uri: &str, disc: bool) -> SegmentSnapshot {
        SegmentSnapshot {
            uri: uri.to_string(),
            duration: 10.0,
            discontinuity: disc,
            cue_out: false,
            cue_in: false,
            cue_out_cont: None,
        }
    }

    fn seg_info(uri: &str, disc: bool) -> SegmentInfo {
        SegmentInfo {
            uri: uri.to_string(),
            discontinuity: disc,
        }
    }

    fn make_prev(
        mseq: u64,
        dseq: u64,
        uris: &[&str],
        prev_segs: Vec<SegmentInfo>,
        next_is_disc: bool,
    ) -> VariantState {
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: mseq,
            segment_uris: uris.iter().map(|s| s.to_string()).collect(),
            discontinuity_sequence: dseq,
            next_is_discontinuity: next_is_disc,
            prev_segments: prev_segs,
            duration: uris.len() as f64 * 10.0,
            cue_out_count: 0,
            cue_in_count: 0,
            in_cue_out: false,
            cue_out_duration: None,
        }
    }

    fn make_snap(mseq: u64, dseq: u64, segs: Vec<SegmentSnapshot>) -> PlaylistSnapshot {
        let dur = segs.len() as f64 * 10.0;
        PlaylistSnapshot {
            media_sequence: mseq,
            discontinuity_sequence: dseq,
            segments: segs,
            duration: dur,
            cue_out_count: 0,
            cue_in_count: 0,
            has_cue_out: false,
            cue_out_duration: None,
        }
    }

    #[test]
    fn error_too_big_dseq_increment() {
        let check = DiscontinuityCheck;
        let prev = make_prev(
            2,
            10,
            &["other_0_1.ts", "other_0_2.ts"],
            vec![
                seg_info("other_0_1.ts", true),
                seg_info("other_0_2.ts", false),
            ],
            true,
        );
        let curr = make_snap(
            3,
            12,
            vec![
                seg_snap("other_0_2.ts", false),
                seg_snap("other_0_3.ts", false),
            ],
        );
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .details
            .contains("Wrong count increment in mseq(3)"));
        assert!(errors[0].details.contains("Expected: 11"));
        assert!(errors[0].details.contains("Got: 12"));
    }

    #[test]
    fn error_no_dseq_increment_when_expected() {
        let check = DiscontinuityCheck;
        let prev = make_prev(
            2,
            10,
            &["other_0_1.ts", "other_0_2.ts"],
            vec![
                seg_info("other_0_1.ts", true),
                seg_info("other_0_2.ts", false),
            ],
            true,
        );
        let curr = make_snap(
            3,
            10,
            vec![
                seg_snap("other_0_2.ts", false),
                seg_snap("other_0_3.ts", false),
            ],
        );
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .details
            .contains("Wrong count increment in mseq(3)"));
        assert!(errors[0].details.contains("Expected: 11"));
        assert!(errors[0].details.contains("Got: 10"));
    }

    #[test]
    fn error_early_increment_tag_at_top() {
        let check = DiscontinuityCheck;
        let prev = make_prev(
            21,
            10,
            &["index_0_1.ts", "other_0_1.ts"],
            vec![
                seg_info("index_0_1.ts", false),
                seg_info("other_0_1.ts", true),
            ],
            false,
        );
        let curr = make_snap(
            22,
            11,
            vec![
                seg_snap("other_0_1.ts", true),
                seg_snap("other_0_2.ts", false),
            ],
        );
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .details
            .contains("Early count increment in mseq(22)"));
        assert!(errors[0].details.contains("Expected: 10"));
        assert!(errors[0].details.contains("Got: 11"));
    }

    #[test]
    fn error_early_increment_tag_under_top() {
        let check = DiscontinuityCheck;
        let prev = make_prev(
            20,
            10,
            &["index_0_0.ts", "index_0_1.ts", "index_0_2.ts"],
            vec![
                seg_info("index_0_0.ts", false),
                seg_info("index_0_1.ts", false),
                seg_info("index_0_2.ts", false),
            ],
            false,
        );
        let curr = make_snap(
            21,
            11,
            vec![
                seg_snap("index_0_1.ts", false),
                seg_snap("index_0_2.ts", false),
                seg_snap("other_0_1.ts", true),
            ],
        );
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .details
            .contains("Early count increment in mseq(21)"));
    }

    #[test]
    fn passable_large_mseq_jump_skips_validation() {
        let check = DiscontinuityCheck;
        let prev = make_prev(
            20,
            10,
            &["a.ts", "b.ts", "c.ts"],
            vec![
                seg_info("a.ts", false),
                seg_info("b.ts", true),
                seg_info("c.ts", false),
            ],
            false,
        );
        let curr = make_snap(
            123,
            12,
            vec![seg_snap("x.ts", true), seg_snap("y.ts", false)],
        );
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_when_dseq_unchanged() {
        let check = DiscontinuityCheck;
        let prev = make_prev(
            0,
            10,
            &["a.ts", "b.ts"],
            vec![seg_info("a.ts", false), seg_info("b.ts", false)],
            false,
        );
        let curr = make_snap(
            1,
            10,
            vec![seg_snap("b.ts", false), seg_snap("c.ts", false)],
        );
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }
}
