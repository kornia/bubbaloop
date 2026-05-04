# Using bubbaloop with Ollama

The bubbaloop MCP server is **model-agnostic** — it speaks the protocol, not Anthropic's API. You can drive it with Ollama (Llama 3, Mistral, Qwen, DeepSeek, …) in two well-tested ways.

## Path A — Native: bubbaloop's internal agent uses Ollama

The cleanest path. The agent runtime inside the bubbaloop daemon (`bubbaloop up`) supports Ollama as a first-class provider. No bridge, no extra process.

Edit `~/.bubbaloop/agents.toml`:

```toml
[agents.jean-clawd]
enabled = true
default = true
provider = "ollama"
model = "llama3.1:70b"   # any model you have pulled in Ollama

# Optional: point at a non-default Ollama endpoint
# endpoint = "http://192.168.1.10:11434"
```

Then:

```bash
ollama pull llama3.1:70b
bubbaloop up
bubbaloop agent chat "what is the system status?"
```

The agent reasons with Llama and dispatches every MCP tool through the same internal path it uses for Claude. **All 47 tools are available — no subset, no compromise.**

### Tuning for smaller models

Smaller open models (≤ 13B params) get easily confused by long tool lists. Two knobs in `~/.bubbaloop/agents/{agent_id}/soul/capabilities.toml`:

```toml
compaction_flush_threshold_tokens = 2000
heartbeat_base_interval = 30
```

If you see the model invent tool names or hallucinate parameters, use a model with native function-calling support (Llama 3.1 Instruct, Qwen 2.5 Coder, Hermes 3) — base models without tool-use fine-tuning struggle with MCP-style schemas.

## Path B — External: your own Ollama setup talks to bubbaloop's MCP

For when you have an existing Ollama UI / agent stack and want to hand it the bubbaloop tools as a remote tool source. Use any Ollama↔MCP bridge:

| Bridge | Form | Use when |
|---|---|---|
| **`mcphost`** ([Mark3 Labs](https://github.com/mark3labs/mcphost)) | CLI | Drop-in, closest to "just works". Supports Ollama, Anthropic, OpenAI. |
| **`ollama-mcp-bridge`** ([patruff](https://github.com/patruff/ollama-mcp-bridge)) | Node | Minimal, easy to extend. |
| **`Tome`** ([runebookai](https://github.com/runebookai/tome)) | Desktop GUI | If you want a chat UI rather than a CLI. |
| **LangChain** Ollama + MCP | Library | When you need to compose with other agent logic. |

All of them read the same `.mcp.json` shape we ship in this plugin — including the `Authorization: Bearer ${BUBBALOOP_MCP_TOKEN}` header. Copy `bubbaloop-mcp/.mcp.json` into your bridge's config and export the token:

```bash
export BUBBALOOP_MCP_TOKEN="$(cat ~/.bubbaloop/mcp-token)"
```

### Example — `mcphost` with Ollama

```bash
go install github.com/mark3labs/mcphost@latest
mcphost --config bubbaloop-mcp/.mcp.json --provider ollama --model llama3.1:70b
```

You'll get an interactive shell where Llama can call any of bubbaloop's 47 tools. The bridge converts MCP `inputSchema` → Ollama's OpenAI-compatible `function_calling` format on the fly.

## What does NOT transfer

The plugin's higher-level Claude Code assets are CC-specific:

- **Slash commands** (`/bubbaloop:status`, `/bubbaloop:diagnose <node>`, …) — Ollama has no slash-command concept. *Workaround:* the body of each command's `.md` is just a paragraph telling the model which tools to call in which order — paste it into your Ollama system prompt.
- **`bubbaloop-operator` subagent** — Claude Code's `Agent` tool dispatch. *Workaround:* paste the agent's body into the Ollama system prompt to get the same operating principles.
- **`using-bubbaloop` skill** — auto-loading is a Claude Code feature. *Workaround:* the `SKILL.md` content is plain markdown — copy the "two-plane model" and "RBAC tiers" sections into your Ollama system prompt. **The two-plane rule (MCP control / Zenoh data) is even more important on small local models** than on Claude — context windows are smaller and polling fills them faster.

## Recommended Ollama-friendly defaults

1. **Path A first.** Native dispatch is faster, more reliable, and exposes all 47 tools.
2. **Llama 3.1 70B Instruct or Qwen 2.5 72B Instruct** for production. Smaller models work for read-only Viewer-tier tools but struggle on multi-step Operator/Admin workflows.
3. **Pin the model.** `model = "llama3.1:70b"` not `model = "llama3.1:latest"` — the `:latest` tag changes silently.
4. **Disable Admin-tier tools by default.** The cleanest way is now `BUBBALOOP_MCP_MODE=safe` (or `readonly` for inspection-only) — server rejects destructive tools at the boundary, no prompt-engineering required. Old prompt-layer gating still works as a backup but is no longer the primary defense.

## Notes on the tool-naming PR

Tool names follow GitHub MCP's `noun_verb` convention (`node_list`, `mission_pause`, `belief_update`, etc.). Smaller open models pick the right tool noticeably more often with the namespaced layout — that's part of why the rename happened, and a side benefit of using bubbaloop with Ollama specifically.

## Sources

- [Ollama function calling](https://github.com/ollama/ollama/blob/main/docs/api.md#function-calling)
- [mcphost (Mark3 Labs)](https://github.com/mark3labs/mcphost)
- [ollama-mcp-bridge (patruff)](https://github.com/patruff/ollama-mcp-bridge)
- [Tome desktop](https://github.com/runebookai/tome)
- [LangChain MCP integration](https://python.langchain.com/docs/integrations/tools/mcp/)
