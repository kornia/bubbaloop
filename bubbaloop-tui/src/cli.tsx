#!/usr/bin/env node
import React, { useState, useEffect } from "react";
import { render } from "ink";
import App from "./App.js";

// Wrapper component that manages render key for force re-render on disconnect
const AppWrapper: React.FC = () => {
  const [renderKey, setRenderKey] = useState(0);

  // Expose a global function to trigger re-render
  useEffect(() => {
    (global as any).__bubbaloop_forceRerender = () => {
      setRenderKey((k) => k + 1);
    };
    return () => {
      delete (global as any).__bubbaloop_forceRerender;
    };
  }, []);

  return <App key={renderKey} />;
};

// Check if we're in a TTY
const isTTY = process.stdin.isTTY ?? false;

if (!isTTY) {
  console.error("Error: bubbaloop-tui requires an interactive terminal.");
  console.error("Please run this command in a terminal that supports TTY.");
  process.exit(1);
}

// Enter alternate screen buffer for fullscreen experience
process.stdout.write("\x1b[?1049h"); // Enter alternate screen
process.stdout.write("\x1b[H"); // Move cursor to home (top-left)
process.stdout.write("\x1b[?25l"); // Hide cursor

// Cleanup function to restore terminal
const cleanup = () => {
  process.stdout.write("\x1b[?25h"); // Show cursor
  process.stdout.write("\x1b[?1049l"); // Exit alternate screen
};

// Handle various exit signals
process.on("exit", cleanup);
process.on("SIGINT", () => {
  cleanup();
  process.exit(0);
});
process.on("SIGTERM", () => {
  cleanup();
  process.exit(0);
});

// Render the app with patchConsole disabled to prevent zenoh output from corrupting UI
const { unmount, waitUntilExit } = render(<AppWrapper />, { patchConsole: false });

// Wait for app to exit
waitUntilExit().then(() => {
  unmount();
  cleanup();
  process.exit(0);
});
