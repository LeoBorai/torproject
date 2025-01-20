use std::path::PathBuf;
use std::process::{Child, Command};

use anyhow::{Context, Result};

use super::{Downloader, DOWNLOAD_DIRECTORY_TOR};

pub struct Tor {
    path: PathBuf,
    version: String,
}

impl Tor {
    /// Downloads Tor Expert Bundle into cache and creates an instance
    /// of [`Tor`] to interact with Expert Bundle binaries.
    pub async fn setup() -> Result<Tor> {
        let downloader = Downloader::default();
        downloader.download().await?;

        Ok(Tor {
            path: downloader.download_dir_path(),
            version: downloader.version().to_owned(),
        })
    }

    #[inline]
    pub fn version(&self) -> &String {
        &self.version
    }

    pub fn run(&self) -> Result<Child> {
        let bin_path = self.tor_bin_dir_path();
        let tor_bin = bin_path.join("tor");
        let child = Command::new(tor_bin)
            .spawn()
            .context("Failed to spawn Tor Process")?;

        Ok(child)
    }

    fn tor_bin_dir_path(&self) -> PathBuf {
        let dl_path = self.path.clone();
        dl_path.join(DOWNLOAD_DIRECTORY_TOR)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Tor, DEFAULT_VERSION};

    #[tokio::test]
    async fn setup_tor_instance() {
        let tor = Tor::setup().await.expect("Failed to setup a Tor instance.");
        let version = tor.version();

        assert_eq!(version, DEFAULT_VERSION);
    }

    // #[tokio::test]
    // async fn runs_tor_process() {
    //     let tor = Tor::setup().await.expect("Failed to setup a Tor instance.");
    //     let process = tor.run().expect("Failed to spawn Tor process.");
    // }
}
