use std::fs::{create_dir_all, read, read_to_string, remove_file};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Error, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

use crate::{DownloadOptions, VersionSelection};

use super::{Downloader, DOWNLOAD_DIRECTORY_TOR};

const TOR_DATA_DIRECTORY: &str = "data";
const CONTROL_PORT_FILE: &str = "control_port";
const CONTROL_COOKIE_FILE: &str = "control_auth_cookie";
/// Tor writes a 32 bytes random cookie for `CookieAuthentication`.
const COOKIE_LEN: usize = 32;
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Extracts `PROGRESS=<n>` from a `GETINFO status/bootstrap-phase` reply.
fn bootstrap_progress(reply: &[String]) -> Option<u8> {
    reply
        .iter()
        .flat_map(|line| line.split_whitespace())
        .find_map(|token| token.strip_prefix("PROGRESS="))
        .and_then(|value| value.parse().ok())
}

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
        let tor = Self::setup_with_version(VersionSelection::default()).await?;
        tor.post_setup()
            .with_context(|| Error::msg("Failed to perform Post-Setup Procedure."))?;
        Ok(tor)
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
        let data_dir = self.path.join(TOR_DATA_DIRECTORY);
        let port_file = data_dir.join(CONTROL_PORT_FILE);
        let cookie_file = data_dir.join(CONTROL_COOKIE_FILE);

        create_dir_all(&data_dir).context("Failed to create Tor data directory.")?;
        // Discard leftovers from a previous run, so stale values are never read.
        let _ = remove_file(&port_file);
        let _ = remove_file(&cookie_file);

        let mut child = Command::new(tor_bin)
            .arg("--DataDirectory")
            .arg(&data_dir)
            .args(["--ControlPort", "auto", "--CookieAuthentication", "1"])
            .arg("--ControlPortWriteToFile")
            .arg(&port_file)
            .arg("--CookieAuthFile")
            .arg(&cookie_file)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn Tor Process")?;
        let pid = child.id().ok_or(Error::msg("No Process ID for Tor"))?;

        self.pid = Some(pid);

        let bootstrapped = timeout(
            BOOTSTRAP_TIMEOUT,
            Self::wait_bootstrapped(&mut child, &port_file, &cookie_file),
        )
        .await;

        match bootstrapped {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(_) => anyhow::bail!("Timed out waiting for Tor to bootstrap."),
        }

        tokio::spawn(async move {
            child.wait().await.expect("Tor Process errored.");
        });

        Ok(pid)
    }

    /// Polls Tor's ControlPort until `status/bootstrap-phase` reports 100%.
    async fn wait_bootstrapped(
        child: &mut Child,
        port_file: &Path,
        cookie_file: &Path,
    ) -> Result<()> {
        let addr = Self::wait_for_control_addr(child, port_file).await?;
        let cookie = Self::wait_for_cookie(child, cookie_file).await?;
        let stream = TcpStream::connect(addr)
            .await
            .context("Failed to connect to Tor ControlPort.")?;
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read);

        let auth = format!("AUTHENTICATE {}\r\n", hex(&cookie));
        Self::control_command(&mut reader, &mut write, &auth)
            .await
            .context("Tor ControlPort authentication failed.")?;

        loop {
            if let Some(status) = child.try_wait()? {
                anyhow::bail!("Tor exited before bootstrapping completed ({status}).");
            }

            let reply = Self::control_command(
                &mut reader,
                &mut write,
                "GETINFO status/bootstrap-phase\r\n",
            )
            .await?;

            if bootstrap_progress(&reply) == Some(100) {
                return Ok(());
            }

            sleep(POLL_INTERVAL).await;
        }
    }

    async fn wait_for_control_addr(child: &mut Child, port_file: &Path) -> Result<String> {
        loop {
            if let Some(status) = child.try_wait()? {
                anyhow::bail!("Tor exited before opening its ControlPort ({status}).");
            }

            // File content is `PORT=127.0.0.1:<port>`; it is written once Tor is listening.
            if let Ok(content) = read_to_string(port_file) {
                if let Some(addr) = content.trim().strip_prefix("PORT=") {
                    return Ok(addr.to_string());
                }
            }

            sleep(POLL_INTERVAL).await;
        }
    }

    async fn wait_for_cookie(child: &mut Child, cookie_file: &Path) -> Result<Vec<u8>> {
        loop {
            if let Some(status) = child.try_wait()? {
                anyhow::bail!("Tor exited before writing its auth cookie ({status}).");
            }

            if let Ok(cookie) = read(cookie_file) {
                if cookie.len() == COOKIE_LEN {
                    return Ok(cookie);
                }
            }

            sleep(POLL_INTERVAL).await;
        }
    }

    /// Sends one command and returns the reply lines, failing on a non-250 status.
    async fn control_command(
        reader: &mut BufReader<OwnedReadHalf>,
        write: &mut OwnedWriteHalf,
        command: &str,
    ) -> Result<Vec<String>> {
        write.write_all(command.as_bytes()).await?;

        let mut lines = Vec::new();

        loop {
            let mut line = String::new();

            if reader.read_line(&mut line).await? == 0 {
                anyhow::bail!("Tor closed the ControlPort connection.");
            }

            let line = line.trim_end().to_string();

            if !line.starts_with("250") {
                anyhow::bail!("Unexpected ControlPort reply: {line}");
            }

            // Final line of a reply is `250 <text>`; intermediate ones are `250-<text>`.
            let is_last = line.as_bytes().get(3) == Some(&b' ');

            lines.push(line);

            if is_last {
                return Ok(lines);
            }
        }
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

    #[cfg(not(target_os = "linux"))]
    fn post_setup(&self) -> Result<()> {
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn post_setup(&self) -> Result<()> {
        use std::env::{set_var, var};

        const LD_LIBRARY_PATH: &str = "LD_LIBRARY_PATH";

        let bin_dir_path = self.tor_bin_dir_path().display().to_string();
        let ld_library_path = var(LD_LIBRARY_PATH).unwrap_or_default();

        // check if the bin dir path is already in the LD_LIBRARY_PATH,
        if !ld_library_path.contains(&bin_dir_path) {
            set_var(
                LD_LIBRARY_PATH,
                format!("{}:{}", ld_library_path, self.tor_bin_dir_path().display()),
            );
        }

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

    #[test]
    fn parses_bootstrap_progress() {
        let reply = vec![
            "250-status/bootstrap-phase=NOTICE BOOTSTRAP PROGRESS=100 TAG=done SUMMARY=\"Done\""
                .to_string(),
            "250 OK".to_string(),
        ];

        assert_eq!(super::bootstrap_progress(&reply), Some(100));
        assert_eq!(super::bootstrap_progress(&["250 OK".to_string()]), None);
    }

    #[tokio::test]
    async fn setup_tor_instance() {
        let tor = Tor::setup().await.expect("Failed to setup a Tor instance.");
        let version = tor.version();

        assert_eq!(version, DEFAULT_VERSION);
    }

    #[tokio::test]
    async fn pid_returned_by_run_matches() {
        let _guard = crate::TOR_LOCK.lock().await;
        let mut tor = Tor::setup().await.expect("Failed to setup a Tor instance.");
        let tor_pid = tor.run().await.expect("Failed to run Tor Proxy");
        let instance_pid = tor.pid().unwrap();

        assert_eq!(tor_pid, instance_pid);
    }
}
