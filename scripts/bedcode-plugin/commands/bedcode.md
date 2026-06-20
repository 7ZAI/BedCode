---
description: View BedCode session status
allowed-tools: Read
---

# BedCode Session Status

View the current Claude Code session status and recent plugin events.

## Usage

- `/bedcode status` - Display current session and recent events

## Implementation

### For /bedcode status:

1. Read the plugin log file: `.claude/bedcode-plugin.log`
2. If file exists, show the last 20 lines
3. Display current session_id from the most recent session_start log entry
4. Show the latest task status from the most recent stop/subagent_stop log entry

Example output format:
```
Session: abc123
Project: /path/to/project

Recent events:
[10:00:00] session_start: session_id=abc123 project=/path
[10:00:00] HTTP POST → idle
[10:30:00] stop: session_id=abc123 status=completed
```
