use std::collections::HashMap;

use url::Url;

#[async_trait::async_trait]
pub trait UrlFetcher: Send + Sync {
    async fn fetch_binary(&self, url: &Url) -> Result<Vec<u8>, String>;
}

#[derive(Debug, Clone)]
pub struct DefaultUrlFetcher;

#[async_trait::async_trait]
impl UrlFetcher for DefaultUrlFetcher {
    async fn fetch_binary(&self, url: &Url) -> Result<Vec<u8>, String> {
        let response = reqwest::get(url.as_str())
            .await
            .map_err(|e| format!("Failed to fetch URL {}: {}", url, e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to fetch URL {}: HTTP {}",
                url,
                response.status()
            ));
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|e| format!("Failed to read response body: {}", e))
    }
}

pub struct DummyUrlFetcher {
    pub data: HashMap<Url, Vec<u8>>,
}

impl DummyUrlFetcher {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
    pub fn add_data(&mut self, url: Url, data: impl Into<Vec<u8>>) {
        self.data.insert(url, data.into());
    }
}

#[async_trait::async_trait]
impl UrlFetcher for DummyUrlFetcher {
    async fn fetch_binary(&self, url: &Url) -> Result<Vec<u8>, String> {
        match self.data.get(url) {
            Some(data) => Ok(data.clone()),
            None => Err(format!("DummyUrlFetcher: No data found for URL {}", url)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    #[tokio::test]
    async fn test_dummy_url_fetcher_success() {
        let mut fetcher = DummyUrlFetcher::new();
        let url = Url::parse("https://example.com").unwrap();
        let test_data = b"test data".to_vec();

        fetcher.add_data(url.clone(), test_data.clone());

        let result = fetcher.fetch_binary(&url).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_data);
    }

    #[tokio::test]
    async fn test_dummy_url_fetcher_not_found() {
        let fetcher = DummyUrlFetcher::new();
        let url = Url::parse("https://example.com").unwrap();

        let result = fetcher.fetch_binary(&url).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No data found for URL"));
    }

    #[tokio::test]
    async fn test_dummy_url_fetcher_add_multiple_data() {
        let mut fetcher = DummyUrlFetcher::new();
        let url1 = Url::parse("https://example.com/1").unwrap();
        let url2 = Url::parse("https://example.com/2").unwrap();
        let data1 = b"data1".to_vec();
        let data2 = b"data2".to_vec();

        fetcher.add_data(url1.clone(), data1.clone());
        fetcher.add_data(url2.clone(), data2.clone());

        let result1 = fetcher.fetch_binary(&url1).await;
        let result2 = fetcher.fetch_binary(&url2).await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert_eq!(result1.unwrap(), data1);
        assert_eq!(result2.unwrap(), data2);
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn test_default_url_fetcher_success() {
        let fetcher = DefaultUrlFetcher;
        let url = Url::parse("https://httpbin.org/bytes/10").unwrap();

        let result = fetcher.fetch_binary(&url).await;
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.len(), 10);
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn test_default_url_fetcher_not_found() {
        let fetcher = DefaultUrlFetcher;
        let url = Url::parse("https://httpbin.org/status/404").unwrap();

        let result = fetcher.fetch_binary(&url).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTP 404"));
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn test_default_url_fetcher_server_error() {
        let fetcher = DefaultUrlFetcher;
        let url = Url::parse("https://httpbin.org/status/500").unwrap();

        let result = fetcher.fetch_binary(&url).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTP 500"));
    }
}
