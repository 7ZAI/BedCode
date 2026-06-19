#!/bin/bash
# BedCode Write Event Script
# Writes stop/subagent_stop events to JSONL file after prompt analysis

set -euo pipefail

# Read hook input from stdin
INPUT=$(cat)

# Extract basic fields
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
HOOK_EVENT=$(echo "$INPUT" | jq -r '.hook_event_name // empty')
REASON=$(echo "$INPUT" | jq -r '.reason // empty')

# Validate session_id
if [ -z "$SESSION_ID" ]; then
  echo '{"error": "missing session_id"}' >&2
  exit 2
fi

# Determine event type
EVENT_TYPE="stop"
if [ "$HOOK_EVENT" = "SubagentStop" ]; then
  EVENT_TYPE="subagent_stop"
fi

# Try to parse prompt hook result from tool_result
# The prompt hook output will be in tool_result field
PROMPT_RESULT=$(echo "$INPUT" | jq -r '.tool_result // empty' 2>/dev/null || echo "")

STATUS="unknown"
STATUS_REASON="$REASON"

if [ -n "$PROMPT_RESULT" ] && [ "$PROMPT_RESULT" != "null" ] && [ "$PROMPT_RESULT" != "" ]; then
  # Try to extract JSON from prompt result (might have markdown wrapper)
  EXTRACTED_JSON=$(echo "$PROMPT_RESULT" | grep -o '{[^}]*"status"[^}]*}' | head -1 || echo "")

  if [ -n "$EXTRACTED_JSON" ]; then
    PARSED_STATUS=$(echo "$EXTRACTED_JSON" | jq -r '.status // "unknown"' 2>/dev/null || echo "unknown")
    PARSED_REASON=$(echo "$EXTRACTED_JSON" | jq -r '.reason // ""' 2>/dev/null || echo "")

    if [ "$PARSED_STATUS" != "unknown" ]; then
      STATUS="$PARSED_STATUS"
      STATUS_REASON="$PARSED_REASON"
    fi
  fi
fi

# Fallback: use reason field if prompt didn't give us status
if [ "$STATUS" = "unknown" ] && [ -n "$REASON" ]; then
  case "$REASON" in
    "complete"|":complete")
      STATUS="completed"
      STATUS_REASON="Task completed"
      ;;
    "tool_use")
      STATUS="in_progress"
      STATUS_REASON="Tool use in progress"
      ;;
    *)
      STATUS="in_progress"
      STATUS_REASON="$REASON"
      ;;
  esac
fi

# Generate timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Ensure output directory exists
OUTPUT_FILE="${CLAUDE_PROJECT_DIR}/.claude/bedcode-events.jsonl"
mkdir -p "$(dirname "$OUTPUT_FILE")"

# Write event to JSONL file
echo "{\"event\":\"$EVENT_TYPE\",\"session_id\":\"$SESSION_ID\",\"status\":\"$STATUS\",\"reason\":\"$STATUS_REASON\",\"timestamp\":\"$TIMESTAMP\"}" >> "$OUTPUT_FILE"

# Output success for debugging
echo "{\"event\":\"$EVENT_TYPE\",\"status\":\"$STATUS\",\"written\":true}"

# 推送状态到 BedCode 桌面端 HTTP API
BEDCODE_PORT="${BEDCODE_PORT:-8080}"
if [ -n "$BEDCODE_TOKEN" ]; then
  REASON_ESCAPED=$(echo "$STATUS_REASON" | sed 's/"/\\"/g')
  curl -s -X POST "http://localhost:${BEDCODE_PORT}/api/plugin/task-status" \
    -H "Content-Type: application/json" \
    -d "{\"session_id\":\"$SESSION_ID\",\"status\":\"$STATUS\",\"reason\":\"$REASON_ESCAPED\",\"token\":\"$BEDCODE_TOKEN\"}" \
    > /dev/null 2>&1 &
fi