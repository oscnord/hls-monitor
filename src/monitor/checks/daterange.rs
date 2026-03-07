use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct DateRangeCheck;

impl Check for DateRangeCheck {
    fn name(&self) -> &'static str {
        "DateRange"
    }

    fn check(
        &self,
        _prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        let mut errors = Vec::new();

        for (i, seg) in curr.segments.iter().enumerate() {
            let dr = match &seg.daterange {
                Some(dr) => dr,
                None => continue,
            };

            let mseq = curr.media_sequence + i as u64;

            if let Some(end_date) = dr.end_date {
                if end_date < dr.start_date {
                    errors.push(MonitorError::new(
                        ErrorType::DateRangeViolation,
                        &ctx.media_type,
                        &ctx.variant_key,
                        format!(
                            "EXT-X-DATERANGE '{}': END-DATE {} is before START-DATE {} at index({}) in mseq({})",
                            dr.id, end_date, dr.start_date, i, mseq
                        ),
                        &ctx.stream_url,
                        &ctx.stream_id,
                    ));
                }
            }

            if let Some(duration) = dr.duration {
                if duration < 0.0 {
                    errors.push(MonitorError::new(
                        ErrorType::DateRangeViolation,
                        &ctx.media_type,
                        &ctx.variant_key,
                        format!(
                            "EXT-X-DATERANGE '{}': negative DURATION {:.3}s at index({}) in mseq({})",
                            dr.id, duration, i, mseq
                        ),
                        &ctx.stream_url,
                        &ctx.stream_id,
                    ));
                }
            }

            if dr.end_on_next {
                if dr.class.is_none() {
                    errors.push(MonitorError::new(
                        ErrorType::DateRangeViolation,
                        &ctx.media_type,
                        &ctx.variant_key,
                        format!(
                            "EXT-X-DATERANGE '{}': END-ON-NEXT requires CLASS attribute at index({}) in mseq({})",
                            dr.id, i, mseq
                        ),
                        &ctx.stream_url,
                        &ctx.stream_id,
                    ));
                }

                if dr.duration.is_some() || dr.end_date.is_some() {
                    errors.push(MonitorError::new(
                        ErrorType::DateRangeViolation,
                        &ctx.media_type,
                        &ctx.variant_key,
                        format!(
                            "EXT-X-DATERANGE '{}': END-ON-NEXT must not have DURATION or END-DATE at index({}) in mseq({})",
                            dr.id, i, mseq
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
    use crate::monitor::state::{DateRangeSnapshot, SegmentSnapshot};
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

    fn base_time() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2025-01-01T00:00:00+00:00").expect("valid datetime")
    }

    fn make_daterange(
        id: &str,
        class: Option<&str>,
        start: DateTime<FixedOffset>,
        end_date: Option<DateTime<FixedOffset>>,
        duration: Option<f64>,
        end_on_next: bool,
    ) -> DateRangeSnapshot {
        DateRangeSnapshot {
            id: id.to_string(),
            class: class.map(|c| c.to_string()),
            start_date: start,
            end_date,
            duration,
            end_on_next,
        }
    }

    fn make_segment(uri: &str, daterange: Option<DateRangeSnapshot>) -> SegmentSnapshot {
        SegmentSnapshot {
            uri: uri.into(),
            duration: 10.0,
            discontinuity: false,
            cue_out: false,
            cue_in: false,
            cue_out_cont: None,
            gap: false,
            program_date_time: None,
            daterange,
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

    #[test]
    fn no_error_valid_daterange() {
        let check = DateRangeCheck;
        let t0 = base_time();
        let dr = make_daterange("ad-1", Some("ads"), t0, Some(t0 + Duration::seconds(30)), Some(30.0), false);
        let snap = make_snap(100, vec![make_segment("a.ts", Some(dr))]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_end_before_start() {
        let check = DateRangeCheck;
        let t0 = base_time();
        let dr = make_daterange("ad-1", None, t0, Some(t0 - Duration::seconds(10)), None, false);
        let snap = make_snap(100, vec![make_segment("a.ts", Some(dr))]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::DateRangeViolation);
        assert!(errors[0].details.contains("END-DATE"));
        assert!(errors[0].details.contains("before START-DATE"));
        assert!(errors[0].details.contains("ad-1"));
    }

    #[test]
    fn error_negative_duration() {
        let check = DateRangeCheck;
        let t0 = base_time();
        let dr = make_daterange("ad-2", None, t0, None, Some(-5.0), false);
        let snap = make_snap(100, vec![make_segment("a.ts", Some(dr))]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::DateRangeViolation);
        assert!(errors[0].details.contains("negative DURATION"));
        assert!(errors[0].details.contains("-5.000s"));
    }

    #[test]
    fn error_end_on_next_without_class() {
        let check = DateRangeCheck;
        let t0 = base_time();
        let dr = make_daterange("ad-3", None, t0, None, None, true);
        let snap = make_snap(100, vec![make_segment("a.ts", Some(dr))]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::DateRangeViolation);
        assert!(errors[0].details.contains("END-ON-NEXT requires CLASS"));
    }

    #[test]
    fn error_end_on_next_with_duration() {
        let check = DateRangeCheck;
        let t0 = base_time();
        let dr = make_daterange("ad-4", Some("ads"), t0, None, Some(30.0), true);
        let snap = make_snap(100, vec![make_segment("a.ts", Some(dr))]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].details.contains("END-ON-NEXT must not have DURATION or END-DATE"));
    }

    #[test]
    fn no_error_segments_without_daterange() {
        let check = DateRangeCheck;
        let snap = make_snap(100, vec![
            make_segment("a.ts", None),
            make_segment("b.ts", None),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }
}
