#!/usr/bin/env node

const WebSocket = require('ws');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

// Configuration
const DEFAULT_PORT = 9527;
const HEARTBEAT_INTERVAL = 30000; // 30 seconds

class BedCodeDaemon {
  constructor() {
    this.ws = null;
    this.sessionId = null;
    this.heartbeatTimer = null;
    this.running = true;
  }

  async start() {
    // Get BedCode port from file
    const portFile = process.env.BEDCODE_PORT_FILE ||
      path.join(process.env.APPDATA || '', 'com.bedcode.app', 'bedcode-port.txt');

    let port = DEFAULT_PORT;
    if (fs.existsSync(portFile)) {
      port = parseInt(fs.readFileSync(portFile, 'utf8').trim(), 10);
    }

    // Get JSONL path from environment
    const jsonlPath = process.env.CLAUDE_MESSAGE_LOG;
    if (!jsonlPath) {
      console.error('[BedCode] CLAUDE_MESSAGE_LOG not set. Cannot start.');
      process.exit(1);
    }

    // Get project info
    const projectPath = process.cwd();
    const projectName = path.basename(projectPath);

    console.log(`[BedCode] Starting daemon for project: ${projectName}`);
    console.log(`[BedCode] JSONL path: ${jsonlPath}`);
    console.log(`[BedCode] Connecting to BedCode on port ${port}...`);

    // Connect to BedCode WebSocket
    this.ws = new WebSocket(`ws://127.0.0.1:${port}`);

    this.ws.on('open', () => {
      console.log('[BedCode] Connected to BedCode');
      this.registerSession(projectName, projectPath, jsonlPath);
    });

    this.ws.on('message', (data) => {
      this.handleMessage(JSON.parse(data.toString()));
    });

    this.ws.on('close', () => {
      console.log('[BedCode] Disconnected from BedCode');
      this.stop();
    });

    this.ws.on('error', (err) => {
      console.error('[BedCode] WebSocket error:', err.message);
    });
  }

  registerSession(projectName, projectPath, jsonlPath) {
    this.send({
      type: 'control',
      payload: {
        type: 'register_plugin_session',
        project_name: projectName,
        project_path: projectPath,
        jsonl_path: jsonlPath
      }
    });
  }

  handleMessage(message) {
    if (message.type === 'control') {
      const action = message.payload;

      if (action.type === 'registered_plugin_session') {
        this.sessionId = action.session_id;
        console.log(`[BedCode] Registered with session ID: ${this.sessionId}`);
        this.startHeartbeat();
      }
    } else if (message.type === 'input') {
      // Forward input to Stop Hook mechanism
      this.handleInput(message.payload.data, message.payload.special_key);
    }
  }

  handleInput(data, specialKey) {
    const pendingFile = path.join(process.cwd(), '.claude', 'bedcode-pending-input.txt');

    let content = data;
    if (specialKey) {
      // Convert special key to format expected by Stop Hook
      const keyMap = {
        'ctrl_c': '__CTRL_C__',
        'ctrl_d': '__CTRL_D__',
        'ctrl_z': '__CTRL_Z__',
        'escape': '__ESCAPE__',
        'tab': '__TAB__',
        'enter': '__ENTER__',
        'arrow_up': '__ARROW_UP__',
        'arrow_down': '__ARROW_DOWN__',
        'arrow_left': '__ARROW_LEFT__',
        'arrow_right': '__ARROW_RIGHT__',
        'backspace': '__BACKSPACE__'
      };
      content = keyMap[specialKey] || data;
    }

    fs.writeFileSync(pendingFile, content);
    console.log(`[BedCode] Input written to pending file: ${content}`);
  }

  startHeartbeat() {
    this.heartbeatTimer = setInterval(() => {
      if (this.ws && this.ws.readyState === WebSocket.OPEN && this.sessionId) {
        this.send({
          type: 'control',
          payload: {
            type: 'plugin_heartbeat',
            session_id: this.sessionId
          }
        });
      }
    }, HEARTBEAT_INTERVAL);
  }

  send(message) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    }
  }

  stop() {
    this.running = false;
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
    }

    if (this.ws && this.sessionId) {
      this.send({
        type: 'control',
        payload: {
          type: 'unregister_plugin_session',
          session_id: this.sessionId
        }
      });
    }

    process.exit(0);
  }
}

// Handle shutdown signals
const daemon = new BedCodeDaemon();
process.on('SIGTERM', () => daemon.stop());
process.on('SIGINT', () => daemon.stop());

// Start the daemon
daemon.start().catch(err => {
  console.error('[BedCode] Failed to start:', err);
  process.exit(1);
});