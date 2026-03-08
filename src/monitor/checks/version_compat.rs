use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct VersionCompatibilityCheck;

impl Check for VersionCompatibilityCheck {
    fn name(&self) -> &'static str {
        "VersionCompatibility"
    }

    fn check(
        &self,
        _prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        let version = match curr.version {
            Some(v) => v,
            None => return vec![],
        };

        let mut min_required: u16 = 1;
        let mut reason = "";

        if curr.has_key_iv && min_required < 2 {
            min_required = 2;
            reason = "EXT-X-KEY with IV requires version 2+";
        }

        if curr.segments.iter().any(|s| s.duration.fract() != 0.0) && min_required < 3 {
            min_required = 3;
            reason = "fractional EXTINF duration requires version 3+";
        }

        if curr.has_byte_range && min_required < 4 {
            min_required = 4;
            reason = "EXT-X-BYTERANGE requires version 4+";
        }

        if curr.i_frames_only && min_required < 4 {
            min_required = 4;
            reason = "EXT-X-I-FRAMES-ONLY requires version 4+";
        }

        if curr.has_key_format && min_required < 5 {
            min_required = 5;
            reason = "KEYFORMAT requires version 5+";
        }

        if curr.has_map && !curr.i_frames_only && min_required < 6 {
            min_required = 6;
            reason = "EXT-X-MAP without I-FRAMES-ONLY requires version 6+";
        }

        if version < min_required {
            return vec![MonitorError::new(
                ErrorType::VersionCompatibility,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "EXT-X-VERSION {} is below minimum required {}: {}",
                    version, min_required, reason
                ),
                &ctx.stream_url,
                &ctx.stream_id,
            )];
        }

        vec![]
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
            target_duration: 10.0,
            playlist_type: None,
            has_endlist: false,
        }
    }

    fn make_segment(uri: &str, duration: f64) -> SegmentSnapshot {
        SegmentSnapshot {
            uri: uri.into(),
            duration,
            discontinuity: false,
            cue_out: false,
            cue_in: false,
            cue_out_cont: None,
            gap: false,
            program_date_time: None,
            daterange: None,
        }
    }

    fn make_snap(version: Option<u16>) -> PlaylistSnapshot {
        PlaylistSnapshot {
            media_sequence: 100,
            discontinuity_sequence: 0,
            segments: vec![make_segment("a.ts", 10.0)],
            duration: 10.0,
            cue_out_count: 0,
            cue_in_count: 0,
            has_cue_out: false,
            cue_out_duration: None,
            target_duration: 10.0,
            playlist_type: None,
            version,
            has_gaps: false,
            has_endlist: false,
            i_frames_only: false,
            has_byte_range: false,
            has_map: false,
            has_key_iv: false,
            has_key_format: false,
            keys: vec![],
        }
    }

    #[test]
    fn no_error_sufficient_version() {
        let check = VersionCompatibilityCheck;
        let mut snap = make_snap(Some(7));
        snap.has_map = true;
        snap.segments = vec![make_segment("a.ts", 10.5)];
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_fractional_below_v3() {
        let check = VersionCompatibilityCheck;
        let mut snap = make_snap(Some(2));
        snap.segments = vec![make_segment("a.ts", 10.5)];
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::VersionCompatibility);
        assert!(errors[0].details.contains("VERSION 2"));
        assert!(errors[0].details.contains("minimum required 3"));
        assert!(errors[0].details.contains("fractional"));
    }

    #[test]
    fn error_byte_range_below_v4() {
        let check = VersionCompatibilityCheck;
        let mut snap = make_snap(Some(3));
        snap.has_byte_range = true;
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::VersionCompatibility);
        assert!(errors[0].details.contains("VERSION 3"));
        assert!(errors[0].details.contains("minimum required 4"));
        assert!(errors[0].details.contains("BYTERANGE"));
    }

    #[test]
    fn error_map_without_iframe_below_v6() {
        let check = VersionCompatibilityCheck;
        let mut snap = make_snap(Some(5));
        snap.has_map = true;
        snap.i_frames_only = false;
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::VersionCompatibility);
        assert!(errors[0].details.contains("VERSION 5"));
        assert!(errors[0].details.contains("minimum required 6"));
        assert!(errors[0].details.contains("MAP"));
    }

    #[test]
    fn no_error_no_version() {
        let check = VersionCompatibilityCheck;
        let snap = make_snap(None);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_no_features() {
        let check = VersionCompatibilityCheck;
        let snap = make_snap(Some(1));
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }
}
