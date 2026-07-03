//! The companion bridge: a localhost-only WebSocket server the browser extension connects to so it
//! can offload separation to the desktop's **resident** engines.
//!
//! Authentication is token-primary: the pairing token (checked during the WebSocket handshake) is
//! the credential. Pairing is automatic: a plain-HTTP `GET /pairing` on the same port hands out
//! the token, but only when the request's `Origin` header is absent (local tools) or is a browser
//! extension (`chrome-extension://`, `moz-extension://`). Web pages always send an `Origin` on a
//! cross-origin fetch, so a page can never read the token. The extension's realtime client runs
//! in a content script, so its WebSocket handshake carries the page's origin (e.g.
//! `https://www.youtube.com`) rather than `chrome-extension://`; any extension or `https` page
//! origin is accepted when the token matches, and a missing or wrong token gets a 403. The token
//! **persists across app restarts** (stored in the app config dir), so the extension pairs once,
//! not on every launch; the server stays loopback-only.
//!
//! Two capabilities share the socket: whole-file cleaning (`hello`/`clean`, JSON text frames) and
//! realtime streaming (the `rt_*` protocol from docs/research/companion-realtime-protocol.md,
//! JSON control plus 16-byte-header binary audio frames; see [`realtime`]). All replies flow
//! through one writer task, so long jobs and server-initiated frames never block the read loop.

mod realtime;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use sukoon_core::engine::EngineKind;
use sukoon_core::{Pipeline, SeparationMode};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;

use crate::EngineService;

/// Fixed loopback port so the extension can discover the companion without configuration.
pub const PORT: u16 = 8765;

/// Companion status, surfaced to the UI and used by the extension for pairing.
#[derive(Clone, Serialize)]
pub struct Companion {
    pub port: u16,
    pub token: String,
}

/// Start the companion server in the background. Returns its status immediately; the accept loop
/// runs on the async runtime. A bind failure (port taken) disables the companion without crashing.
pub fn start(app: AppHandle) -> Companion {
    let token = load_or_create_token(&app);
    let status = Companion {
        port: PORT,
        token: token.clone(),
    };
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(app, token).await {
            log::warn!("companion server stopped: {e}");
        }
    });
    status
}

/// The persistent pairing token: read from the app config dir, minted on first run. Falls back to
/// a session-only token if the config dir is unusable (pairing then lasts until quit).
fn load_or_create_token(app: &AppHandle) -> String {
    let fresh = || uuid::Uuid::new_v4().to_string();
    let Ok(dir) = app.path().app_config_dir() else {
        return fresh();
    };
    let path = dir.join("companion-token");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim();
        if uuid::Uuid::parse_str(existing).is_ok() {
            return existing.to_string();
        }
    }
    let token = fresh();
    if std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&path, &token))
        .is_err()
    {
        log::warn!("could not persist companion token; pairing lasts this session only");
    }
    token
}

async fn serve(app: AppHandle, token: String) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", PORT)).await?;
    log::info!("companion listening on 127.0.0.1:{PORT}");
    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();
        let token = token.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = handle(app, token, stream).await {
                log::debug!("companion connection ended: {e}");
            }
        });
    }
}

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// A sane `/pairing` request head fits well under this; anything larger is routed to WebSocket.
const PAIRING_HEAD_CAP: usize = 2048;
/// How long a client gets to deliver the full `/pairing` head before we stop trying to route it.
const PAIRING_PEEK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Peek (without consuming) the incoming request head, iff it is a `GET /pairing` HTTP request.
///
/// Returns the full head (through the terminating blank line) when it is one; `None` means "not
/// `/pairing`" - a diverging request line, an oversized head, a timeout, EOF, or a socket error -
/// and the caller then hands the untouched stream to the WebSocket handshake, which fails
/// naturally on anything that was not a WebSocket upgrade. Never panics on wire input.
async fn peek_pairing_head(stream: &TcpStream) -> Option<Vec<u8>> {
    const PREFIX: &[u8] = b"GET /pairing";
    let deadline = tokio::time::Instant::now() + PAIRING_PEEK_TIMEOUT;
    let mut buf = vec![0u8; PAIRING_HEAD_CAP];
    let mut seen = 0;
    loop {
        let n = tokio::time::timeout_at(deadline, stream.peek(&mut buf))
            .await
            .ok()?
            .ok()?;
        let head = &buf[..n];
        // Bail to WebSocket as soon as the bytes we have diverge from the `/pairing` request line.
        let overlap = n.min(PREFIX.len());
        if head[..overlap] != PREFIX[..overlap] {
            return None;
        }
        if let Some(end) = head.windows(4).position(|w| w == b"\r\n\r\n") {
            return Some(head[..end + 4].to_vec());
        }
        if n == PAIRING_HEAD_CAP || n == 0 {
            return None; // head too big to be ours, or the peer hung up mid-head
        }
        if n == seen {
            // peek is readiness-based and re-returns the same bytes until more arrive; don't spin.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        seen = n;
    }
}

/// The `/pairing` endpoint as a pure function: given a full request head, the exact HTTP/1.1
/// response to send, or `None` when the request line is not `GET /pairing` (route the stream to
/// the WebSocket handshake instead).
///
/// Access rule: browsers always attach an `Origin` to a cross-origin fetch, so the token is
/// handed out only when `Origin` is absent (local tools) or is a browser-extension origin;
/// anything else gets an empty 403 and learns nothing.
fn pairing_response(head: &str, token: &str) -> Option<String> {
    if !head.starts_with("GET /pairing") {
        return None;
    }
    let origin = head.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("origin")
            .then(|| value.trim())
    });
    let allowed = origin.map_or(true, |o| {
        o.starts_with("chrome-extension://") || o.starts_with("moz-extension://")
    });
    if !allowed {
        return Some(
            "HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n".to_string(),
        );
    }
    let body = json!({
        "port": PORT,
        "token": token,
        "version": env!("CARGO_PKG_VERSION"),
    })
    .to_string();
    Some(format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: {}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        origin.unwrap_or("*"),
        body.len(),
    ))
}

async fn handle(app: AppHandle, token: String, mut stream: TcpStream) -> Result<(), BoxErr> {
    // Tokenless auto-pairing shares the port: a plain `GET /pairing` (detected by peeking, so a
    // real WebSocket upgrade reaches tungstenite untouched) is answered directly and closed.
    if let Some(head) = peek_pairing_head(&stream).await {
        if let Some(response) = pairing_response(&String::from_utf8_lossy(&head), &token) {
            stream.read_exact(&mut vec![0u8; head.len()]).await?; // consume what we peeked
            stream.write_all(response.as_bytes()).await?;
            stream.shutdown().await?;
            return Ok(());
        }
    }

    // Authenticate during the handshake: pairing token (the credential) + a plausible browser
    // origin (an extension page, or an https page carrying the extension's content script).
    // The Result shape (and its large Err) is dictated by tungstenite's `Callback` trait.
    #[allow(clippy::result_large_err)]
    let auth = move |req: &Request, res: Response| -> Result<Response, ErrorResponse> {
        let origin_ok = req
            .headers()
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .map(|o| {
                o.starts_with("chrome-extension://")
                    || o.starts_with("moz-extension://")
                    || o.starts_with("https://")
            })
            .unwrap_or(false);
        let token_ok = req
            .uri()
            .query()
            .map(|q| q.split('&').any(|kv| kv == format!("token={token}")))
            .unwrap_or(false);
        if origin_ok && token_ok {
            Ok(res)
        } else {
            let mut err = ErrorResponse::new(None);
            *err.status_mut() = StatusCode::FORBIDDEN;
            Err(err)
        }
    };

    let ws = tokio_tungstenite::accept_hdr_async(stream, auth).await?;
    let (mut sink, mut rx) = ws.split();

    // The single writer: the read loop, spawned `clean` jobs, and the realtime worker all reply
    // through this channel, so the server can push frames that are not 1:1 responses to reads.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tauri::async_runtime::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut session: Option<realtime::Session> = None;
    while let Some(msg) = rx.next().await {
        match msg? {
            Message::Text(text) => {
                if let Ok(req) = serde_json::from_str::<Value>(text.as_str()) {
                    dispatch(&app, &out_tx, &mut session, req);
                }
            }
            Message::Binary(payload) => match realtime::InputFrame::parse(&payload) {
                Some(frame) => {
                    // `push` refuses a frame only when the input cap is blown: the engine is too
                    // far behind realtime, so tell the client to fall back and end the session.
                    if session.as_ref().is_some_and(|s| !s.push(frame)) {
                        send(&out_tx, json!({ "type": "rt_overloaded" }));
                        session = None;
                    }
                }
                None => log::debug!(
                    "ignoring malformed companion audio frame ({} bytes)",
                    payload.len()
                ),
            },
            Message::Ping(payload) => {
                let _ = out_tx.send(Message::Pong(payload));
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    drop(session); // tears down any live realtime worker
    drop(out_tx);
    let _ = writer.await;
    Ok(())
}

fn send(out: &mpsc::UnboundedSender<Message>, reply: Value) {
    let _ = out.send(Message::Text(reply.to_string()));
}

fn dispatch(
    app: &AppHandle,
    out: &mpsc::UnboundedSender<Message>,
    session: &mut Option<realtime::Session>,
    req: Value,
) {
    match req.get("type").and_then(Value::as_str) {
        Some("hello") => send(
            out,
            json!({
                "type": "ready",
                "version": env!("CARGO_PKG_VERSION"),
                "engines": ["fast", "hq", "fallback"],
                "realtime": true,
                "rtSampleRate": realtime::SAMPLE_RATE,
                "rtChannels": realtime::CHANNELS,
            }),
        ),
        Some("clean") => {
            // Long-running: run as its own task so a file job never stalls the read loop (or an
            // active realtime session sharing the socket).
            let app = app.clone();
            let out = out.clone();
            tauri::async_runtime::spawn(async move {
                let reply = clean(&app, &req).await;
                let _ = out.send(Message::Text(reply.to_string()));
            });
        }
        Some("rt_start") => {
            if session.as_ref().is_some_and(realtime::Session::is_alive) {
                send(
                    out,
                    json!({ "type": "error", "error": "a realtime session is already active" }),
                );
            } else {
                let kind = req
                    .get("engine")
                    .and_then(Value::as_str)
                    .and_then(EngineKind::from_id)
                    .unwrap_or(EngineKind::Hq);
                *session = Some(realtime::Session::start(app.clone(), kind, out.clone()));
            }
        }
        Some("rt_flush") => {
            let epoch = req
                .get("epoch")
                .and_then(Value::as_u64)
                .and_then(|e| u16::try_from(e).ok());
            match (epoch, session.as_ref().filter(|s| s.is_alive())) {
                (Some(epoch), Some(s)) => s.flush(epoch),
                (None, _) => send(
                    out,
                    json!({ "type": "error", "error": "rt_flush requires a u16 epoch" }),
                ),
                (_, None) => send(
                    out,
                    json!({ "type": "error", "error": "no active realtime session" }),
                ),
            }
        }
        Some("rt_stop") => {
            // Idempotent: tearing down a session that already ended still acks. A dropped session
            // never emits audio again, so the client sees nothing after this ack.
            *session = None;
            send(out, json!({ "type": "rt_stopped" }));
        }
        other => send(
            out,
            json!({ "type": "error", "error": format!("unknown message type: {other:?}") }),
        ),
    }
}

async fn clean(app: &AppHandle, req: &Value) -> Value {
    let id = req
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let input = match req.get("input").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => return json!({ "type": "error", "id": id, "error": "missing input" }),
    };
    let kind = req
        .get("engine")
        .and_then(Value::as_str)
        .and_then(EngineKind::from_id)
        .unwrap_or(EngineKind::Fast);
    let output = req
        .get("output")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{input}.clean"));

    let engine = match app.state::<EngineService>().engine(kind) {
        Ok(e) => e,
        Err(e) => return json!({ "type": "error", "id": id, "error": e }),
    };
    let out = output.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        Pipeline::from_engine(engine, SeparationMode::RemoveAll, true)
            .clean_file(&input, &out)
            .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(Ok(())) => json!({ "type": "done", "id": id, "output": output }),
        Ok(Err(e)) => json!({ "type": "error", "id": id, "error": e }),
        Err(e) => json!({ "type": "error", "id": id, "error": e.to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use super::pairing_response;

    const TOKEN: &str = "8e6f7a2c-1b3d-4e5f-9a0b-c1d2e3f4a5b6";

    fn head(origin: Option<&str>) -> String {
        let mut head = String::from("GET /pairing HTTP/1.1\r\nHost: 127.0.0.1:8765\r\n");
        if let Some(o) = origin {
            head.push_str(&format!("Origin: {o}\r\n"));
        }
        head.push_str("\r\n");
        head
    }

    #[test]
    fn websocket_upgrade_is_not_pairing() {
        let head = "GET / HTTP/1.1\r\nUpgrade: websocket\r\n\r\n";
        assert_eq!(pairing_response(head, TOKEN), None);
    }

    #[test]
    fn other_paths_are_not_pairing() {
        assert_eq!(pairing_response("GET /token HTTP/1.1\r\n\r\n", TOKEN), None);
        assert_eq!(pairing_response("POST ", TOKEN), None);
        assert_eq!(pairing_response("", TOKEN), None);
    }

    #[test]
    fn absent_origin_gets_token_with_wildcard_cors() {
        let res = pairing_response(&head(None), TOKEN).unwrap();
        assert!(res.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(res.contains("Access-Control-Allow-Origin: *\r\n"));
        assert!(res.contains(&format!(r#""token":"{TOKEN}""#)));
        assert!(res.contains(r#""port":8765"#));
    }

    #[test]
    fn extension_origins_get_token_with_origin_echoed() {
        for origin in [
            "chrome-extension://abcdefghijklmnop",
            "moz-extension://uuid-here",
        ] {
            let res = pairing_response(&head(Some(origin)), TOKEN).unwrap();
            assert!(res.starts_with("HTTP/1.1 200 OK\r\n"));
            assert!(res.contains(&format!("Access-Control-Allow-Origin: {origin}\r\n")));
            assert!(res.contains(&format!(r#""token":"{TOKEN}""#)));
        }
    }

    #[test]
    fn web_page_origins_are_denied_without_the_token() {
        for origin in ["https://evil.example", "http://127.0.0.1:8765", "null"] {
            let res = pairing_response(&head(Some(origin)), TOKEN).unwrap();
            assert!(res.starts_with("HTTP/1.1 403 Forbidden\r\n"), "{origin}");
            assert!(res.ends_with("\r\n\r\n"), "403 must carry no body");
            assert!(!res.contains(TOKEN));
        }
    }

    #[test]
    fn origin_header_name_is_case_insensitive() {
        let head = "GET /pairing HTTP/1.1\r\norigin: https://evil.example\r\n\r\n";
        let res = pairing_response(head, TOKEN).unwrap();
        assert!(res.starts_with("HTTP/1.1 403 Forbidden\r\n"));
    }

    #[test]
    fn malformed_headers_do_not_panic() {
        let head = "GET /pairing HTTP/1.1\r\n:::\r\nno-colon-line\r\n\r\n";
        let res = pairing_response(head, TOKEN).unwrap();
        assert!(res.starts_with("HTTP/1.1 200 OK\r\n"));
    }

    async fn socket_pair() -> (tokio::net::TcpStream, tokio::net::TcpStream) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        (client, server)
    }

    #[tokio::test]
    async fn peek_sees_a_chunked_pairing_head_without_consuming_it() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut client, mut server) = socket_pair().await;
        let head = head(Some("chrome-extension://abc"));
        let (first, rest) = head.as_bytes().split_at(9); // split mid request line
        let rest = rest.to_vec();
        client.write_all(first).await.unwrap();
        let finish = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            client.write_all(&rest).await.unwrap();
            client
        });

        let peeked = super::peek_pairing_head(&server).await.unwrap();
        assert_eq!(peeked, head.as_bytes());
        // Peeking must not consume: the whole head is still readable off the stream.
        let mut readable = vec![0u8; head.len()];
        server.read_exact(&mut readable).await.unwrap();
        assert_eq!(readable, head.as_bytes());
        drop(finish.await.unwrap());
    }

    #[tokio::test]
    async fn peek_bails_to_websocket_on_a_diverging_request_line() {
        use tokio::io::AsyncWriteExt;
        let (mut client, server) = socket_pair().await;
        client.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
        assert_eq!(super::peek_pairing_head(&server).await, None);
    }
}
