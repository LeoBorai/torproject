pub use downloader::{DownloadOptions, Downloader, Target};
pub use tor::Tor;

pub const DEFAULT_VERSION: &str = "15.0.26";
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
