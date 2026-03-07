use std::collections::HashMap;
use std::collections::HashSet;

use crate::monitor::checks::stream_check::{StreamCheck, StreamCheckContext};
use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::VariantState;

pub struct VariantPlaylistTypeConsistencyCheck;

impl StreamCheck for VariantPlaylistTypeConsistencyCheck {
    fn name(&self) -> &'static str {
        "VariantPlaylistTypeConsistency"
    }

    fn check(
        &self,
        variants: &HashMap<String, VariantState>,
        ctx: &StreamCheckContext,
    ) -> Vec<MonitorError> {
        if variants.len() < 2 {
            return Vec::new();
        }

        let types: HashSet<&str> = variants
            .values()
            .filter_map(|s| s.playlist_type.as_deref())
            .collect();

        if types.len() <= 1 {
            return Vec::new();
        }

        let mut by_type: HashMap<&str, Vec<&str>> = HashMap::new();
        for (key, state) in variants {
            if let Some(ref pt) = state.playlist_type {
                by_type.entry(pt).or_default().push(key);
            }
        }

        let breakdown: Vec<String> = by_type
            .iter()
            .map(|(pt, keys)| format!("{}=[{}]", pt, keys.join(", ")))
            .collect();

        vec![MonitorError::new(
            ErrorType::VariantPlaylistTypeInconsistency,
            "ALL",
            "ALL",
            format!(
                "Variants have inconsistent EXT-X-PLAYLIST-TYPE values: {}",
                breakdown.join("; "),
            ),
            &ctx.stream_url,
            &ctx.stream_id,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_variant(playlist_type: Option<&str>) -> VariantState {
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: 100,
            segment_uris: vec![],
            discontinuity_sequence: 0,
            next_is_discontinuity: false,
            prev_segments: vec![],
            duration: 0.0,
            cue_out_count: 0,
            cue_in_count: 0,
            in_cue_out: false,
            cue_out_duration: None,
            version: None,
            target_duration: 10.0,
            playlist_type: playlist_type.map(String::from),
            has_endlist: false,
        }
    }

    fn make_ctx() -> StreamCheckContext {
        StreamCheckContext {
            stream_url: "http://example.com/master.m3u8".to_string(),
            stream_id: "stream_1".to_string(),
            variant_failures: HashMap::new(),
        }
    }

    #[test]
    fn no_error_same_type() {
        let check = VariantPlaylistTypeConsistencyCheck;
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(Some("VOD")));
        variants.insert("1080p".to_string(), make_variant(Some("VOD")));
        let errors = check.check(&variants, &make_ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_different_types() {
        let check = VariantPlaylistTypeConsistencyCheck;
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(Some("VOD")));
        variants.insert("1080p".to_string(), make_variant(Some("EVENT")));
        let errors = check.check(&variants, &make_ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::VariantPlaylistTypeInconsistency);
        assert_eq!(errors[0].media_type, "ALL");
        assert_eq!(errors[0].variant, "ALL");
        assert!(errors[0].details.contains("inconsistent"));
    }

    #[test]
    fn no_error_all_none() {
        let check = VariantPlaylistTypeConsistencyCheck;
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(None));
        variants.insert("1080p".to_string(), make_variant(None));
        let errors = check.check(&variants, &make_ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_single_variant() {
        let check = VariantPlaylistTypeConsistencyCheck;
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(Some("VOD")));
        let errors = check.check(&variants, &make_ctx());
        assert!(errors.is_empty());
    }
}
