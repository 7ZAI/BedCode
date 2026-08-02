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
│       └── hooks.rs     # Claude Code 项目 hooks 管理（安装/清理）
├── scripts/
│   ├── build.js         # 统一构建脚本（Vite + Cargo WASM + 复制产物）
│   └── auto_task_hook.py  # Claude Code hook 脚本（推送任务状态到 HTTP 端点）
├── src/                 # TS 前端源码
│   ├── components/      # TaskHistoryView（侧边栏历史）、AutoTaskModal（队列弹窗）
│   ├── i18n/            # 插件翻译表（zh-CN / en，MessageSchema 编译期校验同步）
│   └── state.ts         # 插件前端共享状态（弹窗可见性）
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

> 产物目录（`**/src-tauri/resources/plugins/`）已加入 .gitignore，不入库；
> 由 `scripts/build.js` 生成，打包/运行前需先执行构建。

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

## 命令（WASM invoke_command）

| 命令 | 用途 |
|------|------|
| `auto-task.add-task` | 添加任务到队列（空队列时自动开启自动模式并立即调度） |
| `auto-task.remove-task` | 从队列删除待执行任务（删空后退出自动模式） |
| `auto-task.clear-queue` | 清空队列并退出自动模式 |
| `auto-task.update-task` | 编辑待执行任务的 prompt（仅 pending 状态可改） |
| `auto-task.reorder-queue` | 按给定 id 顺序重排队列（id 集合必须与 pending 集合一致） |
| `auto-task.list-task-queue` | 查询会话队列 |
| `auto-task.list-task-history` | 查询会话任务历史 |
| `auto-task.get-task-status` | 查询会话任务状态 |
| `auto-task.set-auto-mode` | 设置会话自动授权模式 |
| `auto-task.cleanup-project-hooks` | 清理项目 hooks（保留用户自定义 hooks） |

> 命令 ID 与 manifest `contributes.commands[].id` 全名一致，前端按全名调用。

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
| `ui:input` | 终端工具栏按钮（打开队列弹窗） |

## 生命周期钩子

| 钩子 | 触发时机 | 行为 |
|------|----------|------|
| `onStartup` | 插件启动 | 清理旧版全局 hooks、初始化数据库表 |
| `onShutdown` | 插件关闭 | 日志记录 |
| `onSessionLifecycle(creating)` | Claude 会话创建前 | 自动安装项目 `.claude/settings.json` hooks |

## 事件通道

同一事件名经三条通道投递，消费方各取所需：

| Topic | 消息总线（插件间） | emit_event（前端 UI） | broadcast_sync（移动端） |
|-------|:---:|:---:|:---:|
| `task:status-changed` | ✓ | ✓ | ✓ |
| `session:mode-changed` | ✓ | ✓ | ✓ |
| `task:queue-changed` | ✓ | ✓ | ✓ |

