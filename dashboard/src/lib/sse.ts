export interface SseFrame {
  event: string;
  data: string;
}

/**
 * Parse an SSE ReadableStream body into an async iterable of SseFrame objects.
 *
 * Pass an AbortSignal so every reader.read() races against it — without this,
 * a pending read() blocks generator.return() indefinitely when the server keeps
 * the TCP connection alive (e.g. Vite dev proxy), preventing setStreaming(false).
 */
export async function* parseSseStream(
  body: ReadableStream<Uint8Array>,
  signal?: AbortSignal
): AsyncGenerator<SseFrame> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let currentEvent = "";

  // Rejects when the signal fires; raced against reader.read() each iteration.
  // Suppressed in finally so a late-firing signal doesn't become an unhandled rejection.
  const abortPromise: Promise<never> | null = signal
    ? new Promise<never>((_, reject) => {
        if (signal.aborted) {
          reject(signal.reason ?? new DOMException("Aborted", "AbortError"));
        } else {
          signal.addEventListener(
            "abort",
            () => reject(signal.reason ?? new DOMException("Aborted", "AbortError")),
            { once: true }
          );
        }
      })
    : null;

  try {
    while (true) {
      const { done, value } = abortPromise
        ? await Promise.race([reader.read(), abortPromise])
        : await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      let nl: number;
      while ((nl = buffer.indexOf("\n")) !== -1) {
        const rawLine = buffer.slice(0, nl);
        buffer = buffer.slice(nl + 1);
        const line = rawLine.replace(/\r$/, "");

        if (line === "") {
          currentEvent = "";
          continue;
        }

        if (line.startsWith("event:")) {
          currentEvent = line.slice(6).trim();
          continue;
        }

        if (line.startsWith("data:")) {
          const data = line.slice(5).trim();
          yield { event: currentEvent || "message", data };
          continue;
        }

        // Ignore comment lines (`:`) and unknown fields
      }
    }
  } finally {
    abortPromise?.catch(() => {}); // suppress late-firing signal rejection
    reader.cancel().catch(() => {});
  }
}
