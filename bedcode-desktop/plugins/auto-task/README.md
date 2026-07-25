# Auto Task Plugin (Desktop)

桌面端 Claude Code 任务状态同步与自动授权插件。核心业务逻辑在 Rust WASM 层实现，TS 前端负责 UI 渲染。

## 架构

- **Rust WASM 层**：全部业务逻辑（任务状态管理、队列调度、hooks 管理、数据库操作）
- **TS 前端**：UI 渲染与用户交互，通过 Tauri invoke 调用 Rust 命令
- **Claude Code Hooks**：`auto_task_hook.py` 在 Claude Code 生命周期事件时通过 HTTP API 推送任务状态
- **HTTP 端点**：`/api/plugin/com.bedcode.auto-task/{path}` 代理路由，供 hooks 和移动端调用

## 目录结构

```
auto-task/
├── plugin.json          # 插件清单（权限、命令、视图、生命周期钩子）
├── rust/
│   ├── Cargo.toml       # Rust 依赖配置
│   └── src/
│       ├── lib.rs       # WASM 入口 + 命令路由 + 数据库建表
│       ├── state.rs     # 任务状态与自动授权模式管理（HTTP 端点处理）
│       ├── queue.rs     # 任务队列管理（添加、删除、调度、自动模式切换）
│       ├── hooks.rs     # Claude Code 项目 hooks 管理（安装/清理）
│       └── token.rs     # 插件 Token 校验
├── scripts/
│   ├── build.js         # 统一构建脚本（Vite + Cargo WASM + 复制产物）
│   └── auto_task_hook.py  # Claude Code hook 脚本（推送任务状态到 HTTP 端点）
├── src/                 # TS 前端源码
├── dist/                # Vite 构建产物
└── vite.config.ts       # Vite 配置
```

## 编译

### 完整构建（前端 + Rust WASM）

```bash
cd bedcode-desktop/plugins/auto-task
node scripts/build.js
```

### 仅构建前端

```bash
node scripts/build.js --frontend-only
```

### 仅构建 Rust WASM

```bash
node scripts/build.js --rust-only
```

### 手动编译 Rust WASM

Release：

```bash
cd bedcode-desktop/plugins/auto-task/rust
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release
```

Debug：

```bash
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm
```

## 产物部署

构建脚本自动将产物复制到：

```
bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.auto-task/
├── index.js                              # TS 前端
├── plugin.json                           # 插件清单
├── bedcode_plugin_auto_task.wasm         # Rust WASM
└── auto_task_hook.py                       # Claude Code hook 脚本
```

手动复制 WASM（Debug 构建）：

```bash
cp rust/target/wasm32-unknown-unknown/debug/bedcode_plugin_auto_task.wasm \
   ../../src-tauri/resources/plugins/desktop/com.bedcode.auto-task/
```

## 依赖

- `bedcode-plugin-api`：桌面端插件 SDK（`packages/plugin-sdk-desktop/rust`，启用 `wasm` feature）
- `serde` / `serde_json` / `anyhow`

## 数据库表

插件使用独立 SQLite 数据库（通过 `plugin_db_execute` / `plugin_db_query` 操作）：

| 表 | 用途 |
|----|------|
| `task_history` | 任务历史记录（状态、时间戳、自动授权标记） |
| `session_mapping` | Claude Code session ↔ BedCode PTY session 映射 |
| `task_queue` | 待执行任务队列（position 排序） |

## HTTP 端点

通过 `/api/plugin/com.bedcode.auto-task/{path}` 访问：

| 方法 | 路径 | 用途 |
|------|------|------|
| POST | `task-status` | 接收 Claude Code hook 推送的任务状态 |
| POST | `session-mode` | 设置会话自动授权模式 |
| GET | `session-mode` | 查询会话自动授权模式 |
| POST | `task-queue/add` | 添加任务到队列 |
| DELETE | `task-queue/remove` | 从队列删除任务 |
| GET | `task-queue/list` | 查询队列 |
| POST | `task-queue/clear` | 清空队列 |

## 插件权限

| 权限 | 用途 |
|------|------|
| `storage` | 插件独立数据库 |
| `broadcast` | 广播状态变更到移动端 |
| `terminal:input` | 向终端发送命令（队列调度） |
| `terminal:output` | 读取终端输出 |
| `session:read` | 读取会话信息 |
| `fs:read` / `fs:write` | 读写项目 hooks 文件 |
| `ui:sidebar` | 侧边栏任务历史视图 |

## 生命周期钩子

| 钩子 | 触发时机 | 行为 |
|------|----------|------|
| `onStartup` | 插件启动 | 清理旧版全局 hooks、初始化数据库表 |
| `onShutdown` | 插件关闭 | 日志记录 |
| `onSessionLifecycle(creating)` | Claude 会话创建 Claude 会话前 | 自动安装项目 `.claude/hooks.json` |

## 消息总线 Topic

| Topic | 用途 |
|-------|------|
| `task:status-changed` | 任务状态变更通知 |
| `session:mode-changed` | 会话自动授权模式变更通知 |
| `task:queue-changed` | 任务队列变更通知 |
