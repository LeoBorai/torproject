use std::fmt::Display;
use std::fs::{create_dir, remove_file, File};
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result};
use dirs::cache_dir;
use flate2::read::GzDecoder;
use tar::Archive;
use tracing::{debug, info};

use crate::{DEFAULT_VERSION, DOWNLOAD_DIRECTORY};

/// Tor Build Targets Available
pub enum Target {
    AndroidAarch64,
    AndroidArmv7,
    AndroidX86,
    AndroidX8664,
    GnuLinuxI686,
    GnuLinuxX8664,
    MacOSAarch64,
    MacOSX8664,
    WindowsI686,
    WindowsX8664,
}

impl Default for Target {
    fn default() -> Self {
        #[cfg(all(target_arch = "aarch64", target_os = "android"))]
        return Self::AndroidAarch64;

        #[cfg(all(target_arch = "arm", target_os = "android"))]
        return Self::AndroidArmv7;

        #[cfg(all(target_arch = "x86", target_os = "android"))]
        return Self::AndroidX86;

        #[cfg(all(target_arch = "x86_64", target_os = "android"))]
        return Self::AndroidX8664;

        #[cfg(all(target_arch = "x86", target_os = "linux"))]
        return Self::GnuLinuxI686;

        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        return Self::GnuLinuxX8664;

        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        return Self::MacOSAarch64;

        #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
        return Self::MacOSX8664;

        #[cfg(all(target_arch = "x86", target_os = "windows"))]
        return Self::WindowsI686;

        #[cfg(all(target_arch = "x86_64", target_os = "windows"))]
        return Self::WindowsX8664;
    }
}

impl Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let target_str = match self {
            Target::AndroidAarch64 => "android-aarch64",
            Target::AndroidArmv7 => "android-armv7",
            Target::AndroidX86 => "android-x86",
            Target::AndroidX8664 => "android-x86_64",
            Target::GnuLinuxI686 => "linux-i686",
            Target::GnuLinuxX8664 => "linux-x86_64",
            Target::MacOSAarch64 => "macos-aarch64",
            Target::MacOSX8664 => "macos-x86_64",
            Target::WindowsI686 => "windows-i686",
            Target::WindowsX8664 => "windows-x86_64",
        };

        write!(f, "{target_str}")
    }
}

#[derive(Default)]
pub struct DownloadOptions {
    pub download_path: Option<PathBuf>,
    pub target: Option<Target>,
    pub version: Option<String>,
}

impl DownloadOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_download_path(mut self, download_path: PathBuf) -> Self {
        self.download_path = Some(download_path);
        self
    }

    pub fn with_target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    pub fn with_version<S: Into<String>>(mut self, version: S) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn build(self) -> Result<Downloader> {
        let download_path = if let Some(download_path) = self.download_path {
            download_path
        } else {
            Downloader::default_download_path()?
        };

        let target = self.target.unwrap_or_default();
        let version = self.version.unwrap_or_else(|| DEFAULT_VERSION.to_string());

        Ok(Downloader {
            download_path,
            target,
            version,
        })
    }
}

/// Tor Expert Bundle Downloader
/// https://www.torproject.org/download/tor/
pub struct Downloader {
    download_path: PathBuf,
    target: Target,
    version: String,
}

impl Downloader {
    pub fn new() -> Result<Self> {
        Self::new_with_options(DownloadOptions::default())
    }

    pub fn new_with_options(options: DownloadOptions) -> Result<Self> {
        options.build()
    }

    #[inline]
    pub fn download_path(&self) -> &PathBuf {
        &self.download_path
    }

    #[inline]
    pub fn version(&self) -> &String {
        &self.version
    }

    /// Downloads the Tor Expert Bundle and returns the path to its assets.
    pub async fn download(&self) -> Result<()> {
        let download_url = self.download_url();

        info!(%download_url, "Downloading Tor Expert Bundle.");

        let bytes = reqwest::get(download_url)
            .await
            .context("Failed to download Tor Expert Bundle from origin.")?
            .bytes()
            .await
            .context("Failed to retrieve files from response.")?
            .to_vec();

        self.store_downloaded_assets(bytes)?;
        self.decompress_tarball()?;

        Ok(())
    }

    pub fn download_tarball_path(&self) -> PathBuf {
        self.download_path.join(self.tarball_name())
    }

    fn decompress_tarball(&self) -> Result<()> {
        let tarball_path = self.download_tarball_path();
        let tar_gz = File::open(tarball_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);

        info!(download_dir_path=?self.download_path, "Unpacking tarball.");

        archive.unpack(&self.download_path)?;

        Ok(())
    }

    fn default_download_path() -> Result<PathBuf> {
        let mut download_path =
            cache_dir().context("No cache directory available on this platform.")?;
        download_path.push(DOWNLOAD_DIRECTORY);
        Ok(download_path)
    }

    fn download_url(&self) -> String {
        format!("https://archive.torproject.org/tor-package-archive/torbrowser/{version}/tor-expert-bundle-{target}-{version}.tar.gz",
          target=self.target,
          version=self.version)
    }

    fn store_downloaded_assets(&self, bytes: Vec<u8>) -> Result<()> {
        let download_path = self.download_path.clone();

        if !download_path.exists() {
            create_dir(&download_path).context("Failed to create download directory.")?;
        }

        info!(?download_path, "Storing Tor Artifacts.");

        let mut bytes = bytes.as_slice();
        let download_tarball_path = self.download_tarball_path();

        if download_tarball_path.exists() {
            debug!(download_tarball_path=%download_tarball_path.display(), "Found output file tarball. Clearing.");
            remove_file(&download_tarball_path)
                .context("Failed to delete previous Tor Cached installation.")?;
        }

        let mut output = File::create_new(&download_tarball_path)
            .context("Failed to create output tarball file.")?;

        io::copy(&mut bytes, &mut output).context("Failed to copy output bytes.")?;

        Ok(())
    }

    fn tarball_name(&self) -> String {
        format!(
            "tor-expert-bundle-{target}-{version}.tar.gz",
            target = self.target,
            version = self.version
        )
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::{Target, DEFAULT_VERSION};

    use super::Downloader;

    #[test]
    fn build_download_url_for_default() -> Result<()> {
        let downloader = Downloader::new()?;
        let target = Target::default();
        let have = downloader.download_url();
        let want = format!("https://archive.torproject.org/tor-package-archive/torbrowser/{DEFAULT_VERSION}/tor-expert-bundle-{target}-{DEFAULT_VERSION}.tar.gz");

        assert_eq!(want, have);

        Ok(())
    }

    #[tokio::test]
    async fn downloads() -> Result<()> {
        let downloader = Downloader::new()?;

        downloader
            .download()
            .await
            .expect("Failed to perform download.");

        Ok(())
    }
}
