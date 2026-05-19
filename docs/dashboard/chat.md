---
description: "Talk to your hardware from the browser. Multi-agent chat via HTTP+SSE with tool-call traces, image-in-chat (Gemini Vision via grab_frame), and Ctrl-C-style turn cancellation."
---

# Chat Tab

The dashboard ships a Chat tab that talks to the same multi-agent runtime as `bubbaloop agent chat`, but over **HTTP+SSE** instead of Zenoh. The browser is a thin client — every LLM call runs daemon-side. No SDK or API key lives in the browser.

```
Browser              Daemon (agent runtime)            LLM provider
 Chat tab ── HTTP ──> /api/agent/* ── SSE ── AgentEvent stream
   │                       │
   │                       ├── agent loop (per-agent tokio task)
   │                       ├── tool dispatch (47 MCP + 43 internal tools)
   │                       └── ContentBlock::Image for vision turns
   │
   └── POST /cancel ─> abort the in-flight turn (same semantics as Ctrl-C)
```

## What you can do

- **Pick an agent.** The picker is populated from the daemon's per-agent `manifest` queryables. Unknown agent IDs surface a 404.
- **Pick a provider.** The status indicator shows which providers are configured. Gemini is the recommended default (free tier, strong vision); Claude works once `bubbaloop login` is done; Ollama works if `ollama serve` is running locally. If you choose a Claude OAuth subscription, the UI shows a risk warning — see the `bubbaloop login --status` docs.
- **Use tools.** Tool calls show as collapsible chips inline; the result follows. Tool-call traces help you debug what the agent actually did vs. what it said it did.
- **See images.** When the agent calls `grab_frame`, the daemon decodes the JPEG payload + JSON metadata attachment from the camera node and emits a `ContentBlock::Image` for the next LLM turn. The Chat tab renders the image inline so you can compare what the model saw vs. what it said.
- **Cancel a turn.** Press the cancel button (or just close the tab) — the dashboard posts to the per-agent cancel endpoint, which publishes to `bubbaloop/global/{machine_id}/agent/{agent_id}/cancel`. The daemon aborts the in-flight LLM call.

## Transport

- HTTP for request/control, SSE for the response stream. Connects to the daemon's MCP HTTP port (`8088` by default — override with `BUBBALOOP_MCP_PORT`).
- Bearer token auth: the token is read from `~/.bubbaloop/mcp-token` on the daemon side and is picked up automatically when the dashboard is served from the same host.
- The wire shapes mirror `AgentMessage` / `AgentEvent` from the Zenoh gateway. See [Concepts → Messaging](../concepts/messaging.md#agent-gateway-protocol).

## When to use Chat vs. `bubbaloop agent chat`

- **Chat tab** — best for visual workflows, multi-camera vision, remote (LAN) operators, anyone who already has the dashboard open.
- **`bubbaloop agent chat`** — best for scripting (pipe-friendly stdout), SSH sessions, and quick one-liners. Same agent, same memory, same tools.

## Related

- [`grab_frame` tool](../agent-guide.md#quick-reference) — the agent-side primitive that produces the image-in-chat entries
- [Agent Guide → Turn Cancellation](../agent-guide.md#interact-via-cli) — the cancel semantics in detail
- [Remote Access](remote-access.md) — exposing the dashboard (and Chat tab) over LAN with HTTPS
