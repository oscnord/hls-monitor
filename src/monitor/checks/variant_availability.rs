use std::collections::HashMap;

use crate::monitor::checks::stream_check::{StreamCheck, StreamCheckContext};
use crate::monitor::error::{ErrorType, MonitorError};
use crate::monitor::state::VariantState;

pub struct VariantAvailabilityCheck {
    failure_threshold: u32,
}

impl VariantAvailabilityCheck {
    pub fn new(failure_threshold: u32) -> Self {
        Self { failure_threshold }
    }
}

impl StreamCheck for VariantAvailabilityCheck {
    fn name(&self) -> &'static str {
        "VariantAvailability"
    }

    fn check(
        &self,
        _variants: &HashMap<String, VariantState>,
        ctx: &StreamCheckContext,
    ) -> Vec<MonitorError> {
        let has_healthy = ctx
            .variant_failures
            .values()
            .any(|&count| count == 0);

        let all_zero_or_missing = ctx.variant_failures.is_empty()
            || ctx.variant_failures.values().all(|&count| count == 0);

        if all_zero_or_missing {
            return Vec::new();
        }

        let all_failing = !ctx.variant_failures.is_empty()
            && ctx.variant_failures.values().all(|&count| count >= self.failure_threshold);

        if all_failing {
            return Vec::new();
        }

        if !has_healthy {
            return Vec::new();
        }

        let mut errors = Vec::new();
        for (variant_key, &failures) in &ctx.variant_failures {
            if failures >= self.failure_threshold {
                errors.push(MonitorError::new(
                    ErrorType::VariantUnavailable,
                    "ALL",
                    variant_key.as_str(),
                    format!(
                        "Variant '{}' unavailable for {} consecutive polls while other variants are active",
                        variant_key, failures,
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

    fn make_ctx_with_failures(failures: Vec<(&str, u32)>) -> StreamCheckContext {
        let mut variant_failures = HashMap::new();
        for (key, count) in failures {
            variant_failures.insert(key.to_string(), count);
        }
        StreamCheckContext {
            stream_url: "http://example.com/master.m3u8".to_string(),
            stream_id: "stream_1".to_string(),
            variant_failures,
        }
    }

    #[test]
    fn no_error_all_healthy() {
        let check = VariantAvailabilityCheck::new(3);
        let ctx = make_ctx_with_failures(vec![("720p", 0), ("1080p", 0)]);
        let errors = check.check(&HashMap::new(), &ctx);
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_unavailable_variant() {
        let check = VariantAvailabilityCheck::new(3);
        let ctx = make_ctx_with_failures(vec![("720p", 3), ("1080p", 0)]);
        let errors = check.check(&HashMap::new(), &ctx);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].error_type, ErrorType::VariantUnavailable);
        assert!(errors[0].details.contains("720p"));
        assert!(errors[0].details.contains("3 consecutive polls"));
        assert_eq!(errors[0].media_type, "ALL");
        assert_eq!(errors[0].variant, "720p");
    }

    #[test]
    fn no_error_below_threshold() {
        let check = VariantAvailabilityCheck::new(3);
        let ctx = make_ctx_with_failures(vec![("720p", 2), ("1080p", 0)]);
        let errors = check.check(&HashMap::new(), &ctx);
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_all_failing() {
        let check = VariantAvailabilityCheck::new(3);
        let ctx = make_ctx_with_failures(vec![("720p", 5), ("1080p", 4)]);
        let errors = check.check(&HashMap::new(), &ctx);
        assert!(errors.is_empty());
    }

    #[test]
    fn no_error_empty_variants() {
        let check = VariantAvailabilityCheck::new(3);
        let ctx = make_ctx_with_failures(vec![]);
        let errors = check.check(&HashMap::new(), &ctx);
        assert!(errors.is_empty());
    }
}
