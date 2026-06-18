# BedCode Claude Code Plugin

Monitor Claude Code sessions through the BedCode desktop app.

## Installation

1. Copy this directory to `~/.claude/plugins/bedcode/`:
   ```bash
   cp -r scripts/bedcode-plugin ~/.claude/plugins/bedcode
   ```

2. Or run the installer:
   ```bash
   cd scripts/bedcode-plugin
   ./install.sh
   ```

3. Restart Claude Code to load the plugin

## Usage

### Check Session Status
```
/bedcode status
```

## How It Works

1. **Session Start**: When Claude Code starts a new session, the `SessionStart` hook records the session info to `.claude/bedcode-events.jsonl`

2. **Task Monitoring**: `Stop` and `SubagentStop` hooks analyze task status using LLM-based prompt hooks, writing events like:
   - `completed` - Task finished
   - `in_progress` - Task ongoing
   - `asking` - Waiting for user input
   - `interrupted` - Task interrupted

3. **Event Log**: All events are written to `.claude/bedcode-events.jsonl` in JSONL format for the BedCode desktop app to consume

## Event Format

Events are written to `.claude/bedcode-events.jsonl`:

```jsonl
{"event":"session_start","session_id":"abc123","project_path":"/path","timestamp":"2026-06-18T10:00:00Z"}
{"event":"stop","session_id":"abc123","status":"completed","reason":"Task done","timestamp":"2026-06-18T10:30:00Z"}
```

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
    ├── session-start.sh     # Session start hook (Unix)
    ├── session-start.cmd    # Session start hook (Windows)
    ├── write-event.sh       # Write event script (Unix)
    └── write-event.cmd      # Write event script (Windows)
```

## Requirements

- BedCode desktop app running
- `jq` for JSON parsing (Unix only, Windows uses built-in commands)
