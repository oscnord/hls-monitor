use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::{CheckContext, PlaylistSnapshot, VariantState};

use super::Check;

pub struct EncryptionConsistencyCheck;

impl Check for EncryptionConsistencyCheck {
    fn name(&self) -> &'static str {
        "EncryptionConsistency"
    }

    fn check(
        &self,
        _prev: &VariantState,
        curr: &PlaylistSnapshot,
        ctx: &CheckContext,
    ) -> Vec<MonitorError> {
        let mut errors = Vec::new();

        for key in &curr.keys {
            if key.method == "NONE" && key.has_uri {
                errors.push(MonitorError::new(
                    ErrorType::EncryptionViolation,
                    &ctx.media_type,
                    &ctx.variant_key,
                    "METHOD=NONE must not have URI",
                    &ctx.stream_url,
                    &ctx.stream_id,
                ));
            }

            if key.method != "NONE" && !key.has_uri {
                errors.push(MonitorError::new(
                    ErrorType::EncryptionViolation,
                    &ctx.media_type,
                    &ctx.variant_key,
                    format!("METHOD={} requires URI", key.method),
                    &ctx.stream_url,
                    &ctx.stream_id,
                ));
            }

            if key.method == "AES-128" && curr.has_map && !key.has_iv {
                errors.push(MonitorError::new(
                    ErrorType::EncryptionViolation,
                    &ctx.media_type,
                    &ctx.variant_key,
                    "AES-128 with EXT-X-MAP requires IV",
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
    use crate::monitor::state::{KeySnapshot, SegmentSnapshot};

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

    fn make_snap(keys: Vec<KeySnapshot>, has_map: bool) -> PlaylistSnapshot {
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
            version: None,
            has_gaps: false,
            has_endlist: false,
            i_frames_only: false,
            has_byte_range: false,
            has_map,
            has_key_iv: false,
            has_key_format: false,
            keys,
        }
    }

    #[test]
    fn no_error_no_keys() {
        let check = EncryptionConsistencyCheck;
        let snap = make_snap(vec![], false);
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_valid_aes128() {
        let check = EncryptionConsistencyCheck;
        let snap = make_snap(
            vec![KeySnapshot {
                method: "AES-128".into(),
                has_uri: true,
                has_iv: true,
                has_keyformat: false,
            }],
            false,
        );
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_none_with_uri() {
        let check = EncryptionConsistencyCheck;
        let snap = make_snap(
            vec![KeySnapshot {
                method: "NONE".into(),
                has_uri: true,
                has_iv: false,
                has_keyformat: false,
            }],
            false,
        );
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::EncryptionViolation);
        assert!(errors[0].details.contains("NONE must not have URI"));
    }

    #[test]
    fn error_method_without_uri() {
        let check = EncryptionConsistencyCheck;
        let snap = make_snap(
            vec![KeySnapshot {
                method: "AES-128".into(),
                has_uri: false,
                has_iv: true,
                has_keyformat: false,
            }],
            false,
        );
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::EncryptionViolation);
        assert!(errors[0].details.contains("AES-128 requires URI"));
    }

    #[test]
    fn error_aes128_map_without_iv() {
        let check = EncryptionConsistencyCheck;
        let snap = make_snap(
            vec![KeySnapshot {
                method: "AES-128".into(),
                has_uri: true,
                has_iv: false,
                has_keyformat: false,
            }],
            true,
        );
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::EncryptionViolation);
        assert!(errors[0].details.contains("AES-128 with EXT-X-MAP requires IV"));
    }

    #[test]
    fn no_error_aes128_map_with_iv() {
        let check = EncryptionConsistencyCheck;
        let snap = make_snap(
            vec![KeySnapshot {
                method: "AES-128".into(),
                has_uri: true,
                has_iv: true,
                has_keyformat: false,
            }],
            true,
        );
        let errors = check.check(&make_prev(), &snap, &ctx());
        assert!(errors.is_empty());
    }
}
