---
description: View BedCode session status
allowed-tools: Read
---

# BedCode Session Status

View the current Claude Code session status and recent events.

## Usage

- `/bedcode status` - Display current session and recent events

## Implementation

### For /bedcode status:

1. Read the events file: `cat .claude/bedcode-events.jsonl 2>/dev/null || echo "No events file"`
2. If file exists, show the last 10 events
3. Display current session_id from the most recent session_start event
4. Show the latest task status from the most recent stop event

Example output format:
```
Session: abc123
Project: /path/to/project

Recent events:
- session_start (10:00:00)
- stop (completed, 10:30:00)
```