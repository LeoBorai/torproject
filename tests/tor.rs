use anyhow::Result;
use reqwest::{Client, Proxy};
use serial_test::serial;
use torproject::{Tor, DEFAULT_VERSION};

const TOR_CHECK_API: &str = "https://check.torproject.org/api/ip";
const TOR_SOCK5_LOCAL: &str = "socks5://127.0.0.1:9050";

#[tokio::test]
#[serial]
async fn setup_tor_instance() {
    let tor = Tor::setup().await.expect("Failed to setup a Tor instance.");
    let version = tor.version();

    assert_eq!(version, DEFAULT_VERSION);
}

#[tokio::test]
#[serial]
async fn pid_returned_by_run_matches() {
    let mut tor = Tor::setup().await.expect("Failed to setup a Tor instance.");
    let tor_pid = tor.run().await.expect("Failed to run Tor Proxy");
    let instance_pid = tor.pid().unwrap();

    assert_eq!(tor_pid, instance_pid);
}

#[tokio::test]
#[serial]
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
