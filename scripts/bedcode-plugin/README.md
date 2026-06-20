# BedCode Claude Code Plugin

Monitor Claude Code sessions through the BedCode desktop app.

## Requirements

- Python 3.6+（系统自带，零额外依赖）
- BedCode desktop app running（可选，用于状态推送）

## Installation

1. Copy this directory to `~/.claude/plugins/bedcode/`:
   ```bash
   cp -r scripts/bedcode-plugin ~/.claude/plugins/bedcode
   ```

2. Or load locally for testing:
   ```bash
   claude --plugin-dir ./scripts/bedcode-plugin
   ```

3. Restart Claude Code to load the plugin

## Usage

### Check Session Status
```
/bedcode status
```

## How It Works

1. **SessionStart**: When Claude Code starts a new session, the hook records session info and pushes `idle` status to the BedCode desktop app

2. **Task Monitoring**: `Stop` and `SubagentStop` hooks analyze task status using LLM-based prompt hooks, detecting:
   - `completed` - Task finished
   - `in_progress` - Task ongoing
   - `asking` - Waiting for user input
   - `interrupted` - Task interrupted

3. **Logging**: All hook events and HTTP requests are logged to `.claude/bedcode-plugin.log` with daily rotation (7-day retention)

4. **HTTP Push**: Task status changes are pushed to `POST /api/plugin/task-status` when `BEDCODE_TOKEN` is set

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `CLAUDE_PROJECT_DIR` | Auto | - | Project root (set by Claude Code) |
| `CLAUDE_PLUGIN_ROOT` | Auto | - | Plugin root (set by Claude Code) |
| `BEDCODE_TOKEN` | Optional | - | Auth token for HTTP push |
| `BEDCODE_PORT` | Optional | `8765` | HTTP API port |

## Files

```
bedcode/
├── .claude-plugin/
│   └── plugin.json          # Plugin manifest
├── commands/
│   └── bedcode.md           # /bedcode command
├── hooks/
│   └── hooks.json           # Hook configuration
└── scripts/
    └── bedcode_hook.py      # Unified hook script (Python)
```

## Event Log

Events are logged to `.claude/bedcode-plugin.log`:

```
[2026-06-20T10:00:00Z] [INFO] HOOK session_start: session_id=abc123 project=/path source=startup permission=default
[2026-06-20T10:00:00Z] [INFO] HTTP POST http://localhost:8765/api/plugin/task-status session_id=abc123 status=idle
[2026-06-20T10:00:00Z] [INFO] HTTP response: 200 {"code":0,"data":null}
[2026-06-20T10:30:00Z] [INFO] HOOK stop: session_id=abc123 status=completed reason=Task done
```
