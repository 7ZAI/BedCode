#!/bin/bash
# BedCode Session Start Hook
# Records session creation event to JSONL file

set -euo pipefail

# Read hook input from stdin
INPUT=$(cat)

# Extract fields
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
CWD=$(echo "$INPUT" | jq -r '.cwd // empty')
TRANSCRIPT_PATH=$(echo "$INPUT" | jq -r '.transcript_path // empty')
PERMISSION_MODE=$(echo "$INPUT" | jq -r '.permission_mode // empty')

# Validate session_id
if [ -z "$SESSION_ID" ]; then
  echo '{"error": "missing session_id"}' >&2
  exit 2
fi

# Generate timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Ensure output directory exists
OUTPUT_FILE="${CLAUDE_PROJECT_DIR}/.claude/bedcode-events.jsonl"
mkdir -p "$(dirname "$OUTPUT_FILE")"

# Write session_start event to JSONL file
# Using printf for safe JSON escaping
printf '{"event":"session_start","session_id":"%s","project_path":"%s","transcript_path":"%s","permission_mode":"%s","timestamp":"%s"}\n' \
  "$SESSION_ID" "$CWD" "$TRANSCRIPT_PATH" "$PERMISSION_MODE" "$TIMESTAMP" >> "$OUTPUT_FILE"

# Output success for debugging
echo "{\"status\":\"recorded\",\"session_id\":\"$SESSION_ID\",\"file\":\"$OUTPUT_FILE\"}"