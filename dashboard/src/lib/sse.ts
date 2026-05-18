export interface SseFrame {
  event: string;
  data: string;
}

/**
 * Parse an SSE ReadableStream body into an async iterable of SseFrame objects.
 *
 * Pass an AbortSignal to make every reader.read() race against it so the
 * generator can be interrupted immediately when the signal fires — without
 * this, a pending read() blocks generator.return() indefinitely when the
 * server keeps the TCP connection alive (e.g. Vite dev proxy).
 *
 * Usage:
 *   const ctrl = new AbortController();
 *   const res  = await fetch(url, { signal: ctrl.signal, ... });
 *   for await (const frame of parseSseStream(res.body!, ctrl.signal)) { ... }
 */
export async function* parseSseStream(
  body: ReadableStream<Uint8Array>,
  signal?: AbortSignal
): AsyncGenerator<SseFrame> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let currentEvent = "";

  // A promise that rejects the moment the abort signal fires.
  // Kept outside the loop so we only allocate it once.
  // Suppressed in finally to avoid unhandled-rejection warnings after the
  // generator exits (the signal may fire after we've already cleaned up).
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
      // Race the read against the abort signal so that ctrl.abort() (Stop
      // button) or an explicit abort on "done"/"error" immediately unblocks
      // a pending reader.read() — otherwise generator.return() can't run
      // and the outer finally { setStreaming(false) } never executes.
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
    // Suppress any future rejection from abortPromise so it doesn't become
    // an unhandled rejection after the generator has exited.
    abortPromise?.catch(() => {});
    reader.cancel().catch(() => {});
  }
}
