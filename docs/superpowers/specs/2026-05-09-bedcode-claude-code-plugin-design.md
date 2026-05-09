# BedCode Claude Code Plugin Design

## 概述

通过 Claude Code 插件机制实现非 BedCode 启动的 Claude Code 进程的远程监控和输入注入。用户离开电脑前输入 `/bedcode-on`，即可在手机上监看对话并发送输入。

**核心思路**：输出通过 BedCode 监听 Claude Code 的 JSONL 对话文件（`notify` 监听文件追加），输入通过 Claude Code 的 Stop Hook 机制（读检查文件注入），插件 Daemon 通过 WebSocket 与 BedCode 做注册和心跳。

## 架构总览

```
┌──────────┐  WebSocket   ┌────────────────────┐   notify     ┌──────────────┐
│ 手机 App  │◄───────────►│  BedCode Desktop    │◄────────────│  JSONL 文件   │
└──────────┘              │                     │              └──────────────┘
                          │  ┌───────────────┐  │  WebSocket   ┌──────────────┐
                          │  │ PluginManager  │◄─────────────│ 插件 Daemon   │
                          │  │ (新模块)       │  │              │ (Claude Code │
                          │  └───┬───────────┘  │              │  进程内)     │
                          │      │ 文件写        │              └──────────────┘
                          │      ▼               │  Stop Hook   ┌──────────────┐
                          │  ┌───────────────┐  │◄─────────────│ Claude Code  │
                          │  │ pending-input │  │  轮询文件     │ 进程         │
                          │  │ .txt          │  │              │              │
                          │  └───────────────┘  │              └──────────────┘
                          └────────────────────┘
```

### 关键设计决策

- **输入走文件，不走 HTTP**：Stop Hook 脚本读写约定检查文件，零网络依赖，最稳定
- **插件不启动额外 HTTP 端口**：仅通过 WebSocket 连 BedCode 做注册和心跳
- **输出监控用 `notify` crate**：监听 JSONL 文件追加，不轮询
- **插件会话与 PTY 会话平行**：共用 WebSocket 通道和前端，但输出来源和输入路径不同

## 组件设计

### 1. PluginManager（Rust，`src-tauri/src/plugin/`）

BedCode 内的新模块，负责管理所有插件会话。

| 职责 | 说明 |
|------|------|
| 接受注册 | 监听 WebSocket 新消息类型 `RegisterPluginSession`，记录会话元数据 |
| 文件监听 | 用 `notify` crate 监听 JSONL 文件 `Write` 事件，读取新增行，解析 JSON |
| 输出推送 | 将解析后的结构化内容推入现有 broadcast channel，复用 `OutputForwarder` |
| 输入暂存 | 收到手机端输入后，写入 `<project>/.claude/bedcode-pending-input.txt` |
| 心跳检测 | 90s 无心跳标记会话为 `Disconnected` |

```rust
// 内部结构
struct PluginSessionState {
    session_id: String,
    project_name: String,
    project_path: PathBuf,
    jsonl_path: PathBuf,
    watcher: Option<RecommendedWatcher>,
    last_heartbeat: Instant,
}

struct PluginManager {
    sessions: Arc<RwLock<HashMap<String, PluginSessionState>>>,
    output_tx: broadcast::Sender<PtyOutputEvent>,
    db: Arc<Mutex<Database>>,
}
```

**激活方式**：支持两种路径
- A: Claude Code 内 `/bedcode-on` → 启动 daemon → daemon 连 BedCode WebSocket
- B: BedCode 桌面端 UI → 扫描进程 → 写入 `.claude/settings.json` hook 配置

### 2. 插件 Daemon（Node 脚本）

Claude Code 内启动的轻量进程，由 `/bedcode-on` slash command 触发。

| 职责 | 说明 |
|------|------|
| 注册会话 | 连 BedCode WebSocket，发 `RegisterPluginSession` |
| 心跳保活 | 每 30s 发 `PluginHeartbeat` |
| 退出通知 | 收到 SIGTERM/SIGINT 时发 `UnregisterPluginSession` |

Daemon 生命期与 Claude Code 进程绑定。Claude Code 退出时子进程自动终止。

### 3. Stop Hook 脚本（Shell）

Claude Code 每次回复完成时调用。

```bash
#!/bin/bash
# 位置：<project>/.claude/hooks/bedcode-stop-hook.sh
PENDING_FILE="./.claude/bedcode-pending-input.txt"

if [ -f "$PENDING_FILE" ] && [ -s "$PENDING_FILE" ]; then
    cat "$PENDING_FILE"
    > "$PENDING_FILE"
fi
```

### 4. 会话类型扩展

`SessionInfo` 新增字段：

```rust
enum SessionType {
    Pty,
    Plugin,
}
```

插件会话不经过 `PtySession` 生命周期（无需 PTY pair、写线程、kill），直接在 `PluginManager` 内管理。

## 通信协议

在现有 `ControlAction` 基础上新增：

```rust
// Plugin → BedCode
RegisterPluginSession {
    project_name: String,
    project_path: String,
    jsonl_path: String,      // 绝对路径
}

// BedCode → Plugin
RegisteredPluginSession {
    session_id: String,
}

// Plugin → BedCode
UnregisterPluginSession {
    session_id: String,
}

// Plugin → BedCode (heartbeat)
PluginHeartbeat {
    session_id: String,
}
```

- 输入/输出消息**复用现有** `Message::Input` / `Message::Output`，手机端不感知差异
- 会话 ID 由 BedCode 分配（UUID v4）

## 数据流

### 输出流（Claude Code → 手机）

```
Claude 回复追加到 JSONL
       ↓
notify 检测到 Write 事件
       ↓
PluginManager 读取新增行，解析 JSON
       ↓
按类型格式化文本 → PtyOutputEvent
       ↓
broadcast channel → OutputForwarder → WebSocket → 手机
```

JSONL 解析与格式化规则：

| 消息类型 | 显示格式 |
|---------|---------|
| `user` | `[You]: <message>` |
| `assistant` | `<text>` (直接输出) |
| `tool_use` | `[Tool: <name>]: <input>` |
| `tool_result` | `[Result]: <content truncated to 500 chars>` |
| `system` | 跳过，不显示 |

### 输入流（手机 → Claude Code）

```
手机发 Input 消息 → WebSocket → PluginManager
       ↓
PluginManager 写入 .claude/bedcode-pending-input.txt
       ↓
Claude Code 回复完成 → Stop Hook 触发
       ↓
Hook 脚本读取并清空文件 → stdout 输出内容
       ↓
Claude Code 将 stdout 作为下一轮上下文，自动回复
```

输入只支持纯文本，不支持特殊键（与 PTY 会话不同）。

## 会话生命周期

```
/bedcode-on → 1. slash command 启动 daemon
               2. daemon 连 WebSocket，发 RegisterPluginSession
               3. BedCode 分配 sessionId，启动 JSONL 监听
               4. 创建 SessionInfo(type=Plugin)
       ↓
[Active] → daemon 心跳，文件监听运行
       ↓
/bedcode-off 或 Claude Code 退出 → daemon 发 UnregisterPluginSession
       ↓
BedCode 收到 → 停止文件监听，标记 SessionInfo 为 Stopped
```

异常断开处理：daemon 崩溃 / 90s 无心跳 → 标记为 `Disconnected`，保留在会话列表，用户可手动清理。

## 端口发现

Daemon 需要知道 BedCode WebSocket 的端口才能连接。方案：

**BedCode 启动时写入端口文件**：
- 路径：`<app_data_dir>/bedcode-port.txt`（如 `C:\Users\xxx\AppData\Roaming\com.bedcode.app\bedcode-port.txt`）
- 内容：纯文本端口号，如 `9527`
- 每次启动覆盖写入

Daemon 启动后读取此文件获取端口，连接 `ws://127.0.0.1:<port>`。

`install.js` 安装时也会将端口文件的路径写入 slash command 配置，作为命令行参数传给 daemon。

## 文件布局

```
src-tauri/src/plugin/         # Rust 新模块
├── mod.rs                     # PluginManager 和公共类型
└── jsonl.rs                   # JSONL 解析器

src/composables/usePluginSession.ts  # 前端插件会话操作

scripts/bedcode-plugin/        # 插件 daemon 脚本
├── daemon.js                  # daemon 入口
└── install.js                 # 安装 slash command 和 hook 配置
```

## 测试策略

| 层 | 测试内容 |
|----|---------|
| `jsonl.rs` 单元测试 | 各消息类型解析、异常 JSON、空文件 |
| `PluginManager` 测试 | 注册/注销/超时/文件监听 |
| 集成测试 | daemon → WebSocket → PluginManager 端到端 |
| 前端测试 | Plugin 会话在列表中的展示 |
