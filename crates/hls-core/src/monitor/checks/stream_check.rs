use std::collections::HashMap;

use crate::monitor::error::MonitorError;
use crate::monitor::state::VariantState;

pub struct StreamCheckContext {
    pub stream_url: String,
    pub stream_id: String,
    pub variant_failures: HashMap<String, u32>,
}

pub trait StreamCheck: Send + Sync {
    fn name(&self) -> &'static str;
    fn check(
        &self,
        variants: &HashMap<String, VariantState>,
        ctx: &StreamCheckContext,
    ) -> Vec<MonitorError>;
}
