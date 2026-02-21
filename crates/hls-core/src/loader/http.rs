use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use tracing::{debug, warn};

use super::{LoadError, ManifestLoader};

/// HTTP-based manifest loader with connection pooling, retries, and backoff.
#[derive(Debug, Clone)]
pub struct HttpLoader {
    client: Client,
    max_retries: u32,
    base_backoff: Duration,
}

impl HttpLoader {
    pub fn new(timeout: Duration, max_retries: u32, base_backoff: Duration) -> Self {
        let client = Self::build_client(timeout);
        Self {
            client,
            max_retries,
            base_backoff,
        }
    }

    pub fn with_client(client: Client, max_retries: u32, base_backoff: Duration) -> Self {
        Self {
            client,
            max_retries,
            base_backoff,
        }
    }

    pub fn from_config(config: &crate::config::MonitorConfig) -> Self {
        Self::new(config.request_timeout, config.max_retries, config.retry_backoff)
    }

    pub fn from_config_with_client(config: &crate::config::MonitorConfig, client: Client) -> Self {
        Self::with_client(client, config.max_retries, config.retry_backoff)
    }

    pub fn build_client(timeout: Duration) -> Client {
        Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(20)
            .gzip(true)
            .build()
            .expect("Failed to build HTTP client")
    }
}

impl Default for HttpLoader {
    fn default() -> Self {
        Self::new(
            Duration::from_secs(10),
            3,
            Duration::from_millis(100),
        )
    }
}

#[async_trait]
impl ManifestLoader for HttpLoader {
    async fn load(&self, uri: &str) -> Result<String, LoadError> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            let is_last = attempt == self.max_retries;

            if attempt > 0 {
                let backoff = self.base_backoff * 2u32.saturating_pow(attempt - 1);
                debug!(uri, attempt, backoff_ms = backoff.as_millis(), "Retrying manifest fetch");
                tokio::time::sleep(backoff).await;
            }

            match self.client.get(uri).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.text().await {
                            Ok(body) => return Ok(body),
                            Err(e) => {
                                last_error = Some(LoadError::Network {
                                    url: uri.to_string(),
                                    reason: e.to_string(),
                                    is_last_retry: is_last,
                                });
                            }
                        }
                    } else {
                        let status = response.status().as_u16();
                        let message = response.status().canonical_reason()
                            .unwrap_or("Unknown")
                            .to_string();
                        warn!(uri, status, attempt, "Manifest fetch returned error status");
                        let err = LoadError::Http {
                            url: uri.to_string(),
                            status,
                            message,
                            is_last_retry: is_last,
                        };

                        if status >= 400 && status < 500 && status != 429 {
                            return Err(err);
                        }
                        last_error = Some(err);
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        warn!(uri, attempt, "Manifest fetch timed out");
                        last_error = Some(LoadError::Timeout {
                            url: uri.to_string(),
                            is_last_retry: is_last,
                        });
                    } else {
                        warn!(uri, attempt, error = %e, "Manifest fetch network error");
                        last_error = Some(LoadError::Network {
                            url: uri.to_string(),
                            reason: e.to_string(),
                            is_last_retry: is_last,
                        });
                    }
                }
            }
        }

        Err(last_error.expect("Loop must have produced an error"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn load_returns_body_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/test.m3u8"))
            .respond_with(ResponseTemplate::new(200).set_body_string("#EXTM3U\n#EXT-X-VERSION:3"))
            .mount(&server)
            .await;

        let loader = HttpLoader::new(Duration::from_secs(5), 0, Duration::from_millis(10));
        let result = loader.load(&format!("{}/test.m3u8", server.uri())).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("#EXTM3U"));
    }

    #[tokio::test]
    async fn load_returns_error_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing.m3u8"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let loader = HttpLoader::new(Duration::from_secs(5), 2, Duration::from_millis(10));
        let result = loader.load(&format!("{}/missing.m3u8", server.uri())).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), Some(404));
    }

    #[tokio::test]
    async fn load_retries_on_500_then_succeeds() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/retry.m3u8"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(2)
            .expect(2)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/retry.m3u8"))
            .respond_with(ResponseTemplate::new(200).set_body_string("#EXTM3U\nOK"))
            .mount(&server)
            .await;

        let loader = HttpLoader::new(Duration::from_secs(5), 3, Duration::from_millis(10));
        let result = loader.load(&format!("{}/retry.m3u8", server.uri())).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("OK"));
    }

    #[tokio::test]
    async fn load_returns_error_after_max_retries() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/fail.m3u8"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let loader = HttpLoader::new(Duration::from_secs(5), 2, Duration::from_millis(10));
        let result = loader.load(&format!("{}/fail.m3u8", server.uri())).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_last_retry());
    }
}
