/**
 * Minimal SSE frame parser for ReadableStream bodies.
 *
 * Handles the server-sent events format: lines prefixed with `data:` or `event:`.
 * Each complete event block (terminated by blank line) is yielded.
 */
export interface SseFrame {
  event: string;
  data: string;
}

/**
 * Parse an SSE ReadableStream body into an async iterable of SseFrame objects.
 *
 * Usage:
 *   const res = await fetch(url, { method: 'POST', ... });
 *   for await (const frame of parseSseStream(res.body!)) { ... }
 */
export async function* parseSseStream(
  body: ReadableStream<Uint8Array>
): AsyncGenerator<SseFrame> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let currentEvent = "";

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      // Process complete lines
      let nl: number;
      while ((nl = buffer.indexOf("\n")) !== -1) {
        const rawLine = buffer.slice(0, nl);
        buffer = buffer.slice(nl + 1);
        const line = rawLine.replace(/\r$/, "");

        if (line === "") {
          // Blank line = end of event block; reset event type
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
    reader.releaseLock();
  }
}
