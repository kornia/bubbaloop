import { useEffect, useRef, useState } from "react";
import { Session } from "@eclipse-zenoh/zenoh-ts";

export interface LivelinessEvent {
  keyExpr: string;
  type: "join" | "leave";
  timestamp: number;
}

export interface LivelinessState {
  aliveNodes: Set<string>;
  events: LivelinessEvent[];
}

/**
 * Hook that subscribes to Zenoh liveliness changes for instant node detection.
 * Falls back gracefully if liveliness API is not available.
 */
export function useLiveliness(
  session: Session | null,
  onNodeJoin?: (keyExpr: string) => void,
  onNodeLeave?: (keyExpr: string) => void,
): LivelinessState {
  const [aliveNodes, setAliveNodes] = useState<Set<string>>(new Set());
  const [events, setEvents] = useState<LivelinessEvent[]>([]);
  const onNodeJoinRef = useRef(onNodeJoin);
  const onNodeLeaveRef = useRef(onNodeLeave);

  // Keep callback refs updated without triggering re-subscription
  useEffect(() => {
    onNodeJoinRef.current = onNodeJoin;
  }, [onNodeJoin]);

  useEffect(() => {
    onNodeLeaveRef.current = onNodeLeave;
  }, [onNodeLeave]);

  useEffect(() => {
    if (!session) return;

    let cleanup: (() => void) | undefined;

    // Try to subscribe to liveliness
    const setupLiveliness = async () => {
      try {
        // Check if liveliness API exists on session
        const livelinessApi = (session as any).liveliness?.();
        if (!livelinessApi) {
          console.log(
            "[Liveliness] API not available in this zenoh-ts version",
          );
          return;
        }

        const subscriber = await livelinessApi.declare_subscriber(
          "bubbaloop/**",
          {
            callback: (sample: any) => {
              const keyExpr =
                sample.key_expr?.toString?.() ||
                sample.keyexpr?.toString?.() ||
                "";
              const kind = sample.kind; // PUT = join, DELETE = leave

              const event: LivelinessEvent = {
                keyExpr,
                type: kind === "DELETE" ? "leave" : "join",
                timestamp: Date.now(),
              };

              setEvents((prev) => [...prev.slice(-99), event]); // Keep last 100 events

              if (event.type === "join") {
                setAliveNodes((prev) => {
                  const next = new Set(prev);
                  next.add(keyExpr);
                  return next;
                });
                onNodeJoinRef.current?.(keyExpr);
              } else {
                setAliveNodes((prev) => {
                  const next = new Set(prev);
                  next.delete(keyExpr);
                  return next;
                });
                onNodeLeaveRef.current?.(keyExpr);
              }
            },
          },
        );

        cleanup = () => subscriber?.undeclare?.();
      } catch (e) {
        console.log("[Liveliness] Not supported:", e);
      }
    };

    setupLiveliness();

    return () => cleanup?.();
  }, [session]);

  return { aliveNodes, events };
}
