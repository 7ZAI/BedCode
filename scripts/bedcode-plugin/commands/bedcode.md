---
description: Enable BedCode remote monitoring
argument-hint: [on|off|status]
allowed-tools: Bash, Read
---

# BedCode Remote Monitoring

This command enables remote monitoring of your Claude Code session through the BedCode desktop app.

## Usage

- `/bedcode on` - Enable remote monitoring
- `/bedcode off` - Disable remote monitoring
- `/bedcode status` - Check monitoring status

## How It Works

When enabled, BedCode desktop app will:
1. Monitor your Claude Code session output via JSONL log file
2. Allow you to send input from mobile devices

## First Time Setup

1. Install the BedCode desktop app
2. Make sure BedCode is running
3. Run `/bedcode on` to enable monitoring

## Implementation

### For /bedcode on:

1. Read the session info file: `cat .claude/bedcode-session.json`
2. If the file doesn't exist, run `echo '{"error": "no session"}'`
3. Extract the jsonl_path from the file
4. Read BedCode port from:
   - Windows: `%APPDATA%\com.bedcode.app\bedcode-port.txt`
   - macOS: `~/Library/Application Support/com.bedcode.app/bedcode-port.txt`
   - Linux: `~/.config/com.bedcode.app/bedcode-port.txt`
5. Connect to BedCode WebSocket at `ws://127.0.0.1:<port>`
6. Send register message:
   ```json
   {
     "type": "control",
     "payload": {
       "type": "register_plugin_session",
       "project_name": "<project name>",
       "project_path": "<project path>",
       "jsonl_path": "<jsonl path from session file>"
     }
   }
   ```
7. Confirm success and inform user

### For /bedcode off:

1. Read the session info file: `cat .claude/bedcode-session.json`
2. Extract session_id from the file
3. Connect to BedCode WebSocket
4. Send unregister message:
   ```json
   {
     "type": "control",
     "payload": {
       "type": "unregister_plugin_session",
       "session_id": "<session id>"
     }
   }
   ```
5. Confirm success

### For /bedcode status:

1. Check if .claude/bedcode-session.json exists
2. If it exists, show the recorded session info
3. If not, show "Not monitoring"