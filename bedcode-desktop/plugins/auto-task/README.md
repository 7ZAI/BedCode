# Auto Task Plugin (Desktop)

桌面端 Agent 任务队列与自动授权插件：同步 Claude Code / pi / opencode / Codex 任务状态，支持任务队列调度、预设任务、定时任务与历史统计；agent 请求授权时自动放行。核心业务逻辑在 Rust WASM 层实现，TS 前端负责 UI 渲染。

## 功能

- **多 Agent 支持**：Claude Code / pi / opencode / Codex 四种 CLI Agent 的完整适配（见下方「Agent 集成」）
- **任务队列**：添加 / 移除 / 清空 / 编辑 / 排序 / 取消，队列顺序调度；`source` 标记区分手动与定时来源
- **自动执行**：开启后入队任务自动调度到当前会话（空队列时自动开启并立即调度）
- **自动应答**：Agent 提问（权限请求 / AskUserQuestion）由 hook 自动回复，无需人工干预
- **预设任务**：常用任务一键预存，随时入队（one-shot 消耗语义，入队即删除）
- **定时任务**：指定触发时刻新建会话并依次执行，错过可 reset 重新调度
- **任务历史**：状态统计（`task-history-stats`）、按会话/来源筛选、失败任务可重试

## 使用

Claude Code 会话创建时自动安装项目 hooks 同步任务状态；终端工具栏按钮打开任务队列弹窗，侧边栏「任务历史」查看执行记录与统计。

## Agent 集成

| Agent | 集成载体 | 上下文清理 | 说明 |
|-------|----------|-----------|------|
| claude | `.claude/settings.json` hooks + `auto_task_hook.py` | `/clear` | 完整适配 |
| pi | `.pi/extensions/pi_task_hook.ts`（pi 自动发现，无需注册） | `/new` | 完整适配；pi 无 `/clear`，用 `/new` 重建上下文 |
| opencode | `.opencode/plugins/opencode_task_hook.ts`（自动加载，无需注册） | 无 | 调度跳过 clear 直接下发；TUI 型 agent 需首轮下发兜底（见「定时任务」） |
| codex | `.codex/hooks.json` hooks + `codex_task_hook.py` | `/clear` | 完整适配；非托管 hooks 需用户在 `/hooks` 中信任后才能运行 |

`set-platform` 命令按会话设置 agent 平台（CLI 命令关键词自动识别：`claude` / `codex` / `opencode` / `pi`）。

## 架构

- **Rust WASM 层**：全部业务逻辑（任务状态管理、队列调度、hooks 管理、数据库操作、定时调度）
- **TS 前端**：UI 渲染与用户交互，通过 Tauri invoke 调用 Rust 命令
- **Agent hooks 脚本**：`scripts/` 下 4 个集成脚本在 agent 生命周期事件时通过 HTTP API 推送任务状态
- **HTTP 端点**：`/api/plugin/com.bedcode.auto-task/{path}` 代理路由，供 hooks 和移动端调用
- **宿主定时器**：宿主按 1s 周期回调 `auto-task.scheduler-tick`（注入 `now_utc`），驱动定时任务调度与 waiting 态延迟 clear

## 目录结构

```
auto-task/
├── plugin.json          # 插件清单（权限、命令、视图、生命周期钩子）
├── rust/
│   ├── Cargo.toml       # Rust 依赖配置
│   └── src/
│       ├── lib.rs       # WASM 入口 + 命令路由 + 数据库建表 + 定时器注册
│       ├── agent.rs     # Agent 支持表（claude / pi / opencode / codex）与 CLI 识别
│       ├── state.rs     # 任务状态、自动授权/自动应答模式管理（HTTP 端点处理）
│       ├── queue.rs     # 任务队列管理（添加、删除、调度、自动模式切换、延迟 clear）
│       ├── scheduled.rs # 定时任务状态机（pending → creating → executed / failed / missed）
│       ├── preset.rs    # 预设任务管理（one-shot 消耗语义）
│       └── hooks.rs     # 多 agent 项目 hooks 管理（安装/清理）
├── scripts/
│   ├── build.js         # 统一构建脚本（Vite + Cargo WASM + 复制产物）
│   ├── auto_task_hook.py    # Claude Code hook 脚本
│   ├── pi_task_hook.ts      # pi 扩展（部署到项目 .pi/extensions/）
│   ├── opencode_task_hook.ts # opencode 插件（部署到项目 .opencode/plugins/）
│   └── codex_task_hook.py   # Codex hook 脚本
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
├── auto_task_hook.py                     # Claude Code hook 脚本
├── pi_task_hook.ts                       # pi 扩展脚本
├── opencode_task_hook.ts                 # opencode 插件脚本
└── codex_task_hook.py                    # Codex hook 脚本
```

> 产物目录（`**/src-tauri/resources/plugins/`）已加入 .gitignore，不入库；
> 由 `scripts/build.js` 生成，打包/运行前需先执行构建。

## 依赖

- `bedcode-plugin-api`：桌面端插件 SDK（`packages/plugin-sdk-desktop/rust`，启用 `wasm` feature）
- `serde` / `serde_json` / `anyhow`

## 数据库表

插件使用独立 SQLite 数据库（通过 `plugin_db_execute` / `plugin_db_query` 操作）：

| 表 | 用途 |
|----|------|
| `task_history` | 任务历史记录（状态、时间戳、自动授权标记） |
| `session_mapping` | Agent session ↔ BedCode PTY session 映射 |
| `task_queue` | 待执行任务队列（position 排序；`source` 标记来源：queue / scheduled，`dispatch_attempts` 调度计数） |
| `session_settings` | 会话级开关（`auto_execute` 自动执行、`auto_answer` 自动应答） |
| `scheduled_jobs` | 定时任务（trigger_at、prompts、状态机字段） |
| `preset_tasks` | 预设任务（one-shot 消耗，入队即删除） |

## HTTP 端点

通过 `/api/plugin/com.bedcode.auto-task/{path}` 访问：

| 方法 | 路径 | 用途 |
|------|------|------|
| POST | `task-status` | 接收 agent hook 推送的任务状态 |
| GET | `task-status` | 查询任务状态 |
| POST | `session-mode` | 设置会话自动授权/自动应答模式 |
| GET | `session-mode` | 查询会话模式 |
| GET | `session-settings` | 查询会话设置（auto_execute / auto_answer） |
| GET | `task-history/current` | 查询会话当前任务 |
| GET | `task-history/list` | 查询任务历史（带筛选） |
| GET | `supported-agents` | 查询支持的 agent 列表 |
| POST | `task-queue/add` | 添加任务到队列 |
| DELETE | `task-queue/remove` | 从队列删除任务 |
| GET | `task-queue/list` | 查询队列 |
| POST | `task-queue/clear` | 清空队列 |

## 命令（WASM invoke_command）

| 命令 | 用途 |
|------|------|
| `auto-task.add-task` | 添加任务到队列（空队列时自动开启自动模式并立即调度） |
| `auto-task.cancel-task` | 取消执行中的任务 |
| `auto-task.remove-task` | 从队列删除待执行任务（删空后退出自动模式） |
| `auto-task.clear-queue` | 清空队列并退出自动模式 |
| `auto-task.update-task` | 编辑待执行任务的 prompt（仅 pending 状态可改） |
| `auto-task.reorder-queue` | 按给定 id 顺序重排队列（id 集合必须与 pending 集合一致） |
| `auto-task.list-task-queue` | 查询会话队列 |
| `auto-task.list-task-history` | 查询会话任务历史 |
| `auto-task.task-history-stats` | 任务历史统计 |
| `auto-task.get-task-status` | 查询会话任务状态 |
| `auto-task.set-auto-mode` | 设置会话自动授权模式 |
| `auto-task.get-session-settings` | 查询会话设置（auto_execute / auto_answer） |
| `auto-task.list-running-sessions` | 查询运行中会话 |
| `auto-task.set-platform` | 设置会话 agent 平台（claude / pi / opencode / codex） |
| `auto-task.list-session-configs` | 查询会话配置列表 |
| `auto-task.list-supported-agents` | 查询支持的 agent 列表 |
| `auto-task.list-preset-tasks` | 查询预设任务 |
| `auto-task.create-preset-task` / `update-preset-task` / `delete-preset-task` | 预设任务 CRUD |
| `auto-task.add-preset-to-queue` | 预设任务入队（原子：先删预设后入队） |
| `auto-task.list-scheduled-jobs` | 查询定时任务 |
| `auto-task.create-scheduled-job` | 创建定时任务（触发时新建会话并依次执行 prompts） |
| `auto-task.delete-scheduled-job` | 删除定时任务 |
| `auto-task.reset-scheduled-job` | 重置定时任务（改触发时间后重新调度） |
| `auto-task.scheduler-tick` | 宿主定时器回调（1s 周期，驱动定时任务调度与延迟 clear） |
| `auto-task.cleanup-project-hooks` | 清理项目 hooks（保留用户自定义 hooks） |

> 命令 ID 与 manifest `contributes.commands[].id` 全名一致，前端按全名调用。

## 插件权限

| 权限 | 用途 |
|------|------|
| `storage` | 插件独立数据库 |
| `broadcast` | 广播状态变更到移动端 |
| `terminal:input` | 向终端发送命令（队列调度） |
| `terminal:output` | 读取终端输出 |
| `terminal:observe` | 观察终端输出（任务完成判定） |
| `session:read` | 读取会话信息 |
| `session:write` | 会话生命周期管理（定时任务创建会话） |
| `fs:read` / `fs:write` | 读写项目 hooks 文件 |
| `timer:schedule` | 宿主周期定时器（scheduler-tick 回调） |
| `ui:sidebar` | 侧边栏任务历史视图 |
| `ui:input` | 终端工具栏按钮（打开队列弹窗） |

## 生命周期钩子

| 钩子 | 触发时机 | 行为 |
|------|----------|------|
| `onStartup` | 插件启动 | 清理旧版全局 hooks、初始化数据库表、注册会话生命周期监听、注册宿主定时器 |
| `onShutdown` | 插件关闭 | 日志记录 |
| 会话生命周期（creating） | Agent 会话创建前 | 按会话 agent 平台自动安装对应集成（`.claude` / `.pi` / `.opencode` / `.codex`） |

## 定时任务状态机

```text
pending ──(到期，session_create 成功)──▶ creating ──(Created 事件到达，prompts 入队)──▶ executed
pending ──(到期，session_create 失败)──▶ failed
pending ──(超过宽限期仍未执行，如应用关闭期间到期)──▶ missed（不补跑）
creating ──(应用重启，会话丢失)──▶ failed
missed / failed ──(reset)──▶ pending（重新加入调度）
```

- 时间基准：WASM 无系统时钟，所有时间比较使用宿主 `scheduler-tick` 回调注入的 `now_utc`
- 到期宽限 120s：应用未运行期间到期的任务标 `missed` 不补跑
- 首轮下发兜底（15s）：TUI 型 agent（opencode）不输入 prompt 不创建会话，SessionStart idle 推送永不产生时由 scheduler-tick 主动调度下发

## 事件通道

同一事件名经三条通道投递，消费方各取所需：

| Topic | 消息总线（插件间） | emit_event（前端 UI） | broadcast_sync（移动端） |
|-------|:---:|:---:|:---:|
| `task:status-changed` | ✓ | ✓ | ✓ |
| `session:mode-changed` | ✓ | ✓ | ✓ |
| `task:queue-changed` | ✓ | ✓ | ✓ |
| `task:scheduled-changed` | ✓ | ✓ | ✓ |
| `task:preset-changed` | ✓ | ✓ | （仅桌面端功能，不同步移动端） |

> 预设任务为桌面端功能：事件只走 emit_event + 消息总线，不广播移动端同步通道。
