# Production Roadmap — bubbaloop as the de facto edge agentic framework

This is the synthesis document. It takes the proven patterns from GitHub MCP, Figma Dev Mode MCP, ROS 2, Erlang/OTP, Home Assistant, LangChain, Kubernetes, Stripe, Datadog, and the broader 2026 agentic ecosystem; combines them with bubbaloop's structural advantages (edge-native, multi-agent runtime, Zenoh transport, language-agnostic node SDK); and lays out the work to make bubbaloop the boring-obvious choice for production edge agentic deployments.

The vision: **the rock-solid framework someone reaches for when they need an LLM agent to control physical infrastructure at the edge — observable, multi-tenant, language-agnostic, ecosystem-friendly**.

---

## Where bubbaloop already wins (the foundation, do not rebuild)

| Capability | What's shipped | Why it matters |
|---|---|---|
| Plugin layer (Claude Code) | `bubbaloop-mcp/` with 8 slash commands, 3 subagents, skill, INSTALL.md | Ergonomic UX matches GitHub/Figma plugins |
| MCP wire surface | 44 tools, `noun_verb` naming, ToolAnnotations on every tool | Matches GitHub MCP convention; unique among edge runtimes |
| Mode-ceiling defense | `BUBBALOOP_MCP_MODE=readonly\|safe\|admin` | Red Hat k8s-mcp-server pattern, server-enforced |
| Dynamic toolsets | 11 toolsets, 3 meta-tools (`toolset_list`/`enable`/`get_tools`), session-scoped | GitHub Jan 2026 changelog pattern; 47→25 visible with `node` only |
| Two-plane architecture | MCP control plane (≤1 rps), Zenoh data plane (1000s msg/s) | The single most important arch decision; sidesteps MCP rate-limit failure modes |
| Per-node manifests | Self-describing capabilities, commands, topics queryable per node | Daemon doesn't know what tools mean; nodes are swappable |
| Multi-agent runtime | jean-clawd (Claude), qwen (Ollama), agents.toml-driven | Genuinely novel — no LangChain / AutoGen / CrewAI exposes their runtime via MCP |
| Multi-language node SDK | Rust (`bubbaloop-sdk`) + Python (`bubbaloop_node`) | Cross-language by Zenoh contract, not by polyfill |
| Edge structural exemption | No upstream API dependency on MCP path | Sidesteps the 52% dead-endpoint failure mode of remote-server land |

This is ~60% of "production-ready 2026 agentic framework" — better than most MCP servers in the wild. The remaining 40% is sequenced below.

---

## The novel synthesis — what bubbaloop uniquely combines

No other framework combines these. Each is borrowed from a different ecosystem; the *combination* is bubbaloop's moat:

| Pattern | Borrowed from | Bubbaloop's twist |
|---|---|---|
| Self-describing components via runtime manifest | ROS 2 + agentic-community/mcp-gateway-registry | Manifests are CBOR queryables, not config files — discovery is live, not on-restart |
| Lifecycle state machine (`unconfigured → inactive → active → finalized`) | ROS 2 REP-2002 | Exposed via MCP tools; LLM agents can drive lifecycle, not just operators |
| Supervision tree with restart strategies | Erlang/OTP | Encoded in node manifests; daemon enforces; agents see propagation in real time |
| Dynamic toolset disclosure | GitHub MCP `X-MCP-Toolsets` | Per-session, runtime-mutable via meta-tools (more dynamic than a header) |
| Mode-ceiling + per-tool RBAC composability | Red Hat k8s-mcp-server + GitHub | Three-layer defense: mode → tier → token, all server-enforced |
| Standard test suite for component graduation | LangChain partner packages + Home Assistant quality scale | Per-node validation that runs at register-time, blocks broken nodes from joining the fleet |
| Structured error bodies | Stripe / Datadog / Salesforce | Encoded with retry hints; multi-step LLM chains stop crashing |
| In-band observability via MCP tools | Self-introspection (none of the comparable frameworks do this) | `mcp_metrics_get`, `mcp_recent_calls` — the agent can debug itself |
| Live-deployment evals against real fleet | inspect-evals, promptfoo (offline pattern) | Eval scenarios run against the LIVE daemon, not mocks — true production-fidelity |
| Declarative pipelines (YAML wiring) | LangChain LCEL + ROS 2 launch files | Cross-machine, cross-language, validated against `dataflow_graph` at parse time |
| Multi-agent runtime introspection via MCP | None | Genuinely novel — exposing `mission_list`/`belief_get`/`agent_list` over MCP gives the user *and* external agents a way to reason about the agent fleet |

The synthesis insight: nobody else has bothered building this for *the edge*. Cloud-native frameworks (LangChain, k8s, GitHub MCP) optimize for one big always-on cluster. Robotics frameworks (ROS 2) optimize for hardware control without LLM agents. Bubbaloop is the only framework optimizing for "small fleet of edge devices, each running multiple LLM agents and physical sensors, observed and controlled through a single MCP plane."

---

## The six themes — what's missing, ranked by enterprise blockage

### Theme 1 — Multi-tenant security (the biggest enterprise block)

Single largest production block today. `let caller_tier = rbac::Tier::Admin;` in `mod.rs:143` means every authenticated caller gets Admin. Works for single-user localhost; fails immediately for any team scenario.

**Patterns to borrow:**
- GitHub MCP — tier-encoded PATs + OAuth scope filtering via `X-MCP-Toolsets` header
- Salesforce Agentforce — named credentials, OAuth 2.1 PKCE, per-scope token introspection
- Kubernetes — RBAC subjects (User/ServiceAccount/Group) + verbs + resources matrix

**Bubbaloop's deliverables:**

| # | Deliverable | Pattern source | Effort | Closes |
|---|---|---|---|---|
| 1.1 | **Tier-encoded token format**: `bb_<tier>_<random32>` (e.g., `bb_viewer_a3f2c1...`). Auth middleware parses and attaches `Tier` to request context. | GitHub PATs | M | gap #4 |
| 1.2 | **Token rotation CLI**: `bubbaloop daemon token issue --tier operator`, `bubbaloop daemon token revoke <id>`, `bubbaloop daemon token list` | Stripe restricted keys | L | new |
| 1.3 | **Audit log**: durable `(timestamp, token_id, tool, args_hash, result_status, latency_ms)` written to SQLite at `~/.bubbaloop/audit.db`. Queryable via `mcp_audit_search` MCP tool (Admin-tier). | SOC2 / k8s audit | M | new |
| 1.4 | **OAuth 2.1 PKCE for hosted** (deferred until hosted ships): scaffolding now, full flow later | MCP 2025-11-25 spec | H | gap #8 |
| 1.5 | **Per-tool rate limits**: declarative limits per tool (e.g., `node_install` capped at 10/hour per token); 429 with `Retry-After` | GitHub Apps | L | new |
| 1.6 | **Secrets backend**: keychain (macOS), pass (Linux), DPAPI (Windows); plus optional Vault/AWS-KMS plugin | Kubernetes secret stores | M | new |

**Sequencing:** 1.1 first (unblocks everything else). 1.2 + 1.3 ship together (token mgmt + audit go hand in hand). 1.5 + 1.6 are polish. 1.4 deferred until hosted variant is real.

### Theme 2 — Observability (the silent killer)

No team ships to production without seeing what their system is doing. Bubbaloop has zero observability today beyond `log::info!`.

**Patterns to borrow:**
- Datadog Bits AI MCP — instrumented tool calls with span attributes
- claude_telemetry (TechNickAI) — OTEL wrapper for Claude Code
- LangSmith — agent-trace visualization with tool-call timeline
- Honeycomb — wide-event observability (one event per request, many fields)

**Bubbaloop's deliverables:**

| # | Deliverable | Pattern source | Effort | Notes |
|---|---|---|---|---|
| 2.1 | **OpenTelemetry traces**: every MCP call emits a span with `tool`, `tier`, `mode`, `caller_token_id`, `latency_ms`, `outcome`. Exporters: OTLP HTTP (Datadog/Honeycomb/Tempo) | Datadog | M | |
| 2.2 | **Prometheus metrics endpoint** at `:8088/metrics`: `mcp_calls_total{tool,outcome}`, `mcp_call_duration_seconds{tool}` (histogram), `mcp_rbac_denied_total{tool,reason}`, `node_health_total{name,status}` | Standard Prom exposition | L | |
| 2.3 | **Structured `tracing` crate logs** with JSON output: replace `log::info!("[MCP] tool=node_list")` with `tracing::info!(tool = "node_list", ...)`. Fields: `request_id`, `session_id`, `tool`, `caller_token_id`. | Tracing-rs convention | L | |
| 2.4 | **Self-introspection MCP tools** (genuinely novel): `mcp_metrics_get` (counters snapshot), `mcp_recent_calls(limit)` (last N calls with latencies), `mcp_session_list`. New `observability` toolset. | None — bubbaloop original | M | |
| 2.5 | **`/health/live` and `/health/ready` endpoints** distinct from `/health`. Live = process up; Ready = Zenoh connected + RBAC initialized + at least one node registered | Kubernetes liveness/readiness | L | |
| 2.6 | **Dashboard observability tab**: integrate with the existing `bubbaloop-dash` to show live metrics + recent-calls table | LangSmith UI | M | |

**Sequencing:** 2.3 first (foundation for 2.1 + 2.4). 2.1 + 2.2 ship together (OTEL and Prom both wire off the same instrumentation). 2.4 lights up the in-band observability UX. 2.5 + 2.6 are polish.

### Theme 3 — Resilience & operability

Production systems fail constantly. Bubbaloop assumes the happy path more than it should.

**Patterns to borrow:**
- Erlang/OTP — supervision trees, restart strategies (`one_for_one`, `one_for_all`, `rest_for_one`), max-restart-intensity
- ROS 2 REP-2002 — managed-node lifecycle (`unconfigured → inactive → active → finalized`)
- Stripe / Datadog / Salesforce — structured error bodies (`{code, message, retry, hint}`)
- Kafka — backpressure via consumer-group lag; producer-side `acks=all` semantics

**Bubbaloop's deliverables:**

| # | Deliverable | Pattern source | Effort | Closes |
|---|---|---|---|---|
| 3.1 | **Structured error bodies**: every tool returns `{"code":"E_X","message":"...","retry":bool,"hint":"..."}` instead of `"Error: ..."` strings; `isError: true` flag set. Standardized error code namespace per toolset. | Stripe | L | gap #13 |
| 3.2 | **Lifecycle state machine** for nodes: `unconfigured → inactive → active → finalized`. New MCP tools: `node_configure`, `node_activate`, `node_deactivate`, `node_cleanup`. Existing `node_start`/`node_stop` become composite shortcuts. | ROS 2 REP-2002 | M | new gap #15 |
| 3.3 | **Supervision tree in node manifests**: each node declares `dependencies: [other_node]` + `restart_strategy: {one_for_one\|one_for_all\|rest_for_one}` + `max_restarts_per_minute`. Daemon enforces propagation. | Erlang/OTP | M | new gap #16 |
| 3.4 | **Mcp-Session-Id resilience**: support stateless mode (rmcp's `StatelessSessionManager`) as alternative; on stale session-id 404, automatically re-init rather than fail | Multiple python-sdk issues | M | new |
| 3.5 | **Backpressure / queue limits**: per-node command-queue depth; 503 with `Retry-After` when exceeded; drop-oldest vs reject-newest configurable | Kafka | L | new |
| 3.6 | **Graceful shutdown**: daemon receives SIGTERM, stops accepting new requests, drains in-flight, hands off (HA scenario), exits clean | Standard k8s pattern | L | new |

**Sequencing:** 3.1 first (foundation for everything else's error-handling). 3.2 + 3.3 are tightly coupled (lifecycle states need supervision to make sense); ship together. 3.4 fixes the widespread session-not-found bug. 3.5 + 3.6 are operational polish.

### Theme 4 — Ecosystem & community (the scaling lever)

Solid frameworks have community contributors. Bubbaloop has the architecture for it (node contract, plugin model) but lacks the social infrastructure.

**Patterns to borrow:**
- Home Assistant — quality scale (`bronze → silver → gold → platinum`), strict integration manifest, quarterly release windows
- LangChain — partner packages with `langchain-tests` standard suite for graduation
- VS Code — declarative contribution points in `package.json`; lazy activation events
- Anthropic Plugin Marketplace — curated submission process

**Bubbaloop's deliverables:**

| # | Deliverable | Pattern source | Effort | Closes |
|---|---|---|---|---|
| 4.1 | **`bubbaloop-node-tests` validation suite**: a Rust crate exposing standard tests every node must pass — manifest-shape, command-envelope, heartbeat-liveness, topic-naming, error-format, lifecycle-state. CI plugs in. | LangChain partner-test | M | new |
| 4.2 | **Quality scale** for nodes: `experimental → community → official → blessed`. Each tier has machine-checkable criteria. `bubbaloop node validate --quality-target=official` reports pass/fail per criterion. | Home Assistant | M | new |
| 4.3 | **Marketplace JSON schema + public registry**: extend `marketplace.json`; host at `marketplace.bubbaloop.dev/registry.json`. Tags for hardware compat (Jetson Orin, RPi 5, Mac M-series, x86_64). Submit-via-PR process. | Anthropic Plugin Marketplace | M | new |
| 4.4 | **Declarative contribution points in plugin manifest**: extend `.claude-plugin/plugin.json` with `contributes: {commands, subagents, skills, toolsets}` block; CC/Cursor/etc render plugin capability before install | VS Code | L | new |
| 4.5 | **Public docs site at `docs.bubbaloop.dev`**: mkdocs / Docusaurus, searchable API reference, tutorials, ADRs. Generated from existing `bubbaloop-mcp/docs/` + `bubbaloop/docs/`. | Standard practice | M | new |
| 4.6 | **Reference deployments**: "5-min Raspberry Pi 5 setup", "Jetson Orin tutorial", "Mac M-series dev". Each end-to-end with hardware + software + first-recording. | Home Assistant getting-started | L | new |
| 4.7 | **CI/CD with live wire smoke test**: every PR runs `cargo test` + spins up a daemon + exercises a half-dozen MCP calls. Catches the rename-PR class of bug (cargo passes, runtime breaks). | Standard practice | M | new |

**Sequencing:** 4.1 first (the suite blocks 4.2 graduation criteria). 4.2 + 4.3 ship together (quality scale + marketplace are useless apart). 4.4 is independent, low-cost. 4.5 + 4.6 are docs efforts; can run in parallel. 4.7 is a one-time CI investment.

### Theme 5 — Agentic-era specifics (the differentiation)

Where bubbaloop can leapfrog generic MCP servers and define what edge agentic looks like. These problems are *largely unsolved across the industry* — the chance to set bars.

**Patterns to borrow:**
- inspect-evals / promptfoo / ragas — structured eval harnesses
- LangSmith — agent-trace cost tracking + token counts
- GitHub Copilot Spaces — versioned context bundles
- Anthropic Caching — prompt caching with TTL
- ROS 2 launch files — declarative system composition

**Bubbaloop's deliverables:**

| # | Deliverable | Pattern source | Effort | Notes |
|---|---|---|---|---|
| 5.1 | **Eval framework**: `bubbaloop eval run examples/incident-response.yaml`. Records a scenario; replays through the agent runtime; scores against rubric. Live-deployment mode: scenarios run against actual fleet, not mocks. | inspect-evals + bubbaloop original | H | |
| 5.2 | **Prompt versioning**: `~/.bubbaloop/agents/{id}/soul/` becomes a git repo; `bubbaloop agent prompt diff <a> <b>`, `bubbaloop agent prompt revert <sha>`, `bubbaloop agent prompt history` | Git-as-CMS | L | |
| 5.3 | **Cost tracking**: every LLM call records token counts + provider price; aggregated as `mcp_agent_tokens_total{agent,model,kind=input\|output\|cache_hit\|cache_write}`. Daily/monthly cost snapshot. New `cost` toolset. | LangSmith | M | |
| 5.4 | **Streaming tool responses**: long-running operations (e.g., `node_build`, recorder `start_recording`) emit progress events via SSE rather than block-and-return | rmcp 0.15 SSE support | M | |
| 5.5 | **Resources + Prompts** (gap #5): node manifests, schemas, mission YAMLs become MCP Resources (with TTL/ETag per June 2026 spec); reusable agent workflows become MCP Prompts | MCP spec + Postman | M | gap #5 |
| 5.6 | **Declarative pipelines** (`flow.yaml`): YAML that wires `tapo-terrace.frame → object_detector.input → motion_filter.input → recorder.trigger`. Validated at parse time against `dataflow_graph`; failures surfaced before deploy. | LangChain LCEL + ROS 2 launch | M | new gap #17 |
| 5.7 | **Agent memory introspection MCP tools**: `agent_episodic_search(agent_id, query)`, `agent_belief_summary(agent_id)`, `agent_recent_thoughts(agent_id, limit)`. Lets external agents debug what jean-clawd is "thinking." | None — bubbaloop original | L | |
| 5.8 | **Prompt caching integration**: detect repeated context (system prompt, large manifests, episodic snippets) and route through Anthropic's prompt-cache API; surfaces cache_hit_rate metric | Anthropic prompt caching | M | |

**Sequencing:** 5.1 + 5.2 + 5.3 are the foundation triad — without these, every agent change is a guess. 5.5 unlocks 5.6 (Resources are the substrate for declarative pipelines). 5.4 + 5.7 + 5.8 are independent, can land in any order.

### Theme 6 — HA / scale-out (the eventual)

Single daemon = single point of failure. Most users don't care today; some will tomorrow.

**Patterns to borrow:**
- Kubernetes — etcd-backed shared state; controllers; leader election via leases
- HashiCorp Nomad — gossip-based clustering; raft for state
- Erlang/OTP — distributed Erlang nodes; net_kernel; pg2 process groups

**Bubbaloop's novel angle:**

The "fleet of bubbaloops" model. Each daemon owns its own machine. Zenoh's gossip layer already discovers peers. No central control plane needed if you embrace eventual consistency for cross-machine coordination. This is genuinely novel — k8s/Nomad/Erlang all have a central authority; bubbaloop can be peer-to-peer because Zenoh already is.

**Deliverables (defer until concrete user need):**

| # | Deliverable | Pattern source | Effort |
|---|---|---|---|
| 6.1 | **Cross-machine node discovery via Zenoh gossip**: extend `node_discover` to surface remote nodes, with origin-machine annotation | Zenoh gossip | M |
| 6.2 | **Federated `node_command_send` to remote machines**: route commands across the gossip mesh; use Zenoh's existing routing | Zenoh routing | M |
| 6.3 | **Per-machine token translation**: a token issued on machine A is honored on machine B with a configured trust relationship | mTLS-style | H |
| 6.4 | **HA shared state for mission scheduler**: raft-replicated mission queue across N daemons. Or accept eventual consistency and let mission start on any daemon, with idempotent commands. | Raft via etcd or embedded | H |
| 6.5 | **Failover for `tapo-terrace` etc.**: if machine A dies, machine B takes over its registered nodes (where hardware presence allows) | Standard HA | H |

**Sequencing:** All deferred until a concrete user need surfaces. The Zenoh-native peer-to-peer model means "HA" probably doesn't look like k8s HA — it looks like "any machine can be the control plane for any node it can physically reach." Worth a separate design doc when the need is real.

---

## Phased sequencing — six waves, ~2 weeks each

| Phase | Theme(s) | Deliverables | Why this order |
|---|---|---|---|
| **Phase 1** (week 1-2) | Theme 1 + Theme 3.1 | 1.1 token format, 1.2 token CLI, 1.3 audit log, 3.1 structured errors | Hard blockers for multi-user. Structured errors are the foundation everything else's error-handling builds on. |
| **Phase 2** (week 3-4) | Theme 2 (whole) | 2.1 OTEL, 2.2 Prom, 2.3 tracing, 2.4 self-introspection tools, 2.5 health endpoints | Once you have multiple tokens you need to see what they're doing. Without observability you're flying blind in production. |
| **Phase 3** (week 5-6) | Theme 3.2-3.6 | 3.2 lifecycle states, 3.3 supervision, 3.4 session resilience, 3.5 backpressure, 3.6 graceful shutdown | Resilience layer. Builds on Phase 1's structured errors. |
| **Phase 4** (week 7-8) | Theme 5.1-5.3 | 5.1 eval framework, 5.2 prompt versioning, 5.3 cost tracking | Makes every subsequent change measurable. Without this, agent improvements are subjective. |
| **Phase 5** (week 9-10) | Theme 4 + Theme 5.5 | 4.1 node-tests suite, 4.2 quality scale, 4.3 marketplace, 4.4 contribution points, 4.7 CI live smoke; 5.5 Resources + Prompts | Ecosystem layer. Resources/Prompts ship here because the docs effort (4.5/4.6) needs them as examples. |
| **Phase 6** (week 11-12) | Theme 5.4-5.8 + Theme 4.5/4.6 | 5.4 streaming, 5.6 declarative pipelines, 5.7 memory introspection, 5.8 prompt caching, 4.5 docs site, 4.6 reference deployments | The agentic-era differentiation polish + docs site launch. |

Theme 6 (HA / scale-out) deferred indefinitely. Revisit when a concrete user runs a 3+ machine fleet and surfaces a coordination need.

After Phase 6: bubbaloop is at ~90% production-ready and can be positioned as **"the de facto framework for edge agentic deployments"**. The remaining 10% (HA, hosted variant, full OAuth 2.1 PKCE, public marketplace launch) is the difference between "framework I'd run for paying customers" and "framework that's the obvious choice across the industry" — worth getting to once 90% is rock-solid; not worth racing.

---

## What makes this novel (the moat statement)

Read these together; the synthesis is greater than the sum:

1. **Edge-native**: structurally exempt from the dominant 2026 ecosystem failure modes (52% of remote MCP endpoints are dead).
2. **Multi-agent runtime exposed via MCP**: no other framework lets external agents introspect their internal agents.
3. **Per-node manifests = live discovery**: nodes self-describe via Zenoh queryables; daemon is opaque-text bridge; new node types ship without daemon changes.
4. **Three-layer security composability**: mode-ceiling × per-token tier × per-tool annotations = the most flexible RBAC surface in any MCP server.
5. **Self-introspection MCP tools**: agents debug themselves. `mcp_recent_calls`, `agent_belief_summary`, `agent_recent_thoughts` — the framework eats its own dog food.
6. **Dynamic toolsets gating the 44 tools**: 47 → 25 visible with one toolset enabled. Small models pick the right tool at GitHub-MCP accuracy levels.
7. **Live-deployment evals**: scenarios run against the actual fleet, not mocks. Closes the lab-to-production gap that LangSmith/promptfoo can't.
8. **Lifecycle state machine + supervision tree**: ROS 2's lifecycle + Erlang's fault tolerance, exposed as MCP tools agents can drive.
9. **Declarative pipelines wired across machines**: ROS 2 launch + LangChain LCEL, but cross-language and cross-machine via Zenoh.
10. **Edge-first marketplace with hardware tags**: nobody else's marketplace cares about Jetson Orin vs Pi 5 vs M3 Mac. Bubbaloop's does.

No competitor has all ten. ROS 2 has 1 + 8. LangChain has 9 (LCEL) but in-process only. GitHub MCP has 6. Anthropic's connectors have 4. Bubbaloop will be the first to combine them.

---

## Open questions for the user (decisions blocking sequencing)

These shape the plan in ways I shouldn't decide unilaterally:

1. **Multi-tenant priority**: do you have a concrete user / customer / team scenario that needs Theme 1 NOW (next 2 weeks), or is single-user-localhost still the dominant case for the next quarter? If the latter, Theme 1 work can be deprioritized to Phase 3.

2. **Self-hosted vs hosted future**: do you envision a hosted bubbaloop variant (managed daemon-as-a-service) ever, or is edge-self-hosted the only mode forever? If the former, Theme 1.4 (OAuth 2.1 PKCE) needs scaffolding earlier.

3. **Eval framework ambition**: Phase 4 includes a substantial eval framework. Is that worth 2 weeks of effort early, or should it be deferred until concrete agent-quality regressions hurt? My recommendation is "early" — without it, every subsequent change is undiagnosable.

4. **HA appetite**: are you ever going to want a 3+ machine fleet with cross-machine failover? If the answer is "probably not — each Jetson is its own world", Theme 6 stays deferred and we ship Phase 1-5 as the complete production readiness story.

5. **Marketplace governance**: who reviews the first 50 community-contributed nodes? A small core team? The author themselves? Automated `bubbaloop-node-tests` pass = automatic merge? This shapes 4.2 + 4.3.

Answer these and the plan converges to a concrete schedule. Default in absence of input: assume single-user-localhost stays dominant for ~3 months → Theme 1 ships in Phase 1 anyway because the work is small and unblocks future scenarios; Theme 6 deferred; eval framework ships early.

---

## Citations

Patterns lifted from (in order of how heavily they're borrowed):

- [GitHub MCP server](https://github.com/github/github-mcp-server) — naming, toolsets, OAuth scope filtering, mode flags
- [ROS 2 design docs / REP-2002 lifecycle](https://design.ros2.org/articles/node_lifecycle.html) — lifecycle states, manifest contracts
- [Erlang/OTP design principles](https://www.erlang.org/doc/system/design_principles.html) — supervision trees, restart strategies
- [Home Assistant integration manifest](https://developers.home-assistant.io/docs/creating_integration_manifest/) — quality scale
- [LangChain partner-package model + langchain-tests](https://python.langchain.com/docs/contributing/integrations/) — graduation suite
- [Stripe MCP](https://docs.stripe.com/mcp) — structured errors, restricted keys, audit log
- [Datadog Bits AI MCP](https://docs.datadoghq.com/bits_ai/mcp_server/) — observability instrumentation
- [Red Hat kubernetes-mcp-server](https://github.com/containers/kubernetes-mcp-server) — mode-ceiling pattern
- [Anthropic Plugin Marketplace](https://github.com/anthropics/claude-plugins-official) — submission/marketplace process
- [MCP 2025-11-25 spec + 2026 roadmap](https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/) — protocol forward-compat
- [Klavis benchmark on progressive disclosure](https://matthewkruczek.ai/blog/progressive-disclosure-mcp-servers.html) — toolset ROI
- [Framelink Figma MCP](https://github.com/GLips/Figma-Context-MCP) — descriptive JSON outputs
- [arxiv 2602.14878 — MCP tool description quality study](https://arxiv.org/html/2602.14878v1) — 97.1% description defect baseline
- [The April 2026 dead-endpoint scan](https://modelslab.com/blog/api/mcp-vs-cli-ai-agents-developers-2026) — bubbaloop's structural exemption
