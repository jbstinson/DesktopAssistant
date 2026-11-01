#!/usr/bin/env bash
# Spike: runs claude -p once per hook mode in a throwaway project and reports what happened.
set -u
SRC="$(cd "$(dirname "$0")" && pwd)"
ROOT="$1"; shift
unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT
mkdir -p "$ROOT/.claude"; cp "$SRC/hook.mjs" "$ROOT/hook.mjs"; cd "$ROOT"
cat > .claude/settings.json <<JSON
{ "hooks": { "PermissionRequest": [{ "hooks": [{ "type": "command", "command": "node \"$(cygpath -m "$ROOT")/hook.mjs\"", "timeout": 3 }] }] } }
JSON
for mode in "$@"; do
  echo "$mode" > mode.txt; rm -f marker.txt
  start=$(date +%s)
  out=$(claude -p "Use the Bash tool to run exactly this command and nothing else: node -e \"require('fs').writeFileSync('marker.txt','x')\". Then reply with one word: finished." --model haiku --max-turns 3 2>&1 | tail -2 | tr '\n' ' ')
  secs=$(( $(date +%s) - start ))
  ran=no; [ -f marker.txt ] && ran=YES
  fired=$(grep -c "\"mode\":\"$mode\"" events.jsonl 2>/dev/null || echo 0)
  echo "$mode | command ran: $ran | hook fired: $fired | ${secs}s | ${out:0:110}"
done
