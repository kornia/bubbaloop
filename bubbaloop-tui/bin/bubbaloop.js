#!/usr/bin/env node

// Wrapper to enable experimental WASM modules
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const cli = join(__dirname, '..', 'dist', 'cli.js');

const child = spawn(process.execPath, ['--experimental-wasm-modules', cli, ...process.argv.slice(2)], {
  stdio: 'inherit',
  env: { ...process.env, NODE_NO_WARNINGS: '1' }
});

child.on('exit', (code) => process.exit(code ?? 0));
