use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};
use hsp_core::{
    cdp::{Client, Error},
    discovery,
    management::evaluate_harness,
};
use serde_json::json;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(
    version,
    about = "Harness Session Plus: local CDP diagnostics; no session mutations"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Args)]
struct Connection {
    #[arg(long, default_value_t = 9223, value_parser = clap::value_parser!(u16).range(1..))]
    port: u16,
    #[arg(long)]
    target_id: Option<String>,
    /// Retry only connection setup, at most 3 times; never replay an operation.
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(0..=3))]
    retries: u8,
}
#[derive(Subcommand)]
enum Command {
    Diagnose(Connection),
    InjectTest(Connection),
    RemoveInjection(Connection),
    /// Read-only structural probe. Does not infer or invoke Harness interfaces.
    ProbeCapabilities(Connection),
}
async fn connect(
    options: &Connection,
    cancel: &CancellationToken,
) -> Result<(Client, discovery::Version, discovery::Target)> {
    for attempt in 0..=options.retries {
        let operation = async {
            let (version, targets) = discovery::discover(options.port).await?;
            let target = discovery::select(&targets, options.target_id.as_deref())?;
            let address = target.websocket.as_deref().ok_or(Error::InvalidEndpoint)?;
            let client = Client::connect(address, options.port).await?;
            if let Err(error) = client.enable(cancel).await {
                client.close();
                return Err(error);
            }
            Ok::<_, Error>((client, version, target))
        };
        let result = tokio::select! { biased; _ = cancel.cancelled() => return Err(Error::Cancelled.into()), result = operation => result };
        match result {
            Ok(value) => return Ok(value),
            Err(error)
                if attempt < options.retries
                    && matches!(
                        error,
                        Error::Discovery | Error::Disconnected | Error::Timeout
                    ) =>
            {
                eprintln!(
                    "Connection unavailable; retry {}/{}",
                    attempt + 1,
                    options.retries
                );
                tokio::select! { _ = cancel.cancelled() => return Err(Error::Cancelled.into()), _ = tokio::time::sleep(Duration::from_millis(250 * (1_u64 << attempt))) => {} }
            }
            Err(error) => return Err(error.into()),
        }
    }
    bail!("connection attempts exhausted")
}
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });
    let options = match &cli.command {
        Command::Diagnose(c)
        | Command::InjectTest(c)
        | Command::RemoveInjection(c)
        | Command::ProbeCapabilities(c) => c,
    };
    let (client, version, target) = connect(options, &cancel).await?;
    // Reconfirm the current URL immediately before touching the page; discovered targets can navigate.
    if client
        .evaluate("location.href === 'dsh-app://app/'", &cancel)
        .await?
        != json!(true)
    {
        client.close();
        bail!("Selected page navigated away from Harness; refusing action");
    }
    let result: Result<serde_json::Value> = match cli.command {
        Command::Diagnose(_) => evaluate_harness(&client, "window.__HARNESS_SESSION_PLUS__?.getStatus?.() ?? {installed:false,dataOperations:false}", &cancel).await.map(|status| json!({"port":options.port,"browser":version.browser,"protocolVersion":version.protocol_version,"pageRecognized":true,"targetId":target.id,"injection":status})).map_err(Into::into),
        Command::InjectTest(_) => evaluate_harness(&client, hsp_core::BOOTSTRAP, &cancel).await.map_err(Into::into),
        Command::RemoveInjection(_) => evaluate_harness(&client, "window.__HARNESS_SESSION_PLUS__?.uninstall?.() ?? {installed:false,dataOperations:false}", &cancel).await.map_err(Into::into),
        Command::ProbeCapabilities(_) => evaluate_harness(&client, hsp_core::PROBE, &cancel).await.map(|evidence| serde_json::to_value(hsp_core::probe::Report::new(version, evidence)).expect("report serializes")).map_err(Into::into),
    };
    client.close();
    println!("{}", serde_json::to_string_pretty(&result?)?);
    Ok(())
}
