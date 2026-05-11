# BedCode Claude Code Plugin

Remote monitoring and control for BedCode desktop app.

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

### Enable Monitoring
```
/bedcode on
```

### Disable Monitoring
```
/bedcode off
```

### Check Status
```
/bedcode status
```

## How It Works

1. **Session Start**: When Claude Code starts a new session, the `SessionStart` hook automatically records the JSONL log path to `.claude/bedcode-session.json`

2. **Enable Monitoring**: Running `/bedcode on` reads the session file and connects to BedCode desktop app via WebSocket to register the session

3. **Remote Monitoring**: BedCode monitors the JSONL file and streams output to connected mobile devices

4. **Input**: Mobile devices can send input which is written to `.claude/bedcode-pending-input.txt`, read by the Stop hook

## Requirements

- BedCode desktop app running
- WebSocket client (socat, nc, or curl) for sending messages

## Platform-Specific

- **Windows**: Port file at `%APPDATA%\com.bedcode.app\bedcode-port.txt`
- **macOS**: Port file at `~/Library/Application Support/com.bedcode.app/bedcode-port.txt`
- **Linux**: Port file at `~/.config/com.bedcode.app/bedcode-port.txt`

## Files

```
bedcode/
├── .claude-plugin/
│   └── plugin.json          # Plugin manifest
├── commands/
│   └── bedcode.md           # /bedcode command
├── hooks/
│   └── hooks.json           # SessionStart hook config
└── scripts/
    ├── lib.sh               # WebSocket utilities
    └── session-start.sh     # Session start hook
```