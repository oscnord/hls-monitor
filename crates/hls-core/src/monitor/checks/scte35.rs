use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

/// Validates SCTE-35/CUE marker consistency in HLS playlists.
pub struct Scte35Check;

impl Check for Scte35Check {
    fn name(&self) -> &'static str {
        "SCTE35"
    }

    fn check(
        &self,
        prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        let mut errors = Vec::new();

        let has_cue_out = curr.segments.iter().any(|s| s.cue_out);
        let has_cue_in = curr.segments.iter().any(|s| s.cue_in);
        let has_cue_out_cont = curr.segments.iter().any(|s| s.cue_out_cont.is_some());

        if prev.in_cue_out
            && !has_cue_out
            && !has_cue_in
            && !has_cue_out_cont
            && curr.media_sequence > prev.media_sequence
        {
            errors.push(MonitorError::new(
                ErrorType::Scte35Violation,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "CUE-OUT markers disappeared without CUE-IN in mseq({})",
                    curr.media_sequence
                ),
                &ctx.stream_url,
                &ctx.stream_id,
            ));
        }

        if has_cue_in && !prev.in_cue_out && !has_cue_out {
            errors.push(MonitorError::new(
                ErrorType::Scte35Violation,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "CUE-IN found without preceding CUE-OUT state in mseq({})",
                    curr.media_sequence
                ),
                &ctx.stream_url,
                &ctx.stream_id,
            ));
        }

        if has_cue_out_cont && !prev.in_cue_out && !has_cue_out {
            errors.push(MonitorError::new(
                ErrorType::Scte35Violation,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "CUE-OUT-CONT found without active CUE-OUT in mseq({})",
                    curr.media_sequence
                ),
                &ctx.stream_url,
                &ctx.stream_id,
            ));
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

    fn make_seg(uri: &str, cue_out: bool, cue_in: bool, cont: Option<&str>) -> SegmentSnapshot {
        SegmentSnapshot {
            uri: uri.to_string(),
            duration: 10.0,
            discontinuity: false,
            cue_out,
            cue_in,
            cue_out_cont: cont.map(|s| s.to_string()),
            gap: false,
            program_date_time: None,
            daterange: None,
        }
    }

    fn make_prev(mseq: u64, in_cue_out: bool) -> VariantState {
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: mseq,
            segment_uris: vec!["a.ts".into()],
            discontinuity_sequence: 0,
            next_is_discontinuity: false,
            prev_segments: vec![],
            duration: 10.0,
            cue_out_count: 0,
            cue_in_count: 0,
            in_cue_out,
            cue_out_duration: None,
            version: None,
        }
    }

    fn make_snap(mseq: u64, segments: Vec<SegmentSnapshot>) -> PlaylistSnapshot {
        let cue_out_count = segments.iter().filter(|s| s.cue_out).count();
        let cue_in_count = segments.iter().filter(|s| s.cue_in).count();
        let has_cue_out = cue_out_count > 0;
        PlaylistSnapshot {
            media_sequence: mseq,
            discontinuity_sequence: 0,
            segments,
            duration: 10.0,
            cue_out_count,
            cue_in_count,
            has_cue_out,
            cue_out_duration: None,
            target_duration: 10.0,
            playlist_type: None,
            version: None,
            has_gaps: false,
        }
    }

    #[test]
    fn error_cue_out_disappeared_without_cue_in() {
        let check = Scte35Check;
        let prev = make_prev(10, true);
        let curr = make_snap(11, vec![make_seg("a.ts", false, false, None)]);
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].details.contains("disappeared without CUE-IN"));
    }

    #[test]
    fn no_error_cue_out_followed_by_cue_in() {
        let check = Scte35Check;
        let prev = make_prev(10, true);
        let curr = make_snap(11, vec![make_seg("a.ts", false, true, None)]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_cue_in_without_preceding_cue_out() {
        let check = Scte35Check;
        let prev = make_prev(10, false);
        let curr = make_snap(11, vec![make_seg("a.ts", false, true, None)]);
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .details
            .contains("CUE-IN found without preceding CUE-OUT"));
    }

    #[test]
    fn no_error_normal_cue_out_cont() {
        let check = Scte35Check;
        let prev = make_prev(10, true);
        let curr = make_snap(11, vec![make_seg("a.ts", false, false, Some("20.0/60.0"))]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_cue_out_cont_without_active_cue_out() {
        let check = Scte35Check;
        let prev = make_prev(10, false);
        let curr = make_snap(11, vec![make_seg("a.ts", false, false, Some("20.0/60.0"))]);
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .details
            .contains("CUE-OUT-CONT found without active CUE-OUT"));
    }

    #[test]
    fn no_error_fresh_cue_out() {
        let check = Scte35Check;
        let prev = make_prev(10, false);
        let curr = make_snap(
            11,
            vec![
                make_seg("a.ts", true, false, None),
                make_seg("b.ts", false, false, None),
            ],
        );
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }
}
