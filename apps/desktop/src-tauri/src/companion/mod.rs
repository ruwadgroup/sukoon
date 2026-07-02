//! The companion bridge: a localhost-only WebSocket server the browser extension connects to so it
//! can offload separation to the desktop's **resident** engines.
//!
//! Authenticated by extension origin **and** a per-session pairing token, both checked during the
//! WebSocket handshake — any other local page (which can also reach `ws://127.0.0.1`) gets a 403.
//! Real-time audio streaming is a planned follow-up (it needs a streaming engine mode in core); this
//! v1 establishes the secure bridge, advertises capabilities (`hello` → `ready`), and can clean a
//! file end-to-end (`clean`) against the warm engines.

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use sukoon_core::engine::EngineKind;
use sukoon_core::{Pipeline, SeparationMode};
use tauri::{AppHandle, Manager};
use tokio::net::{TcpListener, TcpStream};
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
    let token = uuid::Uuid::new_v4().to_string();
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

async fn handle(app: AppHandle, token: String, stream: TcpStream) -> Result<(), BoxErr> {
    // Authenticate during the handshake: extension origin + matching pairing token.
    let auth = move |req: &Request, res: Response| -> Result<Response, ErrorResponse> {
        let origin_ok = req
            .headers()
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .map(|o| o.starts_with("chrome-extension://") || o.starts_with("moz-extension://"))
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
    let (mut tx, mut rx) = ws.split();

    while let Some(msg) = rx.next().await {
        match msg? {
            Message::Text(text) => {
                if let Ok(req) = serde_json::from_str::<Value>(text.as_str()) {
                    let reply = handle_message(&app, &req).await;
                    tx.send(Message::Text(reply.to_string().into())).await?;
                }
            }
            Message::Ping(payload) => tx.send(Message::Pong(payload)).await?,
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

async fn handle_message(app: &AppHandle, req: &Value) -> Value {
    match req.get("type").and_then(Value::as_str) {
        Some("hello") => json!({
            "type": "ready",
            "version": env!("CARGO_PKG_VERSION"),
            "engines": ["fast", "hq", "fallback"],
            "realtime": false
        }),
        Some("clean") => clean(app, req).await,
        other => json!({ "type": "error", "error": format!("unknown message type: {other:?}") }),
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
