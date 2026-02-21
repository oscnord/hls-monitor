use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct ProgramDateTimeCheck;

impl Check for ProgramDateTimeCheck {
    fn name(&self) -> &'static str {
        "ProgramDateTime"
    }

    fn check(
        &self,
        _prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        let mut errors = Vec::new();

        for (idx, pair) in curr.segments.windows(2).enumerate() {
            let prev_seg = &pair[0];
            let next_seg = &pair[1];

            let seg_idx = idx + 1;

            if next_seg.discontinuity {
                continue;
            }

            let (prev_pdt, next_pdt) = match (&prev_seg.program_date_time, &next_seg.program_date_time) {
                (Some(p), Some(n)) => (p, n),
                _ => continue,
            };

            let expected_ms = (prev_seg.duration * 1000.0) as i64;
            let actual_ms = (*next_pdt - *prev_pdt).num_milliseconds();
            let drift = (actual_ms - expected_ms).abs();

            if drift > 1000 {
                let mseq = curr.media_sequence + seg_idx as u64;
                errors.push(MonitorError::new(
                    ErrorType::ProgramDateTimeJump,
                    &ctx.media_type,
                    &ctx.variant_key,
                    format!(
                        "PDT discontinuity at index({seg_idx}) in mseq({mseq}): expected +{expected_ms}ms, actual diff {actual_ms}ms (drift {drift}ms)"
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
    use chrono::{DateTime, Duration, FixedOffset};

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

    fn make_segment(uri: &str, duration: f64, pdt: Option<DateTime<FixedOffset>>, disc: bool) -> SegmentSnapshot {
        SegmentSnapshot {
            uri: uri.into(),
            duration,
            discontinuity: disc,
            cue_out: false,
            cue_in: false,
            cue_out_cont: None,
            gap: false,
            program_date_time: pdt,
            daterange: None,
        }
    }

    fn make_snap(mseq: u64, segments: Vec<SegmentSnapshot>) -> PlaylistSnapshot {
        let duration: f64 = segments.iter().map(|s| s.duration).sum();
        PlaylistSnapshot {
            media_sequence: mseq,
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

    fn base_pdt() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2025-01-01T00:00:00+00:00").expect("valid datetime")
    }

    #[test]
    fn no_error_correct_progression() {
        let check = ProgramDateTimeCheck;
        let t0 = base_pdt();
        let snap = make_snap(100, vec![
            make_segment("a.ts", 10.0, Some(t0), false),
            make_segment("b.ts", 10.0, Some(t0 + Duration::seconds(10)), false),
            make_segment("c.ts", 10.0, Some(t0 + Duration::seconds(20)), false),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_pdt_jump() {
        let check = ProgramDateTimeCheck;
        let t0 = base_pdt();
        let snap = make_snap(100, vec![
            make_segment("a.ts", 10.0, Some(t0), false),
            make_segment("b.ts", 10.0, Some(t0 + Duration::seconds(50)), false),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::ProgramDateTimeJump);
        assert!(errors[0].details.contains("index(1)"));
        assert!(errors[0].details.contains("mseq(101)"));
        assert!(errors[0].details.contains("expected +10000ms"));
        assert!(errors[0].details.contains("actual diff 50000ms"));
    }

    #[test]
    fn skips_discontinuity_boundary() {
        let check = ProgramDateTimeCheck;
        let t0 = base_pdt();
        let snap = make_snap(100, vec![
            make_segment("a.ts", 10.0, Some(t0), false),
            make_segment("b.ts", 10.0, Some(t0 + Duration::seconds(500)), true),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn skips_missing_pdt() {
        let check = ProgramDateTimeCheck;
        let t0 = base_pdt();
        let snap = make_snap(100, vec![
            make_segment("a.ts", 10.0, Some(t0), false),
            make_segment("b.ts", 10.0, None, false),
            make_segment("c.ts", 10.0, Some(t0 + Duration::seconds(20)), false),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn allows_within_tolerance() {
        let check = ProgramDateTimeCheck;
        let t0 = base_pdt();
        let snap = make_snap(100, vec![
            make_segment("a.ts", 10.0, Some(t0), false),
            make_segment("b.ts", 10.0, Some(t0 + Duration::milliseconds(10_800)), false),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }
}
