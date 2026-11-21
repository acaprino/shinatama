//! A Chrome DevTools Protocol client over any byte stream: the Moli host's
//! named pipe in production, an in-memory duplex in tests. The stream is
//! authenticated by writing the host's token before anything else.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{broadcast, mpsc, oneshot},
};
use tokio_tungstenite::tungstenite::{Message, protocol::WebSocketConfig};

pub const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Sent once on the event stream when the connection ends, so listeners stop.
pub const CLOSED_EVENT: &str = "Shinatama.connectionClosed";
/// Screenshots of a large viewport are a few MiB of base64.
const MAX_MESSAGE: usize = 64 << 20;
const EVENT_BACKLOG: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CdpError {
    Closed,
    Protocol { code: i64, message: String },
    Timeout,
    Handshake(String),
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => f.write_str("the browser connection closed"),
            Self::Protocol { code, message } => write!(f, "browser error {code}: {message}"),
            Self::Timeout => f.write_str("the browser did not answer in time"),
            Self::Handshake(why) => write!(f, "browser handshake failed: {why}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CdpEvent {
    pub session: Option<String>,
    pub method: String,
    pub params: Value,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, CdpError>>>>>;

pub struct Cdp {
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicU64,
    events: broadcast::Sender<CdpEvent>,
    closed: Arc<AtomicBool>,
}

/// Reads the browser WebSocket path from `/json/version` on a fresh,
/// token-authenticated stream.
pub async fn discover<S>(mut stream: S, token: &[u8; 32]) -> Result<String, CdpError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let handshake = |e: std::io::Error| CdpError::Handshake(e.to_string());
    stream.write_all(token).await.map_err(handshake)?;
    stream
        .write_all(
            b"GET /json/version HTTP/1.1\r\nHost: 127.0.0.1:9222\r\nConnection: close\r\n\r\n",
        )
        .await
        .map_err(handshake)?;
    let mut response = Vec::new();
    tokio::time::timeout(CALL_TIMEOUT, stream.read_to_end(&mut response))
        .await
        .map_err(|_| CdpError::Timeout)?
        .map_err(handshake)?;
    let text = String::from_utf8_lossy(&response);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_default();
    let url = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("webSocketDebuggerUrl")?.as_str().map(str::to_owned))
        .ok_or_else(|| CdpError::Handshake("no webSocketDebuggerUrl".into()))?;
    ws_path(&url).ok_or_else(|| CdpError::Handshake(format!("unexpected debugger url {url}")))
}

fn ws_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("ws://")?;
    let slash = rest.find('/')?;
    Some(rest[slash..].to_owned())
}

impl Cdp {
    pub async fn connect<S>(mut stream: S, token: &[u8; 32], path: &str) -> Result<Self, CdpError>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        stream
            .write_all(token)
            .await
            .map_err(|e| CdpError::Handshake(e.to_string()))?;
        let mut config = WebSocketConfig::default();
        config.max_message_size = Some(MAX_MESSAGE);
        config.max_frame_size = Some(MAX_MESSAGE);
        let (socket, _) = tokio_tungstenite::client_async_with_config(
            format!("ws://127.0.0.1:9222{path}"),
            stream,
            Some(config),
        )
        .await
        .map_err(|e| CdpError::Handshake(e.to_string()))?;
        let (mut sink, mut source) = socket.split();
        let (out, mut outgoing) = mpsc::unbounded_channel::<String>();
        let pending: Pending = Arc::default();
        let (events, _) = broadcast::channel(EVENT_BACKLOG);

        tokio::spawn(async move {
            while let Some(text) = outgoing.recv().await {
                if sink.send(Message::text(text)).await.is_err() {
                    break;
                }
            }
            let _ = sink.close().await;
        });

        let reader_pending = Arc::clone(&pending);
        let reader_events = events.clone();
        let closed = Arc::new(AtomicBool::new(false));
        let reader_closed = Arc::clone(&closed);
        tokio::spawn(async move {
            while let Some(Ok(message)) = source.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                dispatch(value, &reader_pending, &reader_events);
            }
            // Closed before draining: a call that sees `closed` false and inserts its
            // waiter after the drain is answered by the check in `call`.
            reader_closed.store(true, Ordering::Release);
            for (_, waiter) in reader_pending.lock().unwrap().drain() {
                let _ = waiter.send(Err(CdpError::Closed));
            }
            let _ = reader_events.send(CdpEvent {
                session: None,
                method: CLOSED_EVENT.into(),
                params: Value::Null,
            });
        });

        Ok(Self {
            out,
            pending,
            next_id: AtomicU64::new(1),
            events,
            closed,
        })
    }

    pub async fn call(
        &self,
        session: Option<&str>,
        method: &str,
        params: Value,
    ) -> Result<Value, CdpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if self.closed.load(Ordering::Acquire) {
                return Err(CdpError::Closed);
            }
            pending.insert(id, reply);
        }
        let mut message = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            message["sessionId"] = Value::String(session.to_owned());
        }
        if self.out.send(message.to_string()).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(CdpError::Closed);
        }
        match tokio::time::timeout(CALL_TIMEOUT, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(CdpError::Closed),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(CdpError::Timeout)
            }
        }
    }

    pub fn events(&self) -> broadcast::Receiver<CdpEvent> {
        self.events.subscribe()
    }

    /// True once the browser side ended the connection or the writer failed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire) || self.out.is_closed()
    }
}

fn dispatch(value: Value, pending: &Pending, events: &broadcast::Sender<CdpEvent>) {
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        let Some(waiter) = pending.lock().unwrap().remove(&id) else {
            return;
        };
        let result = match value.get("error") {
            Some(error) => Err(CdpError::Protocol {
                code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            }),
            None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
        };
        let _ = waiter.send(result);
    } else if let Some(method) = value.get("method").and_then(Value::as_str) {
        let _ = events.send(CdpEvent {
            session: value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_owned),
            method: method.to_owned(),
            params: value.get("params").cloned().unwrap_or(Value::Null),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::DuplexStream;
    use tokio_tungstenite::WebSocketStream;

    const TOKEN: [u8; 32] = [7; 32];

    async fn pair() -> (Cdp, WebSocketStream<DuplexStream>) {
        let (client, mut server) = tokio::io::duplex(1 << 20);
        let accept = tokio::spawn(async move {
            let mut token = [0u8; 32];
            server.read_exact(&mut token).await.unwrap();
            assert_eq!(token, TOKEN);
            tokio_tungstenite::accept_async(server).await.unwrap()
        });
        let cdp = Cdp::connect(client, &TOKEN, "/devtools/browser/x")
            .await
            .unwrap();
        (cdp, accept.await.unwrap())
    }

    async fn next_request(server: &mut WebSocketStream<DuplexStream>) -> Value {
        loop {
            if let Message::Text(text) = server.next().await.unwrap().unwrap() {
                return serde_json::from_str(&text).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn call_matches_replies_by_id_out_of_order() {
        let (cdp, mut server) = pair().await;
        let cdp = Arc::new(cdp);
        let (a, b) = (Arc::clone(&cdp), Arc::clone(&cdp));
        let first = tokio::spawn(async move { a.call(None, "A.one", json!({})).await });
        let r1 = next_request(&mut server).await;
        let second = tokio::spawn(async move { b.call(None, "B.two", json!({})).await });
        let r2 = next_request(&mut server).await;
        server
            .send(Message::text(
                json!({"id": r2["id"], "result": {"n": 2}}).to_string(),
            ))
            .await
            .unwrap();
        server
            .send(Message::text(
                json!({"id": r1["id"], "result": {"n": 1}}).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(first.await.unwrap().unwrap(), json!({"n": 1}));
        assert_eq!(second.await.unwrap().unwrap(), json!({"n": 2}));
    }

    #[tokio::test]
    async fn protocol_error_is_typed() {
        let (cdp, mut server) = pair().await;
        let call = cdp.call(Some("s1"), "X.y", json!({}));
        let serve = async {
            let request = next_request(&mut server).await;
            assert_eq!(request["sessionId"], "s1");
            server
                .send(Message::text(
                    json!({"id": request["id"], "error": {"code": -32601, "message": "x"}})
                        .to_string(),
                ))
                .await
                .unwrap();
        };
        let (result, ()) = tokio::join!(call, serve);
        assert_eq!(
            result,
            Err(CdpError::Protocol {
                code: -32601,
                message: "x".into()
            })
        );
    }

    #[tokio::test]
    async fn events_carry_session() {
        let (cdp, mut server) = pair().await;
        let mut events = cdp.events();
        server
            .send(Message::text(
                json!({"method": "Page.loadEventFired", "sessionId": "s9", "params": {"t": 1}})
                    .to_string(),
            ))
            .await
            .unwrap();
        let event = events.recv().await.unwrap();
        assert_eq!(
            event,
            CdpEvent {
                session: Some("s9".into()),
                method: "Page.loadEventFired".into(),
                params: json!({"t": 1})
            }
        );
    }

    #[tokio::test]
    async fn a_closed_peer_is_seen_at_once_and_later_calls_fail_fast() {
        let (cdp, server) = pair().await;
        let mut events = cdp.events();
        drop(server);
        let closed = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(closed.method, CLOSED_EVENT);
        assert!(cdp.is_closed());
        let started = std::time::Instant::now();
        assert_eq!(
            cdp.call(None, "A.b", json!({})).await,
            Err(CdpError::Closed)
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn closing_fails_pending_calls() {
        let (cdp, server) = pair().await;
        let call = cdp.call(None, "A.b", json!({}));
        drop(server);
        assert_eq!(call.await, Err(CdpError::Closed));
    }

    #[tokio::test(start_paused = true)]
    async fn call_times_out_after_10s() {
        let (cdp, _server) = pair().await;
        assert_eq!(
            cdp.call(None, "A.b", json!({})).await,
            Err(CdpError::Timeout)
        );
    }

    #[tokio::test]
    async fn discover_reads_the_debugger_path() {
        let (client, mut server) = tokio::io::duplex(1 << 16);
        tokio::spawn(async move {
            let mut token = [0u8; 32];
            server.read_exact(&mut token).await.unwrap();
            let mut request = [0u8; 256];
            let _ = server.read(&mut request).await.unwrap();
            let body = r#"{"Browser":"Moli","webSocketDebuggerUrl":"ws://127.0.0.1:9222/devtools/browser/abc"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            server.write_all(response.as_bytes()).await.unwrap();
        });
        assert_eq!(
            discover(client, &TOKEN).await.unwrap(),
            "/devtools/browser/abc"
        );
    }
}
