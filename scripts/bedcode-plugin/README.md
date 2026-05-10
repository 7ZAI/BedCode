# BedCode Claude Code Plugin

Enable remote monitoring of Claude Code from your mobile device.

## Installation

1. Copy this folder to `~/.claude/bedcode/`
2. Run the installer:
   ```bash
   cd ~/.claude/bedcode
   npm install
   node install.js
   ```

## Usage

1. Ensure BedCode desktop app is running
2. In Claude Code, type:
   ```
   /bedcode-on
   ```
3. Open BedCode mobile app to monitor the session
4. To disable, type:
   ```
   /bedcode-off
   ```

## How It Works

- **Output Monitoring**: BedCode listens to Claude Code's JSONL message log file
- **Input Injection**: Your input is written to a pending file, read by the Stop Hook

## Requirements

- BedCode desktop app running
- Node.js 18+