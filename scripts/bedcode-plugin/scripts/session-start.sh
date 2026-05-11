#!/bin/bash
# BedCode Session Start Hook
# Records the JSONL log path for remote monitoring

SESSION_FILE="$CLAUDE_PROJECT_DIR/.claude/bedcode-session.json"

# CLAUDE_MESSAGE_LOG is set by Claude Code when it starts a session
# It contains the path to the message log file

if [ -n "$CLAUDE_MESSAGE_LOG" ]; then
    # Extract session ID from the path (last component of projects directory)
    SESSION_ID=$(basename "$(dirname "$CLAUDE_MESSAGE_LOG")")

    # Write session info
    echo "{\"jsonl_path\": \"$CLAUDE_MESSAGE_LOG\", \"session_id\": \"$SESSION_ID\", \"project_path\": \"$CLAUDE_PROJECT_DIR\"}" > "$SESSION_FILE"

    echo "{\"status\": \"recorded\", \"jsonl_path\": \"$CLAUDE_MESSAGE_LOG\"}"
else
    # Claude Code may not set this in all versions
    echo "{\"status\": \"no_env\", \"message\": \"CLAUDE_MESSAGE_LOG not set\"}"
fi