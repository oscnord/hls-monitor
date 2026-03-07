use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct MinPlaylistDurationCheck;

impl Check for MinPlaylistDurationCheck {
    fn name(&self) -> &'static str {
        "MinPlaylistDuration"
    }

    fn check(
        &self,
        _prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        if curr.has_endlist {
            return vec![];
        }

        if curr.playlist_type.as_deref() == Some("VOD") {
            return vec![];
        }

        let required = 3.0 * curr.target_duration;
        if curr.duration < required {
            return vec![MonitorError::new(
                ErrorType::MinPlaylistDuration,
                &ctx.media_type,
                &ctx.variant_key,
                format!(
                    "Live playlist duration {:.3}s is below required minimum {:.3}s (3 x TARGETDURATION {}s)",
                    curr.duration, required, curr.target_duration
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

    fn make_snap(target_duration: f64, segments: Vec<SegmentSnapshot>) -> PlaylistSnapshot {
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
            target_duration,
            playlist_type: None,
            version: None,
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
    fn no_error_sufficient_duration() {
        let check = MinPlaylistDurationCheck;
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 10.0),
            make_segment("b.ts", 10.0),
            make_segment("c.ts", 10.0),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_insufficient_duration() {
        let check = MinPlaylistDurationCheck;
        let snap = make_snap(10.0, vec![
            make_segment("a.ts", 10.0),
            make_segment("b.ts", 10.0),
        ]);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::MinPlaylistDuration);
        assert!(errors[0].details.contains("20.000s"));
        assert!(errors[0].details.contains("30.000s"));
    }

    #[test]
    fn no_error_vod_playlist() {
        let check = MinPlaylistDurationCheck;
        let mut snap = make_snap(10.0, vec![
            make_segment("a.ts", 10.0),
        ]);
        snap.has_endlist = true;
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_playlist_type_vod() {
        let check = MinPlaylistDurationCheck;
        let mut snap = make_snap(10.0, vec![
            make_segment("a.ts", 10.0),
        ]);
        snap.playlist_type = Some("VOD".to_string());
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }
}
