import { useState, useEffect, useRef, useCallback } from "react";
import type { Session } from "@eclipse-zenoh/zenoh-ts";
import {
  discoverAgents,
  loadToken,
  saveToken,
  type ResolvedAgent,
  type AgentEvent,
} from "../lib/agent";
import { parseSseStream } from "../lib/sse";

// ── Types ────────────────────────────────────────────────────────────

type ChatEntry =
  | { kind: "user"; text: string }
  | { kind: "assistant"; text: string }
  | { kind: "tool"; name: string; input: string }
  | { kind: "tool_result"; text: string }
  | { kind: "error"; text: string }
  | { kind: "system"; text: string }
  | { kind: "connection_lost" }
  | { kind: "oauth_blocked" };

// ── Helpers ──────────────────────────────────────────────────────────

const PROVIDER_COLORS: Record<string, string> = {
  gemini: "#4285F4",
  claude: "#D4A574",
  ollama: "#7C3AED",
};

function providerFromModel(model: string): string {
  if (model.startsWith("gemini")) return "gemini";
  if (model.startsWith("claude")) return "claude";
  return "ollama";
}

function AgentSelector({
  agents,
  selected,
  onSelect,
  discovering,
  onRefresh,
}: {
  agents: ResolvedAgent[];
  selected: ResolvedAgent | null;
  onSelect: (a: ResolvedAgent) => void;
  discovering: boolean;
  onRefresh: () => void;
}) {
  const [open, setOpen] = useState(false);

  if (discovering) {
    return <span className="agent-status-hint">Discovering agents…</span>;
  }
  if (agents.length === 0) {
    return (
      <div className="agent-empty">
        <span>No agents found</span>
        <button className="agent-refresh-btn" onClick={onRefresh} title="Retry">↻</button>
      </div>
    );
  }
  if (agents.length === 1) {
    const a = agents[0];
    return (
      <div className="agent-trigger agent-trigger-single">
        <span className="trigger-dot" style={{ background: PROVIDER_COLORS[providerFromModel(a.agent.model)] ?? "#888" }} />
        <span className="trigger-name">{a.agent.agent_id}</span>
        <span className="trigger-model">{a.agent.model.split("-")[0]}</span>
      </div>
    );
  }

  return (
    <div className="agent-selector-wrap">
      {/* Trigger chip showing the active agent */}
      <button
        className="agent-trigger"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        {selected ? (
          <>
            <span
              className="trigger-dot"
              style={{ background: PROVIDER_COLORS[providerFromModel(selected.agent.model)] ?? "#888" }}
            />
            <span className="trigger-name">{selected.agent.agent_id}</span>
            <span className="trigger-model">{selected.agent.model.split("-")[0]}</span>
          </>
        ) : (
          <span className="trigger-name">Select agent</span>
        )}
        <svg
          className={`trigger-chevron${open ? " open" : ""}`}
          width="12" height="12" viewBox="0 0 24 24"
          fill="none" stroke="currentColor" strokeWidth="2.5"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>

      {/* Dropdown panel */}
      {open && (
        <>
          <div className="agent-backdrop" onClick={() => setOpen(false)} />
          <div className="agent-panel">
            <div className="agent-panel-header">
              <span style={{flex:1, minWidth:0, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap'}}>Agents on {agents[0]?.agent.machine_id}</span>
              <button className="agent-refresh-btn" onClick={() => { setOpen(false); onRefresh(); }} title="Refresh">↻</button>
            </div>
            {agents.map((a) => {
              const provider = providerFromModel(a.agent.model);
              const color = PROVIDER_COLORS[provider] ?? "#888";
              const isSelected = a.agent.agent_id === selected?.agent.agent_id;
              return (
                <button
                  key={a.agent.agent_id}
                  className={`agent-pill${isSelected ? " agent-pill-selected" : ""}`}
                  onClick={() => { onSelect(a); setOpen(false); }}
                  style={{ "--pill-color": color } as React.CSSProperties}
                >
                  <span className="pill-dot" />
                  <span className="pill-name">{a.agent.agent_id}</span>
                  <span className="pill-model">{a.agent.model.split("-")[0]}</span>
                  {a.agent.is_default && <span className="pill-default">default</span>}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}

function EntryRow({ entry, onDismissOauth }: { entry: ChatEntry; onDismissOauth: () => void }) {
  switch (entry.kind) {
    case "user":
      return (
        <div className="chat-entry chat-user">
          <span className="entry-role">You</span>
          <span className="entry-text">{entry.text}</span>
        </div>
      );
    case "assistant":
      return (
        <div className="chat-entry chat-assistant">
          <span className="entry-role">Agent</span>
          <span className="entry-text">{entry.text}</span>
        </div>
      );
    case "tool":
      return (
        <div className="chat-entry chat-tool">
          <div className="tool-tool-row">
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" style={{flexShrink:0,opacity:0.5}}>
              <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
            </svg>
            <span className="tool-name">{entry.name}</span>
          </div>
          {entry.input && (
            <details className="tool-details">
              <summary className="tool-summary">args</summary>
              <pre className="tool-json">{entry.input}</pre>
            </details>
          )}
        </div>
      );
    case "tool_result": {
      const preview = entry.text.length > 80 ? entry.text.slice(0, 80) + "…" : entry.text;
      const hasMore = entry.text.length > 80;
      return (
        <div className="chat-entry chat-tool-result">
          {hasMore ? (
            <details className="tool-details" style={{width:"100%"}}>
              <summary className="tool-summary tool-result-summary">{preview}</summary>
              <pre className="tool-json">{entry.text}</pre>
            </details>
          ) : (
            <span className="tool-result-text">{entry.text}</span>
          )}
        </div>
      );
    }
    case "error":
      return (
        <div className="chat-entry chat-error">
          <span className="entry-role">Error</span>
          <span className="entry-text">{entry.text}</span>
        </div>
      );
    case "system":
      return (
        <div className="chat-entry chat-system">
          <span className="entry-text system-text">{entry.text}</span>
        </div>
      );
    case "connection_lost":
      return (
        <div className="chat-entry chat-connection-lost">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          Connection lost — stream ended without completion
        </div>
      );
    case "oauth_blocked":
      return (
        <div className="oauth-gate">
          <div className="oauth-gate-icon">🔒</div>
          <h3>Dashboard chat unavailable with Claude OAuth</h3>
          <p>
            Per Anthropic's Feb 2026 Terms of Service, OAuth credentials are reserved
            for Claude Code CLI. Account-level bans are actively enforced for
            non-CLI OAuth usage.
          </p>
          <div className="oauth-options">
            <div className="oauth-option">
              <strong>Option 1 (recommended): Gemini API key (free)</strong>
              <code>export GEMINI_API_KEY=your_key</code>
              <span>15 RPM / 1M tokens/day free via Google AI Studio</span>
            </div>
            <div className="oauth-option">
              <strong>Option 2: Claude API key (pay-per-token)</strong>
              <code>export ANTHROPIC_API_KEY=your_key</code>
              <span>Billed separately from Claude Pro/Max subscription</span>
            </div>
            <div className="oauth-option">
              <strong>Option 3: Use the CLI</strong>
              <code>bubbaloop agent chat</code>
              <span>CLI path remains available at operator risk</span>
            </div>
          </div>
          <button className="oauth-dismiss" onClick={onDismissOauth}>
            Dismiss
          </button>
        </div>
      );
  }
}

// ── Main component ────────────────────────────────────────────────────

export function ChatView({ session }: { session?: Session }) {
  const [token, setToken] = useState(loadToken);
  const [tokenDraft, setTokenDraft] = useState(loadToken);
  const [agents, setAgents] = useState<ResolvedAgent[]>([]);
  const [selectedAgent, setSelectedAgent] = useState<ResolvedAgent | null>(null);
  const [discovering, setDiscovering] = useState(false);
  const [input, setInput] = useState("");
  const [entries, setEntries] = useState<ChatEntry[]>([]);
  const [streaming, setStreaming] = useState(false);

  const abortRef = useRef<AbortController | null>(null);
  const logEndRef = useRef<HTMLDivElement>(null);
  const chatBodyRef = useRef<HTMLDivElement>(null);
  const userScrolledRef = useRef(false);
  // Accumulates assistant delta text across SSE frames before flushing to state
  const deltaBufferRef = useRef("");

  // Auto-scroll to bottom when new entries arrive, unless user has scrolled up
  useEffect(() => {
    if (userScrolledRef.current) return;
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [entries]);

  const runDiscovery = useCallback((s: Session) => {
    setDiscovering(true);
    discoverAgents(s)
      .then((found) => {
        setAgents(found);
        const def = found.find((a) => a.agent.is_default) ?? found[0] ?? null;
        setSelectedAgent(def);
      })
      .finally(() => setDiscovering(false));
  }, []);

  // Discover agents when session becomes available
  useEffect(() => {
    if (!session) return;
    runDiscovery(session);
  }, [session, runDiscovery]);

  const flushDeltaBuffer = useCallback(() => {
    if (!deltaBufferRef.current) return;
    const text = deltaBufferRef.current;
    deltaBufferRef.current = "";
    setEntries((prev) => {
      const last = prev[prev.length - 1];
      if (last?.kind === "assistant") {
        return [...prev.slice(0, -1), { kind: "assistant", text: last.text + text }];
      }
      return [...prev, { kind: "assistant", text }];
    });
  }, []);

  const handleSend = useCallback(async () => {
    if (!selectedAgent || !input.trim() || streaming) return;

    const message = input.trim();
    setInput("");
    setEntries((prev) => [...prev, { kind: "user", text: message }]);
    userScrolledRef.current = false;
    setStreaming(true);
    deltaBufferRef.current = "";

    const ctrl = new AbortController();
    abortRef.current = ctrl;

    // Use the Vite proxy path (/api → http://127.0.0.1:8088) to avoid mixed-content
    // blocks when the dashboard is served over HTTPS. Works for both dev (Vite proxy)
    // and production (nginx proxy_pass /api → daemon).
    const url = `/api/v1/agents/${selectedAgent.agent.agent_id}/turns`;

    let gotDone = false;

    try {
      const res = await fetch(url, {
        method: "POST",
        headers: {
          "Authorization": `Bearer ${token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ message }),
        signal: ctrl.signal,
      });

      if (!res.ok) {
        if (res.status === 403) {
          let body: { code?: string } = {};
          try { body = await res.json(); } catch { /* ignore */ }
          if (body.code === "oauth_chat_disabled") {
            setEntries((prev) => [...prev, { kind: "oauth_blocked" }]);
            return;
          }
        }
        const text = await res.text().catch(() => res.statusText);
        setEntries((prev) => [...prev, { kind: "error", text: `HTTP ${res.status}: ${text}` }]);
        return;
      }

      if (!res.body) {
        setEntries((prev) => [...prev, { kind: "error", text: "No response body" }]);
        return;
      }

      for await (const frame of parseSseStream(res.body)) {
        if (ctrl.signal.aborted) break;

        let event: AgentEvent;
        try {
          event = JSON.parse(frame.data) as AgentEvent;
        } catch {
          continue;
        }

        switch (event.type) {
          case "delta":
            deltaBufferRef.current += event.text ?? "";
            // Flush in batches to avoid excessive re-renders
            if (deltaBufferRef.current.length > 20) flushDeltaBuffer();
            break;

          case "tool":
            flushDeltaBuffer();
            setEntries((prev) => [
              ...prev,
              { kind: "tool", name: event.text ?? "tool", input: event.tool_input ?? "" },
            ]);
            break;

          case "tool_result":
            flushDeltaBuffer();
            setEntries((prev) => [...prev, { kind: "tool_result", text: event.text ?? "" }]);
            break;

          case "error":
            flushDeltaBuffer();
            setEntries((prev) => [...prev, { kind: "error", text: event.text ?? "Unknown error" }]);
            break;

          case "system":
            flushDeltaBuffer();
            setEntries((prev) => [...prev, { kind: "system", text: event.text ?? "" }]);
            break;

          case "done":
            flushDeltaBuffer();
            gotDone = true;
            break;
        }
      }

      flushDeltaBuffer();

      if (!gotDone && !ctrl.signal.aborted) {
        setEntries((prev) => [...prev, { kind: "connection_lost" }]);
      }
    } catch (err) {
      if (ctrl.signal.aborted) return;
      flushDeltaBuffer();
      const msg = err instanceof Error ? err.message : String(err);
      setEntries((prev) => [...prev, { kind: "error", text: msg }]);
    } finally {
      abortRef.current = null;
      setStreaming(false);
    }
  }, [selectedAgent, input, streaming, token, flushDeltaBuffer]);

  const handleStop = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const handleTokenSave = useCallback(() => {
    saveToken(tokenDraft);
    setToken(tokenDraft.trim());
  }, [tokenDraft]);

  const handleRefresh = useCallback(() => {
    if (!session) return;
    runDiscovery(session);
  }, [session, runDiscovery]);

  const dismissOauth = useCallback(() => {
    setEntries((prev) => prev.filter((e) => e.kind !== "oauth_blocked"));
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend]
  );

  // ── Render ────────────────────────────────────────────────────────

  return (
    <div className="chat-view">

      {/* Scrollable area: token setup + agent bar + messages */}
      <div className="chat-body" ref={chatBodyRef} onScroll={() => { userScrolledRef.current = true; }}>

        {!token && (
          <div className="token-setup">
            <h3>Enter daemon token</h3>
            <p>Paste the MCP token from your Jetson (<code>cat ~/.bubbaloop/mcp-token</code>)</p>
            <div className="token-row">
              <input
                type="password"
                className="token-input"
                placeholder="bubbaloop-token-..."
                value={tokenDraft}
                onChange={(e) => setTokenDraft(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") handleTokenSave(); }}
              />
              <button className="token-save-btn" onClick={handleTokenSave} disabled={!tokenDraft.trim()}>
                Save
              </button>
            </div>
          </div>
        )}

        {/* Agent selector bar */}
        <div className="chat-header">
          {!token ? null : !session ? (
            <span className="agent-status-hint">Connect to Zenoh to discover agents</span>
          ) : (
            <AgentSelector
              agents={agents}
              selected={selectedAgent}
              onSelect={setSelectedAgent}
              discovering={discovering}
              onRefresh={handleRefresh}
            />
          )}
          {token && (
            <button
              className="token-reset-btn"
              onClick={() => { setToken(""); setTokenDraft(""); localStorage.removeItem("bubbaloop_mcp_token"); }}
              title="Change token"
            >
              ✕ token
            </button>
          )}
        </div>

        {/* Messages */}
        <div className="chat-log">
          {entries.length === 0 && token && selectedAgent && (
            <div className="chat-empty">
              <p>Talking to <strong>{selectedAgent.agent.agent_id}</strong></p>
              <p className="chat-empty-hint">Try: "what nodes are running?" or "describe the current system state"</p>
            </div>
          )}
          {entries.map((entry, i) => (
            <EntryRow key={i} entry={entry} onDismissOauth={dismissOauth} />
          ))}
          <div ref={logEndRef} />
        </div>

      </div>

      {/* Input bar — full width, always pinned at bottom */}
      <div className="chat-input-bar">
        <div className="chat-input-area">
          <textarea
            className="chat-textarea"
            placeholder={
              !token
                ? "Paste token to start"
                : !selectedAgent
                ? "No agent selected"
                : streaming
                ? "Responding…"
                : "Message agent…"
            }
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={!token || !selectedAgent || streaming}
            rows={1}
          />
          {streaming ? (
            <button className="stop-btn" onClick={handleStop}>
              <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
                <rect x="4" y="4" width="16" height="16" rx="2" />
              </svg>
            </button>
          ) : (
            <button className="send-btn" onClick={handleSend} disabled={!token || !selectedAgent || !input.trim()} aria-label="Send">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                <line x1="22" y1="2" x2="11" y2="13" />
                <polygon points="22 2 15 22 11 13 2 9 22 2" />
              </svg>
            </button>
          )}
        </div>
      </div>

      <style>{`
        .chat-view {
          flex: 1;
          display: flex;
          flex-direction: column;
          min-height: 0;
          overflow: hidden;
          width: 100%;
        }

        /* Token setup */
        .token-setup {
          padding: 20px 24px;
          background: var(--bg-secondary);
          border: 1px solid var(--border-color);
          border-radius: 10px;
        }
        .token-setup h3 {
          margin: 0 0 6px;
          font-size: 15px;
          color: var(--text-primary);
        }
        .token-setup p {
          margin: 0 0 14px;
          font-size: 13px;
          color: var(--text-secondary);
        }
        .token-setup code {
          font-family: 'JetBrains Mono', monospace;
          background: var(--bg-tertiary);
          padding: 2px 6px;
          border-radius: 4px;
          font-size: 12px;
        }
        .token-row {
          display: flex;
          gap: 8px;
        }
        .token-input {
          flex: 1;
          padding: 8px 12px;
          background: var(--bg-primary);
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-primary);
          font-family: 'JetBrains Mono', monospace;
          font-size: 13px;
          outline: none;
        }
        .token-input:focus {
          border-color: var(--accent-primary);
        }
        .token-save-btn {
          padding: 8px 18px;
          background: var(--accent-primary);
          border: none;
          border-radius: 6px;
          color: white;
          font-size: 13px;
          font-weight: 600;
          cursor: pointer;
        }
        .token-save-btn:disabled { opacity: 0.5; cursor: default; }

        /* Header */
        .chat-header {
          display: flex;
          align-items: center;
          gap: 8px;
          min-height: 32px;
        }
        .agent-status-hint {
          font-size: 13px;
          color: var(--text-muted);
        }
        .token-reset-btn {
          margin-left: auto;
          padding: 4px 10px;
          background: none;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-muted);
          font-size: 11px;
          cursor: pointer;
          transition: all 0.15s;
          white-space: nowrap;
        }
        .token-reset-btn:hover { border-color: var(--error); color: var(--error); }

        /* Agent selector */
        .agent-selector-wrap { position: relative; }

        .agent-trigger {
          display: flex;
          align-items: center;
          gap: 7px;
          padding: 7px 12px;
          background: var(--bg-secondary);
          border: 1px solid var(--border-color);
          border-radius: 20px;
          color: var(--text-primary);
          font-size: 13px;
          font-family: inherit;
          cursor: pointer;
          transition: border-color 0.15s, background 0.15s;
          min-width: 0;
        }
        .agent-trigger:hover { border-color: var(--accent-primary); background: var(--bg-tertiary); }
        .trigger-dot {
          width: 8px; height: 8px;
          border-radius: 50%;
          flex-shrink: 0;
        }
        .trigger-name {
          font-weight: 600;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
          max-width: 140px;
        }
        .trigger-model {
          font-size: 11px;
          color: var(--text-muted);
          font-family: 'JetBrains Mono', monospace;
        }
        .trigger-chevron {
          margin-left: 2px;
          transition: transform 0.2s;
          flex-shrink: 0;
          color: var(--text-muted);
        }
        .trigger-chevron.open { transform: rotate(180deg); }

        .agent-backdrop {
          position: fixed;
          inset: 0;
          z-index: 150;
        }
        .agent-panel {
          position: absolute;
          top: calc(100% + 6px);
          left: 0;
          z-index: 151;
          min-width: 240px;
          background: var(--bg-secondary);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          box-shadow: 0 8px 32px rgba(0,0,0,0.35);
          overflow: hidden;
        }
        .agent-panel-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 10px 14px 8px;
          font-size: 11px;
          color: var(--text-muted);
          border-bottom: 1px solid var(--border-color);
          text-transform: uppercase;
          letter-spacing: 0.06em;
        }
        .agent-refresh-btn {
          background: none;
          border: none;
          color: var(--text-muted);
          font-size: 14px;
          cursor: pointer;
          padding: 0 4px;
          line-height: 1;
          transition: color 0.15s;
        }
        .agent-refresh-btn:hover { color: var(--text-primary); }

        .agent-pill {
          display: flex;
          align-items: center;
          gap: 10px;
          width: 100%;
          padding: 11px 14px;
          background: none;
          border: none;
          cursor: pointer;
          text-align: left;
          transition: background 0.12s;
          font-family: inherit;
        }
        .agent-pill:hover { background: var(--bg-tertiary); }
        .agent-pill-selected { background: rgba(61, 90, 254, 0.08); }
        .agent-pill-selected:hover { background: rgba(61, 90, 254, 0.12); }

        .pill-dot {
          width: 10px; height: 10px;
          border-radius: 50%;
          background: var(--pill-color, #888);
          flex-shrink: 0;
        }
        .pill-name {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-primary);
          flex: 1;
        }
        .pill-model {
          font-size: 11px;
          color: var(--text-muted);
          font-family: 'JetBrains Mono', monospace;
        }
        .pill-default {
          font-size: 10px;
          padding: 2px 6px;
          background: rgba(0, 229, 255, 0.12);
          border: 1px solid rgba(0, 229, 255, 0.25);
          border-radius: 10px;
          color: var(--accent-secondary);
          font-weight: 600;
        }

        .agent-empty {
          display: flex;
          align-items: center;
          gap: 8px;
          font-size: 13px;
          color: var(--text-muted);
        }

        /* Scrollable body: token setup + header + messages */
        .chat-body {
          flex: 1;
          overflow-y: auto;
          overflow-x: hidden;
          display: flex;
          flex-direction: column;
          min-height: 0;
          padding: 16px 16px 0;
          max-width: 900px;
          width: 100%;
          margin: 0 auto;
          gap: 12px;
        }

        /* Message log — only this region scrolls */
        .chat-log {
          display: flex;
          flex-direction: column;
          gap: 8px;
          padding-bottom: 8px;
        }
        .chat-empty {
          flex: 1;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          text-align: center;
          color: var(--text-secondary);
          padding: 40px 0;
        }
        .chat-empty p { margin: 4px 0; }
        .chat-empty strong { color: var(--accent-secondary); }
        .chat-empty-hint {
          font-size: 13px;
          color: var(--text-muted);
          margin-top: 8px !important;
        }

        /* Entries */
        .chat-entry {
          display: flex;
          gap: 10px;
          align-items: flex-start;
          padding: 10px 14px;
          border-radius: 8px;
          font-size: 14px;
          line-height: 1.6;
        }
        .entry-role {
          font-size: 11px;
          font-weight: 700;
          letter-spacing: 0.05em;
          text-transform: uppercase;
          min-width: 44px;
          padding-top: 2px;
          flex-shrink: 0;
        }
        .entry-text { white-space: pre-wrap; word-break: break-word; }

        .chat-user {
          background: rgba(61, 90, 254, 0.08);
          border: 1px solid rgba(61, 90, 254, 0.15);
        }
        .chat-user .entry-role { color: var(--accent-primary); }

        .chat-assistant {
          background: var(--bg-secondary);
          border: 1px solid var(--border-color);
          flex-direction: column;
          gap: 2px;
        }
        .chat-assistant .entry-role {
          color: var(--accent-secondary);
          font-size: 10px;
        }

        .chat-tool {
          background: transparent;
          border: none;
          padding: 2px 14px;
          gap: 6px;
          align-items: flex-start;
          flex-direction: column;
        }
        .tool-tool-row {
          display: flex;
          align-items: center;
          gap: 6px;
        }
        .tool-name {
          font-size: 12px;
          font-weight: 600;
          font-family: 'JetBrains Mono', monospace;
          color: var(--accent-secondary);
        }
        .tool-details { display: block; width: 100%; }
        .tool-summary {
          font-size: 11px;
          color: var(--text-muted);
          cursor: pointer;
          user-select: none;
          list-style: none;
          padding: 1px 7px;
          border: 1px solid var(--border-color);
          border-radius: 10px;
          transition: border-color 0.15s, color 0.15s;
        }
        .tool-summary:hover { border-color: var(--accent-secondary); color: var(--text-secondary); }
        .tool-summary::-webkit-details-marker { display: none; }
        .tool-json {
          width: 100%;
          margin: 4px 0 2px;
          padding: 8px 10px;
          background: var(--bg-primary);
          border-radius: 6px;
          font-size: 12px;
          font-family: 'JetBrains Mono', monospace;
          color: var(--text-secondary);
          white-space: pre-wrap;
          word-break: break-all;
          overflow: hidden;
        }

        .chat-tool-result {
          background: transparent;
          border: none;
          padding: 1px 14px 3px 30px;
        }
        .tool-result-summary {
          font-family: 'JetBrains Mono', monospace;
        }
        .tool-result-text {
          font-size: 12px;
          color: var(--text-secondary);
          font-family: 'JetBrains Mono', monospace;
          white-space: pre-wrap;
          word-break: break-all;
        }

        .chat-error {
          background: rgba(255, 23, 68, 0.08);
          border: 1px solid rgba(255, 23, 68, 0.2);
        }
        .chat-error .entry-role { color: var(--error); }

        .chat-system {
          justify-content: center;
        }
        .system-text {
          font-size: 12px;
          color: var(--text-muted);
          font-style: italic;
        }

        .chat-connection-lost {
          justify-content: center;
          align-items: center;
          gap: 6px;
          color: var(--warning);
          font-size: 12px;
          background: rgba(255, 214, 0, 0.06);
          border: 1px solid rgba(255, 214, 0, 0.2);
        }

        /* Input bar — full page width, always pinned at bottom */
        .chat-input-bar {
          flex-shrink: 0;
          border-top: 1px solid var(--border-color);
          background: var(--bg-secondary);
          padding: 10px 16px;
        }
        .chat-input-area {
          display: flex;
          gap: 10px;
          align-items: flex-end;
          max-width: 900px;
          margin: 0 auto;
          width: 100%;
        }
        .chat-textarea {
          flex: 1;
          padding: 10px 14px;
          background: var(--bg-secondary);
          border: 1px solid var(--border-color);
          border-radius: 8px;
          color: var(--text-primary);
          font-family: inherit;
          font-size: 14px;
          line-height: 1.5;
          resize: none;
          outline: none;
          transition: border-color 0.15s;
        }
        .chat-textarea:focus { border-color: var(--accent-primary); }
        .chat-textarea:disabled { opacity: 0.5; cursor: not-allowed; }

        .send-btn {
          width: 44px;
          height: 44px;
          flex-shrink: 0;
          background: var(--accent-primary);
          border: none;
          border-radius: 50%;
          color: white;
          cursor: pointer;
          display: flex;
          align-items: center;
          justify-content: center;
          transition: filter 0.15s;
        }
        .send-btn:hover:not(:disabled) { filter: brightness(1.15); }
        .send-btn:disabled { opacity: 0.35; cursor: not-allowed; }
        .stop-btn {
          width: 44px;
          height: 44px;
          flex-shrink: 0;
          background: rgba(255, 23, 68, 0.15);
          border: 1px solid rgba(255, 23, 68, 0.3);
          border-radius: 50%;
          color: var(--error);
          cursor: pointer;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 11px;
          font-weight: 600;
        }
        .stop-btn:hover { background: rgba(255, 23, 68, 0.25); }

        /* OAuth gate */
        .oauth-gate {
          max-width: 580px;
          margin: 60px auto;
          padding: 32px;
          background: var(--bg-secondary);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          text-align: center;
        }
        .oauth-gate-icon { font-size: 36px; margin-bottom: 12px; }
        .oauth-gate h3 {
          margin: 0 0 12px;
          font-size: 17px;
          color: var(--text-primary);
        }
        .oauth-gate > p {
          margin: 0 0 24px;
          font-size: 13px;
          color: var(--text-secondary);
          line-height: 1.6;
        }
        .oauth-options {
          display: flex;
          flex-direction: column;
          gap: 12px;
          text-align: left;
          margin-bottom: 24px;
        }
        .oauth-option {
          padding: 12px 16px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          border-radius: 8px;
          display: flex;
          flex-direction: column;
          gap: 4px;
        }
        .oauth-option strong { font-size: 13px; color: var(--text-primary); }
        .oauth-option code {
          font-family: 'JetBrains Mono', monospace;
          font-size: 12px;
          color: var(--accent-secondary);
          background: var(--bg-primary);
          padding: 3px 8px;
          border-radius: 4px;
        }
        .oauth-option span { font-size: 12px; color: var(--text-muted); }
        .oauth-dismiss {
          padding: 8px 24px;
          background: none;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-secondary);
          font-size: 13px;
          cursor: pointer;
          transition: all 0.15s;
        }
        .oauth-dismiss:hover { border-color: var(--accent-primary); color: var(--text-primary); }

        /* Single-agent trigger: non-interactive chip */
        .agent-trigger-single { cursor: default; pointer-events: none; }

        /* Mobile */
        @media (max-width: 768px) {
          .chat-body { padding: 10px 10px 0; }
          .token-setup { padding: 14px 16px; }
          .token-row { flex-direction: column; }
          .token-save-btn { width: 100%; }
          .chat-entry { padding: 8px 10px; font-size: 13px; }
          .entry-role { min-width: 36px; }
          .oauth-gate { margin: 20px 0; padding: 20px; }
          /* Full-width agent panel on mobile */
          .agent-panel {
            position: fixed;
            left: 10px; right: 10px;
            top: auto;
            bottom: 80px;
            min-width: unset;
          }
          .agent-trigger { font-size: 12px; padding: 6px 10px; }
        }
      `}</style>
    </div>
  );
}
