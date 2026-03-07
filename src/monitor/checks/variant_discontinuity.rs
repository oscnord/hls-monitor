use std::collections::HashMap;

use crate::monitor::checks::stream_check::{StreamCheck, StreamCheckContext};
use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::VariantState;

pub struct VariantDiscontinuityConsistencyCheck;

impl StreamCheck for VariantDiscontinuityConsistencyCheck {
    fn name(&self) -> &'static str {
        "VariantDiscontinuityConsistency"
    }

    fn check(
        &self,
        variants: &HashMap<String, VariantState>,
        ctx: &StreamCheckContext,
    ) -> Vec<MonitorError> {
        if variants.len() < 2 {
            return Vec::new();
        }

        let mut by_mseq: HashMap<u64, Vec<(&str, u64)>> = HashMap::new();
        for (key, state) in variants {
            by_mseq
                .entry(state.media_sequence)
                .or_default()
                .push((key, state.discontinuity_sequence));
        }

        let mut errors = Vec::new();
        for (mseq, group) in &by_mseq {
            if group.len() < 2 {
                continue;
            }

            let first_dseq = group[0].1;
            if group.iter().all(|(_, dseq)| *dseq == first_dseq) {
                continue;
            }

            let breakdown: Vec<String> = group
                .iter()
                .map(|(key, dseq)| format!("{}=dseq({})", key, dseq))
                .collect();

            errors.push(MonitorError::new(
                ErrorType::VariantDiscontinuityInconsistency,
                "ALL",
                "ALL",
                format!(
                    "Variants at mseq({}) have mismatched discontinuity sequences: {}",
                    mseq,
                    breakdown.join(", "),
                ),
                &ctx.stream_url,
                &ctx.stream_id,
            ));
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_variant(mseq: u64, dseq: u64) -> VariantState {
        VariantState {
            media_type: "VIDEO".to_string(),
            media_sequence: mseq,
            segment_uris: vec![],
            discontinuity_sequence: dseq,
            next_is_discontinuity: false,
            prev_segments: vec![],
            duration: 0.0,
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

    fn make_ctx() -> StreamCheckContext {
        StreamCheckContext {
            stream_url: "http://example.com/master.m3u8".to_string(),
            stream_id: "stream_1".to_string(),
            variant_failures: HashMap::new(),
        }
    }

    #[test]
    fn no_error_matching_dseq() {
        let check = VariantDiscontinuityConsistencyCheck;
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(100, 5));
        variants.insert("1080p".to_string(), make_variant(100, 5));
        let errors = check.check(&variants, &make_ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn error_mismatched_dseq() {
        let check = VariantDiscontinuityConsistencyCheck;
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(100, 5));
        variants.insert("1080p".to_string(), make_variant(100, 7));
        let errors = check.check(&variants, &make_ctx());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::VariantDiscontinuityInconsistency);
        assert_eq!(errors[0].media_type, "ALL");
        assert_eq!(errors[0].variant, "ALL");
        assert!(errors[0].details.contains("mseq(100)"));
        assert!(errors[0].details.contains("mismatched"));
    }

    #[test]
    fn no_error_different_mseq() {
        let check = VariantDiscontinuityConsistencyCheck;
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(100, 5));
        variants.insert("1080p".to_string(), make_variant(101, 7));
        let errors = check.check(&variants, &make_ctx());
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_single_variant() {
        let check = VariantDiscontinuityConsistencyCheck;
        let mut variants = HashMap::new();
        variants.insert("720p".to_string(), make_variant(100, 5));
        let errors = check.check(&variants, &make_ctx());
        assert!(errors.is_empty());
    }
}
