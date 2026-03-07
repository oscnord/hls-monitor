use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct VersionCheck;

impl Check for VersionCheck {
    fn name(&self) -> &'static str {
        "Version"
    }

    fn check(
        &self,
        prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        match (prev.version, curr.version) {
            (Some(prev_ver), Some(curr_ver)) if prev_ver != curr_ver => {
                vec![MonitorError::new(
                    ErrorType::VersionViolation,
                    &ctx.media_type,
                    &ctx.variant_key,
                    format!(
                        "EXT-X-VERSION changed from {} to {} in mseq({})",
                        prev_ver, curr_ver, curr.media_sequence
                    ),
                    &ctx.stream_url,
                    &ctx.stream_id,
                )]
            }
            _ => vec![],
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

    fn make_prev(version: Option<u16>) -> VariantState {
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: 100,
            segment_uris: vec!["a.ts".into()],
            discontinuity_sequence: 0,
            next_is_discontinuity: false,
            prev_segments: vec![],
            duration: 10.0,
            cue_out_count: 0,
            cue_in_count: 0,
            in_cue_out: false,
            cue_out_duration: None,
            version,
        }
    }

    fn make_snap(version: Option<u16>) -> PlaylistSnapshot {
        PlaylistSnapshot {
            media_sequence: 100,
            discontinuity_sequence: 0,
            segments: vec![SegmentSnapshot {
                uri: "a.ts".into(),
                duration: 10.0,
                discontinuity: false,
                cue_out: false,
                cue_in: false,
                cue_out_cont: None,
                gap: false,
                program_date_time: None,
                daterange: None,
            }],
            duration: 10.0,
            cue_out_count: 0,
            cue_in_count: 0,
            has_cue_out: false,
            cue_out_duration: None,
            target_duration: 10.0,
            playlist_type: None,
            version,
            has_gaps: false,
        }
    }

    #[test]
    fn no_error_same_version() {
        let check = VersionCheck;
        let errors = check.check(&make_prev(Some(3)), &make_snap(Some(3)), &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_on_version_change() {
        let check = VersionCheck;
        let errors = check.check(&make_prev(Some(3)), &make_snap(Some(7)), &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::VersionViolation);
        assert!(errors[0].details.contains("from 3 to 7"));
        assert!(errors[0].details.contains("mseq(100)"));
    }

    #[test]
    fn no_error_when_prev_missing() {
        let check = VersionCheck;
        let errors = check.check(&make_prev(None), &make_snap(Some(3)), &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_when_curr_missing() {
        let check = VersionCheck;
        let errors = check.check(&make_prev(Some(3)), &make_snap(None), &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_when_both_missing() {
        let check = VersionCheck;
        let errors = check.check(&make_prev(None), &make_snap(None), &ctx());
        assert!(errors.is_empty());
    }
}
