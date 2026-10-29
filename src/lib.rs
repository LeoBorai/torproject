pub use downloader::{DownloadOptions, Downloader, Target};
pub use tor::Tor;

pub(crate) const DEFAULT_VERSION: &str = "15.0.26";
pub(crate) const DOWNLOAD_DIRECTORY: &str = "RustTorProject";
pub(crate) const DOWNLOAD_DIRECTORY_TOR: &str = "tor";

mod downloader;
mod tor;

#[derive(Debug, Clone)]
pub enum VersionSelection {
    Version(String),
    Latest,
    Stable,
}

impl Default for VersionSelection {
    fn default() -> Self {
        Self::Version(DEFAULT_VERSION.to_string())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use reqwest::{Client, Proxy};

    use crate::Tor;

    const TOR_CHECK_API: &str = "https://check.torproject.org/api/ip";
    const TOR_SOCK5_LOCAL: &str = "socks5://127.0.0.1:9050";

    #[tokio::test]
    async fn check_tor_conn() -> Result<()> {
        let mut tor = Tor::setup().await?;
        tor.run().await?;
        let proxy = Proxy::all(TOR_SOCK5_LOCAL)?;
        let client = Client::builder().proxy(proxy).build()?;
        let res_body = client.get(TOR_CHECK_API).send().await?.text().await?;
        let res_json: serde_json::Value = serde_json::from_str(&res_body)?;

        assert_eq!(res_json["IsTor"], serde_json::Value::Bool(true));
        assert!(res_json["IP"].is_string());

        Ok(())
    }
}
