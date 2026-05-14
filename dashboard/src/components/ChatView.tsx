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
  | { kind: "connection_lost" };

// ── Helpers ──────────────────────────────────────────────────────────

function AgentSelector({
  agents,
  selected,
  onSelect,
}: {
  agents: ResolvedAgent[];
  selected: ResolvedAgent | null;
  onSelect: (a: ResolvedAgent) => void;
}) {
  if (agents.length === 0) return null;

  return (
    <select
      className="agent-select"
      value={selected?.agent.agent_id ?? ""}
      onChange={(e) => {
        const found = agents.find((a) => a.agent.agent_id === e.target.value);
        if (found) onSelect(found);
      }}
    >
      {agents.map((a) => (
        <option key={a.agent.agent_id} value={a.agent.agent_id}>
          {a.agent.name} ({a.agent.machine_id.slice(0, 8)})
          {a.agent.is_default ? " ★" : ""}
        </option>
      ))}
    </select>
  );
}

function EntryRow({ entry }: { entry: ChatEntry }) {
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
          <span className="tool-chip">
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
            </svg>
            {entry.name}
          </span>
          {entry.input && (
            <details className="tool-details">
              <summary>input</summary>
              <pre className="tool-json">{entry.input}</pre>
            </details>
          )}
        </div>
      );
    case "tool_result":
      return (
        <div className="chat-entry chat-tool-result">
          <span className="tool-result-chip">result</span>
          <span className="entry-text tool-result-text">{entry.text}</span>
        </div>
      );
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
  }
}

// ── OAuth explainer ──────────────────────────────────────────────────

function OAuthGate({ onDismiss }: { onDismiss: () => void }) {
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
      <button className="oauth-dismiss" onClick={onDismiss}>
        Dismiss
      </button>
    </div>
  );
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
  const [oauthBlocked, setOauthBlocked] = useState(false);

  const abortRef = useRef<AbortController | null>(null);
  const logEndRef = useRef<HTMLDivElement>(null);
  // Accumulates assistant delta text across SSE frames before flushing to state
  const deltaBufferRef = useRef("");

  // Auto-scroll to bottom when new entries arrive
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [entries]);

  // Discover agents when session becomes available
  useEffect(() => {
    if (!session) return;
    setDiscovering(true);
    discoverAgents(session)
      .then((found) => {
        setAgents(found);
        const def = found.find((a) => a.agent.is_default) ?? found[0] ?? null;
        setSelectedAgent(def);
      })
      .finally(() => setDiscovering(false));
  }, [session]);

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
    setStreaming(true);
    setOauthBlocked(false);
    deltaBufferRef.current = "";

    const ctrl = new AbortController();
    abortRef.current = ctrl;

    const url = `${selectedAgent.http_endpoint}/api/v1/agents/${selectedAgent.agent.agent_id}/turns`;

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
            setOauthBlocked(true);
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

  if (oauthBlocked) {
    return (
      <div className="chat-view">
        <OAuthGate onDismiss={() => setOauthBlocked(false)} />
      </div>
    );
  }

  return (
    <div className="chat-view">
      {/* Token setup */}
      {!token && (
        <div className="token-setup">
          <h3>Enter daemon token</h3>
          <p>
            Paste the MCP token from your Jetson (<code>cat ~/.bubbaloop/mcp-token</code>)
          </p>
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

      {/* Header bar */}
      <div className="chat-header">
        <div className="chat-header-left">
          {token ? (
            <>
              {!session ? (
                <span className="no-session-hint">Connect to Zenoh to discover agents</span>
              ) : discovering ? (
                <span className="discovering-hint">Discovering agents...</span>
              ) : agents.length === 0 ? (
                <span className="no-agents-hint">No agents found — is the daemon running?</span>
              ) : (
                <AgentSelector
                  agents={agents}
                  selected={selectedAgent}
                  onSelect={setSelectedAgent}
                />
              )}
            </>
          ) : null}
        </div>
        <div className="chat-header-right">
          {token && (
            <button
              className="token-reset-btn"
              onClick={() => { setToken(""); setTokenDraft(""); localStorage.removeItem("bubbaloop_mcp_token"); }}
              title="Change token"
            >
              Token ✕
            </button>
          )}
          {agents.length > 0 && session && (
            <button
              className="rediscover-btn"
              onClick={() => {
                setDiscovering(true);
                discoverAgents(session)
                  .then((found) => {
                    setAgents(found);
                    const def = found.find((a) => a.agent.is_default) ?? found[0] ?? null;
                    setSelectedAgent(def);
                  })
                  .finally(() => setDiscovering(false));
              }}
              disabled={discovering}
              title="Refresh agent list"
            >
              ↻
            </button>
          )}
        </div>
      </div>

      {/* Message log */}
      <div className="chat-log">
        {entries.length === 0 && token && selectedAgent && (
          <div className="chat-empty">
            <p>Connected to <strong>{selectedAgent.agent.name}</strong></p>
            <p className="chat-empty-hint">Try: "what nodes are running?" or "describe the current system state"</p>
          </div>
        )}
        {entries.map((entry, i) => (
          <EntryRow key={i} entry={entry} />
        ))}
        <div ref={logEndRef} />
      </div>

      {/* Input area */}
      <div className="chat-input-area">
        <textarea
          className="chat-textarea"
          placeholder={
            !token
              ? "Enter token above to start chatting"
              : !selectedAgent
              ? "No agent selected"
              : streaming
              ? "Waiting for response..."
              : "Message agent (Enter to send, Shift+Enter for newline)"
          }
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={!token || !selectedAgent || streaming}
          rows={3}
        />
        <div className="chat-actions">
          {streaming ? (
            <button className="stop-btn" onClick={handleStop}>
              Stop
            </button>
          ) : (
            <button
              className="send-btn"
              onClick={handleSend}
              disabled={!token || !selectedAgent || !input.trim()}
            >
              Send
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
          max-width: 900px;
          margin: 0 auto;
          width: 100%;
          padding: 16px;
          gap: 12px;
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
          justify-content: space-between;
          align-items: center;
          gap: 8px;
          min-height: 32px;
        }
        .chat-header-left {
          display: flex;
          align-items: center;
          gap: 8px;
          flex: 1;
        }
        .chat-header-right {
          display: flex;
          align-items: center;
          gap: 6px;
        }
        .no-session-hint, .discovering-hint, .no-agents-hint {
          font-size: 13px;
          color: var(--text-muted);
        }
        .agent-select {
          padding: 5px 10px;
          background: var(--bg-secondary);
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-primary);
          font-size: 13px;
          cursor: pointer;
          outline: none;
        }
        .agent-select:focus { border-color: var(--accent-primary); }
        .token-reset-btn, .rediscover-btn {
          padding: 4px 10px;
          background: none;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-muted);
          font-size: 12px;
          cursor: pointer;
          transition: all 0.15s;
        }
        .token-reset-btn:hover, .rediscover-btn:hover {
          border-color: var(--accent-primary);
          color: var(--text-primary);
        }

        /* Message log */
        .chat-log {
          flex: 1;
          overflow-y: auto;
          display: flex;
          flex-direction: column;
          gap: 8px;
          padding: 4px 0;
          min-height: 0;
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
        }
        .chat-assistant .entry-role { color: var(--accent-secondary); }

        .chat-tool {
          background: rgba(0, 229, 255, 0.05);
          border: 1px solid rgba(0, 229, 255, 0.15);
          flex-direction: column;
          gap: 6px;
        }
        .tool-chip {
          display: inline-flex;
          align-items: center;
          gap: 5px;
          padding: 3px 10px;
          background: rgba(0, 229, 255, 0.12);
          border: 1px solid rgba(0, 229, 255, 0.25);
          border-radius: 20px;
          font-size: 12px;
          font-weight: 600;
          color: var(--accent-secondary);
          font-family: 'JetBrains Mono', monospace;
        }
        .tool-details summary {
          font-size: 11px;
          color: var(--text-muted);
          cursor: pointer;
          user-select: none;
          margin-left: 4px;
        }
        .tool-json {
          margin: 4px 0 0;
          padding: 8px;
          background: var(--bg-primary);
          border-radius: 4px;
          font-size: 11px;
          font-family: 'JetBrains Mono', monospace;
          color: var(--text-secondary);
          overflow-x: auto;
          white-space: pre;
        }

        .chat-tool-result {
          background: rgba(0, 200, 83, 0.05);
          border: 1px solid rgba(0, 200, 83, 0.12);
          align-items: flex-start;
        }
        .tool-result-chip {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.06em;
          text-transform: uppercase;
          color: var(--success);
          min-width: 44px;
          padding-top: 2px;
          flex-shrink: 0;
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

        /* Input area */
        .chat-input-area {
          display: flex;
          gap: 10px;
          align-items: flex-end;
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

        .send-btn, .stop-btn {
          padding: 10px 20px;
          border: none;
          border-radius: 8px;
          font-size: 14px;
          font-weight: 600;
          cursor: pointer;
          transition: all 0.15s;
          white-space: nowrap;
        }
        .send-btn {
          background: var(--accent-primary);
          color: white;
        }
        .send-btn:hover:not(:disabled) { filter: brightness(1.1); }
        .send-btn:disabled { opacity: 0.4; cursor: not-allowed; }
        .stop-btn {
          background: rgba(255, 23, 68, 0.15);
          border: 1px solid rgba(255, 23, 68, 0.3);
          color: var(--error);
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

        /* Mobile */
        @media (max-width: 768px) {
          .chat-view { padding: 10px; }
          .token-setup { padding: 14px 16px; }
          .token-row { flex-direction: column; }
          .token-save-btn { width: 100%; }
          .chat-entry { padding: 8px 10px; font-size: 13px; }
          .entry-role { min-width: 36px; }
          .oauth-gate { margin: 20px 0; padding: 20px; }
          .chat-input-area { flex-direction: column; }
          .send-btn, .stop-btn { width: 100%; }
        }
      `}</style>
    </div>
  );
}
