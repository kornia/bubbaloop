#!/usr/bin/env node
import React, { useState, useEffect } from "react";
import { render, Box, Text } from "ink";
import App from "./App.js";

// Double Ctrl+C state
let ctrlCCount = 0;
let ctrlCTimeout: NodeJS.Timeout | null = null;
const CTRL_C_TIMEOUT_MS = 2000;

// Global callback for notifying app of first Ctrl+C
let onFirstCtrlC: (() => void) | null = null;
let onCtrlCReset: (() => void) | null = null;

// Wrapper component that manages render key for force re-render on disconnect
const AppWrapper: React.FC = () => {
  const [renderKey, setRenderKey] = useState(0);
  const [showExitWarning, setShowExitWarning] = useState(false);

  // Expose a global function to trigger re-render
  useEffect(() => {
    (global as any).__bubbaloop_forceRerender = () => {
      setRenderKey((k) => k + 1);
    };

    // Set up Ctrl+C callbacks
    onFirstCtrlC = () => setShowExitWarning(true);
    onCtrlCReset = () => setShowExitWarning(false);

    return () => {
      delete (global as any).__bubbaloop_forceRerender;
      onFirstCtrlC = null;
      onCtrlCReset = null;
    };
  }, []);

  return (
    <Box flexDirection="column">
      <App key={renderKey} />
      {showExitWarning && (
        <Box marginX={1} marginTop={1}>
          <Text color="#FF6B6B" bold>Press Ctrl+C again to exit</Text>
        </Box>
      )}
    </Box>
  );
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
  ctrlCCount++;

  if (ctrlCCount === 1) {
    // First Ctrl+C - show warning
    onFirstCtrlC?.();

    // Reset after timeout
    ctrlCTimeout = setTimeout(() => {
      ctrlCCount = 0;
      onCtrlCReset?.();
    }, CTRL_C_TIMEOUT_MS);
  } else if (ctrlCCount >= 2) {
    // Second Ctrl+C - exit
    if (ctrlCTimeout) clearTimeout(ctrlCTimeout);
    cleanup();
    process.exit(0);
  }
});
process.on("SIGTERM", () => {
  cleanup();
  process.exit(0);
});

// Render the app with patchConsole disabled to prevent zenoh output from corrupting UI
// exitOnCtrlC: false to handle double Ctrl+C manually
const { unmount, waitUntilExit } = render(<AppWrapper />, { patchConsole: false, exitOnCtrlC: false });

// Wait for app to exit
waitUntilExit().then(() => {
  unmount();
  cleanup();
  process.exit(0);
});
