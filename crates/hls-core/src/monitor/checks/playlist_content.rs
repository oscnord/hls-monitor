use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

/// Detects unexpected content changes when mseq and playlist size are identical.
pub struct PlaylistContentCheck;

impl PlaylistContentCheck {
    fn normalize_uri(uri: &str) -> &str {
        uri.split('?').next().unwrap_or(uri)
    }
}

impl Check for PlaylistContentCheck {
    fn name(&self) -> &'static str {
        "PlaylistContent"
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
        if prev.segment_uris.len() != curr.segments.len() {
            return vec![];
        }
        if prev.segment_uris.is_empty() {
            return vec![];
        }

        let mut errors = Vec::new();
        for (i, (prev_uri, curr_seg)) in prev
            .segment_uris
            .iter()
            .zip(curr.segments.iter())
            .enumerate()
        {
            let prev_normalized = Self::normalize_uri(prev_uri);
            let curr_normalized = Self::normalize_uri(&curr_seg.uri);

            if prev_normalized != curr_normalized {
                errors.push(MonitorError::new(
                    ErrorType::PlaylistContent,
                    &ctx.media_type,
                    &ctx.variant_key,
                    format!(
                        "Expected playlist item-uri in mseq({}) at index({}) to be: '{}'. Got: '{}'",
                        curr.media_sequence, i, prev_uri, curr_seg.uri
                    ),
                    &ctx.stream_url,
                    &ctx.stream_id,
                ));
                break;
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
    fn detects_content_change_on_equal_mseq() {
        let check = PlaylistContentCheck;
        let prev = make_prev(5, &["a.ts", "b.ts"]);
        let curr = make_snap(5, &["c.ts", "b.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].details.contains("index(0)"));
        assert!(errors[0].details.contains("a.ts"));
    }

    #[test]
    fn no_error_when_content_matches() {
        let check = PlaylistContentCheck;
        let prev = make_prev(5, &["a.ts", "b.ts"]);
        let curr = make_snap(5, &["a.ts", "b.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn ignores_query_params_in_comparison() {
        let check = PlaylistContentCheck;
        let prev = make_prev(5, &["a.ts?token=abc", "b.ts?token=abc"]);
        let curr = make_snap(5, &["a.ts?token=xyz", "b.ts?token=xyz"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn skip_when_mseq_differs() {
        let check = PlaylistContentCheck;
        let prev = make_prev(5, &["a.ts", "b.ts"]);
        let curr = make_snap(6, &["c.ts", "d.ts"]);
        let errors = check.check(&prev, &curr, &ctx());
        assert!(errors.is_empty());
    }
}
