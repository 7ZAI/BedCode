#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const os = require('os');

const BEDCODE_DIR = path.join(os.homedir(), '.claude', 'bedcode');
const DAEMON_PATH = path.join(BEDCODE_DIR, 'daemon.js');
const PORT_FILE_TEMPLATE = path.join(process.env.APPDATA || '', 'com.bedcode.app', 'bedcode-port.txt');

console.log('BedCode Plugin Installer');
console.log('=========================\n');

// Step 1: Create BedCode directory
if (!fs.existsSync(BEDCODE_DIR)) {
  fs.mkdirSync(BEDCODE_DIR, { recursive: true });
  console.log('[OK] Created directory:', BEDCODE_DIR);
} else {
  console.log('[OK] Directory already exists:', BEDCODE_DIR);
}

// Step 2: Copy daemon.js
const daemonSrc = path.join(__dirname, 'daemon.js');
if (fs.existsSync(daemonSrc)) {
  fs.copyFileSync(daemonSrc, DAEMON_PATH);
  console.log('[OK] Copied daemon.js to:', DAEMON_PATH);
} else {
  console.error('[ERROR] daemon.js not found in current directory');
  process.exit(1);
}

// Step 3: Read current Claude Code settings
const claudeSettingsPath = path.join(os.homedir(), '.claude', 'settings.json');
let settings = {};

if (fs.existsSync(claudeSettingsPath)) {
  try {
    settings = JSON.parse(fs.readFileSync(claudeSettingsPath, 'utf8'));
  } catch (e) {
    console.warn('[WARN] Failed to parse settings.json, creating new one');
  }
}

// Step 4: Add slash command
if (!settings.slashCommands) {
  settings.slashCommands = {};
}

settings.slashCommands['bedcode-on'] = {
  description: 'Enable BedCode remote monitoring',
  command: `node "${DAEMON_PATH}"`
};

settings.slashCommands['bedcode-off'] = {
  description: 'Disable BedCode remote monitoring',
  command: `pkill -f "node.*daemon.js"`
};

fs.writeFileSync(claudeSettingsPath, JSON.stringify(settings, null, 2));
console.log('[OK] Added slash commands to settings.json');

// Step 5: Find or create .claude directory in current project
const projectClaudeDir = path.join(process.cwd(), '.claude');
if (!fs.existsSync(projectClaudeDir)) {
  fs.mkdirSync(projectClaudeDir, { recursive: true });
  console.log('[OK] Created .claude directory in project');
}

// Step 6: Create Stop Hook script
const hookScript = `#!/bin/bash
# BedCode Stop Hook
# This script is called by Claude Code after each response

PENDING_FILE="./.claude/bedcode-pending-input.txt"

if [ -f "$PENDING_FILE" ] && [ -s "$PENDING_FILE" ]; then
    content=$(cat "$PENDING_FILE")

    if [[ "$content" == __*__ ]]; then
        key_name=$(echo "$content" | sed 's/__//g')
        case "$key_name" in
            CTRL_C) echo -e "\\x03" ;;
            CTRL_D) echo -e "\\x04" ;;
            CTRL_Z) echo -e "\\x1A" ;;
            ESCAPE) echo -e "\\x1B" ;;
            TAB) echo -e "\\t" ;;
            ENTER) echo -e "\\n" ;;
            ARROW_UP) echo -e "\\e[A" ;;
            ARROW_DOWN) echo -e "\\e[B" ;;
            ARROW_LEFT) echo -e "\\e[D" ;;
            ARROW_RIGHT) echo -e "\\e[C" ;;
            BACKSPACE) echo -e "\\x7F" ;;
            *) echo "$content" ;;
        esac
    else
        cat "$PENDING_FILE"
    fi

    > "$PENDING_FILE"
fi
`;

const hookScriptPath = path.join(projectClaudeDir, 'hooks');
if (!fs.existsSync(hookScriptPath)) {
  fs.mkdirSync(hookScriptPath, { recursive: true });
}

const hookFilePath = path.join(hookScriptPath, 'bedcode-stop-hook.sh');
fs.writeFileSync(hookFilePath, hookScript);
fs.chmodSync(hookFilePath, '755');
console.log('[OK] Created Stop Hook script:', hookFilePath);

// Step 7: Add hook to settings
settings.hooks = settings.hooks || {};
settings.hooks['Stop'] = `./.claude/hooks/bedcode-stop-hook.sh`;

fs.writeFileSync(claudeSettingsPath, JSON.stringify(settings, null, 2));
console.log('[OK] Registered Stop Hook in settings.json');

console.log('\n=========================');
console.log('Installation complete!');
console.log('\nUsage:');
console.log('  In Claude Code, type: /bedcode-on');
console.log('  To disable, type: /bedcode-off');
console.log('\nNote: BedCode desktop app must be running.');