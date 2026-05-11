# BedCode Claude Code Plugin Design

## 概述

通过 Claude Code 插件机制实现非 BedCode 启动的 Claude Code 进程的远程监控和输入注入。用户输入 `/bedcode on` 即可在手机上监看对话并发送输入。

**核心思路**：使用 Claude Code 插件的 Hook 机制，在会话开始时记录 JSONL 路径，通过 WebSocket 与 BedCode 桌面端通信。

## 架构总览

```
┌──────────┐  WebSocket   ┌────────────────────┐   notify     ┌──────────────┐
│ 手机 App  │◄───────────►│  BedCode Desktop    │◄────────────│  JSONL 文件   │
└──────────┘              │                     │              └──────────────┘
                          │  ┌───────────────┐  │
                          │  │ PluginManager  │  │
                          │  │ (已有实现)     │  │
                          │  └───┬───────────┘  │
                          │      │               │
                          │      ▼               │
                          │  ┌───────────────┐  │  Stop Hook   ┌──────────────┐
                          │  │ pending-input │  │◄─────────────│ Claude Code  │
                          │  │ .txt          │  │              │  进程        │
                          │  └───────────────┘  │              └──────────────┘
                          └────────────────────┘
```

### 关键设计决策

- **真插件**：安装到 `~/.claude/plugins/bedcode/`，通过 `/bedcode` 命令启用
- **Hook 记录 JSONL 路径**：使用 `SessionStart` hook 自动记录 `CLAUDE_MESSAGE_LOG` 环境变量
- **输出监控**：BedCode 使用 `notify` 监听 JSONL 文件（已有实现）
- **输入注入**：BedCode 写入 `pending-input.txt`，Stop hook 读取（已有实现）

## 组件设计

### 1. 插件目录结构

```
~/.claude/plugins/bedcode/
├── .claude-plugin/
│   └── plugin.json          # 插件清单
├── commands/
│   └── bedcode.md           # /bedcode on/off/status 命令
├── hooks/
│   └── hooks.json           # Hook 配置
└── scripts/
    ├── lib.sh               # WebSocket 工具函数
    └── session-start.sh     # 会话开始 hook
```

### 2. 插件清单 (plugin.json)

```json
{
  "version": "1.0.0",
  "name": "bedcode",
  "description": "Remote monitoring and control for BedCode desktop app"
}
```

### 3. Hook 配置 (hooks.json)

使用 `SessionStart` hook 记录 JSONL 路径：

```json
{
  "description": "BedCode plugin hooks",
  "hooks": {
    "SessionStart": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PLUGIN_ROOT}/scripts/session-start.sh"
          }
        ]
      }
    ]
  }
}
```

### 4. SessionStart Hook 脚本

```bash
#!/bin/bash
SESSION_FILE="$CLAUDE_PROJECT_DIR/.claude/bedcode-session.json"

if [ -n "$CLAUDE_MESSAGE_LOG" ]; then
    SESSION_ID=$(basename "$(dirname "$CLAUDE_MESSAGE_LOG")")
    echo "{\"jsonl_path\": \"$CLAUDE_MESSAGE_LOG\", \"session_id\": \"$SESSION_ID\", \"project_path\": \"$CLAUDE_PROJECT_DIR\"}" > "$SESSION_FILE"
    echo "{\"status\": \"recorded\", \"jsonl_path\": \"$CLAUDE_MESSAGE_LOG\"}"
else
    echo "{\"status\": \"no_env\", \"message\": \"CLAUDE_MESSAGE_LOG not set\"}"
fi
```

### 5. 命令实现 (/bedcode)

- **/bedcode on** - 启用监控
  1. 读取 `.claude/bedcode-session.json`
  2. 从端口文件获取 BedCode WebSocket 端口
  3. 连接 WebSocket，发送 `register_plugin_session` 消息

- **/bedcode off** - 禁用监控
  1. 读取 session 文件获取 session_id
  2. 发送 `unregister_plugin_session` 消息

- **/bedcode status** - 查看状态

## 通信协议

复用现有协议：

```rust
// Plugin → BedCode
RegisterPluginSession {
    project_name: String,
    project_path: String,
    jsonl_path: String,      // 从 session-start.sh 记录
}

// BedCode → Plugin
RegisteredPluginSession {
    session_id: String,
}
```

## 数据流

### 启用流程

```
用户运行 /bedcode on
       ↓
读取 .claude/bedcode-session.json
       ↓
提取 jsonl_path
       ↓
连接 BedCode WebSocket
       ↓
发送 register_plugin_session
       ↓
BedCode 启动 JSONL 监听
       ↓
移动端开始接收输出
```

### SessionStart 自动记录

```
Claude Code 会话开始
       ↓
SessionStart hook 触发
       ↓
session-start.sh 执行
       ↓
写入 .claude/bedcode-session.json
       ↓
用户运行 /bedcode on 时读取
```

## 端口发现

BedCode 启动时写入端口文件：
- Windows: `%APPDATA%\com.bedcode.app\bedcode-port.txt`
- macOS: `~/Library/Application Support/com.bedcode.app/bedcode-port.txt`
- Linux: `~/.config/com.bedcode.app/bedcode-port.txt`

## 文件布局

```
scripts/bedcode-plugin/           # 插件源码（用户复制到 ~/.claude/plugins/）
├── .claude-plugin/
│   └── plugin.json
├── commands/
│   └── bedcode.md
├── hooks/
│   └── hooks.json
└── scripts/
    ├── lib.sh
    └── session-start.sh

src-tauri/src/plugin/             # BedCode 桌面端（已有）
├── mod.rs                        # PluginManager
└── jsonl.rs                      # JSONL 解析器

docs/superpowers/specs/           # 本文档
```

## 插件安装

用户需要将插件目录复制到 Claude Code 插件目录：

```bash
cp -r scripts/bedcode-plugin ~/.claude/plugins/bedcode
```

然后重启 Claude Code 以加载插件。

## 测试策略

| 层 | 测试内容 |
|----|---------|
| SessionStart hook | JSONL 路径正确记录到 session 文件 |
| /bedcode on | WebSocket 连接成功，会话注册成功 |
| /bedcode off | 会话正确注销 |
| 移动端输出 | 能接收 JSONL 监听推送的输出 |
| 移动端输入 | 能发送输入，Stop hook 正确读取 |