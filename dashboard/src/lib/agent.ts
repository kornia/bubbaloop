/**
 * Agent types and daemon discovery utilities.
 *
 * Mirrors the Rust wire format from agent/gateway.rs.
 */
import { Reply, ReplyError, Duration } from "@eclipse-zenoh/zenoh-ts";
import type { Session } from "@eclipse-zenoh/zenoh-ts";

// ── Wire types (mirror agent/gateway.rs) ────────────────────────────

export type AgentEventType =
  | "delta"
  | "tool"
  | "tool_result"
  | "image"
  | "error"
  | "done"
  | "system";

export interface AgentEvent {
  /** Correlation ID matching the inbox message. */
  id: string;
  type: AgentEventType;
  /** Text payload (delta text, tool name, error message, etc.) */
  text?: string;
  /** Tool input JSON snippet (only on "tool" events) */
  input?: string;
  /** Base64-encoded image data (only on "image" events) */
  image?: string;
  /** MIME type for the image, e.g. "image/jpeg" (only on "image" events) */
  media_type?: string;
}

export interface AgentManifest {
  agent_id: string;
  name: string;
  capabilities: string[];
  model: string;
  is_default: boolean;
  machine_id: string;
}

export interface DaemonManifest {
  http_endpoint: string;
  version: string;
  machine_id: string;
}

/** A discovered agent bundled with its daemon's HTTP endpoint. */
export interface ResolvedAgent {
  agent: AgentManifest;
  /** HTTP base URL for the daemon hosting this agent. */
  http_endpoint: string;
}

// ── Discovery ────────────────────────────────────────────────────────

const AGENT_MANIFEST_WILDCARD = "bubbaloop/global/*/agent/*/manifest";
const DISCOVERY_TIMEOUT_MS = 4000;

/**
 * Discover all agents and their hosting daemon HTTP endpoints via Zenoh.
 *
 * Queries both the agent manifest and daemon manifest topics, then joins
 * by machine_id so each agent carries its daemon's HTTP endpoint.
 */
export async function discoverAgents(
  session: Session
): Promise<ResolvedAgent[]> {
  const agentReplies = await queryWithTimeout(
    session,
    AGENT_MANIFEST_WILDCARD,
    DISCOVERY_TIMEOUT_MS
  );

  // http_endpoint is empty — ChatView uses the Vite proxy (/api/v1/...) directly.
  const resolved: ResolvedAgent[] = [];
  for (const reply of agentReplies) {
    const agent = tryParseJson<AgentManifest>(reply);
    if (!agent?.agent_id || !agent?.machine_id) continue;
    resolved.push({ agent, http_endpoint: "" });
  }

  return resolved;
}

async function queryWithTimeout(
  session: Session,
  selector: string,
  timeoutMs: number
): Promise<string[]> {
  // Collect replies from the receiver into the array, stopping after timeoutMs
  // regardless of whether the zenoh-js async iterator closes on its own.
  const collect = async (): Promise<string[]> => {
    const receiver = await session.get(selector, {
      timeout: Duration.milliseconds.of(timeoutMs),
    });
    if (!receiver) return [];

    const replies: string[] = [];
    for await (const replyItem of receiver) {
      let sample: unknown;
      if (replyItem instanceof Reply) {
        const result = replyItem.result();
        if (result instanceof ReplyError) continue;
        sample = result;
      } else {
        sample = replyItem;
      }
      const bytes = (sample as { payload: () => { toBytes: () => Uint8Array } })
        ?.payload?.()?.toBytes?.();
      if (!bytes) continue;
      replies.push(new TextDecoder().decode(bytes));
    }
    return replies;
  };

  // Race against a hard deadline — zenoh-js receivers don't always close when
  // the server-side timeout fires, which leaves the for-await loop hanging.
  const deadline = new Promise<string[]>((resolve) =>
    setTimeout(() => resolve([]), timeoutMs + 500)
  );

  try {
    return await Promise.race([collect(), deadline]);
  } catch {
    return [];
  }
}

function tryParseJson<T>(text: string): T | null {
  try {
    return JSON.parse(text) as T;
  } catch {
    return null;
  }
}

// ── Auth token storage ────────────────────────────────────────────────

const TOKEN_KEY = "bubbaloop_mcp_token";

export function loadToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? "";
}

export function saveToken(token: string): void {
  if (token.trim()) {
    localStorage.setItem(TOKEN_KEY, token.trim());
  }
}
