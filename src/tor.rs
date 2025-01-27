use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Error, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::{Downloader, DOWNLOAD_DIRECTORY_TOR};
use crate::{DownloadOptions, VersionSelection};

/// Message printed on Tor Console when completely bootstraped.
const TOR_BOOTSTRAPED_LOG: &str = "Bootstrapped 100% (done): Done";

pub struct Tor {
    pid: Option<u32>,
    path: PathBuf,
    version: String,
}

impl Tor {
    /// Downloads Tor Expert Bundle into cache and creates an instance
    /// of [`Tor`] to interact with Expert Bundle binaries.
    pub async fn setup_with_version(version_selection: VersionSelection) -> Result<Tor> {
        let downloader = Downloader::new_with_options(
            DownloadOptions::default().with_version_selection(version_selection),
        )
        .await?;

        downloader.download().await?;

        Ok(Tor {
            pid: None,
            path: downloader.download_path().to_owned(),
            version: downloader.version().to_owned(),
        })
    }

    // Keep existing setup() for backward compatibility
    pub async fn setup() -> Result<Tor> {
        Self::setup_with_version(VersionSelection::default()).await
    }

    #[inline]
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    #[inline]
    pub fn version(&self) -> &String {
        &self.version
    }

    pub async fn run(&mut self) -> Result<u32> {
        let bin_path = self.tor_bin_dir_path();
        let tor_bin = bin_path.join("tor");
        let mut child = Command::new(tor_bin)
            .stdout(Stdio::piped())
            .spawn()
            .context("Failed to spawn Tor Process")?;
        let pid = child.id().ok_or(Error::msg("No Process ID for Tor"))?;

        self.pid = Some(pid);

        let stdout = child.stdout.take().context("Failed to retrieve Stdout")?;
        let mut reader = BufReader::new(stdout).lines();

        tokio::spawn(async move {
            child.wait().await.expect("Tor Process errored.");
        });

        while let Some(line) = reader.next_line().await? {
            if line.contains(TOR_BOOTSTRAPED_LOG) {
                break;
            }
        }

        Ok(pid)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn kill(&self) -> Result<()> {
        use nix::sys::signal::{kill, SIGKILL};
        use nix::unistd::Pid;

        if let Some(pid) = &self.pid {
            let pid = Pid::from_raw(*pid as i32);
            kill(pid, Some(SIGKILL))?;
            return Ok(());
        }

        anyhow::bail!("No process for Tor avaialable.")
    }

    #[cfg(target_os = "windows")]
    pub fn kill(&self) -> Result<()> {
        eprintln!("Not implemented!");
        Ok(())
    }

    fn tor_bin_dir_path(&self) -> PathBuf {
        let dl_path = self.path.clone();
        dl_path.join(DOWNLOAD_DIRECTORY_TOR)
    }
}

impl Drop for Tor {
    fn drop(&mut self) {
        // intentionally ignore error due to exec context
        let _ = self.kill();
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

    #[tokio::test]
    async fn pid_returned_by_run_matches() {
        let mut tor = Tor::setup().await.expect("Failed to setup a Tor instance.");
        let tor_pid = tor.run().await.expect("Failed to run Tor Proxy");
        let instance_pid = tor.pid().unwrap();

        assert_eq!(tor_pid, instance_pid);
    }
}
