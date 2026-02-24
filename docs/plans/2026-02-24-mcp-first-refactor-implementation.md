# MCP-First Universal Runtime — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor bubbaloop into an MCP-first universal runtime where any agentic framework can discover and control physical AI nodes via MCP.

**Architecture:** Dual-plane model — MCP handles control (discovery, lifecycle, config, rules), Zenoh handles data (streaming sensors). Security foundation first. 4 phases: security → MCP enhancement → dashboard migration → cleanup + ecosystem.

**Tech Stack:** Rust, Zenoh, axum, rmcp, prost, zbus, tokio, argh, thiserror/anyhow, serde, tower (rate limiting)

**Design Document:** `docs/plans/2026-02-24-mcp-first-refactor-design.md`

---

## Phase 0: Security Foundation

Phase 0 blocks all other phases. Every task here fixes an existing vulnerability or adds a security primitive that subsequent phases depend on.

---

### Task 1: Fix Dashboard Binding to 0.0.0.0

**Files:**
- Modify: `crates/bubbaloop/src/bin/bubbaloop_dash.rs:171`
- Test: `crates/bubbaloop/src/bin/bubbaloop_dash.rs` (manual verification)

**Step 1: Fix the bind address**

In `crates/bubbaloop/src/bin/bubbaloop_dash.rs`, change line 171:

```rust
// BEFORE (line 171):
let addr = format!("0.0.0.0:{}", args.port);

// AFTER:
let addr = format!("127.0.0.1:{}", args.port);
```

**Step 2: Run check to verify it compiles**

Run: `cd /home/nvidia/bubbaloop && pixi run check`
Expected: compiles without errors

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/bin/bubbaloop_dash.rs
git commit -m "fix: bind dashboard to 127.0.0.1 instead of 0.0.0.0

Security fix: prevents remote access to dashboard server.
Per CLAUDE.md convention: bind localhost only, never 0.0.0.0."
```

---

### Task 2: Fix journalctl Absolute Path

**Files:**
- Modify: `crates/bubbaloop/src/daemon/node_manager.rs:726`

**Step 1: Write a failing test**

Add to the test module in `crates/bubbaloop/src/daemon/node_manager.rs`:

```rust
#[test]
fn test_journalctl_uses_absolute_path() {
    // Verify the constant points to an absolute path
    assert!(JOURNALCTL_PATH.starts_with('/'));
}
```

**Step 2: Run test to verify it fails**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib test_journalctl_uses_absolute_path`
Expected: FAIL — `JOURNALCTL_PATH` not defined

**Step 3: Add the constant and use it**

Near the top of `crates/bubbaloop/src/daemon/node_manager.rs`, add:

```rust
/// Absolute path to journalctl — never rely on PATH for system binaries.
const JOURNALCTL_PATH: &str = "/usr/bin/journalctl";
```

At line 726, change:

```rust
// BEFORE:
let journal_output = Command::new("journalctl")

// AFTER:
let journal_output = Command::new(JOURNALCTL_PATH)
```

**Step 4: Run test to verify it passes**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib test_journalctl_uses_absolute_path`
Expected: PASS

**Step 5: Run full check**

Run: `cd /home/nvidia/bubbaloop && pixi run check`
Expected: compiles without errors

**Step 6: Commit**

```bash
git add crates/bubbaloop/src/daemon/node_manager.rs
git commit -m "fix: use absolute path for journalctl

Security fix: prevents PATH hijacking when reading node logs.
Adds JOURNALCTL_PATH constant pointing to /usr/bin/journalctl."
```

---

### Task 3: Validate Rule Action Fields

**Files:**
- Modify: `crates/bubbaloop/src/validation.rs`
- Modify: `crates/bubbaloop/src/agent/rules.rs:207-274`

**Step 1: Write failing tests for action validation**

Add to `crates/bubbaloop/src/validation.rs` tests:

```rust
#[test]
fn test_validate_action_command_node() {
    // Valid node names pass
    assert!(validate_node_name("openmeteo").is_ok());
    // Path traversal rejected
    assert!(validate_node_name("../etc/passwd").is_err());
}

#[test]
fn test_validate_action_publish_topic() {
    // Valid topics pass
    assert!(validate_publish_topic("bubbaloop/local/jetson1/my-node/data").is_ok());
    // Must start with bubbaloop/
    assert!(validate_publish_topic("other/topic").is_err());
    // Reject wildcards in publish topics (write, not read)
    assert!(validate_publish_topic("bubbaloop/**/all").is_err());
    // Reject empty
    assert!(validate_publish_topic("").is_err());
}
```

**Step 2: Run tests to verify they fail**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib test_validate_action_publish_topic`
Expected: FAIL — `validate_publish_topic` not defined

**Step 3: Implement `validate_publish_topic` in validation.rs**

Add to `crates/bubbaloop/src/validation.rs`:

```rust
/// Validate a publish topic: must start with `bubbaloop/`, no wildcards, max 256 chars.
pub fn validate_publish_topic(topic: &str) -> Result<(), String> {
    if topic.is_empty() || topic.len() > 256 {
        return Err(format!(
            "Publish topic must be 1-256 characters, got {}",
            topic.len()
        ));
    }
    if !topic.starts_with("bubbaloop/") {
        return Err("Publish topic must start with 'bubbaloop/'".to_string());
    }
    if topic.contains('*') {
        return Err("Publish topic must not contain wildcards".to_string());
    }
    // Validate characters: alphanumeric, slash, hyphen, underscore, dot
    if !topic.chars().all(|c| c.is_alphanumeric() || "/-_.".contains(c)) {
        return Err("Publish topic contains invalid characters".to_string());
    }
    Ok(())
}
```

**Step 4: Run tests to verify they pass**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib test_validate_action_publish_topic`
Expected: PASS

**Step 5: Add validation to `Action::execute()` in rules.rs**

In `crates/bubbaloop/src/agent/rules.rs`, modify `Action::execute()`:

```rust
// In Action::Command arm (line ~217), add validation before executing:
Action::Command { node, command, params } => {
    if let Err(e) = crate::validation::validate_node_name(node) {
        log::warn!("[RULE] Invalid node name in command action: {}", e);
        return;
    }
    // ... existing code
}

// In Action::Publish arm (line ~265), add validation before publishing:
Action::Publish { topic, payload } => {
    if let Err(e) = crate::validation::validate_publish_topic(topic) {
        log::warn!("[RULE] Invalid publish topic in action: {}", e);
        return;
    }
    // ... existing code
}
```

**Step 6: Run full tests**

Run: `cd /home/nvidia/bubbaloop && pixi run test`
Expected: all tests pass

**Step 7: Run clippy**

Run: `cd /home/nvidia/bubbaloop && pixi run clippy`
Expected: zero warnings

**Step 8: Commit**

```bash
git add crates/bubbaloop/src/validation.rs crates/bubbaloop/src/agent/rules.rs
git commit -m "fix: validate rule action fields before execution

CRITICAL security fix: Action::Publish topic and Action::Command node
were completely unvalidated. Adds validate_publish_topic() and applies
validate_node_name() to command actions. Prevents arbitrary Zenoh
topic injection via rule engine."
```

---

### Task 4: Scope `send_command` to Local Machine

**Files:**
- Modify: `crates/bubbaloop/src/mcp/mod.rs:205`
- Modify: `crates/bubbaloop/src/agent/rules.rs:222`

**Step 1: Write a failing test**

Add to `crates/bubbaloop/src/validation.rs`:

```rust
/// Build a scoped key expression for a node resource.
/// Uses scope + machine_id to prevent cross-machine broadcast.
pub fn scoped_node_key(scope: &str, machine_id: &str, node_name: &str, resource: &str) -> Result<String, String> {
    validate_node_name(node_name)?;
    Ok(format!("bubbaloop/{}/{}/{}/{}", scope, machine_id, node_name, resource))
}
```

Add test:

```rust
#[test]
fn test_scoped_node_key() {
    let key = scoped_node_key("local", "jetson1", "openmeteo", "command").unwrap();
    assert_eq!(key, "bubbaloop/local/jetson1/openmeteo/command");
    assert!(!key.contains("**"));
}

#[test]
fn test_scoped_node_key_rejects_invalid_name() {
    assert!(scoped_node_key("local", "jetson1", "../bad", "command").is_err());
}
```

**Step 2: Run test to verify it fails**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib test_scoped_node_key`
Expected: FAIL — function not defined yet (write the function first in Step 1, then the test verifies compile + logic)

**Step 3: Update `send_command` in mcp/mod.rs**

In `crates/bubbaloop/src/mcp/mod.rs`, the `send_command` tool (line ~205) currently uses `bubbaloop/**/{}/**/command` which broadcasts to all machines. Update the MCP server to store scope/machine_id and use scoped keys:

Add fields to `BubbaLoopMcpServer`:

```rust
pub struct BubbaLoopMcpServer {
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    agent: Option<Arc<Agent>>,
    tool_router: ToolRouter<Self>,
    scope: String,
    machine_id: String,
}
```

Update `new()` to accept and store scope/machine_id:

```rust
pub fn new(
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    agent: Option<Arc<Agent>>,
    scope: String,
    machine_id: String,
) -> Self {
    Self {
        session,
        node_manager,
        agent,
        tool_router: Self::tool_router(),
        scope,
        machine_id,
    }
}
```

Update `send_command` to use scoped key:

```rust
// BEFORE:
let key_expr = format!("bubbaloop/**/{}/**/command", req.node_name);

// AFTER:
let key_expr = format!(
    "bubbaloop/{}/{}/{}/command",
    self.scope, self.machine_id, req.node_name
);
```

Also update `get_node_config` (line ~147) and `get_node_manifest` (line ~159) with similar scoping.

**Step 4: Update all callers of `BubbaLoopMcpServer::new()`**

In `crates/bubbaloop/src/daemon/mod.rs` (line ~169) and `crates/bubbaloop/src/mcp/mod.rs:552`:

```rust
// In the StreamableHttpService factory:
let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
let machine_id = crate::daemon::util::get_machine_id();

// Update the factory closure:
move || Ok(BubbaLoopMcpServer::new(
    session_for_factory.clone(),
    manager_for_factory.clone(),
    agent_for_factory.clone(),
    scope.clone(),
    machine_id.clone(),
))
```

**Step 5: Similarly scope `Action::Command` in rules.rs**

In `crates/bubbaloop/src/agent/rules.rs:222`, update to use scoped key expression (the Agent already has `self.scope` and `self.machine_id`). Pass these to `Action::execute()`:

```rust
// Change Action::execute signature to accept scope/machine_id:
pub async fn execute(&self, session: &zenoh::Session, scope: &str, machine_id: &str) {
    // ...
    Action::Command { node, command, params } => {
        // BEFORE: let key_expr = format!("bubbaloop/**/{}/**/command", node);
        // AFTER:
        let key_expr = format!("bubbaloop/{}/{}/{}/command", scope, machine_id, node);
        // ...
    }
}
```

Update all callers of `execute()` in `agent/mod.rs` to pass scope and machine_id.

**Step 6: Run full check + test**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test`
Expected: compiles, all tests pass

**Step 7: Run clippy**

Run: `cd /home/nvidia/bubbaloop && pixi run clippy`
Expected: zero warnings

**Step 8: Commit**

```bash
git add crates/bubbaloop/src/mcp/mod.rs crates/bubbaloop/src/agent/rules.rs \
       crates/bubbaloop/src/daemon/mod.rs crates/bubbaloop/src/validation.rs
git commit -m "fix: scope send_command and rule actions to local machine

Security fix: replaces bubbaloop/**/node/**/command wildcards with
bubbaloop/{scope}/{machine_id}/node/command. Prevents MCP tools and
rule actions from broadcasting commands to all machines on the Zenoh
network."
```

---

### Task 5: Harden `query_zenoh` Tool

**Files:**
- Modify: `crates/bubbaloop/src/mcp/mod.rs:300-307`
- Modify: `crates/bubbaloop/src/validation.rs`

**Step 1: Write failing tests for key expression validation**

Add to `crates/bubbaloop/src/validation.rs`:

```rust
/// Validate a Zenoh key expression for query_zenoh.
/// Must start with `bubbaloop/`, no wildcard-only queries.
pub fn validate_query_key_expr(key_expr: &str) -> Result<(), String> {
    if key_expr.is_empty() || key_expr.len() > 512 {
        return Err(format!("Key expression must be 1-512 characters, got {}", key_expr.len()));
    }
    if !key_expr.starts_with("bubbaloop/") {
        return Err("Key expression must start with 'bubbaloop/'".to_string());
    }
    // Reject wildcard-only queries
    let stripped = key_expr.trim_start_matches("bubbaloop/");
    if stripped == "**" || stripped == "*" || stripped.is_empty() {
        return Err("Key expression too broad — specify a more specific path".to_string());
    }
    Ok(())
}
```

Add tests:

```rust
#[test]
fn test_validate_query_key_expr_valid() {
    assert!(validate_query_key_expr("bubbaloop/local/jetson1/openmeteo/status").is_ok());
    assert!(validate_query_key_expr("bubbaloop/**/telemetry/status").is_ok());
    assert!(validate_query_key_expr("bubbaloop/local/**/health/*").is_ok());
}

#[test]
fn test_validate_query_key_expr_invalid() {
    assert!(validate_query_key_expr("").is_err());
    assert!(validate_query_key_expr("other/namespace/topic").is_err());
    assert!(validate_query_key_expr("bubbaloop/**").is_err());
    assert!(validate_query_key_expr("bubbaloop/*").is_err());
    assert!(validate_query_key_expr("**").is_err());
}
```

**Step 2: Run tests to verify they fail**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib test_validate_query_key_expr`
Expected: FAIL — function not defined

**Step 3: Implement the function (already in Step 1 above) and run tests**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib test_validate_query_key_expr`
Expected: PASS

**Step 4: Apply validation to `query_zenoh` tool**

In `crates/bubbaloop/src/mcp/mod.rs`, update `query_zenoh` (line ~301):

```rust
#[tool(description = "Query a Zenoh key expression (admin only). Key must start with 'bubbaloop/'. Returns up to 100 results.")]
async fn query_zenoh(
    &self,
    Parameters(req): Parameters<QueryTopicRequest>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    if let Err(e) = crate::validation::validate_query_key_expr(&req.key_expr) {
        return Ok(CallToolResult::success(vec![Content::text(format!("Validation error: {}", e))]));
    }
    let result = self.zenoh_get_text(&req.key_expr).await;
    Ok(CallToolResult::success(vec![Content::text(result)]))
}
```

**Step 5: Run check + test**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test`
Expected: compiles, all tests pass

**Step 6: Commit**

```bash
git add crates/bubbaloop/src/mcp/mod.rs crates/bubbaloop/src/validation.rs
git commit -m "fix: validate query_zenoh key expressions

Security fix: query_zenoh now requires key_expr to start with
'bubbaloop/' and rejects wildcard-only queries. Prevents unrestricted
Zenoh network scanning via MCP."
```

---

### Task 6: Add Bearer Token Authentication to MCP Server

**Files:**
- Create: `crates/bubbaloop/src/mcp/auth.rs`
- Modify: `crates/bubbaloop/src/mcp/mod.rs`

**Step 1: Write failing tests for token generation and validation**

Create `crates/bubbaloop/src/mcp/auth.rs`:

```rust
//! MCP bearer token authentication.
//!
//! Token is auto-generated on first daemon start and stored in
//! `~/.bubbaloop/mcp-token` with 0600 permissions.

use std::path::PathBuf;

/// Path to the MCP authentication token.
pub fn token_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".bubbaloop")
        .join("mcp-token")
}

/// Load or generate the MCP authentication token.
///
/// If the token file exists, reads it. Otherwise generates a new random
/// token, writes it with 0600 permissions, and returns it.
pub fn load_or_generate_token() -> Result<String, std::io::Error> {
    let path = token_path();
    if path.exists() {
        let token = std::fs::read_to_string(&path)?.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // Generate a new token
    let token = format!("bb_{}", uuid::Uuid::new_v4().as_simple());

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write with restrictive permissions
    std::fs::write(&path, &token)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

    log::info!("Generated MCP token at {:?}", path);
    Ok(token)
}

/// Validate a bearer token from an Authorization header.
pub fn validate_token(header_value: &str, expected: &str) -> bool {
    let token = header_value
        .strip_prefix("Bearer ")
        .unwrap_or(header_value);
    // Constant-time comparison to prevent timing attacks
    constant_time_eq(token.as_bytes(), expected.as_bytes())
}

/// Constant-time byte comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_or_generate_token_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-token");
        // Override for test: generate directly
        let token = format!("bb_{}", uuid::Uuid::new_v4().as_simple());
        std::fs::write(&path, &token).unwrap();
        let loaded = std::fs::read_to_string(&path).unwrap().trim().to_string();
        assert_eq!(loaded, token);
        assert!(loaded.starts_with("bb_"));
    }

    #[test]
    fn test_validate_token_correct() {
        assert!(validate_token("Bearer bb_abc123", "bb_abc123"));
        assert!(validate_token("bb_abc123", "bb_abc123"));
    }

    #[test]
    fn test_validate_token_incorrect() {
        assert!(!validate_token("Bearer bb_wrong", "bb_abc123"));
        assert!(!validate_token("", "bb_abc123"));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }
}
```

**Step 2: Run tests to verify they pass**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib mcp::auth`
Expected: PASS (or compile error until mod is declared — fix mod declaration first)

**Step 3: Declare the module**

In `crates/bubbaloop/src/mcp/mod.rs`, add near the top:

```rust
pub mod auth;
```

**Step 4: Add authentication middleware to MCP server**

In `crates/bubbaloop/src/mcp/mod.rs`, update `run_mcp_server()`:

```rust
pub async fn run_mcp_server(
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    agent: Option<Arc<Agent>>,
    port: u16,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpService,
    };

    // Load or generate auth token
    let token = auth::load_or_generate_token()
        .map_err(|e| format!("Failed to load MCP token: {}", e))?;
    log::info!("MCP authentication enabled (token in ~/.bubbaloop/mcp-token)");

    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = crate::daemon::util::get_machine_id();

    let session_for_factory = session;
    let manager_for_factory = node_manager;
    let agent_for_factory = agent;

    let mcp_service = StreamableHttpService::new(
        move || Ok(BubbaLoopMcpServer::new(
            session_for_factory.clone(),
            manager_for_factory.clone(),
            agent_for_factory.clone(),
            scope.clone(),
            machine_id.clone(),
        )),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    // Build router with auth middleware
    let token_for_middleware = token.clone();
    let auth_middleware = axum::middleware::from_fn(move |req: axum::extract::Request, next: axum::middleware::Next| {
        let expected = token_for_middleware.clone();
        async move {
            // Check Authorization header
            let authorized = req.headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(|v| auth::validate_token(v, &expected))
                .unwrap_or(false);

            if !authorized {
                return Err(axum::http::StatusCode::UNAUTHORIZED.into_response());
            }
            Ok(next.run(req).await)
        }
    });

    let router = axum::Router::new()
        .nest_service("/mcp", mcp_service)
        .layer(auth_middleware);

    let bind_addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    log::info!("MCP server listening on http://{}/mcp", bind_addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_rx.changed().await.ok();
        })
        .await?;

    log::info!("MCP server stopped.");
    Ok(())
}
```

**Note:** The exact middleware pattern depends on rmcp's StreamableHttpService compatibility with axum layers. If rmcp doesn't compose with axum middleware directly, an alternative is to add auth checking inside each tool handler via a shared `Arc<String>` token. Verify at implementation time.

**Step 5: Run check + test**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test`
Expected: compiles, all tests pass

**Step 6: Commit**

```bash
git add crates/bubbaloop/src/mcp/auth.rs crates/bubbaloop/src/mcp/mod.rs
git commit -m "feat: add bearer token authentication to MCP server

Auto-generates token at ~/.bubbaloop/mcp-token (0600 perms) on first
daemon start. All MCP requests require Authorization: Bearer <token>.
Uses constant-time comparison to prevent timing attacks."
```

---

### Task 7: Add RBAC Authorization Tiers

**Files:**
- Create: `crates/bubbaloop/src/mcp/rbac.rs`
- Modify: `crates/bubbaloop/src/mcp/auth.rs`
- Modify: `crates/bubbaloop/src/mcp/mod.rs`

**Step 1: Write failing tests for RBAC**

Create `crates/bubbaloop/src/mcp/rbac.rs`:

```rust
//! Role-Based Access Control for MCP tools.
//!
//! Three tiers: viewer (read-only), operator (day-to-day), admin (system).
//! Token file format: `<token>:<tier>` (e.g., `bb_abc123:admin`).
//! Default tier if unspecified: `operator`.

use serde::{Deserialize, Serialize};

/// Authorization tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Viewer = 0,
    Operator = 1,
    Admin = 2,
}

impl Tier {
    /// Check if this tier has at least the required level.
    pub fn has_permission(self, required: Tier) -> bool {
        (self as u8) >= (required as u8)
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Viewer => write!(f, "viewer"),
            Tier::Operator => write!(f, "operator"),
            Tier::Admin => write!(f, "admin"),
        }
    }
}

impl std::str::FromStr for Tier {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "viewer" => Ok(Tier::Viewer),
            "operator" => Ok(Tier::Operator),
            "admin" => Ok(Tier::Admin),
            _ => Err(format!("Unknown tier '{}' — must be viewer, operator, or admin", s)),
        }
    }
}

/// Tool-to-tier mapping.
pub fn required_tier(tool_name: &str) -> Tier {
    match tool_name {
        // Viewer tools (read-only)
        "list_nodes" | "get_node_health" | "get_node_schema" | "get_stream_info"
        | "list_topics" | "get_system_status" | "get_machine_info"
        | "list_agent_rules" | "get_agent_status" | "get_events" | "doctor"
        | "discover_nodes" | "get_node_manifest" | "list_commands" => Tier::Viewer,

        // Operator tools (day-to-day operations)
        "start_node" | "stop_node" | "restart_node" | "get_node_config"
        | "set_node_config" | "read_sensor" | "send_command"
        | "add_rule" | "remove_rule" | "update_rule" | "test_rule"
        | "get_node_logs" => Tier::Operator,

        // Admin tools (system modification)
        "install_node" | "remove_node" | "build_node" | "create_node_instance"
        | "set_system_config" | "query_zenoh" => Tier::Admin,

        // Unknown tools default to admin (principle of least privilege)
        _ => Tier::Admin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_ordering() {
        assert!(Tier::Admin.has_permission(Tier::Viewer));
        assert!(Tier::Admin.has_permission(Tier::Operator));
        assert!(Tier::Admin.has_permission(Tier::Admin));
        assert!(Tier::Operator.has_permission(Tier::Viewer));
        assert!(Tier::Operator.has_permission(Tier::Operator));
        assert!(!Tier::Operator.has_permission(Tier::Admin));
        assert!(Tier::Viewer.has_permission(Tier::Viewer));
        assert!(!Tier::Viewer.has_permission(Tier::Operator));
    }

    #[test]
    fn test_tier_parse() {
        assert_eq!("viewer".parse::<Tier>().unwrap(), Tier::Viewer);
        assert_eq!("operator".parse::<Tier>().unwrap(), Tier::Operator);
        assert_eq!("admin".parse::<Tier>().unwrap(), Tier::Admin);
        assert!("unknown".parse::<Tier>().is_err());
    }

    #[test]
    fn test_required_tier_viewer_tools() {
        assert_eq!(required_tier("list_nodes"), Tier::Viewer);
        assert_eq!(required_tier("get_node_health"), Tier::Viewer);
        assert_eq!(required_tier("discover_nodes"), Tier::Viewer);
    }

    #[test]
    fn test_required_tier_operator_tools() {
        assert_eq!(required_tier("start_node"), Tier::Operator);
        assert_eq!(required_tier("add_rule"), Tier::Operator);
    }

    #[test]
    fn test_required_tier_admin_tools() {
        assert_eq!(required_tier("query_zenoh"), Tier::Admin);
        assert_eq!(required_tier("install_node"), Tier::Admin);
    }

    #[test]
    fn test_unknown_tool_requires_admin() {
        assert_eq!(required_tier("nonexistent_tool"), Tier::Admin);
    }
}
```

**Step 2: Run tests**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib mcp::rbac`
Expected: PASS (after adding `pub mod rbac;` to `mcp/mod.rs`)

**Step 3: Integrate RBAC into auth middleware**

This is deferred to implementation time — depends on how tool names can be extracted from MCP requests before they reach the handler. The two approaches:
1. **Middleware approach:** Parse the MCP JSON-RPC request body to extract tool name pre-dispatch.
2. **Per-tool approach:** Pass caller tier into each tool handler via request extensions.

Choose approach at implementation time based on rmcp API.

**Step 4: Run check + test + clippy**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test && pixi run clippy`
Expected: all pass

**Step 5: Commit**

```bash
git add crates/bubbaloop/src/mcp/rbac.rs crates/bubbaloop/src/mcp/mod.rs
git commit -m "feat: add RBAC authorization tiers (viewer/operator/admin)

Three permission levels: viewer (read-only), operator (day-to-day),
admin (system modification). Tool-to-tier mapping defined in
required_tier(). Unknown tools default to admin (least privilege)."
```

---

### Task 8: Add Rate Limiting and Audit Logging

**Files:**
- Modify: `crates/bubbaloop/src/mcp/mod.rs`
- Modify: `Cargo.toml` (add `tower` dependency if needed)

**Step 1: Add audit logging to each tool invocation**

In each tool handler in `crates/bubbaloop/src/mcp/mod.rs`, add a log line at the start:

```rust
log::info!("[MCP] tool={} caller=mcp", "list_nodes");
```

This is the simplest audit logging — caller identity can be enhanced once RBAC is integrated.

For now, log tool name + parameters (excluding sensitive data) to stderr via `log::info!`.

**Step 2: Add rate limiting via tower**

Check if `tower` is already available via `axum`. If not, add to `Cargo.toml`:

```toml
tower = { version = "0.5", features = ["limit"], optional = true }
```

Add to `mcp` feature:

```toml
mcp = ["dep:rmcp", "dep:schemars", "dep:axum", "dep:tower"]
```

In `run_mcp_server()`, add rate limiting layer:

```rust
use tower::limit::RateLimitLayer;
use std::time::Duration;

let router = axum::Router::new()
    .nest_service("/mcp", mcp_service)
    .layer(RateLimitLayer::new(100, Duration::from_secs(60))); // 100 req/min
```

**Note:** Exact rate limiting strategy depends on axum + rmcp compatibility. Per-tool rate limits may need to be implemented inside handlers rather than as middleware. Determine at implementation time.

**Step 3: Run check + test**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test`
Expected: compiles, all tests pass

**Step 4: Commit**

```bash
git add crates/bubbaloop/src/mcp/mod.rs crates/bubbaloop/Cargo.toml
git commit -m "feat: add audit logging and rate limiting to MCP server

Every MCP tool invocation is logged with tool name and timestamp.
Rate limiting at 100 requests/minute via tower::RateLimitLayer."
```

---

### Task 9: Phase 0 Verification

**Step 1: Run full test suite**

Run: `cd /home/nvidia/bubbaloop && pixi run fmt && pixi run clippy && pixi run test`
Expected: all pass, zero clippy warnings

**Step 2: Verify security checklist**

Manually verify each item:
- [ ] MCP server requires authentication → Check `auth.rs` loads token
- [ ] Authorization tiers defined → Check `rbac.rs` exists with tests
- [ ] `query_zenoh` validated → Check `validate_query_key_expr` called
- [ ] Rule actions validated → Check `validate_node_name`/`validate_publish_topic` in `rules.rs`
- [ ] Dashboard binds `127.0.0.1` → Check `bubbaloop_dash.rs:171`
- [ ] `send_command` scoped → Check no `**` wildcards in MCP tools
- [ ] `journalctl` absolute path → Check `JOURNALCTL_PATH` constant
- [ ] Audit logging → Check `log::info!("[MCP]")` in tool handlers

**Step 3: Commit a checkpoint**

```bash
git add -A
git commit -m "chore: Phase 0 (Security Foundation) complete

All security tasks verified:
- Bearer token authentication on MCP
- RBAC tier definitions (viewer/operator/admin)
- query_zenoh key expression validation
- Rule action field validation
- Dashboard bound to localhost
- send_command scoped to local machine
- journalctl absolute path
- Audit logging on all MCP tools"
```

---

## Phase 1: MCP Enhancement

Phase 1 makes MCP the primary control plane with ~22 tools, the `PlatformOperations` trait, and stdio transport.

---

### Task 10: Define `trait PlatformOperations`

**Files:**
- Create: `crates/bubbaloop/src/mcp/platform.rs`

**Step 1: Write the trait definition with tests**

Create `crates/bubbaloop/src/mcp/platform.rs`:

```rust
//! Clean layer boundary between MCP server and daemon internals.
//!
//! Both the MCP server and any future internal consumer implement against
//! this trait, making the layer separation enforceable at compile time.

use serde_json::Value;
use std::future::Future;

/// Result type for platform operations.
pub type PlatformResult<T> = Result<T, PlatformError>;

/// Errors from platform operations.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Node summary for list operations.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeInfo {
    pub name: String,
    pub status: String,
    pub health: String,
    pub node_type: String,
    pub installed: bool,
    pub is_built: bool,
}

/// Command to execute on a node.
#[derive(Debug, Clone)]
pub enum NodeCommand {
    Start,
    Stop,
    Restart,
    Build,
    GetLogs,
}

/// Abstraction over daemon internals.
///
/// MCP tools call this trait instead of `Arc<NodeManager>` directly.
/// This makes the MCP server testable with mock implementations.
pub trait PlatformOperations: Send + Sync + 'static {
    fn list_nodes(&self) -> impl Future<Output = PlatformResult<Vec<NodeInfo>>> + Send;
    fn get_node_detail(&self, name: &str) -> impl Future<Output = PlatformResult<Value>> + Send;
    fn execute_command(&self, name: &str, cmd: NodeCommand) -> impl Future<Output = PlatformResult<String>> + Send;
    fn get_node_config(&self, name: &str) -> impl Future<Output = PlatformResult<Value>> + Send;
    fn query_zenoh(&self, key_expr: &str) -> impl Future<Output = PlatformResult<String>> + Send;
}
```

**Step 2: Add module declaration**

In `crates/bubbaloop/src/mcp/mod.rs`:

```rust
pub mod platform;
```

**Step 3: Run check**

Run: `cd /home/nvidia/bubbaloop && pixi run check`
Expected: compiles

**Step 4: Commit**

```bash
git add crates/bubbaloop/src/mcp/platform.rs crates/bubbaloop/src/mcp/mod.rs
git commit -m "feat: define trait PlatformOperations for MCP layer boundary

Clean abstraction over daemon internals. MCP tools will call this trait
instead of Arc<NodeManager> directly, enabling mock-based testing."
```

---

### Task 11: Implement `PlatformOperations` for NodeManager

**Files:**
- Modify: `crates/bubbaloop/src/mcp/platform.rs`

**Step 1: Write the concrete implementation**

Add to `platform.rs`:

```rust
use crate::daemon::node_manager::NodeManager;
use crate::schemas::daemon::v1::{CommandType, NodeCommand as ProtoNodeCommand};
use std::sync::Arc;
use zenoh::Session;

/// Real platform backed by NodeManager + Zenoh.
pub struct DaemonPlatform {
    pub node_manager: Arc<NodeManager>,
    pub session: Arc<Session>,
    pub scope: String,
    pub machine_id: String,
}

impl PlatformOperations for DaemonPlatform {
    async fn list_nodes(&self) -> PlatformResult<Vec<NodeInfo>> {
        let list = self.node_manager.get_node_list().await;
        Ok(list.nodes.iter().map(|n| NodeInfo {
            name: n.name.clone(),
            status: format!("{:?}", crate::schemas::daemon::v1::NodeStatus::try_from(n.status)
                .unwrap_or(crate::schemas::daemon::v1::NodeStatus::Unknown)),
            health: format!("{:?}", crate::schemas::daemon::v1::HealthStatus::try_from(n.health_status)
                .unwrap_or(crate::schemas::daemon::v1::HealthStatus::Unknown)),
            node_type: n.node_type.clone(),
            installed: n.installed,
            is_built: n.is_built,
        }).collect())
    }

    async fn get_node_detail(&self, name: &str) -> PlatformResult<Value> {
        match self.node_manager.get_node(name).await {
            Some(node) => Ok(serde_json::to_value(&node).unwrap_or_default()),
            None => Err(PlatformError::NodeNotFound(name.to_string())),
        }
    }

    async fn execute_command(&self, name: &str, cmd: NodeCommand) -> PlatformResult<String> {
        let cmd_type = match cmd {
            NodeCommand::Start => CommandType::Start,
            NodeCommand::Stop => CommandType::Stop,
            NodeCommand::Restart => CommandType::Restart,
            NodeCommand::Build => CommandType::Build,
            NodeCommand::GetLogs => CommandType::GetLogs,
        };

        let proto_cmd = ProtoNodeCommand {
            command: cmd_type as i32,
            node_name: name.to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: crate::mcp::now_ms(),
            source_machine: "mcp".to_string(),
            target_machine: String::new(),
            node_path: String::new(),
            name_override: String::new(),
            config_override: String::new(),
        };

        let result = self.node_manager.execute_command(proto_cmd).await;
        if result.success {
            Ok(result.message)
        } else {
            Err(PlatformError::CommandFailed(result.message))
        }
    }

    // ... etc for remaining methods
}
```

**Step 2: Run check**

Run: `cd /home/nvidia/bubbaloop && pixi run check`
Expected: compiles

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/mcp/platform.rs
git commit -m "feat: implement PlatformOperations for DaemonPlatform

Concrete implementation backed by Arc<NodeManager> + Zenoh session.
This will replace direct NodeManager access in MCP tool handlers."
```

---

### Task 12: Create Mock Platform for Testing

**Files:**
- Create: `crates/bubbaloop/src/mcp/platform_mock.rs` (or in `#[cfg(test)]` module)

**Step 1: Write MockPlatform**

```rust
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    pub struct MockPlatform {
        nodes: Mutex<Vec<NodeInfo>>,
        configs: Mutex<HashMap<String, Value>>,
    }

    impl MockPlatform {
        pub fn new() -> Self {
            Self {
                nodes: Mutex::new(vec![
                    NodeInfo {
                        name: "test-node".to_string(),
                        status: "Running".to_string(),
                        health: "Healthy".to_string(),
                        node_type: "rust".to_string(),
                        installed: true,
                        is_built: true,
                    },
                ]),
                configs: Mutex::new(HashMap::new()),
            }
        }
    }

    impl PlatformOperations for MockPlatform {
        async fn list_nodes(&self) -> PlatformResult<Vec<NodeInfo>> {
            Ok(self.nodes.lock().unwrap().clone())
        }

        async fn get_node_detail(&self, name: &str) -> PlatformResult<Value> {
            self.nodes.lock().unwrap()
                .iter()
                .find(|n| n.name == name)
                .map(|n| serde_json::to_value(n).unwrap())
                .ok_or_else(|| PlatformError::NodeNotFound(name.to_string()))
        }

        async fn execute_command(&self, _name: &str, _cmd: NodeCommand) -> PlatformResult<String> {
            Ok("mock: command executed".to_string())
        }

        async fn get_node_config(&self, name: &str) -> PlatformResult<Value> {
            self.configs.lock().unwrap()
                .get(name)
                .cloned()
                .ok_or_else(|| PlatformError::NodeNotFound(name.to_string()))
        }

        async fn query_zenoh(&self, key_expr: &str) -> PlatformResult<String> {
            Ok(format!("mock: query {}", key_expr))
        }
    }
}
```

**Step 2: Run check + test**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test`
Expected: compiles, passes

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/mcp/platform.rs
git commit -m "feat: add MockPlatform for MCP contract testing

In-memory mock implementing PlatformOperations. Enables testing all
MCP tools without Zenoh, systemd, or filesystem."
```

---

### Task 13: Refactor MCP Tools to Use PlatformOperations

**Files:**
- Modify: `crates/bubbaloop/src/mcp/mod.rs`

**Step 1: Replace direct NodeManager/Session access with PlatformOperations**

Update `BubbaLoopMcpServer` to be generic over the platform:

```rust
pub struct BubbaLoopMcpServer<P: PlatformOperations> {
    platform: Arc<P>,
    agent: Option<Arc<Agent>>,
    tool_router: ToolRouter<Self>,
    scope: String,
    machine_id: String,
}
```

**Note:** Whether rmcp's `#[tool_router]` macro supports generics needs to be verified at implementation time. If not, use a trait object `Arc<dyn PlatformOperations>` instead.

Refactor each tool to call `self.platform.list_nodes()` instead of `self.node_manager.get_node_list()`, etc.

**Step 2: Run check + test**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test`
Expected: compiles, all existing tests pass

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/mcp/mod.rs
git commit -m "refactor: MCP tools use PlatformOperations trait

MCP server no longer has direct Arc<NodeManager> dependency. All
operations go through the PlatformOperations trait, enabling mock
testing and future platform swapping."
```

---

### Task 14: Add MCP Contract Tests (~100 tests)

**Files:**
- Create: `crates/bubbaloop/src/mcp/tests.rs`

**Step 1: Write contract tests for each tool**

Create `crates/bubbaloop/src/mcp/tests.rs` with ~5 tests per tool:

```rust
#[cfg(test)]
mod tests {
    use super::platform::mock::MockPlatform;
    // ... test infrastructure

    // == list_nodes ==

    #[tokio::test]
    async fn test_list_nodes_returns_all() {
        let mock = MockPlatform::new();
        let nodes = mock.list_nodes().await.unwrap();
        assert!(!nodes.is_empty());
        assert_eq!(nodes[0].name, "test-node");
    }

    // == start_node ==

    #[tokio::test]
    async fn test_start_node_valid_name() {
        let mock = MockPlatform::new();
        let result = mock.execute_command("test-node", NodeCommand::Start).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_start_node_invalid_name() {
        // Test validation at the MCP handler level, not platform level
        assert!(crate::validation::validate_node_name("../etc/passwd").is_err());
    }

    // == query_zenoh ==

    #[tokio::test]
    async fn test_query_zenoh_rejects_wildcard_only() {
        assert!(crate::validation::validate_query_key_expr("**").is_err());
        assert!(crate::validation::validate_query_key_expr("bubbaloop/**").is_err());
    }

    #[tokio::test]
    async fn test_query_zenoh_accepts_valid() {
        assert!(crate::validation::validate_query_key_expr("bubbaloop/local/jetson1/node/data").is_ok());
    }

    // ... ~90 more tests covering all tools
}
```

**Step 2: Run tests**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --lib mcp::tests`
Expected: all pass

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/mcp/tests.rs crates/bubbaloop/src/mcp/mod.rs
git commit -m "test: add MCP contract tests (~100 tests)

Tests cover all MCP tool handlers against MockPlatform:
input validation, response schema, error handling, RBAC tiers.
No Zenoh, systemd, or filesystem needed."
```

---

### Task 15: Add New MCP Tools (5-7 new tools → ~22 total)

**Files:**
- Modify: `crates/bubbaloop/src/mcp/mod.rs`

New tools to add:

1. **`get_stream_info`** (viewer) — Returns Zenoh connection params for a node's data topic
2. **`get_system_status`** (viewer) — Overall daemon/Zenoh/node health summary
3. **`get_machine_info`** (viewer) — CPU, GPU, memory, architecture, OS
4. **`test_rule`** (operator) — Dry-run a rule against current data
5. **`get_events`** (viewer) — Recent rule triggers from `trigger_log`
6. **`build_node`** (admin) — Trigger build for a node
7. **`get_node_schema`** (viewer) — Protobuf schema as human-readable JSON

Each tool follows the existing pattern:
1. Write the failing test (contract test with MockPlatform)
2. Verify it fails
3. Implement the tool handler + add to PlatformOperations if needed
4. Verify it passes
5. Commit

**Example for `get_stream_info`:**

```rust
#[tool(description = "Get Zenoh connection parameters for subscribing to a node's data stream. Returns topic, encoding, and recommended endpoint.")]
async fn get_stream_info(
    &self,
    Parameters(req): Parameters<NodeNameRequest>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    if let Err(e) = validation::validate_node_name(&req.node_name) {
        return Ok(CallToolResult::success(vec![Content::text(e)]));
    }
    let info = serde_json::json!({
        "zenoh_topic": format!("bubbaloop/{}/{}/{}/**", self.scope, self.machine_id, req.node_name),
        "encoding": "protobuf",
        "endpoint": "tcp/localhost:7447",
        "note": "Subscribe to this topic via Zenoh client library for real-time data. MCP is control-plane only."
    });
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&info).unwrap_or_default(),
    )]))
}
```

**Step per-tool: Write test → implement → verify → commit**

Each tool gets its own commit:

```bash
git commit -m "feat: add get_stream_info MCP tool (viewer)"
git commit -m "feat: add get_system_status MCP tool (viewer)"
git commit -m "feat: add get_machine_info MCP tool (viewer)"
git commit -m "feat: add test_rule MCP tool (operator)"
git commit -m "feat: add get_events MCP tool (viewer)"
git commit -m "feat: add build_node MCP tool (admin)"
git commit -m "feat: add get_node_schema MCP tool (viewer)"
```

---

### Task 16: Add stdio Transport

**Files:**
- Modify: `crates/bubbaloop/src/bin/bubbaloop.rs`
- Modify: `crates/bubbaloop/src/mcp/mod.rs`

**Step 1: Add `Mcp` subcommand to CLI**

In `crates/bubbaloop/src/bin/bubbaloop.rs`:

```rust
// Add to Command enum:
Mcp(McpArgs),

// Add args struct:
/// Run MCP server in stdio mode (for Claude Code, OpenClaw, etc.)
#[derive(FromArgs)]
#[argh(subcommand, name = "mcp")]
struct McpArgs {
    /// run in stdio mode (default: false, runs HTTP server)
    #[argh(switch)]
    stdio: bool,

    /// HTTP port (only used without --stdio)
    #[argh(option, short = 'p', default = "8088")]
    port: u16,
}
```

**Step 2: Implement stdio MCP server**

Add to `crates/bubbaloop/src/mcp/mod.rs`:

```rust
/// Run MCP server on stdio (stdin/stdout).
///
/// Logs go to a file (not stderr, which would corrupt the MCP protocol).
pub async fn run_mcp_stdio(
    session: Arc<Session>,
    node_manager: Arc<NodeManager>,
    agent: Option<Arc<Agent>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = crate::daemon::util::get_machine_id();

    let server = BubbaLoopMcpServer::new(
        session, node_manager, agent, scope, machine_id,
    );

    // rmcp provides stdio transport
    let transport = rmcp::transport::stdio();
    server.serve(transport).await?;

    Ok(())
}
```

**Note:** The exact rmcp stdio API needs verification at implementation time. Check rmcp docs for `StdioTransport` or similar.

**Step 3: Wire up in main**

```rust
// In main() match:
Some(Command::Mcp(args)) => {
    if args.stdio {
        // Redirect logs to file, not stderr (corrupts MCP protocol)
        // ... set up file logging
        run_mcp_stdio(session, node_manager, agent).await?;
    } else {
        run_mcp_server(session, node_manager, agent, args.port, shutdown_rx).await?;
    }
}
```

**Step 4: Run check**

Run: `cd /home/nvidia/bubbaloop && pixi run check`
Expected: compiles

**Step 5: Commit**

```bash
git add crates/bubbaloop/src/bin/bubbaloop.rs crates/bubbaloop/src/mcp/mod.rs
git commit -m "feat: add stdio transport for MCP (bubbaloop mcp --stdio)

Enables local agent integration (Claude Code, OpenClaw) via stdin/stdout.
Logs redirected to file to avoid corrupting MCP protocol on stderr."
```

---

### Task 17: Update ServerInfo Instructions

**Files:**
- Modify: `crates/bubbaloop/src/mcp/mod.rs:409-427`

**Step 1: Update the instructions string**

Update `get_info()` in the `ServerHandler` impl to reflect all ~22 tools with proper workflow guidance:

```rust
fn get_info(&self) -> ServerInfo {
    ServerInfo {
        protocol_version: ProtocolVersion::V_2024_11_05,
        capabilities: ServerCapabilities::builder().enable_tools().build(),
        server_info: Implementation::from_build_env(),
        instructions: Some(
            "Bubbaloop MCP-first physical AI runtime. Controls physical sensor nodes via MCP.\n\n\
             **Discovery:** list_nodes → get_node_detail → get_node_schema → get_stream_info\n\
             **Lifecycle:** install_node, start_node, stop_node, restart_node, build_node\n\
             **Data:** read_sensor (one-shot), get_stream_info (returns Zenoh topic for streaming)\n\
             **Config:** get_node_config, set_node_config\n\
             **Automation:** list_rules, add_rule, remove_rule, test_rule, get_events\n\
             **System:** get_system_status, get_machine_info, doctor\n\n\
             Streaming data flows through Zenoh (not MCP). Use get_stream_info to get Zenoh connection params.\n\
             Auth: Bearer token required (see ~/.bubbaloop/mcp-token)."
                .into(),
        ),
    }
}
```

**Step 2: Run check**

Run: `cd /home/nvidia/bubbaloop && pixi run check`
Expected: compiles

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/mcp/mod.rs
git commit -m "docs: update MCP server instructions for 22-tool inventory

Reflects new tool categories: discovery, lifecycle, data, config,
automation, system. Documents dual-plane model in agent instructions."
```

---

### Task 18: Phase 1 Verification

**Step 1: Run full suite**

Run: `cd /home/nvidia/bubbaloop && pixi run fmt && pixi run clippy && pixi run test`
Expected: all pass

**Step 2: Verify tool count**

Count tools in `mcp/mod.rs` — should be ~22.

**Step 3: Verify stdio transport**

Run: `cd /home/nvidia/bubbaloop && pixi run build`
Then: `echo '{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}' | ./target/release/bubbaloop mcp --stdio`
Expected: JSON-RPC response with server info

**Step 4: Commit checkpoint**

```bash
git commit -m "chore: Phase 1 (MCP Enhancement) complete

22 MCP tools across 6 categories. PlatformOperations trait for clean
layer separation. stdio transport for local agents. ~100 contract tests."
```

---

## Phase 2: Dashboard Migration + Test Harness

Phase 2 migrates the dashboard from Zenoh API to MCP, builds the integration test harness, and deprecates Zenoh API queryables.

---

### Task 19: Build Integration Test Harness

**Files:**
- Create: `crates/bubbaloop/tests/integration/mod.rs`
- Create: `crates/bubbaloop/tests/integration/harness.rs`

**Step 1: Create test harness**

```rust
//! Integration test harness.
//!
//! Spins up an in-process Zenoh router, daemon, and MCP client for
//! testing full request flows.

use std::sync::Arc;
use std::time::Duration;

pub struct TestHarness {
    pub session: Arc<zenoh::Session>,
    pub node_manager: Arc<bubbaloop::daemon::NodeManager>,
    pub mcp_port: u16,
    shutdown_tx: tokio::sync::watch::Sender<()>,
}

impl TestHarness {
    pub async fn new() -> Self {
        // Start Zenoh session in client mode
        let config = zenoh::Config::default();
        let session = Arc::new(zenoh::open(config).await.unwrap());

        let node_manager = bubbaloop::daemon::NodeManager::new().await.unwrap();

        let (shutdown_tx, _) = tokio::sync::watch::channel(());

        // Find available port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mcp_port = listener.local_addr().unwrap().port();
        drop(listener);

        Self {
            session,
            node_manager: Arc::new(node_manager),
            mcp_port,
            shutdown_tx,
        }
    }

    /// Call an MCP tool via HTTP
    pub async fn mcp(&self, tool: &str, params: serde_json::Value) -> serde_json::Value {
        // Use reqwest or hyper to call http://127.0.0.1:{port}/mcp
        // with JSON-RPC format
        todo!("Implement MCP HTTP client call")
    }

    pub async fn shutdown(self) {
        self.shutdown_tx.send(()).ok();
    }
}
```

**Note:** The full harness implementation depends on whether an in-process zenohd is practical. At implementation time, determine if `zenoh::open()` in peer mode is sufficient for testing or if a zenohd subprocess is needed.

**Step 2: Run check**

Run: `cd /home/nvidia/bubbaloop && pixi run check`
Expected: compiles

**Step 3: Commit**

```bash
git add crates/bubbaloop/tests/
git commit -m "feat: add integration test harness (TestHarness)

Sets up in-process Zenoh + NodeManager + MCP client for full-flow
integration testing. Foundation for Phase 2 integration tests."
```

---

### Task 20: Add Integration Tests (~30 tests)

**Files:**
- Modify: `crates/bubbaloop/tests/integration/`

Write ~30 integration tests covering:
1. Full node lifecycle (list → start → health check → stop)
2. MCP auth enforcement (reject unauthorized requests)
3. Rule CRUD (add → list → test → remove)
4. Config read/write
5. `query_zenoh` scoping
6. `get_stream_info` returns valid Zenoh topic
7. `doctor` returns healthy status

Each test follows: setup harness → call MCP tools → assert results → teardown.

**Commit per test group:**

```bash
git commit -m "test: add lifecycle integration tests (8 tests)"
git commit -m "test: add auth integration tests (5 tests)"
git commit -m "test: add rule engine integration tests (7 tests)"
git commit -m "test: add config integration tests (5 tests)"
git commit -m "test: add system integration tests (5 tests)"
```

---

### Task 21: Deprecate Zenoh API Queryables

**Files:**
- Modify: `crates/bubbaloop/src/daemon/zenoh_api.rs`

**Step 1: Add deprecation warnings**

At the top of each queryable handler in `zenoh_api.rs`, add:

```rust
log::warn!("DEPRECATED: Zenoh API queryable '{}' — use MCP tools instead", key_expr);
```

Do NOT remove the handlers yet — dashboard still uses them until Phase 3.

**Step 2: Run check + test**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test`
Expected: compiles, passes

**Step 3: Commit**

```bash
git add crates/bubbaloop/src/daemon/zenoh_api.rs
git commit -m "chore: add deprecation warnings to Zenoh API queryables

All Zenoh API queryables now log a deprecation warning on each call.
These will be removed in Phase 3 after dashboard migration is complete."
```

---

### Task 22: Phase 2 Verification

**Step 1: Run full suite**

Run: `cd /home/nvidia/bubbaloop && pixi run fmt && pixi run clippy && pixi run test`
Expected: all pass

**Step 2: Commit checkpoint**

```bash
git commit -m "chore: Phase 2 (Dashboard Migration + Test Harness) complete

Integration test harness operational. ~30 integration tests passing.
Zenoh API queryables deprecated with warnings."
```

---

## Phase 3: Cleanup + Ecosystem

Phase 3 removes deprecated code, publishes the Node SDK, and sets up community infrastructure.

---

### Task 23: Remove Zenoh API Queryables

**Files:**
- Delete: `crates/bubbaloop/src/daemon/zenoh_api.rs` (or empty it)
- Modify: `crates/bubbaloop/src/daemon/mod.rs`

**Step 1: Remove the module**

In `crates/bubbaloop/src/daemon/mod.rs`:
- Remove `pub mod zenoh_api;`
- Remove `pub use zenoh_api::run_zenoh_api_server;`
- Remove the `api_task` spawn block (lines ~136-143)
- Remove the `api_task.abort()` line (line ~194)

**Step 2: Run check + test**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test`
Expected: May have compile errors from other files referencing `zenoh_api`. Fix imports.

**Step 3: Run clippy**

Run: `cd /home/nvidia/bubbaloop && pixi run clippy`
Expected: zero warnings

**Step 4: Commit**

```bash
git add -A
git commit -m "refactor: remove deprecated Zenoh API queryables

zenoh_api.rs removed. All external access now goes through MCP tools.
Dashboard has been migrated to MCP + WebSocket in Phase 2."
```

---

### Task 24: Simplify CLI

**Files:**
- Modify: `crates/bubbaloop/src/bin/bubbaloop.rs`

**Step 1: Identify CLI commands that are now redundant with MCP**

Keep:
- `daemon` — starts the daemon
- `mcp --stdio` — stdio MCP transport
- `doctor` — system diagnostics (also available via MCP)
- `node list` — quick CLI check (lightweight wrapper around MCP `list_nodes`)
- `status` — non-interactive status display

Mark as deprecated (log warning but keep functional):
- `node add`, `node start`, `node stop`, etc. — agents should use MCP
- `debug topics`, `debug subscribe`, `debug query` — use `query_zenoh` MCP tool

**Step 2: Add deprecation notices**

```rust
// In the match for deprecated commands:
log::warn!("Note: 'bubbaloop node start' is deprecated. Use MCP tool 'start_node' instead.");
```

**Step 3: Run check + test**

Run: `cd /home/nvidia/bubbaloop && pixi run check && pixi run test`
Expected: passes

**Step 4: Commit**

```bash
git add crates/bubbaloop/src/bin/bubbaloop.rs
git commit -m "chore: add deprecation notices to CLI commands replaced by MCP

CLI commands still work but log deprecation warnings pointing to
equivalent MCP tools. Core commands (daemon, mcp, doctor, status) unchanged."
```

---

### Task 25: Handle TUI Deprecation

**Files:**
- Modify: `crates/bubbaloop/Cargo.toml`
- Modify: `crates/bubbaloop/src/bin/bubbaloop.rs`

**Step 1: Move TUI behind feature flag**

In `Cargo.toml`, the TUI deps (ratatui, crossterm) are already in `[dependencies]`. Move them to optional:

```toml
ratatui = { workspace = true, optional = true }
crossterm = { workspace = true, optional = true }

[features]
default = ["dashboard"]
tui = ["dep:ratatui", "dep:crossterm"]
```

**Step 2: Gate TUI code with `#[cfg(feature = "tui")]`**

In `bubbaloop.rs`, wrap the TUI command handler:

```rust
#[cfg(feature = "tui")]
Some(Command::Tui(_)) => {
    // existing TUI launch code
}
#[cfg(not(feature = "tui"))]
Some(Command::Tui(_)) => {
    eprintln!("TUI is not enabled. Build with --features tui to use it.");
    eprintln!("For monitoring, use the dashboard or MCP tools instead.");
    std::process::exit(1);
}
```

**Step 3: Run check (without tui feature)**

Run: `cd /home/nvidia/bubbaloop && pixi run check`
Expected: compiles without TUI

**Step 4: Commit**

```bash
git add crates/bubbaloop/Cargo.toml crates/bubbaloop/src/
git commit -m "refactor: move TUI behind feature flag

TUI code gated by --features tui. Not included in default build.
Reduces binary size and dependency count. Dashboard + MCP replace
TUI for normal use; TUI kept for SSH debugging scenarios."
```

---

### Task 26: Add Agent E2E Tests (~10 scripted workflows)

**Files:**
- Create: `crates/bubbaloop/tests/e2e/`

**Step 1: Write scripted agent workflows**

These tests simulate what an AI agent would do — multi-step MCP sequences:

```rust
#[tokio::test]
async fn test_agent_discovers_and_monitors() {
    let h = TestHarness::new().await;
    // 1. Discover nodes
    let nodes = h.mcp("list_nodes", json!({})).await;
    // 2. Get details
    // 3. Check health
    // 4. Read sensor data
    // 5. Set up a rule
    // 6. Verify rule triggers
    h.shutdown().await;
}

#[tokio::test]
async fn test_agent_lifecycle_management() {
    let h = TestHarness::new().await;
    // 1. List nodes
    // 2. Start a node
    // 3. Wait for healthy
    // 4. Get config
    // 5. Stop node
    h.shutdown().await;
}

// ... 8 more workflows
```

**Step 2: Run tests**

Run: `cd /home/nvidia/bubbaloop && pixi run test -- --test e2e`
Expected: all pass

**Step 3: Commit**

```bash
git add crates/bubbaloop/tests/e2e/
git commit -m "test: add agent E2E tests (10 scripted workflows)

Multi-step MCP sequences simulating real agent behavior:
discovery, lifecycle, monitoring, rules, config management."
```

---

### Task 27: Design Node SDK Crate

**Files:**
- Create: `docs/plans/2026-XX-XX-node-sdk-design.md`

This task produces a **design document**, not code. The Node SDK is a separate initiative:

1. Define the `Node` trait and `NodeConfig` derive macro
2. Define what the SDK handles vs what node authors write
3. Specify the crate structure (`bubbaloop-node-sdk`)
4. Plan the template system
5. Plan the `bubbaloop node init` integration

**Commit:**

```bash
git add docs/plans/
git commit -m "docs: Node SDK design document

Outlines bubbaloop-node-sdk crate: Node trait, NodeConfig derive macro,
boilerplate handling (Zenoh, health, schema, config, shutdown).
Contributors write ~50 lines of business logic instead of ~300."
```

---

### Task 28: Set Up Community Registry Structure

**Files:**
- Design document only (separate repo)

Create a design document for the `bubbaloop-node-registry` repo:
- `registry.yaml` schema
- PR submission workflow
- CI validation (node.yaml exists, builds, has tests)
- `bubbaloop marketplace search` integration

**Commit:**

```bash
git add docs/plans/
git commit -m "docs: community node registry design

Outlines bubbaloop-node-registry: YAML index, PR-based submission,
CI validation, marketplace CLI integration."
```

---

### Task 29: Phase 3 Verification

**Step 1: Run full suite**

Run: `cd /home/nvidia/bubbaloop && pixi run fmt && pixi run clippy && pixi run test`
Expected: all pass, zero warnings

**Step 2: Verify binary size improvement**

Compare binary size before/after TUI removal:
Run: `ls -la target/release/bubbaloop`

**Step 3: Final commit**

```bash
git commit -m "chore: Phase 3 (Cleanup + Ecosystem) complete

Zenoh API removed. TUI behind feature flag. CLI simplified.
Agent E2E tests passing. SDK and registry designs documented.
Total: ~22 MCP tools, ~140 tests, security foundation in place."
```

---

## Summary

| Phase | Tasks | Tests Added | Key Deliverable |
|-------|-------|-------------|-----------------|
| Phase 0 | 1-9 | ~20 | Security foundation (auth, RBAC, validation) |
| Phase 1 | 10-18 | ~100 | MCP as primary control plane (22 tools, stdio) |
| Phase 2 | 19-22 | ~30 | Integration test harness, Zenoh API deprecated |
| Phase 3 | 23-29 | ~10 | Cleanup, ecosystem design docs |
| **Total** | **29 tasks** | **~160 tests** | **MCP-first universal runtime** |

### Dependency Graph

```
Phase 0 (Tasks 1-9) ──blocks──▶ Phase 1 (Tasks 10-18) ──blocks──▶ Phase 2 (Tasks 19-22) ──blocks──▶ Phase 3 (Tasks 23-29)

Within Phase 0:
  Task 1 (dashboard bind)     ─ independent
  Task 2 (journalctl path)    ─ independent
  Task 3 (rule validation)    ─ independent
  Task 4 (scope send_command) ─ depends on Task 3 (validation.rs changes)
  Task 5 (query_zenoh)        ─ depends on Task 3
  Task 6 (auth)               ─ independent
  Task 7 (RBAC)               ─ depends on Task 6
  Task 8 (rate limit + audit) ─ depends on Task 6
  Task 9 (verification)       ─ depends on all

Within Phase 1:
  Task 10 (trait)             ─ independent
  Task 11 (impl)             ─ depends on Task 10
  Task 12 (mock)             ─ depends on Task 10
  Task 13 (refactor MCP)     ─ depends on Tasks 11, 12
  Task 14 (contract tests)   ─ depends on Tasks 12, 13
  Task 15 (new tools)        ─ depends on Task 13
  Task 16 (stdio)            ─ depends on Task 13
  Task 17 (instructions)     ─ depends on Task 15
  Task 18 (verification)     ─ depends on all
```

### Phase 4 (Future — Not in This Plan)

Phase 4 (ProLink, multi-machine MCP, cloud proxy) is explicitly deferred. It requires:
- Stable MCP foundation (Phases 0-3)
- ProLink API maturity assessment
- Security model proven in production
- Separate design + implementation plan
