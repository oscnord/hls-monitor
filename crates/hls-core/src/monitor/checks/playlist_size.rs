use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

/// Detects playlist size shrinkage when the media sequence hasn't changed.
pub struct PlaylistSizeCheck;

impl Check for PlaylistSizeCheck {
    fn name(&self) -> &'static str {
        "PlaylistSize"
    }

    fn check(
        &self,
        prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        if curr.media_sequence != prev.media_sequence {
            return vec![];
        }

        if prev.segment_uris.is_empty() {
            return vec![];
        }

        if prev.segment_uris.len() > curr.segments.len() {
            vec![MonitorError::new(
                ErrorType::PlaylistSize,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "Expected playlist size in mseq({}) to be: {}. Got: {}",
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
            gap: false,
            program_date_time: None,
            daterange: None,
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
            version: None,
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
            target_duration: 10.0,
            playlist_type: None,
            version: None,
            has_gaps: false,
        }
    }

    #[test]
    fn detects_size_shrinkage_on_equal_mseq() {
        let check = PlaylistSizeCheck;
        let prev = make_prev(10, &["a.ts", "b.ts", "c.ts"]);
        let curr = make_snap(10, &["a.ts", "b.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].details.contains("to be: 3. Got: 2"));
    }

    #[test]
    fn no_error_on_equal_size() {
        let check = PlaylistSizeCheck;
        let prev = make_prev(10, &["a.ts", "b.ts"]);
        let curr = make_snap(10, &["a.ts", "b.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_on_size_growth() {
        let check = PlaylistSizeCheck;
        let prev = make_prev(10, &["a.ts", "b.ts"]);
        let curr = make_snap(10, &["a.ts", "b.ts", "c.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn skip_when_mseq_differs() {
        let check = PlaylistSizeCheck;
        let prev = make_prev(10, &["a.ts", "b.ts", "c.ts"]);
        let curr = make_snap(11, &["b.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }
}
