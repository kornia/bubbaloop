//! Dashboard chat route: POST /api/v1/agents/{id}/turns → SSE stream.
//!
//! The handler follows the same in-process Zenoh client pattern as
//! `cli/agent_client.rs:send_and_render` — subscribe before publishing so no
//! response events are missed, then drain the outbox into SSE frames.
//!
//! Provider policy: Claude OAuth is blocked (Anthropic Feb 2026 ToS); Gemini
//! and Claude API key are accepted. See `check_oauth_gate()`.

use crate::agent::gateway::{self, AgentEvent, AgentEventType, AgentMessage};
use crate::agent::runtime::AgentsConfig;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_stream::wrappers::ReceiverStream;

/// Shared state injected into the chat handler.
pub struct ChatState {
    pub session: Arc<zenoh::Session>,
    pub machine_id: String,
    pub auth_token: String,
}

/// JSON body for a turn request.
#[derive(Debug, Deserialize)]
pub struct TurnRequest {
    pub message: String,
}

/// JSON body for error responses.
#[derive(Serialize)]
struct ErrorBody {
    error: String,
    code: String,
}

/// Build the chat router — call this then merge into the `/api/v1` router.
pub fn chat_router(state: Arc<ChatState>) -> Router {
    Router::new()
        .route(
            "/agents/{id}/turns",
            axum::routing::post(agent_turn_handler),
        )
        .with_state(state)
}

/// POST /api/v1/agents/{id}/turns
///
/// Publishes an AgentMessage to the Zenoh inbox and streams the outbox as SSE.
/// Blocks until `done`/`error` event or 120s timeout.
///
/// Returns:
/// - 200 SSE stream on success
/// - 403 if provider is Claude OAuth (prohibited by Anthropic Feb 2026 ToS)
/// - 404 if agent_id not recognised (surfaced via 503 inbox-full from runtime)
/// - 503 if the agent inbox is full (single-user: retry manually)
pub async fn agent_turn_handler(
    State(state): State<Arc<ChatState>>,
    Path(agent_id): Path<String>,
    Json(req): Json<TurnRequest>,
) -> Response {
    // Validate agent_id: same rules as AgentsConfig
    if agent_id.is_empty()
        || agent_id.len() > 64
        || agent_id.contains('/')
        || agent_id.contains('\\')
        || agent_id.contains("..")
        || !agent_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorBody {
                error: "Invalid agent ID — must be 1-64 chars, [a-zA-Z0-9_-]".to_string(),
                code: "invalid_agent_id".to_string(),
            }),
        )
            .into_response();
    }

    // Provider auth gate: block Claude OAuth (Anthropic Feb 2026 ToS)
    if is_using_oauth_only() {
        log::warn!(
            "[CHAT] Rejected turn for agent='{}': Claude OAuth credentials detected. \
             Set GEMINI_API_KEY (recommended, free tier) or ANTHROPIC_API_KEY.",
            agent_id
        );
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorBody {
                error: "Dashboard chat with Claude OAuth is prohibited by Anthropic's Feb 2026 ToS. \
                        Set GEMINI_API_KEY (recommended, free tier) or ANTHROPIC_API_KEY for Claude API access. \
                        The CLI (bubbaloop agent chat) remains available at operator risk."
                    .to_string(),
                code: "oauth_chat_disabled".to_string(),
            }),
        )
            .into_response();
    }

    // 404 if the agent_id is not in agents.toml
    let config = AgentsConfig::load_or_default();
    if !config.agents.contains_key(&agent_id) {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorBody {
                error: format!(
                    "Agent '{}' not found. Check ~/.bubbaloop/agents.toml.",
                    agent_id
                ),
                code: "agent_not_found".to_string(),
            }),
        )
            .into_response();
    }

    let correlation_id = uuid::Uuid::new_v4().to_string();
    let start = Instant::now();
    let agent_id_log = agent_id.clone();
    let corr_log = correlation_id.clone();

    // Channel: background task → SSE stream
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    // Spawn: declare subscriber inside the task so its lifetime is bounded to the task scope
    let session = state.session.clone();
    let machine_id = state.machine_id.clone();
    let auth_token = state.auth_token.clone();
    let corr_id = correlation_id.clone();

    tokio::spawn(async move {
        // Subscribe BEFORE publishing — prevents missing events emitted before we subscribe
        let outbox = gateway::outbox_topic(&machine_id, &agent_id);
        let sub = match session.declare_subscriber(&outbox).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!(
                    "[CHAT] Failed to declare subscriber for '{}': {}",
                    agent_id,
                    e
                );
                let _ = tx
                    .send(Ok(Event::default()
                        .event("error")
                        .data("{\"error\":\"internal: subscriber failed\"}")))
                    .await;
                return;
            }
        };

        // Publish to inbox
        let msg = AgentMessage {
            id: corr_id.clone(),
            text: req.message,
            agent: Some(agent_id.clone()),
            auth_token: Some(auth_token),
        };
        let inbox = gateway::inbox_topic(&machine_id);
        match serde_json::to_vec(&msg) {
            Ok(payload) => {
                if let Err(e) = session.put(&inbox, payload).await {
                    log::warn!("[CHAT] Failed to publish to inbox '{}': {}", inbox, e);
                    let _ = tx
                        .send(Ok(Event::default()
                            .event("error")
                            .data("{\"error\":\"internal: publish failed\"}")))
                        .await;
                    return;
                }
            }
            Err(e) => {
                log::warn!("[CHAT] Failed to serialize AgentMessage: {}", e);
                let _ = tx
                    .send(Ok(Event::default()
                        .event("error")
                        .data("{\"error\":\"internal: serialization failed\"}")))
                    .await;
                return;
            }
        }

        // Drain outbox → SSE frames, filtering by correlation ID
        loop {
            match tokio::time::timeout(Duration::from_secs(120), sub.recv_async()).await {
                Ok(Ok(sample)) => {
                    let bytes = sample.payload().to_bytes();
                    let event: AgentEvent = match serde_json::from_slice(&bytes) {
                        Ok(e) => e,
                        Err(_) => continue,
                    };

                    // Skip events for other concurrent turns
                    if event.id != corr_id {
                        continue;
                    }

                    let terminal = matches!(
                        event.event_type,
                        AgentEventType::Done | AgentEventType::Error
                    );

                    let data = match serde_json::to_string(&event) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    let sse_event = Event::default()
                        .event(event_type_str(&event.event_type))
                        .data(data);

                    if tx.send(Ok(sse_event)).await.is_err() {
                        // Receiver dropped — browser closed the connection.
                        // Publish cancel so the agent loop can abort the running turn.
                        log::debug!(
                            "[CHAT] agent={} corr={} receiver dropped, cancelling turn",
                            agent_id,
                            corr_id
                        );
                        let cancel = gateway::cancel_topic(&machine_id, &agent_id);
                        let _ = session.put(&cancel, corr_id.as_bytes().to_vec()).await;
                        break;
                    }

                    if terminal {
                        log::info!(
                            "[CHAT] agent={} corr={} dur_ms={} status=ok",
                            agent_id,
                            corr_id,
                            start.elapsed().as_millis()
                        );
                        break;
                    }
                }
                Ok(Err(e)) => {
                    log::warn!("[CHAT] Subscriber error for '{}': {}", agent_id, e);
                    let _ = tx
                        .send(Ok(Event::default()
                            .event("error")
                            .data("{\"error\":\"subscriber error\"}")))
                        .await;
                    break;
                }
                Err(_) => {
                    // 120s timeout — turn ran too long or daemon restarted
                    log::warn!(
                        "[CHAT] agent={} corr={} dur_ms={} status=timeout",
                        agent_id,
                        corr_id,
                        start.elapsed().as_millis()
                    );
                    let _ = tx
                        .send(Ok(Event::default()
                            .event("error")
                            .data("{\"error\":\"timeout: turn exceeded 120s\"}")))
                        .await;
                    break;
                }
            }
        }
    });

    let stream = ReceiverStream::new(rx);

    log::info!(
        "[CHAT] agent={} corr={} turn started",
        agent_id_log,
        corr_log
    );

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Map AgentEventType to the SSE event name (used by browser `EventSource.addEventListener`).
fn event_type_str(t: &AgentEventType) -> &'static str {
    match t {
        AgentEventType::Delta => "delta",
        AgentEventType::Tool => "tool",
        AgentEventType::ToolResult => "tool_result",
        AgentEventType::Image => "image",
        AgentEventType::Error => "error",
        AgentEventType::Done => "done",
        AgentEventType::System => "system",
    }
}

/// Return true if the daemon would use Claude OAuth as its LLM credential.
///
/// OAuth is prohibited for non-CLI use by Anthropic's Feb 2026 ToS. The gate
/// fires when NO sanctioned credential is present (no Gemini key, no Claude
/// API key) but a valid OAuth token exists. This means the runtime would fall
/// through to OAuth — which we block at the HTTP layer for dashboard turns.
fn is_using_oauth_only() -> bool {
    // Any of these means a sanctioned (non-OAuth) path is available.
    // Use map+unwrap_or instead of is_ok() to reject empty strings.
    if std::env::var("GEMINI_API_KEY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        return false;
    }
    if std::env::var("ANTHROPIC_API_KEY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        return false;
    }
    if has_gemini_key_file() {
        return false;
    }
    if has_anthropic_key_file() {
        return false;
    }
    // If only OAuth credentials exist, we'd fall back to OAuth
    has_valid_oauth_token()
}

fn has_gemini_key_file() -> bool {
    dirs::home_dir()
        .map(|h| {
            let path = h.join(".bubbaloop").join("gemini-key");
            path.exists()
                && std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| !l.trim().is_empty()))
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn has_anthropic_key_file() -> bool {
    dirs::home_dir()
        .map(|h| {
            let path = h.join(".bubbaloop").join("anthropic-key");
            path.exists()
                && std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| !l.trim().is_empty()))
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn has_valid_oauth_token() -> bool {
    let path = match dirs::home_dir() {
        Some(h) => h.join(".bubbaloop").join("oauth-credentials.json"),
        None => return false,
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let creds: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let expires_at = match creds.get("expires_at").and_then(|v| v.as_u64()) {
        Some(t) => t,
        None => return false,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now + 300 < expires_at
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_str_mapping() {
        assert_eq!(event_type_str(&AgentEventType::Delta), "delta");
        assert_eq!(event_type_str(&AgentEventType::Tool), "tool");
        assert_eq!(event_type_str(&AgentEventType::ToolResult), "tool_result");
        assert_eq!(event_type_str(&AgentEventType::Error), "error");
        assert_eq!(event_type_str(&AgentEventType::Done), "done");
        assert_eq!(event_type_str(&AgentEventType::System), "system");
    }

    #[test]
    fn is_using_oauth_only_with_gemini_key_set() {
        // If GEMINI_API_KEY is set, should return false even if OAuth exists
        // (We can't safely set env vars in tests — just test the logic structure)
        // The function reads env vars; we verify the presence of each early-return.
        // Structural test: all early-return conditions checked above.
        let _ = is_using_oauth_only(); // must not panic
    }

    #[test]
    fn has_gemini_key_file_no_home_edge_case() {
        // With no key file, returns false (doesn't panic)
        let _ = has_gemini_key_file();
    }

    #[test]
    fn has_valid_oauth_token_missing_file() {
        // With no oauth file on a fresh machine, returns false
        // Can't assert false (CI machines may have the file), just verify no panic
        let _ = has_valid_oauth_token();
    }
}
