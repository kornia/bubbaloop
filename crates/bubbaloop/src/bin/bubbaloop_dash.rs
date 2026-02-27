use argh::FromArgs;
use axum::{
    extract::{
        ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use futures::{SinkExt, StreamExt};
use rust_embed::RustEmbed;
use std::sync::Arc;
use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

#[derive(RustEmbed)]
#[folder = "../../dashboard/dist/"]
struct Assets;

/// Bubbaloop Dashboard Server
#[derive(FromArgs)]
struct Args {
    /// HTTP listen port (default: 8080)
    #[argh(option, short = 'p', default = "8080")]
    port: u16,

    /// zenoh bridge WebSocket URL (default: ws://127.0.0.1:10001)
    #[argh(option, short = 'b', default = "\"ws://127.0.0.1:10001\".to_string()")]
    bridge: String,
}

#[derive(Clone)]
struct AppState {
    bridge_url: String,
}

/// Static file handler: serve embedded files with MIME types, SPA fallback to index.html
async fn static_handler(uri: Uri) -> Response {
    let mut path = uri.path().trim_start_matches('/');

    // Empty path should serve index.html
    if path.is_empty() {
        path = "index.html";
    }

    // Try the exact path first
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref())],
            file.data.into_owned(),
        )
            .into_response();
    }

    // SPA fallback to index.html for routes not found
    if let Some(file) = Assets::get("index.html") {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html")],
            file.data.into_owned(),
        )
            .into_response();
    }

    // If even index.html is missing, return 404
    StatusCode::NOT_FOUND.into_response()
}

/// WebSocket proxy handler: upgrade HTTP to WS and proxy bidirectionally to zenoh bridge
async fn ws_proxy(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> Response {
    ws.on_upgrade(move |socket| handle_ws_proxy(socket, state.bridge_url.clone()))
}

async fn handle_ws_proxy(client_socket: WebSocket, bridge_url: String) {
    // Connect to the zenoh bridge
    let bridge_conn = match connect_async(&bridge_url).await {
        Ok((stream, _)) => stream,
        Err(e) => {
            log::error!("Failed to connect to zenoh bridge at {}: {}", bridge_url, e);
            return;
        }
    };

    log::debug!("WebSocket proxy connection established to {}", bridge_url);

    let (mut bridge_sink, mut bridge_stream) = bridge_conn.split();
    let (mut client_sink, mut client_stream) = client_socket.split();

    // Client -> Bridge
    // Note: axum and tungstenite have distinct Utf8Bytes/Bytes newtypes,
    // so we convert through String/bytes::Bytes as intermediary.
    let client_to_bridge = async {
        while let Some(Ok(msg)) = client_stream.next().await {
            let bridge_msg = match msg {
                AxumMessage::Text(t) => {
                    let s: &str = t.as_ref();
                    TungsteniteMessage::Text(s.into())
                }
                AxumMessage::Binary(b) => TungsteniteMessage::Binary(b),
                AxumMessage::Ping(p) => TungsteniteMessage::Ping(p),
                AxumMessage::Pong(p) => TungsteniteMessage::Pong(p),
                AxumMessage::Close(_) => {
                    log::debug!("Client closed WebSocket connection");
                    let _ = bridge_sink.send(TungsteniteMessage::Close(None)).await;
                    break;
                }
            };
            if let Err(e) = bridge_sink.send(bridge_msg).await {
                log::error!("Failed to send to bridge: {}", e);
                break;
            }
        }
    };

    // Bridge -> Client
    let bridge_to_client = async {
        while let Some(Ok(msg)) = bridge_stream.next().await {
            let client_msg = match msg {
                TungsteniteMessage::Text(t) => {
                    let s: &str = t.as_ref();
                    AxumMessage::Text(s.into())
                }
                TungsteniteMessage::Binary(b) => AxumMessage::Binary(b),
                TungsteniteMessage::Ping(p) => AxumMessage::Ping(p),
                TungsteniteMessage::Pong(p) => AxumMessage::Pong(p),
                TungsteniteMessage::Close(_) => {
                    log::debug!("Bridge closed WebSocket connection");
                    let _ = client_sink.send(AxumMessage::Close(None)).await;
                    break;
                }
                TungsteniteMessage::Frame(_) => continue,
            };
            if let Err(e) = client_sink.send(client_msg).await {
                log::error!("Failed to send to client: {}", e);
                break;
            }
        }
    };

    // Run both directions concurrently, stop when either ends
    tokio::select! {
        _ = client_to_bridge => {
            log::debug!("Client->Bridge stream ended");
        },
        _ = bridge_to_client => {
            log::debug!("Bridge->Client stream ended");
        },
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    let args: Args = argh::from_env();

    let state = Arc::new(AppState {
        bridge_url: args.bridge.clone(),
    });

    let app = Router::new()
        .route("/zenoh", any(ws_proxy))
        .fallback(static_handler)
        .with_state(state);

    let addr = format!("127.0.0.1:{}", args.port);
    log::info!("Dashboard server listening on http://{}", addr);
    log::info!("Proxying /zenoh to {}", args.bridge);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    log::info!("Server shut down gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    ctrl_c.await.expect("failed to install Ctrl+C handler");

    log::info!("Shutdown signal received");
}
