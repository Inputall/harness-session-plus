//! Fixed management actions shared by CLI and desktop. No Harness data operations.
use crate::{
    cdp::{Client, Error, Result},
    discovery, probe, BOOTSTRAP, PROBE,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Options {
    pub port: u16,
    pub target_id: Option<String>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Diagnose,
    InjectTest,
    RemoveInjection,
    ProbeCapabilities,
}

pub async fn evaluate_harness(
    client: &Client,
    expression: &str,
    cancel: &CancellationToken,
) -> Result<Value> {
    let guarded = format!("(() => {{ if (location.href !== 'dsh-app://app/') throw new Error('Harness page mismatch'); return ({expression}); }})()");
    client.evaluate(&guarded, cancel).await
}

pub async fn execute(
    options: &Options,
    action: Action,
    cancel: &CancellationToken,
) -> Result<Value> {
    let (version, targets) = tokio::select! {
        _ = cancel.cancelled() => return Err(Error::Cancelled),
        result = discovery::discover(options.port) => result?,
    };
    let target = discovery::select(&targets, options.target_id.as_deref())?;
    let address = target.websocket.as_deref().ok_or(Error::InvalidEndpoint)?;
    let client = tokio::select! {
        _ = cancel.cancelled() => return Err(Error::Cancelled),
        result = Client::connect(address, options.port) => result?,
    };
    client.enable(cancel).await?;
    let result = match action {
        Action::Diagnose => evaluate_harness(&client, "window.__HARNESS_SESSION_PLUS__?.getStatus?.() ?? {installed:false,dataOperations:false}", cancel).await.map(|status| json!({"port":options.port,"browser":version.browser,"protocolVersion":version.protocol_version,"pageRecognized":true,"targetId":target.id,"injection":status})),
        Action::InjectTest => evaluate_harness(&client, BOOTSTRAP, cancel).await,
        Action::RemoveInjection => evaluate_harness(&client, "window.__HARNESS_SESSION_PLUS__?.uninstall?.() ?? {installed:false,dataOperations:false}", cancel).await,
        Action::ProbeCapabilities => {
            let evidence = evaluate_harness(&client, PROBE, cancel).await?;
            serde_json::to_value(probe::Report::new(version, evidence)).map_err(|_| Error::InvalidResult)
        }
    };
    client.close();
    result
}
