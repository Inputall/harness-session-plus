use futures_util::{SinkExt, StreamExt};
use hsp_core::{
    cdp::{Client, Error},
    discovery::{select, validate_websocket, Target},
};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tokio_util::sync::CancellationToken;

async fn server<F, Fut>(handler: F) -> (Client, JoinHandle<()>)
where
    F: FnOnce(WebSocketStream<TcpStream>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        handler(accept_async(stream).await.unwrap()).await;
    });
    let client = Client::connect(&format!("ws://127.0.0.1:{port}/devtools/page/test"), port)
        .await
        .unwrap();
    (client, task)
}
async fn recv(ws: &mut WebSocketStream<TcpStream>) -> Value {
    let message = ws.next().await.unwrap().unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}
async fn send(ws: &mut WebSocketStream<TcpStream>, value: Value) {
    ws.send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}
#[tokio::test]
async fn out_of_order_and_events() {
    let (client, task) = server(|mut ws| async move {
        let a = recv(&mut ws).await;
        let b = recv(&mut ws).await;
        assert_ne!(a["id"], b["id"]);
        send(
            &mut ws,
            json!({"method":"Runtime.consoleAPICalled","params":{}}),
        )
        .await;
        send(
            &mut ws,
            json!({"id": b["id"],"result":{"method":b["method"]}}),
        )
        .await;
        send(
            &mut ws,
            json!({"id": a["id"],"result":{"method":a["method"]}}),
        )
        .await;
        let _ = ws.next().await;
    })
    .await;
    let mut events = client.events();
    let cancel = CancellationToken::new();
    let (a, b) = tokio::join!(
        client.request("A", json!({}), Duration::from_secs(1), &cancel),
        client.request("B", json!({}), Duration::from_secs(1), &cancel)
    );
    assert_eq!(a.unwrap()["method"], "A");
    assert_eq!(b.unwrap()["method"], "B");
    assert_eq!(
        events.recv().await.unwrap()["method"],
        "Runtime.consoleAPICalled"
    );
    client.close();
    task.await.unwrap();
}
#[tokio::test]
async fn timeout_cancel_and_late_response() {
    let (client, task) = server(|mut ws| async move {
        let first = recv(&mut ws).await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        send(&mut ws, json!({"id":first["id"],"result":{"late":true}})).await;
        let next = recv(&mut ws).await;
        send(&mut ws, json!({"id":next["id"],"result":{"ok":true}})).await;
        let _ = ws.next().await;
    })
    .await;
    let cancel = CancellationToken::new();
    assert_eq!(
        client
            .request("slow", json!({}), Duration::from_millis(20), &cancel)
            .await
            .unwrap_err(),
        Error::Timeout
    );
    let stopped = CancellationToken::new();
    stopped.cancel();
    assert_eq!(
        client
            .request("cancelled", json!({}), Duration::from_secs(1), &stopped)
            .await
            .unwrap_err(),
        Error::Cancelled
    );
    assert_eq!(
        client
            .request("next", json!({}), Duration::from_secs(1), &cancel)
            .await
            .unwrap()["ok"],
        true
    );
    client.close();
    task.await.unwrap();
}
#[tokio::test]
async fn disconnect_fails_every_pending_request() {
    let (client, task) = server(|mut ws| async move {
        recv(&mut ws).await;
        recv(&mut ws).await;
        ws.close(None).await.unwrap();
    })
    .await;
    let cancel = CancellationToken::new();
    let (a, b) = tokio::join!(
        client.request("A", json!({}), Duration::from_secs(2), &cancel),
        client.request("B", json!({}), Duration::from_secs(2), &cancel)
    );
    assert_eq!(a.unwrap_err(), Error::Disconnected);
    assert_eq!(b.unwrap_err(), Error::Disconnected);
    assert_eq!(
        client
            .request("C", json!({}), Duration::from_secs(1), &cancel)
            .await
            .unwrap_err(),
        Error::Disconnected
    );
    task.await.unwrap();
}
#[tokio::test]
async fn navigation_invalidates_inflight_evaluation_and_next_uses_current_context() {
    let (client, task) = server(|mut ws| async move {
        let first = recv(&mut ws).await;
        assert!(first["params"].get("contextId").is_none());
        send(
            &mut ws,
            json!({"method":"Runtime.executionContextsCleared","params":{}}),
        )
        .await;
        send(
            &mut ws,
            json!({"id":first["id"],"result":{"result":{"value":"stale"}}}),
        )
        .await;
        let second = recv(&mut ws).await;
        assert!(second["params"].get("contextId").is_none());
        send(
            &mut ws,
            json!({"id":second["id"],"result":{"result":{"value":"fresh"}}}),
        )
        .await;
        let _ = ws.next().await;
    })
    .await;
    let cancel = CancellationToken::new();
    assert_eq!(
        client.evaluate("1", &cancel).await.unwrap_err(),
        Error::InvalidResult
    );
    assert_eq!(client.evaluate("2", &cancel).await.unwrap(), "fresh");
    client.close();
    task.await.unwrap();
}
#[tokio::test]
async fn protocol_and_javascript_errors_are_not_success() {
    let (client, task) = server(|mut ws| async move {
        let first = recv(&mut ws).await;
        send(
            &mut ws,
            json!({"id":first["id"],"error":{"code":-32601,"message":"secret ignored"}}),
        )
        .await;
        let second = recv(&mut ws).await;
        send(
            &mut ws,
            json!({"id":second["id"],"result":{"exceptionDetails":{"text":"secret ignored"}}}),
        )
        .await;
        let _ = ws.next().await;
    })
    .await;
    let cancel = CancellationToken::new();
    assert_eq!(
        client
            .request("unknown", json!({}), Duration::from_secs(1), &cancel)
            .await
            .unwrap_err(),
        Error::Protocol(-32601)
    );
    assert_eq!(
        client.evaluate("throw 1", &cancel).await.unwrap_err(),
        Error::InvalidResult
    );
    client.close();
    task.await.unwrap();
}
fn target(id: &str) -> Target {
    Target {
        id: id.into(),
        kind: "page".into(),
        url: "dsh-app://app/".into(),
        websocket: None,
    }
}
#[test]
fn multiple_targets_require_explicit_selection() {
    let targets = vec![target("one"), target("two")];
    assert!(matches!(select(&targets, None), Err(Error::Ambiguous(_))));
    assert_eq!(select(&targets, Some("two")).unwrap().id, "two");
    assert!(matches!(
        select(&targets, Some("wrong")),
        Err(Error::TargetNotFound)
    ));
    let mut wrong = target("foreign");
    wrong.url = "https://example.org".into();
    assert!(matches!(
        select(&[wrong], Some("foreign")),
        Err(Error::TargetNotFound)
    ));
}
#[test]
fn endpoints_never_escape_configured_loopback() {
    assert!(validate_websocket("ws://127.0.0.1:9223/devtools/page/a", 9223).is_ok());
    for address in [
        "ws://localhost:9223/devtools/page/a",
        "ws://127.0.0.1:9224/devtools/page/a",
        "ws://example.org:9223/devtools/page/a",
        "ws://user:secret@127.0.0.1:9223/devtools/page/a",
        "ws://127.0.0.1:9223/devtools/page/a?token=secret",
        "ws://127.0.0.1:9223/other",
    ] {
        assert_eq!(
            validate_websocket(address, 9223).unwrap_err(),
            Error::InvalidEndpoint
        );
    }
}
#[test]
fn structural_probe_does_not_enable_mutations() {
    let report = hsp_core::probe::Report::new(
        hsp_core::discovery::Version {
            browser: "mock".into(),
            protocol_version: None,
        },
        json!({"roleCounts":{"menuitem":2}}),
    );
    assert!(!report.instance_identity_verified && !report.mutations_enabled);
    assert!(
        !report.capabilities.archive
            && !report.capabilities.restore
            && !report.capabilities.permanently_delete
            && !report.capabilities.list
            && !report.capabilities.activity_state
    );
}
