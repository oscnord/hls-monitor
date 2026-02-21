use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct GapCheck;

impl Check for GapCheck {
    fn name(&self) -> &'static str {
        "Gap"
    }

    fn check(
        &self,
        _prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        let mut errors = Vec::new();

        for (i, seg) in curr.segments.iter().enumerate() {
            if seg.gap {
                let mseq = curr.media_sequence + i as u64;
                errors.push(MonitorError::new(
                    ErrorType::GapDetected,
                    &ctx.media_type,
                    &ctx.variant_key,
                    format!(
                        "EXT-X-GAP at index({}) in mseq({}) \u{2014} segment: '{}'",
                        i, mseq, seg.uri
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

    fn make_segment(uri: &str, gap: bool) -> SegmentSnapshot {
        SegmentSnapshot {
            uri: uri.into(),
            duration: 10.0,
            discontinuity: false,
            cue_out: false,
            cue_in: false,
            cue_out_cont: None,
            gap,
            program_date_time: None,
            daterange: None,
        }
    }

    fn make_snap(segments: Vec<SegmentSnapshot>) -> PlaylistSnapshot {
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
            target_duration: 10.0,
            playlist_type: None,
            version: None,
            has_gaps: false,
        }
    }

    #[test]
    fn no_error_without_gaps() {
        let check = GapCheck;
        let snap = make_snap(vec![
            make_segment("a.ts", false),
            make_segment("b.ts", false),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_single_gap() {
        let check = GapCheck;
        let snap = make_snap(vec![
            make_segment("a.ts", false),
            make_segment("b.ts", true),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::GapDetected);
        assert!(errors[0].details.contains("index(1)"));
        assert!(errors[0].details.contains("mseq(101)"));
        assert!(errors[0].details.contains("b.ts"));
    }

    #[test]
    fn detects_multiple_gaps() {
        let check = GapCheck;
        let snap = make_snap(vec![
            make_segment("a.ts", true),
            make_segment("b.ts", false),
            make_segment("c.ts", true),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 2);
        assert!(errors[0].details.contains("a.ts"));
        assert!(errors[1].details.contains("c.ts"));
    }
}
