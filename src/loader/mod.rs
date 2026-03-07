mod http;

pub use http::HttpLoader;

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("HTTP error {status} fetching {url}: {message}")]
    Http {
        url: String,
        status: u16,
        message: String,
        is_last_retry: bool,
    },
    #[error("Network error fetching {url}: {reason}")]
    Network {
        url: String,
        reason: String,
        is_last_retry: bool,
    },
    #[error("Parse error for {url}: {message}")]
    Parse { url: String, message: String },
    #[error("Timeout fetching {url}")]
    Timeout { url: String, is_last_retry: bool },
}

impl LoadError {
    pub fn is_last_retry(&self) -> bool {
        match self {
            Self::Http { is_last_retry, .. } => *is_last_retry,
            Self::Network { is_last_retry, .. } => *is_last_retry,
            Self::Timeout { is_last_retry, .. } => *is_last_retry,
            Self::Parse { .. } => true,
        }
    }

    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::Http { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// Trait for loading HLS manifests from a URI.
///
/// Implementations handle HTTP fetching, retries, and returning raw manifest text.
/// The trait is object-safe and Send + Sync for use across async tasks.
#[async_trait]
pub trait ManifestLoader: Send + Sync {
    async fn load(&self, uri: &str) -> Result<String, LoadError>;
}
