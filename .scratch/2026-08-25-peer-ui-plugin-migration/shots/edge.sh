#!/bin/bash
# 确保 CDP Edge 在跑并导航到指定 URL（默认桌面 dev-shell）
# 用法: bash edge.sh [url] [等待秒数]
URL="${1:-http://127.0.0.1:5173/}"
WAIT="${2:-6}"
BT="/c/Users/binblink/.pi/agent/skills/pi-skills/browser-tools"
if ! curl -s http://127.0.0.1:9222/json/version >/dev/null 2>&1; then
  ("C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --remote-debugging-port=9222 \
    --user-data-dir="C:\Users\binblink\.cache\browser-tools\edge-profile2" \
    --no-first-run --disable-features=msSignIn \
    --window-size=1440,900 --window-position=10,10 "$URL" > /dev/null 2>&1 &)
  sleep 6
else
  "$BT/browser-nav.js" "$URL" >/dev/null 2>&1
fi
sleep "$WAIT"
"$BT/browser-screenshot.js"
