# BedCode Code Map

本文档作为项目代码探索的索引入口，记录完整的目录结构和各模块职责。


### 自动化任务执行机制

BedCode 通过 Claude Code 自定义插件 + HTTP API + WebSocket 事件链路实现移动端远程自动执行多个任务。

#### 整体架构

```
Claude Code Hook (Python)
    ↓ HTTP POST
Rust HTTP API (plugin_controller.rs)
    ↓ DesktopSyncEvent
SyncEventHandler → WebSocket broadcast
    ↓ ws_sync_task_status_changed / ws_sync_session_mode_changed
Mobile Tauri Event → useAutoExecutor (状态机)
    ↓ sendInput / HTTP API
Claude Code (PTY)
```

#### 链路详解

**1. Claude Code 插件** (`scripts/bedcode-plugin/`)

Hook 脚本 (`scripts/bedcode_hook.py`) 注册 4 个事件：

| Hook 事件 | 触发时机 | 处理逻辑 |
|-----------|---------|---------|
| SessionStart | Claude Code 会话启动 | 推送 `idle` 状态到桌面端 |
| PreToolUse | Claude Code 调用工具前 | 查询会话模式：自动模式→auto-approve；手动模式→仍推送 asking 状态但不做 auto-approve |
| Stop | Claude Code 停止响应 | 解析任务状态（completed/in_progress/asking/interrupted）并推送 |
| SubagentStop | 子代理停止 | 同 Stop，解析并推送状态 |

**2. HTTP API** (`server/controllers/plugin_controller.rs`)

| 路由 | 方法 | 用途 |
|------|------|------|
| `/plugin/task-status` | POST | 接收插件推送的任务状态变更（含 bedcode_session_id 映射） |
| `/plugin/session-mode` | POST | 移动端设置会话自动/手动模式 |
| `/plugin/session-mode` | GET | Python PreToolUse hook 查询会话模式 |

**3. PluginManager** (`plugin/manager.rs`)

内存存储三个 HashMap：
- `task_states: HashMap<bedcode_session_id, TaskStateEntry>` — 任务状态 + reason + questions
- `auto_modes: HashMap<bedcode_session_id, bool>` — 会话级自动授权模式
- `session_id_map: HashMap<claude_session_id, bedcode_session_id>` — Claude Code ↔ BedCode 会话 ID 映射

每次更新都通过 `DesktopSyncEvent` 广播到所有 WebSocket 客户端。

**4. 移动端自动执行引擎** (`composables/useAutoExecutor.ts`)

按 sessionId 隔离的状态机，核心逻辑：

| 收到状态 | 自动模式行为 | 手动模式行为 |
|---------|-------------|-------------|
| `idle` | 如果有待执行任务则 `startNext()` | 不处理 |
| `in_progress` | 标记当前任务 running | 不处理 |
| `asking` | `handleAsking()` 更新 UI（Python hook 已自动回答） | 不处理（用户在 Claude Code 原生界面操作） |
| `completed` | 标记完成 → `/clear` → 等下次 idle 开始下一个 | 不处理 |
| `interrupted` | 发送"继续"利用上下文从中断点恢复，最多 3 次，超过则标记 failed，开始下一个 | 不处理 |

**5. 模式切换流程**

移动端通过 HTTP 请求切换模式，不经过 PTY：
```
Mobile → POST /api/plugin/session-mode (JWT 认证) → PluginManager 内存更新
    → DesktopSyncEvent::SessionModeChanged → WebSocket broadcast
    → Mobile 收到 ws_sync_session_mode_changed → 同步 UI 状态
```

`POST /api/plugin/session-mode` 支持双认证：Python hook 用 plugin token，移动端用 JWT（`useHttpApi` 自动注入 `Authorization: Bearer <token>` header）。

同时 Python PreToolUse hook 每次被触发时通过 `GET /api/plugin/session-mode` 查询当前模式，
自动模式时返回 `permissionDecision: "allow"` + AskUserQuestion 自动选择推荐项。

**6. 会话 ID 绑定机制**

Claude Code 和 BedCode 各自有独立的 session ID 体系，同一 cwd 下可运行多个 Claude Code 实例，
因此不能通过目录绑定。BedCode 通过进程环境变量实现绑定：

```
BedCode 桌面端启动 PTY 会话
  ↓ pty_process.rs: start()
  ↓ cmd.env("BEDCODE_SESSION_ID", &self.id)
  ↓
Shell 进程继承环境变量 → Claude Code 子进程继承
  ↓
SessionStart hook 触发
  ↓ bedcode_hook.py 读取 os.environ["BEDCODE_SESSION_ID"]
  ↓
POST /plugin/task-status
  ↓ { session_id: "claude-xxx", bedcode_session_id: "pty-uuid", ... }
  ↓
PluginManager.register_session_mapping("claude-xxx" → "pty-uuid")
  ↓
后续所有状态推送和模式查询通过映射关联
```

**无竞态风险**：每个 PTY 进程有独立环境变量空间，多会话互不影响：

```
PTY Session A (PID 1000) → BEDCODE_SESSION_ID=uuid-aaa
PTY Session B (PID 1001) → BEDCODE_SESSION_ID=uuid-bbb
```

**映射使用场景**：

| 场景 | 输入 | 解析 | 查询 key |
|------|------|------|----------|
| task-status 推送 | `claude_session_id` + `bedcode_session_id` | 有 bedcode_sid 时直接用它 | `bedcode_session_id` |
| session-mode 查询 (GET) | `claude_session_id` | `resolve_session_id()` 查映射 | 解析后的 `bedcode_session_id` |
| session-mode 设置 (POST) | `bedcode_session_id`（移动端已知） | 无需解析 | `bedcode_session_id` |

**7. 全局 Hooks 自动配置** (`plugin/setup.rs`)

应用启动时自动完成以下配置，对用户完全无感：

1. 校验/生成 plugin token
2. 将 `bedcode_hook.py` 复制到 `~/.claude/` 目录
3. 在全局 `~/.claude/settings.json` 中注入 hooks 配置（不覆盖已有配置）
4. 注入 `BEDCODE_PORT` 和 `BEDCODE_TOKEN` 环境变量到 hook 命令
5. 验证 hooks 配置是否生效

合并策略：保留用户已有的非 BedCode hooks 和其他顶层字段（如 `permissions`、`env`），
只替换/更新 BedCode 相关的 hook 条目（识别标准：command 字段包含 `bedcode_hook.py`）。




