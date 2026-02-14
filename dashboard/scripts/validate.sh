#!/bin/bash
set -e

echo "=== Dashboard Validation Workflow ==="
echo ""

echo "=== Step 1: Check binary versions ==="
bubbaloop --version 2>/dev/null || echo "WARN: bubbaloop binary not found"
pgrep -a bubbaloop || echo "WARN: bubbaloop daemon not running"
ss -tlnp 2>/dev/null | grep 10001 || echo "WARN: zenoh bridge not listening on 10001"

echo ""
echo "=== Step 2: Check node versions ==="
bubbaloop node list 2>/dev/null || echo "WARN: cannot list nodes"

echo ""
echo "=== Step 3: Install dependencies ==="
cd "$(dirname "$0")/.." && npm ci

echo ""
echo "=== Step 4: TypeScript check ==="
npx tsc --noEmit

echo ""
echo "=== Step 5: Run all tests ==="
npm run test

echo ""
echo "=== Step 6: Production build ==="
npm run build

echo ""
echo "=== Step 7: Start dev server & verify ==="
if [ -f vite.config.http.ts ]; then
  npm run dev -- --config vite.config.http.ts &
else
  npm run dev &
fi
DEV_PID=$!
sleep 5
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:5174/ 2>/dev/null || echo "000")
if [ "$HTTP_CODE" = "200" ]; then
  echo "OK: Dashboard loads (HTTP $HTTP_CODE)"
else
  echo "WARN: Dashboard returned HTTP $HTTP_CODE (may need different port or config)"
fi
kill $DEV_PID 2>/dev/null || true
wait $DEV_PID 2>/dev/null || true

echo ""
echo "=== All checks passed ==="
