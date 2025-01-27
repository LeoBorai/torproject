use std::fmt::Display;
use std::fs::{create_dir, remove_file, File};
use std::io;
use std::path::PathBuf;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use reqwest::Client;
use scraper::{Html, Selector};
use tar::Archive;
use tracing::{debug, info};

use crate::{VersionSelection, DEFAULT_VERSION, DOWNLOAD_DIRECTORY};

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
    pub version_selection: Option<VersionSelection>,
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

    pub fn with_version_selection(mut self, selection: VersionSelection) -> Self {
        self.version_selection = Some(selection);
        self
    }

    pub async fn build(self) -> Result<Downloader> {
        let download_path = self.download_path.unwrap_or_else(|| {
            Downloader::default_download_path().expect("Failed to get default download path")
        });
        let target = self.target.unwrap_or_default();
        let version_selection = self.version_selection.unwrap_or_default();
        let version = Downloader::resolve_version(&version_selection).await?;

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
        Ok(Self {
            download_path: Self::default_download_path()?,
            target: Target::default(),
            version: DEFAULT_VERSION.to_string(),
        })
    }

    pub async fn new_with_options(options: DownloadOptions) -> Result<Self> {
        options.build().await
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

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn default_download_path() -> Result<PathBuf> {
        use dirs::cache_dir;

        let mut download_path =
            cache_dir().context("No cache directory available on this platform.")?;
        download_path.push(DOWNLOAD_DIRECTORY);
        Ok(download_path)
    }

    #[cfg(target_os = "linux")]
    fn default_download_path() -> Result<PathBuf> {
        use dirs::home_dir;

        let mut download_path =
            home_dir().context("No home directory available on this platform.")?;
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

    async fn fetch_tor_versions() -> Result<Vec<String>> {
        let client = Client::new();
        let response = client
            .get("https://archive.torproject.org/tor-package-archive/torbrowser/")
            .send()
            .await
            .context("Failed to fetch Tor versions")?;

        let html = response.text().await?;
        let document = Html::parse_document(&html);

        let selector = Selector::parse("a").unwrap();
        let versions: Vec<String> = document
            .select(&selector)
            .filter_map(|el| {
                let href = el.value().attr("href")?;
                if href.ends_with('/') && href != "../" {
                    Some(href.trim_end_matches('/').to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(versions)
    }

    async fn resolve_version(selection: &VersionSelection) -> Result<String> {
        match selection {
            VersionSelection::Version(version) => Ok(version.clone()),
            VersionSelection::Latest | VersionSelection::Stable => {
                let versions = Self::fetch_tor_versions().await?;
                versions
                    .into_iter()
                    .filter(|v| {
                        if matches!(selection, VersionSelection::Stable) {
                            !v.contains("alpha") && !v.contains("beta") && !v.contains("rc")
                        } else {
                            true
                        }
                    })
                    .max_by(|a, b| {
                        let ver_a = semver::Version::parse(a)
                            .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
                        let ver_b = semver::Version::parse(b)
                            .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
                        ver_a.cmp(&ver_b)
                    })
                    .ok_or_else(|| anyhow::anyhow!("No valid versions found"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::{DownloadOptions, Target, VersionSelection, DEFAULT_VERSION};

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

    #[tokio::test]
    async fn test_specific_version_selection() -> Result<()> {
        let specific_version = "14.0.4".to_string();
        let options = DownloadOptions::default()
            .with_version_selection(VersionSelection::Version(specific_version.clone()));

        let downloader = Downloader::new_with_options(options).await?;
        assert_eq!(downloader.version(), &specific_version);
        Ok(())
    }

    #[tokio::test]
    async fn test_latest_version_selection() -> Result<()> {
        let options = DownloadOptions::default().with_version_selection(VersionSelection::Latest);

        let downloader = Downloader::new_with_options(options).await?;
        let version = downloader.version();

        // Version should be parseable as semver
        assert!(semver::Version::parse(version).is_ok());

        // Latest version should be >= default version
        let latest_ver = semver::Version::parse(version).unwrap();
        let default_ver = semver::Version::parse(DEFAULT_VERSION).unwrap();
        assert!(latest_ver >= default_ver);

        Ok(())
    }

    #[tokio::test]
    async fn test_stable_version_selection() -> Result<()> {
        let options = DownloadOptions::default().with_version_selection(VersionSelection::Stable);

        let downloader = Downloader::new_with_options(options).await?;
        let version = downloader.version();

        // Version should be parseable as semver
        assert!(semver::Version::parse(version).is_ok());

        // Should not contain alpha/beta/rc
        assert!(!version.contains("alpha"));
        assert!(!version.contains("beta"));
        assert!(!version.contains("rc"));

        Ok(())
    }

    #[tokio::test]
    async fn test_default_version_selection() -> Result<()> {
        let options = DownloadOptions::default();
        let downloader = Downloader::new_with_options(options).await?;

        assert_eq!(downloader.version(), DEFAULT_VERSION);
        Ok(())
    }
}
