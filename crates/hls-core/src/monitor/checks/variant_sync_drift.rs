use std::collections::HashMap;

use crate::monitor::checks::stream_check::{StreamCheck, StreamCheckContext};
use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::VariantState;

pub struct VariantSyncDriftCheck {
    threshold: u64,
}

impl VariantSyncDriftCheck {
    pub fn new(threshold: u64) -> Self {
        Self { threshold }
    }
}

impl StreamCheck for VariantSyncDriftCheck {
    fn name(&self) -> &'static str {
        "VariantSyncDrift"
    }

    fn check(
        &self,
        variants: &HashMap<String, VariantState>,
        ctx: &StreamCheckContext,
    ) -> Vec<MonitorError> {
        if variants.len() < 2 {
            return Vec::new();
        }

        let mut min_key = "";
        let mut min_mseq = u64::MAX;
        let mut max_key = "";
        let mut max_mseq = u64::MIN;

        for (key, state) in variants {
            if state.media_sequence < min_mseq {
                min_mseq = state.media_sequence;
                min_key = key;
            }
            if state.media_sequence > max_mseq {
                max_mseq = state.media_sequence;
                max_key = key;
            }
        }

        let drift = max_mseq - min_mseq;
        if drift > self.threshold {
            vec![MonitorError::new(
                ErrorType::VariantSyncDrift,
                "ALL",
                "ALL",
                format!(
                    "Variant sync drift: '{}' at mseq({}) is {} segments ahead of '{}' at mseq({})",
                    max_key, max_mseq, drift, min_key, min_mseq,
                ),
                &ctx.stream_url,
                &ctx.stream_id,
            )]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_variant(mseq: u64) -> VariantState {
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: mseq,
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
    fn no_error_in_sync() {
        let check = VariantSyncDriftCheck::new(3);
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(100));
        variants.insert("1080p".to_string(), make_variant(101));
        let errors = check.check(&variants, &make_ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_drift() {
        let check = VariantSyncDriftCheck::new(3);
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(100));
        variants.insert("1080p".to_string(), make_variant(110));
        let errors = check.check(&variants, &make_ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::VariantSyncDrift);
        assert!(errors[0].details.contains("10 segments ahead"));
        assert_eq!(errors[0].media_type, "ALL");
        assert_eq!(errors[0].variant, "ALL");
    }

    #[test]
    fn no_error_single_variant() {
        let check = VariantSyncDriftCheck::new(3);
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(100));
        let errors = check.check(&variants, &make_ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_within_threshold() {
        let check = VariantSyncDriftCheck::new(3);
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(100));
        variants.insert("1080p".to_string(), make_variant(103));
        let errors = check.check(&variants, &make_ctx());
        assert!(errors.is_empty());
    }
}
