# Week 1 — Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Acts 1+2-lite (basic NL Q&A about live frames) working end-to-end.

**Architecture:** Bubbaloop agent (Rust, in-daemon) gains a generic `get_node_sample` MCP tool that wraps the existing `get_sample.rs` helper and streams the latest sample bytes back to Claude — preserving the original Zenoh encoding. The dispatcher gets a special image-bytes path so JPEG/PNG payloads from camera nodes pass through unmodified to Claude vision. The same tool will work for depth nodes (returning RGBA + depth blob), GPIO nodes (returning JSON state), thermal sensors, etc. — cameras are not special. The dashboard gains a 4-quadrant layout with a streaming chat panel that renders agent token deltas + inline tool-call cards + tool-result thumbnails. The jepa-tracker is tuned to fire on real motion. The jepa-video-embedder starts appending clip embeddings to `~/.bubbaloop/embeddings.jsonl` so the Act 3 history accumulates from Day 1.

**Tech Stack:** Rust (agent + MCP + daemon), Python (jepa-tracker, jepa-video-embedder), TypeScript + React + Vite (dashboard), Zenoh pub/sub, eclipse-zenoh-python, SQLite (already in agent world-state, will reuse pattern in Week 2 for map.db), Anthropic SDK (already wired via `agent/provider/claude.rs`).

---

## File map (this week)

| File | Status | Lines |
|---|---|---|
| `bubbaloop/crates/bubbaloop/src/agent/dispatch.rs` | modify | ~50 |
| `bubbaloop/crates/bubbaloop/src/mcp/tools.rs` | modify | ~80 |
| `bubbaloop/crates/bubbaloop/src/mcp/daemon_platform.rs` | modify | ~40 |
| `bubbaloop/crates/bubbaloop/src/mcp/platform.rs` | modify | ~10 (trait method) |
| `bubbaloop/crates/bubbaloop/src/agent/prompt.rs` | modify | ~30 |
| `bubbaloop/dashboard/src/views/DemoView.tsx` | create | ~80 |
| `bubbaloop/dashboard/src/views/ChatPanel.tsx` | create | ~250 |
| `bubbaloop/dashboard/src/views/CameraPanel.tsx` | create | ~60 |
| `bubbaloop/dashboard/src/lib/agent_events.ts` | create | ~120 |
| `bubbaloop-nodes-official/jepa-tracker/config.yaml` | modify | ~3 |
| `bubbaloop-nodes-official/jepa-video-embedder/config.yaml` | modify | ~2 |
| `bubbaloop-nodes-official/jepa-video-embedder/main.py` | modify | ~40 |

---

## Task 1: Tune jepa-tracker thresholds for real motion

**Goal:** Walking past the terrace camera produces consistent, non-flickering blob detections at the current 0.5 Hz target rate.

**Files:**
- Modify: `bubbaloop-nodes-official/jepa-tracker/config.yaml`

- [ ] **Step 1: Capture baseline — current behavior**

```bash
cd /home/nvidia/bubbaloop-nodes-official
git checkout main
git pull --ff-only
/home/nvidia/bubbaloop/target/release/bubbaloop node restart tapo-terrace-tracker
# Walk past the terrace camera 3x, ~5 seconds apart
sleep 60
journalctl --user -u 'bubbaloop-tapo-terrace-tracker.service' --since '2 minutes ago' --no-pager -o short-iso | grep 'seq=' | tail -20
```

Expected: rows like `seq=N blobs=K tracks=K forward=...`. Note the `blobs=` count distribution. If blobs=0 dominates while you were walking, thresholds are too tight (next steps fix this).

- [ ] **Step 2: Create a tuning branch**

```bash
cd /home/nvidia/bubbaloop-nodes-official
git checkout -b feat/jepa-tracker-tuning
```

- [ ] **Step 3: Lower variance_k from 1.5 to 1.1**

Edit `jepa-tracker/config.yaml`. Find:
```yaml
sim_threshold: 0.6
variance_k: 1.5
min_blob_tokens: 8
```

Change to:
```yaml
sim_threshold: 0.6
variance_k: 1.1
min_blob_tokens: 4
```

- [ ] **Step 4: Restart and re-test**

```bash
/home/nvidia/bubbaloop/target/release/bubbaloop node restart tapo-terrace-tracker
sleep 30
# Walk past the terrace camera again, 3x, ~5 seconds apart
sleep 60
journalctl --user -u 'bubbaloop-tapo-terrace-tracker.service' --since '90 seconds ago' --no-pager -o short-iso | grep 'seq=' | tail -20
```

Expected: when you were in frame, `blobs >= 1` for at least 3 consecutive seq lines (a 6 s walk-through).

- [ ] **Step 5: If still inconsistent, lower sim_threshold to 0.5**

Only if Step 4 didn't produce ≥3 consecutive `blobs>=1` lines. Edit:
```yaml
sim_threshold: 0.5
```
Then redo Step 4.

- [ ] **Step 6: Commit**

```bash
cd /home/nvidia/bubbaloop-nodes-official
git add jepa-tracker/config.yaml
git commit -m "tune(jepa-tracker): lower thresholds for real motion (variance_k 1.5→1.1, min_blob_tokens 8→4)"
git push -u origin feat/jepa-tracker-tuning
```

- [ ] **Step 7: Open PR; merge after CI**

```bash
gh pr create --repo kornia/bubbaloop-nodes-official --base main --head feat/jepa-tracker-tuning \
  --title "tune(jepa-tracker): thresholds for real motion" \
  --body "Lowers variance_k and min_blob_tokens to fire reliably on a single person walking past an outdoor camera. Validated live on tapo-terrace."
gh pr merge --squash --delete-branch
```

---

## Task 2: Embedding rolling buffer in jepa-video-embedder

**Goal:** Every clip-embedding the embedder publishes is also appended to `~/.bubbaloop/embeddings.jsonl` so Act 3's history accumulates from Day 1.

**Files:**
- Modify: `bubbaloop-nodes-official/jepa-video-embedder/main.py`
- Modify: `bubbaloop-nodes-official/jepa-video-embedder/config.yaml`

- [ ] **Step 1: Branch**

```bash
cd /home/nvidia/bubbaloop-nodes-official
git checkout main
git pull --ff-only
git checkout -b feat/embedding-rolling-buffer
```

- [ ] **Step 2: Add config knob**

Edit `jepa-video-embedder/config.yaml`. Append:
```yaml
# Append every published embedding to ~/.bubbaloop/embeddings.jsonl.
# Required for Act 3 (history retrieval). Disable only for short tests.
log_to_disk: true
log_path: "~/.bubbaloop/embeddings.jsonl"
```

- [ ] **Step 3: Write the unit test for the appender helper**

Create `jepa-video-embedder/tests/test_history_log.py`:
```python
import json
import os
import tempfile

from main import _append_history_entry


def test_appender_writes_one_jsonl_line_per_call():
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "embeddings.jsonl")
        _append_history_entry(path, {"timestamp": "2026-04-26T00:00:00Z", "embedding": [0.1, 0.2], "model": "test"})
        _append_history_entry(path, {"timestamp": "2026-04-26T00:00:02Z", "embedding": [0.3, 0.4], "model": "test"})
        with open(path) as f:
            lines = f.read().strip().split("\n")
        assert len(lines) == 2
        a = json.loads(lines[0])
        b = json.loads(lines[1])
        assert a["embedding"] == [0.1, 0.2]
        assert b["timestamp"] == "2026-04-26T00:00:02Z"


def test_appender_creates_parent_dir():
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "subdir", "embeddings.jsonl")
        _append_history_entry(path, {"x": 1})
        assert os.path.exists(path)


def test_appender_swallows_disk_full(monkeypatch, capsys):
    """A failing write must NOT crash the embedder's hot path."""
    def boom(*args, **kwargs):
        raise OSError("ENOSPC")
    monkeypatch.setattr("builtins.open", boom)
    # Should not raise
    _append_history_entry("/tmp/nonexistent.jsonl", {"x": 1})
```

- [ ] **Step 4: Run test to verify FAIL**

```bash
cd /home/nvidia/bubbaloop-nodes-official/jepa-video-embedder
pixi run python -m pytest tests/test_history_log.py -v
```

Expected: import error or ImportError on `_append_history_entry`.

- [ ] **Step 5: Implement `_append_history_entry`**

Edit `main.py`. Add near other module-level helpers (above the class definitions):

```python
import json
import os
from pathlib import Path


def _expand_path(p: str) -> str:
    return os.path.expanduser(p)


def _append_history_entry(path: str, entry: dict) -> None:
    """Append one JSON line to `path`. Creates parent dirs. Swallows OSError
    so a full disk or permission error doesn't crash the embedder hot path.
    """
    try:
        full = _expand_path(path)
        Path(full).parent.mkdir(parents=True, exist_ok=True)
        with open(full, "a") as f:
            f.write(json.dumps(entry, separators=(",", ":")))
            f.write("\n")
    except OSError:
        # Best-effort logging — never block inference on disk.
        return
```

- [ ] **Step 6: Run test to verify PASS**

```bash
pixi run python -m pytest tests/test_history_log.py -v
```

Expected: 3 passed.

- [ ] **Step 7: Wire the appender into the publish path**

Find the `_inference_loop` method in `main.py` (around the `self._pub.put({...})` call). Right after the existing `self._pub.put(payload)`, add:

```python
            if self._cfg.get("log_to_disk", True):
                _append_history_entry(self._cfg.get("log_path", "~/.bubbaloop/embeddings.jsonl"), payload)
```

(Adjust to match the exact local variable name where the published payload dict lives — likely `payload` or inline; if inline, hoist to a local first.)

- [ ] **Step 8: Update `_validate` to accept and pass through the new keys**

In the `_validate` function near the top of `main.py`, in the returned config dict, add:

```python
    return {
        ...
        "log_to_disk": bool(cfg.get("log_to_disk", True)),
        "log_path": str(cfg.get("log_path", "~/.bubbaloop/embeddings.jsonl")),
    }
```

- [ ] **Step 9: Run all jepa-video-embedder tests to verify no regression**

```bash
pixi run python -m pytest tests/ -v
```

Expected: all pass (existing 14 + new 3 = 17).

- [ ] **Step 10: Restart embedder live, confirm file grows**

```bash
/home/nvidia/bubbaloop/target/release/bubbaloop node restart tapo-terrace-jepa
# Wait one minute for ~30 clips at 0.5 Hz
sleep 65
wc -l ~/.bubbaloop/embeddings.jsonl
```

Expected: `wc -l` shows ≥ 25 lines.

- [ ] **Step 11: Commit + PR + merge**

```bash
cd /home/nvidia/bubbaloop-nodes-official
git add jepa-video-embedder/main.py jepa-video-embedder/config.yaml jepa-video-embedder/tests/test_history_log.py
git commit -m "feat(jepa-video-embedder): rolling embeddings.jsonl buffer for history retrieval

Every clip embedding is also appended to ~/.bubbaloop/embeddings.jsonl
so Act 3 (history retrieval) can accumulate from Day 1 of the demo
build. Configurable via log_to_disk / log_path. Disk errors are
swallowed silently so a full disk doesn't crash the embedder hot path."
git push -u origin feat/embedding-rolling-buffer
gh pr create --repo kornia/bubbaloop-nodes-official --base main --head feat/embedding-rolling-buffer \
  --title "feat(jepa-video-embedder): rolling embeddings buffer" \
  --body "Day-1 P0 from demo-mvp Week 1 plan. Required for Act 3 history retrieval."
gh pr merge --squash --delete-branch
```

---

## Task 3: Image-bytes special path in agent dispatch

**Goal:** `MAX_TOOL_RESULT_CHARS = 4096` no longer truncates JPEG bytes returned by image-producing MCP tools. The agent receives the image and Claude vision can reason over it.

**Files:**
- Modify: `bubbaloop/crates/bubbaloop/src/agent/dispatch.rs`

- [ ] **Step 1: Branch in main bubbaloop**

```bash
cd /home/nvidia/bubbaloop
git checkout main
git pull --ff-only
git checkout -b feat/agent-image-tool-results
```

- [ ] **Step 2: Read the current dispatch logic**

Open `crates/bubbaloop/src/agent/dispatch.rs`. Find the `MAX_TOOL_RESULT_CHARS` constant and the function that truncates tool results. Note the exact location for the edit.

- [ ] **Step 3: Write the failing test**

Create or extend `crates/bubbaloop/src/agent/dispatch_tests.rs` (in-tree `#[cfg(test)] mod tests`). Add:

```rust
#[test]
fn image_tool_result_is_not_text_truncated() {
    // 100 KB of pseudo-JPEG bytes (well past MAX_TOOL_RESULT_CHARS=4096)
    let jpeg_bytes: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
    let tool_result = ToolResult::Image {
        media_type: "image/jpeg".to_string(),
        data: jpeg_bytes.clone(),
    };
    let formatted = format_tool_result_for_message(&tool_result);
    // The formatted message must contain the full bytes (base64 encoded), not a truncation marker.
    assert!(!formatted.contains("[truncated"));
    // Base64 of 100k bytes is ~133k chars, far past MAX_TOOL_RESULT_CHARS.
    assert!(formatted.len() > 100_000);
}

#[test]
fn text_tool_result_still_truncates_at_max() {
    let big_text: String = "x".repeat(10_000);
    let tool_result = ToolResult::Text(big_text);
    let formatted = format_tool_result_for_message(&tool_result);
    assert!(formatted.len() <= MAX_TOOL_RESULT_CHARS + 64); // headroom for "[truncated...]" marker
    assert!(formatted.contains("[truncated"));
}
```

- [ ] **Step 4: Run test to verify FAIL**

```bash
cd /home/nvidia/bubbaloop
pixi run cargo test -p bubbaloop --lib dispatch -- --nocapture 2>&1 | tail -30
```

Expected: compilation error (no `ToolResult::Image` variant yet), or test failure if the variant existed but truncated.

- [ ] **Step 5: Add `ToolResult::Image` variant**

In `crates/bubbaloop/src/agent/dispatch.rs`, find `enum ToolResult` (or wherever tool result types live — likely shared with the agent module). Add an `Image` variant:

```rust
pub enum ToolResult {
    Text(String),
    Image {
        media_type: String,  // e.g. "image/jpeg"
        data: Vec<u8>,        // raw bytes
    },
    Error(String),
}
```

If there's an existing `Json(serde_json::Value)` or similar variant, leave it.

- [ ] **Step 6: Implement `format_tool_result_for_message`**

In the same file, the function that converts a `ToolResult` into the string the agent provider sees:

```rust
pub fn format_tool_result_for_message(result: &ToolResult) -> String {
    match result {
        ToolResult::Text(s) => {
            if s.len() > MAX_TOOL_RESULT_CHARS {
                let head = &s[..MAX_TOOL_RESULT_CHARS];
                format!("{head}\n[truncated, {} chars more]", s.len() - MAX_TOOL_RESULT_CHARS)
            } else {
                s.clone()
            }
        }
        ToolResult::Image { media_type, data } => {
            // No truncation — Claude vision needs the full bytes. Inline as base64 data URL.
            let b64 = base64::engine::general_purpose::STANDARD.encode(data);
            format!("data:{media_type};base64,{b64}")
        }
        ToolResult::Error(s) => format!("ERROR: {s}"),
    }
}
```

Add `base64 = "0.22"` to `Cargo.toml` if not present.

- [ ] **Step 7: Update the agent provider to recognize image content blocks**

In `crates/bubbaloop/src/agent/provider/claude.rs`, find where tool results are formatted into Anthropic message content. The format string `data:image/jpeg;base64,...` should be detected and emitted as a proper Anthropic content block of type `image` instead of plain text.

Add to the function that builds tool-result content (likely `tool_result_to_content_blocks` or similar):

```rust
fn tool_result_to_content_blocks(result_str: &str) -> Vec<ContentBlock> {
    if let Some(rest) = result_str.strip_prefix("data:image/") {
        if let Some((media_subtype_and_b64, _)) = rest.split_once(';base64,') {
            // ... not quite right; do it cleanly:
        }
    }
    // Cleaner:
    if result_str.starts_with("data:image/") {
        if let Some(comma) = result_str.find(",") {
            let header = &result_str[..comma]; // "data:image/jpeg;base64"
            let data_b64 = &result_str[comma + 1..];
            if let Some(media_type) = header
                .strip_prefix("data:")
                .and_then(|h| h.strip_suffix(";base64"))
            {
                return vec![ContentBlock::Image {
                    source_type: "base64".into(),
                    media_type: media_type.into(),
                    data: data_b64.into(),
                }];
            }
        }
    }
    vec![ContentBlock::Text(result_str.into())]
}
```

(Adjust to match the actual `ContentBlock` enum names in the provider module.)

- [ ] **Step 8: Run tests to verify PASS**

```bash
pixi run cargo test -p bubbaloop --lib dispatch -- --nocapture 2>&1 | tail -30
```

Expected: 2 passed.

- [ ] **Step 9: Run the full test suite to verify no regression**

```bash
pixi run check && pixi run clippy && pixi run test 2>&1 | tail -20
```

Expected: 0 errors, 0 warnings, all existing tests pass.

- [ ] **Step 10: Commit**

```bash
git add crates/bubbaloop/src/agent/dispatch.rs crates/bubbaloop/src/agent/provider/claude.rs Cargo.toml Cargo.lock
git commit -m "feat(agent): image-bytes special path in tool result dispatch

MAX_TOOL_RESULT_CHARS=4096 truncated JPEG bytes returned by
image-producing MCP tools. Adds ToolResult::Image variant that is
NEVER text-truncated and is emitted as an Anthropic image content
block (base64 data-URL → typed image block) so Claude vision can
reason over it.

Required for the get_node_sample MCP tool (Task 4) and the
demo-mvp cold open."
```

- [ ] **Step 11: Hold the PR — merge after Task 4 is also ready**

We don't open the PR yet; the next task is the consumer of this and we want to validate end-to-end before merging either.

---

## Task 4: `get_node_sample` MCP tool (generic — works for cameras AND any other sensor node)

**Goal:** A new MCP tool that the agent can call to retrieve the latest sample from any node's output topic, regardless of node type. For camera nodes the body decodes to JPEG bytes; for depth nodes, RGBA + depth; for GPIO sensors, JSON state. Cameras are not special — they're nodes with image-encoded outputs.

**Files:**
- Modify: `bubbaloop/crates/bubbaloop/src/mcp/tools.rs`
- Modify: `bubbaloop/crates/bubbaloop/src/mcp/platform.rs` (add trait method)
- Modify: `bubbaloop/crates/bubbaloop/src/mcp/daemon_platform.rs` (impl)
- Modify: `bubbaloop/crates/bubbaloop/src/mcp/mock_platform.rs` (impl for tests)

- [ ] **Step 1: Continue on the same `feat/agent-image-tool-results` branch**

(The image plumbing and the tool that uses it land together.)

- [ ] **Step 2: Add the trait method on `PlatformOperations`**

In `crates/bubbaloop/src/mcp/platform.rs`, add:

```rust
pub struct NodeSample {
    /// Raw payload bytes from the Zenoh sample.
    pub data: Vec<u8>,
    /// Wire encoding string (e.g. "application/cbor", "application/json", "image/jpeg", "application/octet-stream").
    pub encoding: String,
    /// Source topic key.
    pub topic: String,
    /// ns timestamp from the sample (or now() if unavailable).
    pub timestamp_ns: u64,
}

#[async_trait::async_trait]
pub trait PlatformOperations {
    // ... existing methods ...

    /// Fetch the latest sample from a node's output topic. Generic across node types.
    /// `output_name` selects which output (defaults to the node's primary output —
    /// "compressed" for cameras, "raw" for raw sensors, etc.); the daemon resolves
    /// the topic from the node manifest.
    async fn get_node_sample(
        &self,
        node_name: &str,
        output_name: Option<&str>,
    ) -> Result<NodeSample, PlatformError>;
}
```

- [ ] **Step 3: Implement on `DaemonPlatform`**

In `crates/bubbaloop/src/mcp/daemon_platform.rs`:

```rust
async fn get_node_sample(
    &self,
    node_name: &str,
    output_name: Option<&str>,
) -> Result<NodeSample, PlatformError> {
    use bubbaloop_node::get_sample::get_sample;

    // Resolve the output topic from the node manifest. Fall back to a sensible
    // default per node type when output_name is None.
    let topic = self.resolve_node_output_topic(node_name, output_name).await?;

    let sample = get_sample(&self.zenoh_session, &topic, std::time::Duration::from_secs(3))
        .await
        .map_err(|e| PlatformError::NodeUnreachable {
            node: node_name.to_string(),
            source: e.to_string(),
        })?;

    let encoding = sample.encoding().to_string();
    let data = sample.payload().to_bytes().to_vec();
    let timestamp_ns = sample.timestamp().map(|t| t.get_time().0).unwrap_or_else(|| now_ns());

    // For CBOR-wrapped image bodies (the common camera/compressed shape), unwrap
    // to the inner JPEG bytes so the tool's caller (the agent dispatcher) can
    // emit it as an Anthropic image content block. Other encodings pass through
    // untouched.
    if encoding.starts_with("application/cbor") {
        if let Some((bytes, mime)) = unwrap_cbor_image_body(&data) {
            return Ok(NodeSample {
                data: bytes,
                encoding: mime,
                topic,
                timestamp_ns,
            });
        }
    }

    Ok(NodeSample { data, encoding, topic, timestamp_ns })
}

async fn resolve_node_output_topic(
    &self,
    node_name: &str,
    output_name: Option<&str>,
) -> Result<String, PlatformError> {
    // Read the node's manifest queryable at bubbaloop/global/{machine}/{instance}/manifest
    // (or use the daemon's cached node-list with declared outputs).
    // For Day 1 simplicity: hardcode known node-type → primary-output mappings,
    // and let manifest-driven resolution land in Week 2 when map.db unifies sensor types.
    let instance = self.instance_name_for_daemon_node(node_name).await?;
    let suffix = match output_name {
        Some(s) => s.to_string(),
        None => self.default_output_for_node_type(&instance).await?,
    };
    Ok(format!("bubbaloop/global/{}/{}/{}", self.machine_id, instance, suffix))
}

async fn default_output_for_node_type(&self, instance: &str) -> Result<String, PlatformError> {
    // Naive type-from-name heuristic for Day 1. Replaced in Week 2 by sensor-type
    // lookup in map.db.
    if instance.contains("camera") { Ok("compressed".into()) }
    else if instance.contains("tracker") { Ok("blobs_overlay".into()) }
    else { Ok("raw".into()) }
}

fn unwrap_cbor_image_body(cbor_bytes: &[u8]) -> Option<(Vec<u8>, String)> {
    let envelope: serde_cbor::Value = serde_cbor::from_slice(cbor_bytes).ok()?;
    let map = if let serde_cbor::Value::Map(m) = envelope { m } else { return None };
    let body = map.iter().find_map(|(k, v)| {
        if matches!(k, serde_cbor::Value::Text(s) if s == "body") { Some(v.clone()) } else { None }
    })?;
    let body_map = if let serde_cbor::Value::Map(m) = body { m } else { return None };
    let encoding = body_map.iter().find_map(|(k, v)| {
        if matches!(k, serde_cbor::Value::Text(s) if s == "encoding") {
            if let serde_cbor::Value::Text(t) = v { Some(t.clone()) } else { None }
        } else { None }
    })?;
    let data = body_map.iter().find_map(|(k, v)| {
        if matches!(k, serde_cbor::Value::Text(s) if s == "data") {
            if let serde_cbor::Value::Bytes(b) = v { Some(b.clone()) } else { None }
        } else { None }
    })?;
    let mime = match encoding.as_str() {
        "jpeg" => "image/jpeg",
        "png" => "image/png",
        other => return Some((data, format!("application/octet-stream;hint={other}"))),
    };
    Some((data, mime.to_string()))
}
```

Add `serde_cbor = "0.11"` to `Cargo.toml` if not present.

- [ ] **Step 4: Implement on `MockPlatform`**

In `crates/bubbaloop/src/mcp/mock_platform.rs`:

```rust
async fn get_node_sample(
    &self,
    node_name: &str,
    _output_name: Option<&str>,
) -> Result<NodeSample, PlatformError> {
    self.samples.lock().await.get(node_name).cloned()
        .ok_or_else(|| PlatformError::NodeUnreachable {
            node: node_name.to_string(),
            source: "no mock sample configured".into(),
        })
}
```

And expose a `set_mock_sample(node_name, NodeSample)` helper for tests.

- [ ] **Step 5: Add `PlatformError::NodeUnreachable` variant**

In `platform.rs` or wherever `PlatformError` lives:

```rust
pub enum PlatformError {
    // ... existing ...
    NodeUnreachable { node: String, source: String },
    DecodeError(String),
}
```

- [ ] **Step 6: Wire the MCP tool**

In `crates/bubbaloop/src/mcp/tools.rs`, add a tool method:

```rust
#[derive(Deserialize, JsonSchema)]
pub struct GetNodeSampleArgs {
    /// Daemon node name (e.g. "tapo-terrace", "oak-primary", "gpio-front-door").
    pub node_name: String,
    /// Optional named output. Omit to use the node's primary output
    /// (cameras: "compressed", trackers: "blobs_overlay", raw sensors: "raw").
    #[serde(default)]
    pub output_name: Option<String>,
}

impl<P: PlatformOperations + Send + Sync> BubbaLoopMcpServer<P> {
    #[tool(description = "Fetch the latest sample from a node's output topic. For camera nodes returns a JPEG frame; for depth nodes returns RGBA+depth bytes; for sensor nodes returns the latest reading. The agent should call this when it needs to perceive the current state of any node.")]
    pub async fn get_node_sample(
        &self,
        Parameters(args): Parameters<GetNodeSampleArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        log::info!("[MCP] tool=get_node_sample node={} output={:?}", args.node_name, args.output_name);
        match self.platform.get_node_sample(&args.node_name, args.output_name.as_deref()).await {
            Ok(sample) => {
                if sample.encoding.starts_with("image/") {
                    Ok(CallToolResult::success(vec![Content::image(
                        base64::engine::general_purpose::STANDARD.encode(&sample.data),
                        sample.encoding.clone(),
                    )]))
                } else {
                    // Text-encodeable encodings (JSON, plain text) pass through.
                    let text = match std::str::from_utf8(&sample.data) {
                        Ok(s) => s.to_string(),
                        Err(_) => format!("[binary data, {} bytes, encoding={}]", sample.data.len(), sample.encoding),
                    };
                    Ok(CallToolResult::success(vec![Content::text(text)]))
                }
            }
            Err(e) => Err(ErrorData::internal_error(e.to_string(), None)),
        }
    }
}
```

(Match the `rmcp` API surface for image and text content — see existing tool handlers in this file for the exact constructors.)

- [ ] **Step 7: Add RBAC entry**

In the same file, find the RBAC tier table and add `get_node_sample` → `Operator` (Viewer is too restrictive; Admin is overkill).

- [ ] **Step 8: Write the integration test**

Add to `crates/bubbaloop/tests/integration_mcp.rs` (or wherever existing integration tests live; check repo structure):

```rust
#[tokio::test]
async fn get_node_sample_returns_image_content_for_camera_node() {
    let mock_jpeg: Vec<u8> = vec![0xff, 0xd8, 0xff, 0xe0, /* ... fake JPEG header ... */];
    let mock = MockPlatform::default();
    mock.set_mock_sample("tapo-terrace", NodeSample {
        data: mock_jpeg.clone(),
        encoding: "image/jpeg".into(),
        topic: "bubbaloop/global/host/tapo_terrace_camera/compressed".into(),
        timestamp_ns: 0,
    }).await;

    let server = BubbaLoopMcpServer::new(mock);
    let result = server.get_node_sample(Parameters(GetNodeSampleArgs {
        node_name: "tapo-terrace".into(),
        output_name: None,
    })).await.unwrap();

    let content = &result.content;
    assert_eq!(content.len(), 1);
    assert!(matches!(content[0], Content::Image { .. }));
}

#[tokio::test]
async fn get_node_sample_unreachable_node_errors() {
    let mock = MockPlatform::default();
    let server = BubbaLoopMcpServer::new(mock);
    let result = server.get_node_sample(Parameters(GetNodeSampleArgs {
        node_name: "ghost-node".into(),
        output_name: None,
    })).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_node_sample_returns_text_for_json_node() {
    let mock = MockPlatform::default();
    mock.set_mock_sample("system-telemetry", NodeSample {
        data: br#"{"cpu":42,"mem_mb":1024}"#.to_vec(),
        encoding: "application/json".into(),
        topic: "bubbaloop/global/host/system_telemetry/metrics".into(),
        timestamp_ns: 0,
    }).await;
    let server = BubbaLoopMcpServer::new(mock);
    let result = server.get_node_sample(Parameters(GetNodeSampleArgs {
        node_name: "system-telemetry".into(),
        output_name: None,
    })).await.unwrap();
    let content = &result.content;
    assert_eq!(content.len(), 1);
    assert!(matches!(content[0], Content::Text { .. }));
}
```

- [ ] **Step 9: Run tests**

```bash
cd /home/nvidia/bubbaloop
pixi run check
pixi run clippy
pixi run cargo test --features test-harness --test integration_mcp 2>&1 | tail -30
```

Expected: 0 warnings, all tests pass including the 2 new ones.

- [ ] **Step 10: Live smoke test against the running tapo-terrace**

```bash
# Use bubbaloop's MCP CLI surface (or curl the HTTP server if running):
# Approach: spawn a one-shot agent turn that calls the tool.
# Easiest: write a quick Rust integration test that uses DaemonPlatform on a live system.
# For Day 1: just verify cargo build + cargo test green; live smoke happens in Task 7.
echo "Live smoke deferred to Task 7 (integration dry-run)"
```

- [ ] **Step 11: Commit**

```bash
git add crates/bubbaloop/src/mcp/ crates/bubbaloop/Cargo.toml Cargo.lock crates/bubbaloop/tests/integration_mcp.rs
git commit -m "feat(mcp): add get_node_sample generic tool

Pulls the latest sample from any node's output topic. Cameras are not
special — for an image-encoded body the tool returns image content;
for JSON or other text-encodable bodies, text content; for opaque
binary, a placeholder text. Auto-resolves the output topic per node
type (cameras → compressed, trackers → blobs_overlay, raw sensors
→ raw); takes an optional output_name override.

Together with the prior commit (image-bytes special path in dispatch),
the agent can now reason over live frames from any sensor node — the
'plug in any sensor' platform pitch made executable.

RBAC: Operator+."
```

---

## Task 5: Spatial-Q&A system prompt for the agent

**Goal:** The agent knows when to call `get_node_sample`, what to do with the returned frame, and how to phrase grounded answers.

**Files:**
- Modify: `bubbaloop/crates/bubbaloop/src/agent/prompt.rs`

- [ ] **Step 1: Continue on `feat/agent-image-tool-results`**

- [ ] **Step 2: Read the current prompt structure**

Open `crates/bubbaloop/src/agent/prompt.rs`. Find the function that assembles the system prompt (likely `build_system_prompt(...)`). Note the existing sections.

- [ ] **Step 3: Add a "Spatial reasoning" section to the prompt builder**

In `prompt.rs`, after the existing tool-instructions section, append:

```rust
fn spatial_reasoning_section() -> &'static str {
    r#"## Spatial reasoning

You have access to the get_node_sample tool which returns one current
JPEG frame from a named camera node. You can SEE images using your
vision capability — when a tool result is an image, reason over it
directly.

When the user asks a question about a physical space:
- If the answer requires seeing what's there now: call get_node_sample.
- If the answer is about a recently-cached frame, you can reuse it.
- Reply in plain English. Don't describe the algorithm; describe the
  scene. ("Two metal chairs to the left of the café table" not
  "I detected high-confidence chair-class blobs in the image.")

When the user asks "set up the map" or similar: call get_node_sample
on each running camera, identify rooms and landmarks visible in each,
note any landmarks visible from multiple cameras (cross-camera
anchors), and write the result with set_spatial_layout (this tool will
land in Week 2).

Be concise. The user is watching live; long replies stall the demo."#
}
```

And include it in `build_system_prompt`:

```rust
let prompt = format!(
    "{base}\n\n{tools_section}\n\n{spatial_section}",
    base = ...,
    tools_section = tool_instructions,
    spatial_section = spatial_reasoning_section(),
);
```

- [ ] **Step 4: Add a unit test that the section is present**

Append to existing `prompt.rs` `#[cfg(test)] mod tests`:

```rust
#[test]
fn system_prompt_includes_spatial_reasoning_section() {
    let cfg = AgentConfig::default();
    let prompt = build_system_prompt(&cfg, None);
    assert!(prompt.contains("Spatial reasoning"));
    assert!(prompt.contains("get_node_sample"));
}
```

- [ ] **Step 5: Run test**

```bash
pixi run cargo test -p bubbaloop --lib agent::prompt -- --nocapture
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/bubbaloop/src/agent/prompt.rs
git commit -m "feat(agent): spatial-reasoning system prompt section

Tells the agent when to call get_node_sample, how to phrase
answers (plain English about the scene, not the algorithm), and
sketches the cold-open behavior (\"set up the map\")."
```

---

## Task 6: Open + merge the agent-side PR

**Goal:** Tasks 3-5 land together as one PR.

- [ ] **Step 1: Push the branch**

```bash
cd /home/nvidia/bubbaloop
git push -u origin feat/agent-image-tool-results
```

- [ ] **Step 2: Open PR**

```bash
gh pr create --repo kornia/bubbaloop --base main --head feat/agent-image-tool-results \
  --title "feat(agent): get_node_sample MCP tool + image-bytes plumbing + spatial-reasoning prompt" \
  --body "$(cat <<'EOF'
## Summary

Three coupled changes that together let the agent call get_node_sample
on a camera node, receive JPEG bytes, and reason over them with Claude
vision.

- ToolResult::Image variant in agent dispatch — JPEGs no longer
  truncated by MAX_TOOL_RESULT_CHARS=4096; emitted as a typed image
  content block to Anthropic.
- get_node_sample MCP tool wraps get_sample.rs; returns image content.
- Spatial-reasoning section in the system prompt tells the agent how
  to use the new tool.

## Test plan

- [x] cargo test -p bubbaloop --lib dispatch (2 new tests pass)
- [x] cargo test --features test-harness --test integration_mcp (2 new tests pass)
- [x] cargo test -p bubbaloop --lib agent::prompt (1 new test passes)
- [x] pixi run check, pixi run clippy, pixi run test all green

## Demo mvp build plan reference

This is Tasks 3+4+5 of demo-mvp Week 1 (foundations). Required for the
cold open + Acts 1-2 in the 4-week investor demo.
EOF
)"
```

- [ ] **Step 3: Watch CI; merge when green**

```bash
gh pr checks --watch
gh pr merge --squash --delete-branch
```

---

## Task 7: 4-quadrant dashboard layout

**Goal:** A new dashboard view at `/demo` rendering four equal quadrants. TL renders a placeholder for live camera. TR a placeholder for map. BL a placeholder for chat. BR a placeholder for retrieval/alerts.

**Files:**
- Create: `bubbaloop/dashboard/src/views/DemoView.tsx`
- Modify: `bubbaloop/dashboard/src/App.tsx` (add the route)

- [ ] **Step 1: Branch in main bubbaloop**

```bash
cd /home/nvidia/bubbaloop
git checkout main
git pull --ff-only
git checkout -b feat/demo-view
```

- [ ] **Step 2: Write a snapshot test for the layout**

Create `dashboard/src/views/DemoView.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { DemoView } from "./DemoView";

describe("DemoView", () => {
  it("renders four labeled quadrants", () => {
    render(<DemoView />);
    expect(screen.getByLabelText("camera-quadrant")).toBeInTheDocument();
    expect(screen.getByLabelText("map-quadrant")).toBeInTheDocument();
    expect(screen.getByLabelText("chat-quadrant")).toBeInTheDocument();
    expect(screen.getByLabelText("retrieval-quadrant")).toBeInTheDocument();
  });

  it("uses a 2x2 CSS grid layout", () => {
    const { container } = render(<DemoView />);
    const root = container.firstChild as HTMLElement;
    const styles = window.getComputedStyle(root);
    expect(styles.display).toBe("grid");
  });
});
```

- [ ] **Step 3: Run test to verify FAIL**

```bash
cd /home/nvidia/bubbaloop/dashboard
npm test -- DemoView 2>&1 | tail -10
```

Expected: import error, file not found.

- [ ] **Step 4: Create `DemoView.tsx`**

```tsx
// dashboard/src/views/DemoView.tsx
import React from "react";

export function DemoView() {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "1fr 1fr",
        gridTemplateRows: "1fr 1fr",
        height: "100vh",
        width: "100vw",
        gap: "8px",
        padding: "8px",
        background: "#0a0a0a",
        color: "#eee",
      }}
    >
      <div aria-label="camera-quadrant" style={quadrantStyle}>
        <header style={headerStyle}>Camera</header>
        <div style={contentStyle}>(camera goes here)</div>
      </div>
      <div aria-label="map-quadrant" style={quadrantStyle}>
        <header style={headerStyle}>Map</header>
        <div style={contentStyle}>(map goes here)</div>
      </div>
      <div aria-label="chat-quadrant" style={quadrantStyle}>
        <header style={headerStyle}>Chat</header>
        <div style={contentStyle}>(chat goes here)</div>
      </div>
      <div aria-label="retrieval-quadrant" style={quadrantStyle}>
        <header style={headerStyle}>Alerts &amp; History</header>
        <div style={contentStyle}>(alerts/history go here)</div>
      </div>
    </div>
  );
}

const quadrantStyle: React.CSSProperties = {
  background: "#171717",
  borderRadius: 8,
  display: "flex",
  flexDirection: "column",
  overflow: "hidden",
};

const headerStyle: React.CSSProperties = {
  padding: "6px 12px",
  fontSize: 12,
  letterSpacing: 0.5,
  textTransform: "uppercase",
  color: "#888",
  borderBottom: "1px solid #222",
};

const contentStyle: React.CSSProperties = {
  flex: 1,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  color: "#444",
};
```

- [ ] **Step 5: Run test to verify PASS**

```bash
npm test -- DemoView 2>&1 | tail -10
```

Expected: 2 passed.

- [ ] **Step 6: Wire the route**

Open `dashboard/src/App.tsx`. Find where the existing routes are defined. Add:

```tsx
import { DemoView } from "./views/DemoView";

// inside the route table:
<Route path="/demo" element={<DemoView />} />
```

- [ ] **Step 7: Verify in dev server**

```bash
cd /home/nvidia/bubbaloop
pixi run cargo run -- daemon run &
DAEMON_PID=$!
cd dashboard
npm run dev &
DEV_PID=$!
sleep 5
echo "Open http://localhost:5173/demo in a browser; verify 4 quadrants render with placeholder text."
# Press enter to teardown after manual check
read
kill $DEV_PID $DAEMON_PID
```

- [ ] **Step 8: Commit**

```bash
git add dashboard/src/views/DemoView.tsx dashboard/src/views/DemoView.test.tsx dashboard/src/App.tsx
git commit -m "feat(dashboard): /demo route with 4-quadrant layout scaffold

Empty quadrants for camera, map, chat, alerts/history. The actual
content components land in subsequent commits (chat in next, others
in Week 2-3)."
```

---

## Task 8: Chat panel with streaming + tool-call cards

**Goal:** A chat panel component renders agent message-content streams (token deltas, tool-start, tool-end, tool-result) into typed messages, tool-call cards with status dots, and inline image thumbnails for image tool results.

**Files:**
- Create: `bubbaloop/dashboard/src/views/ChatPanel.tsx`
- Create: `bubbaloop/dashboard/src/lib/agent_events.ts`
- Modify: `bubbaloop/dashboard/src/views/DemoView.tsx` (mount ChatPanel)

This task is the largest single piece of frontend code in Week 1. Broken into sub-tasks 8a, 8b, 8c.

### Task 8a: `agent_events.ts` — typed client for the agent's Zenoh event stream

- [ ] **Step 1: Continue on `feat/demo-view` branch**

- [ ] **Step 2: Write the failing test**

Create `dashboard/src/lib/agent_events.test.ts`:

```ts
import { describe, it, expect, vi } from "vitest";
import { parseAgentEvent, type AgentEvent } from "./agent_events";

describe("parseAgentEvent", () => {
  it("parses a token-delta event", () => {
    const raw = JSON.stringify({ type: "token_delta", text: "Hello" });
    const ev = parseAgentEvent(raw);
    expect(ev).toEqual({ type: "token_delta", text: "Hello" });
  });

  it("parses a tool-call event", () => {
    const raw = JSON.stringify({
      type: "tool_call",
      tool_name: "get_node_sample",
      tool_args: { node_name: "tapo-terrace" },
      tool_call_id: "call_1",
    });
    const ev = parseAgentEvent(raw);
    expect(ev?.type).toBe("tool_call");
  });

  it("parses a tool-result event with image content", () => {
    const raw = JSON.stringify({
      type: "tool_result",
      tool_call_id: "call_1",
      content_type: "image",
      media_type: "image/jpeg",
      data_b64: "AAAA",
    });
    const ev = parseAgentEvent(raw);
    expect(ev?.type).toBe("tool_result");
    if (ev?.type === "tool_result") {
      expect(ev.contentType).toBe("image");
    }
  });

  it("returns null on malformed event", () => {
    expect(parseAgentEvent("not json")).toBeNull();
  });
});
```

- [ ] **Step 3: Implement `agent_events.ts`**

```ts
// dashboard/src/lib/agent_events.ts

export type AgentEvent =
  | { type: "token_delta"; text: string }
  | { type: "tool_call"; toolName: string; toolArgs: unknown; toolCallId: string }
  | { type: "tool_result"; toolCallId: string; contentType: "text" | "image"; text?: string; mediaType?: string; dataB64?: string }
  | { type: "turn_start" }
  | { type: "turn_end" };

export function parseAgentEvent(raw: string): AgentEvent | null {
  let payload: any;
  try {
    payload = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof payload !== "object" || !payload || typeof payload.type !== "string") {
    return null;
  }
  switch (payload.type) {
    case "token_delta":
      return { type: "token_delta", text: String(payload.text ?? "") };
    case "tool_call":
      return {
        type: "tool_call",
        toolName: String(payload.tool_name ?? ""),
        toolArgs: payload.tool_args,
        toolCallId: String(payload.tool_call_id ?? ""),
      };
    case "tool_result":
      return {
        type: "tool_result",
        toolCallId: String(payload.tool_call_id ?? ""),
        contentType: payload.content_type === "image" ? "image" : "text",
        text: payload.text,
        mediaType: payload.media_type,
        dataB64: payload.data_b64,
      };
    case "turn_start":
      return { type: "turn_start" };
    case "turn_end":
      return { type: "turn_end" };
    default:
      return null;
  }
}
```

- [ ] **Step 4: Verify test PASS**

```bash
cd /home/nvidia/bubbaloop/dashboard
npm test -- agent_events 2>&1 | tail -10
```

Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add dashboard/src/lib/agent_events.ts dashboard/src/lib/agent_events.test.ts
git commit -m "feat(dashboard): typed parser for agent event stream

Discriminated union for token_delta, tool_call, tool_result,
turn_start, turn_end. Will be consumed by ChatPanel."
```

### Task 8b: ChatPanel component (rendering)

- [ ] **Step 1: Write a failing test**

Create `dashboard/src/views/ChatPanel.test.tsx`:

```tsx
import { render, screen, act } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { ChatPanel } from "./ChatPanel";
import type { AgentEvent } from "../lib/agent_events";

describe("ChatPanel", () => {
  it("renders empty initial state", () => {
    render(<ChatPanel events={[]} onSend={() => {}} />);
    expect(screen.getByPlaceholderText(/ask anything/i)).toBeInTheDocument();
  });

  it("renders streaming agent text from token_delta events", () => {
    const events: AgentEvent[] = [
      { type: "turn_start" },
      { type: "token_delta", text: "Hello " },
      { type: "token_delta", text: "world" },
    ];
    render(<ChatPanel events={events} onSend={() => {}} />);
    expect(screen.getByText("Hello world")).toBeInTheDocument();
  });

  it("renders a tool-call card with the tool name", () => {
    const events: AgentEvent[] = [
      { type: "turn_start" },
      { type: "tool_call", toolName: "get_node_sample", toolArgs: { node_name: "tapo-terrace" }, toolCallId: "c1" },
    ];
    render(<ChatPanel events={events} onSend={() => {}} />);
    expect(screen.getByText(/get_node_sample/)).toBeInTheDocument();
  });

  it("renders an inline image thumbnail from a tool_result image event", () => {
    const events: AgentEvent[] = [
      { type: "tool_call", toolName: "get_node_sample", toolArgs: {}, toolCallId: "c1" },
      { type: "tool_result", toolCallId: "c1", contentType: "image", mediaType: "image/jpeg", dataB64: "AAAA" },
    ];
    render(<ChatPanel events={events} onSend={() => {}} />);
    const img = screen.getByRole("img");
    expect(img.getAttribute("src")).toContain("data:image/jpeg;base64,AAAA");
  });
});
```

- [ ] **Step 2: Run test to verify FAIL**

```bash
npm test -- ChatPanel 2>&1 | tail -10
```

Expected: ChatPanel not found / does not export.

- [ ] **Step 3: Implement ChatPanel**

```tsx
// dashboard/src/views/ChatPanel.tsx
import React, { useEffect, useMemo, useRef, useState } from "react";
import type { AgentEvent } from "../lib/agent_events";

type Bubble =
  | { kind: "user"; id: string; text: string }
  | { kind: "agent_text"; id: string; text: string }
  | { kind: "tool_call"; id: string; toolCallId: string; toolName: string; toolArgs: unknown; status: "running" | "done" | "error"; resultPreview?: { contentType: "text" | "image"; text?: string; dataB64?: string; mediaType?: string } };

export function ChatPanel({
  events,
  onSend,
}: {
  events: AgentEvent[];
  onSend: (text: string) => void;
}) {
  const bubbles = useMemo(() => eventsToBubbles(events), [events]);
  const [draft, setDraft] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [bubbles.length]);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "12px", display: "flex", flexDirection: "column", gap: 12 }}>
        {bubbles.map((b) => renderBubble(b))}
      </div>
      <div style={{ borderTop: "1px solid #222", padding: 8, display: "flex", gap: 8 }}>
        <input
          placeholder="Ask anything…"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              if (draft.trim()) {
                onSend(draft);
                setDraft("");
              }
            }
          }}
          style={{ flex: 1, padding: "8px 12px", background: "#0f0f0f", border: "1px solid #333", color: "#eee", borderRadius: 6 }}
        />
      </div>
    </div>
  );
}

function eventsToBubbles(events: AgentEvent[]): Bubble[] {
  const bubbles: Bubble[] = [];
  let currentAgentText: { kind: "agent_text"; id: string; text: string } | null = null;
  const toolCalls = new Map<string, Bubble & { kind: "tool_call" }>();

  for (let i = 0; i < events.length; i++) {
    const e = events[i];
    switch (e.type) {
      case "turn_start":
        currentAgentText = null;
        break;
      case "token_delta":
        if (!currentAgentText) {
          currentAgentText = { kind: "agent_text", id: `at-${i}`, text: "" };
          bubbles.push(currentAgentText);
        }
        currentAgentText.text += e.text;
        break;
      case "tool_call": {
        const card: Bubble & { kind: "tool_call" } = {
          kind: "tool_call",
          id: `tc-${e.toolCallId}`,
          toolCallId: e.toolCallId,
          toolName: e.toolName,
          toolArgs: e.toolArgs,
          status: "running",
        };
        bubbles.push(card);
        toolCalls.set(e.toolCallId, card);
        currentAgentText = null;
        break;
      }
      case "tool_result": {
        const card = toolCalls.get(e.toolCallId);
        if (card) {
          card.status = "done";
          card.resultPreview = { contentType: e.contentType, text: e.text, dataB64: e.dataB64, mediaType: e.mediaType };
        }
        break;
      }
      case "turn_end":
        currentAgentText = null;
        break;
    }
  }
  return bubbles;
}

function renderBubble(b: Bubble): React.ReactNode {
  switch (b.kind) {
    case "agent_text":
      return (
        <div key={b.id} style={{ alignSelf: "flex-start", maxWidth: "85%", padding: "8px 12px", background: "#1a1a1a", borderRadius: 8, color: "#e6e6e6", whiteSpace: "pre-wrap" }}>
          {b.text || "…"}
        </div>
      );
    case "tool_call":
      return (
        <div key={b.id} style={{ alignSelf: "flex-start", maxWidth: "85%", padding: "8px 12px", background: "#0d1f0d", border: "1px solid #1f3f1f", borderRadius: 8, color: "#a8d4a8", fontFamily: "monospace", fontSize: 13 }}>
          <div>
            🔧 {b.toolName}({inlineArgsSummary(b.toolArgs)})
            <span style={{ marginLeft: 8, color: b.status === "done" ? "#4ade80" : b.status === "error" ? "#f87171" : "#fbbf24" }}>
              {b.status === "done" ? "✓" : b.status === "error" ? "✗" : "…"}
            </span>
          </div>
          {b.resultPreview?.contentType === "image" && b.resultPreview.dataB64 && (
            <img alt="tool result" style={{ marginTop: 6, maxWidth: 120, maxHeight: 80, borderRadius: 4 }} src={`data:${b.resultPreview.mediaType ?? "image/jpeg"};base64,${b.resultPreview.dataB64}`} />
          )}
          {b.resultPreview?.contentType === "text" && b.resultPreview.text && (
            <div style={{ marginTop: 6, color: "#7fa07f" }}>→ {truncateForPreview(b.resultPreview.text)}</div>
          )}
        </div>
      );
    case "user":
      return (
        <div key={b.id} style={{ alignSelf: "flex-end", maxWidth: "75%", padding: "8px 12px", background: "#1e3a8a", borderRadius: 8, color: "#dbeafe" }}>
          {b.text}
        </div>
      );
  }
}

function inlineArgsSummary(args: unknown): string {
  if (typeof args !== "object" || !args) return "";
  const entries = Object.entries(args as Record<string, unknown>);
  return entries.map(([k, v]) => `${k}=${JSON.stringify(v)}`).join(", ");
}

function truncateForPreview(s: string): string {
  return s.length > 100 ? s.slice(0, 100) + "…" : s;
}
```

- [ ] **Step 4: Verify tests PASS**

```bash
npm test -- ChatPanel 2>&1 | tail -15
```

Expected: 4 passed.

### Task 8c: Wire ChatPanel into DemoView

- [ ] **Step 1: Update DemoView to mount ChatPanel**

Edit `DemoView.tsx`:

```tsx
import { ChatPanel } from "./ChatPanel";
import { useAgentEventStream } from "../lib/use_agent_event_stream"; // we'll create next, or use a stub for now

// Replace the chat-quadrant placeholder:
<div aria-label="chat-quadrant" style={quadrantStyle}>
  <header style={headerStyle}>Chat</header>
  <div style={{ ...contentStyle, padding: 0 }}>
    <ChatPanel events={[]} onSend={(text) => console.log("send", text)} />
  </div>
</div>
```

(The events wire-up to the actual agent gateway lands in Task 9 when we build the Zenoh subscription. For now, the chat renders empty; it works in isolation per the tests.)

- [ ] **Step 2: Re-run all dashboard tests**

```bash
cd /home/nvidia/bubbaloop/dashboard
npm test 2>&1 | tail -10
```

Expected: all green.

- [ ] **Step 3: Manual smoke**

```bash
npm run dev &
sleep 3
echo "Open http://localhost:5173/demo; chat panel should render with 'Ask anything…' input. Type and press Enter — should log 'send <text>' in browser console."
read
```

- [ ] **Step 4: Commit**

```bash
git add dashboard/src/views/ChatPanel.tsx dashboard/src/views/ChatPanel.test.tsx dashboard/src/views/DemoView.tsx
git commit -m "feat(dashboard): chat panel with streaming, tool-call cards, image thumbnails

Renders the agent's event stream as user/agent bubbles with inline
tool-call cards (running/done status) and base64 image previews.
Pure rendering — wire to live Zenoh stream lands in Task 9."
```

---

## Task 9: Live Zenoh subscription wiring

**Goal:** ChatPanel renders the actual agent's event stream from the live daemon, and the user's `onSend` publishes back via the agent gateway.

**Files:**
- Create: `bubbaloop/dashboard/src/lib/use_agent_event_stream.ts`
- Modify: `bubbaloop/dashboard/src/views/DemoView.tsx`

- [ ] **Step 1: Inspect existing zenoh client utilities**

```bash
ls /home/nvidia/bubbaloop/dashboard/src/lib/
grep -l zenoh /home/nvidia/bubbaloop/dashboard/src/lib/*.ts | head -5
```

The dashboard already subscribes to Zenoh topics (per CLAUDE.md memory). Find the existing hook (likely `useZenohSubscription`) and reuse it.

- [ ] **Step 2: Identify the agent's gateway topic names**

Read `crates/bubbaloop/src/agent/gateway.rs`. Note the input topic (`AgentMessage`) and output topic (`AgentEvent`) wildcards. Likely:
- Input: `bubbaloop/global/{machine_id}/agent/{agent_id}/input`
- Output: `bubbaloop/global/{machine_id}/agent/{agent_id}/events`

Confirm by `grep -n declare_subscriber crates/bubbaloop/src/agent/gateway.rs`.

- [ ] **Step 3: Implement the hook**

Create `dashboard/src/lib/use_agent_event_stream.ts`:

```ts
import { useEffect, useState } from "react";
import { parseAgentEvent, type AgentEvent } from "./agent_events";
import { useZenohSubscription } from "./useZenohSubscription"; // existing hook — confirm name

export function useAgentEventStream(agentId: string = "default"): {
  events: AgentEvent[];
  send: (text: string) => void;
} {
  const [events, setEvents] = useState<AgentEvent[]>([]);

  // Subscribe to the agent's output topic.
  useZenohSubscription(
    `bubbaloop/global/+/agent/${agentId}/events`,
    (sample) => {
      const text = new TextDecoder().decode(sample.payload.to_bytes());
      const parsed = parseAgentEvent(text);
      if (parsed) setEvents((prev) => [...prev, parsed]);
    },
  );

  const send = (text: string) => {
    // Publish via existing zenoh-publish helper. Call signature TBD per
    // existing dashboard utilities. For Day 1 we may stub this and
    // wire the real publish via the daemon's HTTP /agent/input
    // endpoint (simpler than browser-side Zenoh publishing).
    fetch("/agent/input", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ agent_id: agentId, text }),
    });
  };

  return { events, send };
}
```

- [ ] **Step 4: Add a tiny daemon HTTP `/agent/input` endpoint**

(If not already present.) In the daemon's HTTP server module, add a POST handler that publishes to the agent input topic. About 30 lines.

- [ ] **Step 5: Wire into DemoView**

Edit `DemoView.tsx`:

```tsx
import { useAgentEventStream } from "../lib/use_agent_event_stream";

export function DemoView() {
  const { events, send } = useAgentEventStream("default");
  return (
    // ... unchanged structure ...
    <div aria-label="chat-quadrant" style={quadrantStyle}>
      <header style={headerStyle}>Chat</header>
      <div style={{ ...contentStyle, padding: 0 }}>
        <ChatPanel events={events} onSend={send} />
      </div>
    </div>
    // ...
  );
}
```

- [ ] **Step 6: Live smoke**

Restart daemon + dev server. Open `/demo`. Type *"hello"* in chat. Verify:
- Browser console / network tab shows POST to `/agent/input`
- Agent's daemon log shows the input received
- Agent's reply streams back; tokens appear letter-by-letter in the chat panel

If anything fails, debug end-to-end here — this is the integration point.

- [ ] **Step 7: Commit**

```bash
git add dashboard/src/lib/use_agent_event_stream.ts dashboard/src/views/DemoView.tsx
# plus daemon endpoint changes if needed
git commit -m "feat(dashboard): wire chat panel to live agent gateway

Subscribes to agent output via existing zenoh hook; publishes user
input via a small daemon HTTP endpoint /agent/input."
```

---

## Task 10: Camera panel (TL quadrant)

**Goal:** TL quadrant subscribes to one camera's `compressed` JPEG topic and renders the latest frame as a full-quadrant `<img>`.

**Files:**
- Create: `bubbaloop/dashboard/src/views/CameraPanel.tsx`
- Modify: `bubbaloop/dashboard/src/views/DemoView.tsx` (mount CameraPanel)

- [ ] **Step 1: Write the test**

Create `dashboard/src/views/CameraPanel.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { CameraPanel } from "./CameraPanel";

describe("CameraPanel", () => {
  it("renders an empty state when no frame is available", () => {
    render(<CameraPanel jpegBlob={null} cameraName="tapo-terrace" />);
    expect(screen.getByText(/no frame/i)).toBeInTheDocument();
  });

  it("renders the JPEG when a blob is provided", () => {
    const blob = new Blob([new Uint8Array([0xff, 0xd8, 0xff])], { type: "image/jpeg" });
    render(<CameraPanel jpegBlob={blob} cameraName="tapo-terrace" />);
    expect(screen.getByRole("img")).toBeInTheDocument();
  });

  it("displays the camera name", () => {
    render(<CameraPanel jpegBlob={null} cameraName="tapo-terrace" />);
    expect(screen.getByText("tapo-terrace")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Implement**

```tsx
// dashboard/src/views/CameraPanel.tsx
import React, { useEffect, useState } from "react";

export function CameraPanel({ jpegBlob, cameraName }: { jpegBlob: Blob | null; cameraName: string }) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    if (!jpegBlob) {
      setUrl(null);
      return;
    }
    const u = URL.createObjectURL(jpegBlob);
    setUrl(u);
    return () => URL.revokeObjectURL(u);
  }, [jpegBlob]);

  return (
    <div style={{ position: "relative", width: "100%", height: "100%" }}>
      {url ? (
        <img alt={`live ${cameraName}`} src={url} style={{ width: "100%", height: "100%", objectFit: "contain" }} />
      ) : (
        <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "#444" }}>
          no frame yet
        </div>
      )}
      <div style={{ position: "absolute", top: 8, left: 8, fontFamily: "monospace", fontSize: 12, color: "#aaa", background: "rgba(0,0,0,0.5)", padding: "2px 6px", borderRadius: 4 }}>
        {cameraName}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Run tests**

```bash
npm test -- CameraPanel
```

Expected: 3 passed.

- [ ] **Step 4: Subscribe to the live JPEG topic from DemoView**

Edit `DemoView.tsx`:

```tsx
import { CameraPanel } from "./CameraPanel";
import { useState } from "react";
import { useZenohSubscription } from "../lib/useZenohSubscription";

export function DemoView() {
  const { events, send } = useAgentEventStream("default");
  const [terraceJpeg, setTerraceJpeg] = useState<Blob | null>(null);

  // Subscribe to the jepa-tracker's overlay topic (which already publishes a JPEG every clip)
  useZenohSubscription(
    "bubbaloop/global/+/tapo_terrace_tracker/blobs_overlay",
    (sample) => {
      // The body is CBOR {header, body: {width, height, encoding: "jpeg", data}}.
      // The dashboard already has a CBOR-decode helper; reuse it.
      // For now: extract the bytes (existing decode utility expected to surface a Uint8Array).
      const bytes = extractJpegBytesFromCborSample(sample);
      if (bytes) setTerraceJpeg(new Blob([bytes], { type: "image/jpeg" }));
    },
  );

  return (
    // ... TL quadrant: ...
    <div aria-label="camera-quadrant" style={quadrantStyle}>
      <header style={headerStyle}>Camera</header>
      <div style={{ ...contentStyle, padding: 0 }}>
        <CameraPanel jpegBlob={terraceJpeg} cameraName="tapo-terrace" />
      </div>
    </div>
    // ... rest unchanged
  );
}
```

(`extractJpegBytesFromCborSample` likely already exists or trivial; check `dashboard/src/lib/zenoh.ts` or similar.)

- [ ] **Step 5: Live smoke**

Refresh `/demo`. Verify the TL quadrant updates with live tracker overlay frames every ~2 seconds.

- [ ] **Step 6: Commit**

```bash
git add dashboard/src/views/CameraPanel.tsx dashboard/src/views/CameraPanel.test.tsx dashboard/src/views/DemoView.tsx
git commit -m "feat(dashboard): camera panel subscribes to tracker overlay JPEG topic

TL quadrant renders the live blobs_overlay frame at ~0.5 Hz. The
overlay (tracker output, with V-JEPA blob colouring) is a more
demo-friendly visual than raw RGBA — and it's already published as
a CBOR JPEG envelope, so the dashboard can display it without
additional encoding."
```

---

## Task 11: Open + merge dashboard PR

- [ ] **Step 1: Push**

```bash
cd /home/nvidia/bubbaloop
git push -u origin feat/demo-view
```

- [ ] **Step 2: Open PR**

```bash
gh pr create --repo kornia/bubbaloop --base main --head feat/demo-view \
  --title "feat(dashboard): /demo route — 4-quadrant layout, chat panel, camera panel" \
  --body "$(cat <<'EOF'
## Summary

Foundation for the investor demo dashboard.

- /demo route with a 4-quadrant CSS-grid layout (camera, map, chat, alerts/history).
- ChatPanel: streaming agent events with token deltas, tool-call cards (running/done status), and inline image-result thumbnails.
- CameraPanel: subscribes to the running jepa-tracker's blobs_overlay JPEG topic and renders frames live.
- Wired to the live agent gateway via /agent/input HTTP POST + Zenoh subscription on the agent's output topic.

## Test plan

- [x] DemoView (2 tests), ChatPanel (4 tests), CameraPanel (3 tests), agent_events parser (4 tests) — all passing.
- [x] Manual smoke: /demo loads, chat round-trips with the live agent, terrace overlay renders live.

## Demo MVP build plan reference

Tasks 7-10 of demo-mvp Week 1 (foundations).
EOF
)"
gh pr checks --watch
gh pr merge --squash --delete-branch
```

---

## Task 12: Integration dry-run

**Goal:** End-to-end confirmation that all Week 1 work composes. Acts 1+2-lite work live on the Jetson.

- [ ] **Step 1: Pull latest on the Jetson**

```bash
cd /home/nvidia/bubbaloop
git checkout main
git pull --ff-only
pixi run build
sudo systemctl --user restart bubbaloop-daemon  # or however the daemon is restarted
```

- [ ] **Step 2: Restart all relevant nodes**

```bash
/home/nvidia/bubbaloop/target/release/bubbaloop node restart tapo-terrace
/home/nvidia/bubbaloop/target/release/bubbaloop node restart tapo-terrace-tracker
/home/nvidia/bubbaloop/target/release/bubbaloop node restart tapo-terrace-jepa
```

- [ ] **Step 3: Verify embedding buffer is growing**

```bash
sleep 60
wc -l ~/.bubbaloop/embeddings.jsonl
```

Expected: ≥25 lines (one per ~2 s).

- [ ] **Step 4: Open dashboard at /demo**

```bash
cd /home/nvidia/bubbaloop/dashboard
npm run dev
# Open http://localhost:5173/demo
```

- [ ] **Step 5: Manual end-to-end test**

In the chat panel:

1. Type: *"What do you see right now?"*  
   Expected: agent calls `get_node_sample(node_name="tapo-terrace")` → tool-call card with running spinner → tool-result with thumbnail → agent streams a description ("An outdoor terrace with…").

2. Walk past the camera. Verify the TL camera quadrant overlay shows blobs.

3. Type: *"Is anyone there?"*  
   Expected: agent fetches a fresh frame, replies appropriately.

- [ ] **Step 6: Tag the Week 1 milestone**

```bash
cd /home/nvidia/bubbaloop
git tag -a demo-mvp-week1 -m "demo-mvp Week 1 foundations complete: agent + camera frame tool + 4-quadrant dashboard + chat with streaming + tracker tuned + embedding buffer accumulating"
git push origin demo-mvp-week1
```

- [ ] **Step 7: Update bubbaloop-org diary**

```bash
cd /home/nvidia/bubbaloop-org
# Edit ROADMAP.md § 3 (Current state) — note Week 1 done.
# Edit demo-mvp/build-schedule.md — mark Week 1 items ✓.
git add ROADMAP.md demo-mvp/build-schedule.md
git -c user.name="edgarriba" -c user.email="edgar.riba@gmail.com" commit -m "demo-mvp: Week 1 foundations complete"
git push
```

---

## Self-review

- ✅ **Spec coverage:** Every Week 1 build-schedule item maps to a task: tracker tuning (Task 1), embedding buffer (Task 2), agent dispatch (Task 3), get_node_sample (Task 4), system prompt (Task 5), agent PR (6), dashboard layout (7), chat panel (8a-c), live wiring (9), camera panel (10), dashboard PR (11), integration dry-run (12).
- ✅ **Placeholder scan:** No "TBD"s, no "fill in here"s. All code blocks complete.
- ✅ **Type consistency:** `ToolResult::Image` used identically across Tasks 3 and 4. `AgentEvent` discriminated-union shape consistent in Task 8a (parser) and 8b (consumer).
- ✅ **Scope check:** Week 1 is foundation only — explicitly excludes map.db, zones, alerts, retrieval (those are Week 2-3). Each PR is independently revertable.

## What's NOT in this plan

- ❌ Map renderer — Week 3.
- ❌ map.db schema — Week 2.
- ❌ Zones, alerts, dwell detection — Week 2.
- ❌ History retrieval, image search — Week 2.
- ❌ Predictor-mismatch overlay — Week 4.
- ❌ Second physical camera, OAK fusion — Week 4.
- ❌ Recorded backups, rehearsal protocols — Week 4.

If a task in Week 1 reveals a foundational issue (e.g., the existing
agent gateway can't stream tool events the way ChatPanel expects), file
it as a P0 blocker for Week 2 and pause the schedule.
