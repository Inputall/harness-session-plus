use crate::cdp::{Error, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

#[derive(Debug, Deserialize, Serialize)]
pub struct Version {
    #[serde(rename = "Browser")]
    pub browser: String,
    #[serde(rename = "Protocol-Version")]
    pub protocol_version: Option<String>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Target {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    pub websocket: Option<String>,
}

pub fn select(targets: &[Target], id: Option<&str>) -> Result<Target> {
    let candidates: Vec<_> = targets
        .iter()
        .filter(|t| t.kind == "page" && t.url == "dsh-app://app/")
        .collect();
    if let Some(id) = id {
        return candidates
            .iter()
            .find(|t| t.id == id)
            .map(|t| (*t).clone())
            .ok_or(Error::TargetNotFound);
    }
    match candidates.as_slice() {
        [target] => Ok((*target).clone()),
        [] => Err(Error::TargetNotFound),
        _ => Err(Error::Ambiguous(
            candidates.iter().map(|t| t.id.clone()).collect(),
        )),
    }
}

pub fn validate_websocket(address: &str, port: u16) -> Result<()> {
    let url = Url::parse(address).map_err(|_| Error::InvalidEndpoint)?;
    if url.scheme() != "ws"
        || url.host_str() != Some("127.0.0.1")
        || url.port_or_known_default() != Some(port)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().starts_with("/devtools/page/")
    {
        return Err(Error::InvalidEndpoint);
    }
    Ok(())
}

pub async fn discover(port: u16) -> Result<(Version, Vec<Target>)> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| Error::Discovery)?;
    let base = format!("http://127.0.0.1:{port}");
    let version = client
        .get(format!("{base}/json/version"))
        .send()
        .await
        .map_err(|_| Error::Discovery)?
        .error_for_status()
        .map_err(|_| Error::Discovery)?
        .json()
        .await
        .map_err(|_| Error::Discovery)?;
    let targets = client
        .get(format!("{base}/json/list"))
        .send()
        .await
        .map_err(|_| Error::Discovery)?
        .error_for_status()
        .map_err(|_| Error::Discovery)?
        .json()
        .await
        .map_err(|_| Error::Discovery)?;
    Ok((version, targets))
}
