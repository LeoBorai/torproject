use std::{fmt::Display, fs::File, io, path::PathBuf};

use anyhow::{Context, Error, Result};
use dirs::cache_dir;

const DEFAULT_VERSION: &str = "14.0.4";
const DOWNLOAD_DIRECTORY: &str = "TorProjectRustAssets";

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

/// Tor Expert Bundle Downloader
/// https://www.torproject.org/download/tor/
pub struct Downloader {
    target: Target,
    version: String,
}

impl Default for Downloader {
    fn default() -> Self {
        Self {
            target: Target::default(),
            version: String::from(DEFAULT_VERSION),
        }
    }
}

impl Downloader {
    pub fn new(target: Target, version: String) -> Self {
        Self { target, version }
    }

    /// Downloads the Tor Expert Bundle and returns the path to its assets.
    pub async fn download(&self) -> Result<PathBuf> {
        let mut download_path =
            cache_dir().ok_or(Error::msg("No cache directory available on this platform."))?;
        download_path.push(DOWNLOAD_DIRECTORY);

        let download_url = self.download_url();
        let bytes = reqwest::get(download_url)
            .await
            .context("Failed to download Tor Expert Bundle from origin.")?
            .bytes()
            .await
            .context("Failed to retrieve files from response.")?
            .to_vec();
        let mut bytes = bytes.as_slice();
        let mut output =
            File::create_new(self.tarball_name()).context("Failed to create output file.")?;

        io::copy(&mut bytes, &mut output).context("Failed to copy output bytes.")?;

        Ok(download_path)
    }

    fn download_url(&self) -> String {
        format!("https://archive.torproject.org/tor-package-archive/torbrowser/{version}/tor-expert-bundle-{target}-{version}.tar.gz",
      target=self.target,
      version=self.version)
    }

    fn tarball_name(&self) -> String {
        format!(
            "tor-expert-bundle-{target}-{version}",
            target = self.target,
            version = self.version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Downloader, Target, DEFAULT_VERSION};

    #[test]
    fn build_download_url_for_default() {
        let downloader = Downloader::default();
        let download_url = downloader.download_url();

        assert_eq!(download_url, "https://archive.torproject.org/tor-package-archive/torbrowser/14.0.4/tor-expert-bundle-macos-x86_64-14.0.4.tar.gz")
    }

    #[tokio::test]
    async fn downloads() {
        let downloader = Downloader::new(Target::MacOSAarch64, String::from(DEFAULT_VERSION));

        downloader
            .download()
            .await
            .expect("Failed to perform download.");
    }
}
