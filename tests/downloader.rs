use anyhow::Result;
use serial_test::serial;
use torproject::{DownloadOptions, Downloader, Target, VersionSelection, DEFAULT_VERSION};

#[test]
#[serial]
fn build_download_url_for_default() -> Result<()> {
    let downloader = Downloader::new()?;
    let target = Target::default();
    let have = downloader.download_url();
    let want = format!("https://archive.torproject.org/tor-package-archive/torbrowser/{DEFAULT_VERSION}/tor-expert-bundle-{target}-{DEFAULT_VERSION}.tar.gz");

    assert_eq!(want, have);

    Ok(())
}

#[tokio::test]
#[serial]
async fn downloads() -> Result<()> {
    let downloader = Downloader::new()?;

    downloader
        .download()
        .await
        .expect("Failed to perform download.");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_specific_version_selection() -> Result<()> {
    let specific_version = "14.0.4".to_string();
    let options = DownloadOptions::default()
        .with_version_selection(VersionSelection::Version(specific_version.clone()));

    let downloader = Downloader::new_with_options(options).await?;
    assert_eq!(downloader.version(), &specific_version);
    Ok(())
}

#[tokio::test]
#[serial]
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
#[serial]
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
#[serial]
async fn test_default_version_selection() -> Result<()> {
    let options = DownloadOptions::default();
    let downloader = Downloader::new_with_options(options).await?;

    assert_eq!(downloader.version(), DEFAULT_VERSION);
    Ok(())
}
