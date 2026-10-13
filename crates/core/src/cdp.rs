use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum Error {
    #[error("CDP disconnected")]
    Disconnected,
    #[error("CDP request timed out")]
    Timeout,
    #[error("operation cancelled")]
    Cancelled,
    #[error("CDP returned protocol error {0}")]
    Protocol(i64),
    #[error("invalid CDP result or execution context")]
    InvalidResult,
    #[error("only configured loopback page WebSocket endpoints are allowed")]
    InvalidEndpoint,
    #[error("loopback discovery failed (port unavailable or invalid CDP response)")]
    Discovery,
    #[error("no matching Harness page")]
    TargetNotFound,
    #[error("multiple Harness pages; pass --target-id from {0:?}")]
    Ambiguous(Vec<String>),
}

type Pending = HashMap<u64, oneshot::Sender<Result<Value>>>;
struct State {
    next: AtomicU64,
    pending: Mutex<Pending>,
    closed: AtomicBool,
    epoch: AtomicU64,
    stop: CancellationToken,
    events: broadcast::Sender<Value>,
}
impl State {
    fn finish(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.stop.cancel();
        for (_, tx) in self.pending.lock().unwrap().drain() {
            let _ = tx.send(Err(Error::Disconnected));
        }
    }
    fn dispatch(&self, value: Value) {
        if let Some(id) = value.get("id").and_then(Value::as_u64) {
            if let Some(tx) = self.pending.lock().unwrap().remove(&id) {
                let response = if let Some(error) = value.get("error") {
                    Err(Error::Protocol(error["code"].as_i64().unwrap_or(-1)))
                } else {
                    value.get("result").cloned().ok_or(Error::InvalidResult)
                };
                let _ = tx.send(response);
            }
        } else {
            if matches!(
                value["method"].as_str(),
                Some(
                    "Runtime.executionContextsCleared"
                        | "Runtime.executionContextDestroyed"
                        | "Page.frameNavigated"
                )
            ) {
                self.epoch.fetch_add(1, Ordering::SeqCst);
            }
            let _ = self.events.send(value);
        }
    }
}
pub struct Client {
    state: Arc<State>,
    outgoing: mpsc::Sender<Message>,
}
struct PendingGuard {
    state: Arc<State>,
    id: u64,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.state.pending.lock().unwrap().remove(&self.id);
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.state.finish();
    }
}
impl Client {
    pub async fn connect(address: &str, port: u16) -> Result<Self> {
        crate::discovery::validate_websocket(address, port)?;
        let (socket, _) = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(address),
        )
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(|_| Error::Disconnected)?;
        let (mut sink, mut stream) = socket.split();
        let (outgoing, mut rx) = mpsc::channel::<Message>(64);
        let (events, _) = broadcast::channel(128);
        let state = Arc::new(State {
            next: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            stop: CancellationToken::new(),
            events,
        });
        let writer = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = writer.stop.cancelled() => break,
                    message = rx.recv() => match message { Some(m) => if sink.send(m).await.is_err() { break; }, None => break }
                }
            }
            let _ = tokio::time::timeout(Duration::from_millis(200), sink.close()).await;
            writer.finish();
        });
        let reader = state.clone();
        let pong = outgoing.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = reader.stop.cancelled() => break,
                    message = stream.next() => match message {
                        Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) { Ok(v) => reader.dispatch(v), Err(_) => break },
                        Some(Ok(Message::Ping(data))) => { if pong.try_send(Message::Pong(data)).is_err() { break; } },
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                        _ => {}
                    }
                }
            }
            reader.finish();
        });
        Ok(Self { state, outgoing })
    }
    pub fn events(&self) -> broadcast::Receiver<Value> {
        self.state.events.subscribe()
    }
    pub fn close(&self) {
        self.state.finish();
    }
    pub async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let id = self.state.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.state.pending.lock().unwrap();
            if self.state.closed.load(Ordering::SeqCst) {
                return Err(Error::Disconnected);
            }
            pending.insert(id, tx);
        }
        let _guard = PendingGuard {
            state: self.state.clone(),
            id,
        };
        let message = Message::Text(
            json!({"id":id,"method":method,"params":params})
                .to_string()
                .into(),
        );
        tokio::select! { biased;
            _ = cancel.cancelled() => Err(Error::Cancelled),
            _ = self.state.stop.cancelled() => Err(Error::Disconnected),
            _ = tokio::time::sleep(timeout) => Err(Error::Timeout),
            result = async { self.outgoing.send(message).await.map_err(|_| Error::Disconnected)?; rx.await.map_err(|_| Error::Disconnected)? } => result,
        }
    }
    pub async fn enable(&self, cancel: &CancellationToken) -> Result<()> {
        self.request("Runtime.enable", json!({}), Duration::from_secs(5), cancel)
            .await?;
        self.request("Page.enable", json!({}), Duration::from_secs(5), cancel)
            .await?;
        Ok(())
    }
    // No cached context ID: evaluate in the current main world. Navigation during a request invalidates its result.
    pub async fn evaluate(&self, expression: &str, cancel: &CancellationToken) -> Result<Value> {
        let epoch = self.state.epoch.load(Ordering::SeqCst);
        let result = self
            .request(
                "Runtime.evaluate",
                json!({"expression":expression,"returnByValue":true,"awaitPromise":true}),
                Duration::from_secs(5),
                cancel,
            )
            .await?;
        if self.state.epoch.load(Ordering::SeqCst) != epoch
            || result.get("exceptionDetails").is_some()
        {
            return Err(Error::InvalidResult);
        }
        result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .ok_or(Error::InvalidResult)
    }
}
