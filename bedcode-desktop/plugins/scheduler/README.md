# Task Scheduler Plugin (Desktop)

通用调度框架（id：`com.bedcode.scheduler`）：cron 6 段表达式（本地时区）触发执行 shell 脚本 / 内联命令，与 agent 会话无关；执行记录可审计。

三层结构：**Rust WASM 层**（cron 解析 + tick 调度引擎 + HTTP 端点 + 互调 API）+ **Rust CLI**（`bedtask`，管理主入口）+ **TS 前端**（只读侧边栏面板）。任务 CRUD 全走 CLI，UI 只读展示。

## 功能

- **cron 6 段表达式**：`秒 分 时 日 月 周`，支持 `*` / `?` / 精确值 / `a-b` / `*/n` / 逗号列表；周字段 0 与 7 均为周日；DOM 与 DOW 同字段受限时 OR 匹配（标准 cron 语义）
- **两种执行方式**：`script`（外部脚本文件）或 `inline`（内联命令字符串），经宿主 host-process 在桌面端宿主进程内执行
- **执行上下文**：可选 `cwd` / `env`（JSON 对象） / `timeout_sec`（默认 600s） / `once`（触发成功后自动停用）
- **并发控制**：默认 3 个 running 槽位，可通过 `storage` key `max_concurrency` 调整；等待队列按 FIFO 提升
- **审计日志**：每次触发落盘执行记录（`waiting` → `running` → `succeeded` / `failed` / `timeout` / `missed`），stdout/stderr 落盘到 `<home>/.bedcode/scheduler/<exec_id>.log`；记录保留最近 500 条
- **重启恢复**：应用重启时残留 `waiting` / `running` 执行置 `missed`；超过宽限（120s）的到期任务插入 `missed` 执行记录并把 `next_at` 推进到下一未来时刻（不补跑）
- **CLI 管理**（`bedtask`）：`add` / `list` / `show` / `remove` / `edit` / `enable` / `disable` / `run` / `logs`，人类可读输出 + `--json`
- **侧边栏面板**：只读展示任务列表与最近执行记录，订阅 `scheduler:changed` 事件自动刷新

## 使用

**CLI（管理主入口）** —— 插件 `activate` 时自动把 `bedtask` 复制到用户 bin 目录并注册 PATH：

```bash
# 新建：crontab 风格 6 段 + 脚本或内联命令
bedtask add --cron "0 2 * * *" --script /path/to/backup.sh --name "每日备份"
bedtask add --cron "*/15 * * * * *" --exec "echo tick" --timeout 30 --once

# 查看 / 编辑 / 生命周期
bedtask list
bedtask show <job_id>
bedtask edit <job_id> --cron "0 3 * * *" --timeout 120
bedtask enable <job_id>   # 或 disable
bedtask remove <job_id>

# 手动触发 / 查执行历史
bedtask run <job_id>         # 手动执行一次，不改变 next_at
bedtask logs <job_id> --limit 50
```

端口从环境变量 `BEDCODE_PORT` 读取，缺省 8765（与宿主 `config_get(NetworkPort)` 对齐）。

**侧边栏面板** —— 只读视图：任务列表（名称 / 表达式 / 启用状态 / 下次触发 / 最近执行摘要）+ 选中任务的最近执行记录（触发方式 / 时间 / 退出码 / 输出文件路径，一键复制）。所有写入操作提示用户使用 CLI。

## 架构

- **Rust WASM 层**（`rust/src/`）：全部业务逻辑 —— cron 解析、tick 调度状态机、并发控制、执行记录持久化、HTTP 端点、互调 API 分派
- **Rust CLI**（`cli/src/`）：`bedtask` 薄客户端，仅做参数解析与 HTTP 请求转发（无业务逻辑，与 WASM 层通过 localhost 网关 `/api/plugin/com.bedcode.scheduler/...` 通信）
- **TS 前端**（`src/`）：`SchedulerPanelView` 只读面板 + `useSchedulerApi`（`_http_endpoint` 命令封装）+ i18n 消息；不暴露任何写入操作
- **构建**（`scripts/build.js`）：vite build → cargo WASM build → componentize → CLI build → 复制产物

## 目录结构

```
scheduler/
├── plugin.json          # 插件清单（权限、API、命令、视图、生命周期）
├── icon.svg             # 插件图标
├── package.json         # 前端构建脚本
├── tsconfig.json
├── vite.config.ts
├── vitest.config.ts
├── rust/                # Rust WASM 库（plugin 主体）
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs       # WasmPlugin 入口（activate/deactivate、命令路由、消息分派、生命周期钩子）
│       ├── cron.rs      # cron 6 段解析器 + next_after 计算器（纯函数，可单测）
│       ├── engine.rs    # 调度引擎：JobDef / 执行记录 CRUD、tick 状态机、并发控制、HTTP 端点、事件广播
│       └── api.rs       # 互调 API（#[plugin_api] 宏生成 ScheduleApiDispatcher + ScheduleApiClient）
├── cli/                 # Rust CLI 二进制（bedtask）
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs      # 参数解析 + 命令执行分发
│       └── http.rs      # localhost 网关 HTTP 客户端
├── scripts/
│   └── build.js         # 统一构建脚本（前端 + WASM + componentize + CLI + 复制产物）
├── src/                 # TS 前端源码
│   ├── index.ts         # 插件入口（activate/deactivate、侧边栏注册、i18n 注册、dev-shell mock）
│   ├── vite-env.d.ts
│   ├── dev-mock.ts      # 浏览器预览用命令 mock（真实宿主不启用）
│   ├── __tests__/       # vitest 单元测试（helpers / i18n / schedulerApi / schedulerPanel / smoke）
│   ├── components/
│   │   └── SchedulerPanelView.vue    # 只读侧边栏面板
│   ├── composables/
│   │   └── useSchedulerApi.ts        # 只读数据访问（listJobs / fetchLogs，仅 GET）
│   └── i18n/
│       ├── index.ts       # locale → 翻译表
│       ├── messages.ts    # MessageSchema（编译期校验）
│       ├── zh-CN.ts
│       └── en.ts
└── dist/                # Vite 构建产物（不入库）
```

## 构建

完整构建（前端 + Rust WASM + CLI + 复制产物）：

```bash
cd bedcode-desktop/plugins/scheduler
node scripts/build.js
```

`scripts/build.js` 依次执行：

1. `pnpm exec vite build` —— TS 前端产物 → `dist/index.js`
2. `cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release` —— Rust WASM
3. `componentize`（`packages/plugin-sdk-desktop/rust/tools/componentize`）—— WASM 编码为 Component Model 组件
4. `cargo build --release --manifest-path cli/Cargo.toml` —— `bedtask` 二进制
5. 复制 `plugin.json` / `icon.svg` / `index.js` / `bedcode_plugin_scheduler.wasm` / `cli/bedtask(.exe)` 到 `src-tauri/resources/plugins/desktop/com.bedcode.scheduler/`

分步构建：

```bash
pnpm run build:frontend              # 仅前端
pnpm run build:rust                  # 仅 WASM（未 componentize）
pnpm run test:run                    # vitest 单元测试
```

产物目录 `src-tauri/resources/plugins/` 已加入 `.gitignore`，不入库；由 `build.js` 生成，打包/运行前需先执行构建。

## 插件清单关键字段

- `id`：`com.bedcode.scheduler`
- `pluginType`：`rust-ts`（Rust WASM 主体 + TS 前端面板）
- `rustLibrary`：`bedcode_plugin_scheduler`（WASM 文件名前缀）
- `main`：`index.js`（Vite 产物入口）
- `sandbox`：`inline`
- `lifecycle`：`onStartup` + `onShutdown` 均启用

## 数据库表

插件独立 SQLite 数据库（经宿主 `plugin_db_execute` / `plugin_db_query`，SQL 全部参数绑定）：

| 表 | 用途 |
|----|------|
| `scheduled_jobs` | 任务定义。`id` / `name` / `schedule`（cron 6 段）/ `exec_type`（`script` \| `inline`）/ `exec_value` / `cwd` / `env`（JSON 字符串）/ `timeout_sec`（默认 600）/ `enabled` / `once` / `next_at` / `created_at` / `updated_at` |
| `job_executions` | 执行记录。`exec_id` / `job_id` / `status`（`waiting` \| `running` \| `succeeded` \| `failed` \| `timeout` \| `missed`）/ `trigger`（`cron` \| `manual`）/ `started_at` / `finished_at` / `exit_code` / `output_path` / `run_id` |

## HTTP 端点

通过宿主网关 `/api/plugin/com.bedcode.scheduler/{path}` 访问（`lib.rs` 的 `_http_endpoint` 按路径前缀 `task-scheduler/` 分发到 `engine::handle_scheduler_http`；WASM 端通过 `invoke_command("_http_endpoint", ...)` 走同一语义）：

| 方法 | 路径 | 用途 |
|------|------|------|
| POST | `task-scheduler/add` | 创建任务（body：`name?` / `schedule` / `exec_type` / `exec_value` / `cwd?` / `env?` / `timeout_sec?` / `once?`） |
| GET | `task-scheduler/list` | 任务列表（按 `next_at` 升序，附最近一次执行摘要） |
| GET | `task-scheduler/show?job_id=` | 任务详情 + 最近 20 条执行记录 |
| DELETE | `task-scheduler/remove?job_id=` | 删除任务及其执行记录 |
| POST | `task-scheduler/edit` | 编辑任务（body 含 `job_id` + 待更新字段） |
| POST | `task-scheduler/enable?job_id=` | 启用（若 `next_at` 已过期推进到下一未来时刻） |
| POST | `task-scheduler/disable?job_id=` | 停用 |
| POST | `task-scheduler/run?job_id=` | 手动立即执行一次（`trigger='manual'`，不改变 `next_at`） |
| GET | `task-scheduler/logs?job_id=&limit=` | 最近执行记录（默认 20，封顶 100） |

## WASM 命令（`invoke_command`）

来自 `plugin.json` 的 `contributes.commands` 与 `lib.rs` 的 `invoke_command` 匹配（前端按全名调用）：

| 命令 | 用途 |
|------|------|
| `task-scheduler.tick` | 宿主定时器回调（1s 周期）：注入 `now_local`，驱动到期触发 + 并发槽位调度 |
| `_http_endpoint` | 宿主网关转发 HTTP 请求到 `engine::handle_scheduler_http`（未列入 `contributes.commands`，走宿主内置路由） |

## 互调 API（`plugin.json.api`）

其他插件通过 `#[plugin_api]` 宏生成的 JSON-RPC 客户端调用（`api.rs` 中 `ScheduleApi` trait）：

| API | 用途 |
|-----|------|
| `com.bedcode.scheduler.add` | 创建任务，返回 `job_id` |
| `com.bedcode.scheduler.remove` | 删除任务及其执行记录 |
| `com.bedcode.scheduler.list` | 任务列表（含最近执行摘要） |
| `com.bedcode.scheduler.show` | 任务详情 + 最近 20 条执行记录 |
| `com.bedcode.scheduler.run` | 手动立即执行一次 |
| `com.bedcode.scheduler.logs` | 最近执行记录（默认 20，封顶 100） |

构建期由宏与 `plugin.json.api` 字段做精确集合比对，不一致构建失败（防止文档漂移）。

## 插件权限

| 权限 | 用途 |
|------|------|
| `app:cli` | `activate` 时安装随包 CLI（`bedtask`）到用户 bin 目录并注册 PATH，`deactivate` 时卸载 |
| `broadcast` | 事件广播到移动端（`SyncEvent::TaskScheduledChanged` 通道） |
| `process:run` | 通过宿主 host-process 启动 shell 脚本 / 内联命令 |
| `storage` | 读写 `max_concurrency` 并发上限配置 |
| `timer:schedule` | 注册宿主 1s 周期定时器回调 `task-scheduler.tick` |
| `ui:sidebar` | 注册侧边栏只读面板 |

## 生命周期钩子

| 钩子 | 触发时机 | 行为 |
|------|----------|------|
| `activate` | 插件首次加载 | `recover()` 幂等恢复；安装 `bedtask` CLI；注册宿主定时器；订阅互调请求 topic |
| `deactivate` | 插件卸载 | 卸载 `bedtask` CLI（应用关闭流程中宿主跳过，残留由下次幂等安装覆盖） |
| `on_startup` | 启动生命周期 | 建表（`scheduled_jobs` / `job_executions`，按语句逐条执行） |
| `on_shutdown` | 关闭生命周期 | 日志记录（运行中进程随宿主终止，残留由下次 `activate` 的 `recover()` 置 `missed`） |
| `on_process_done` | 宿主进程完成事件 | 回写执行记录终态 + 推进 `next_at` + 释放槽位继续调度 |

## 事件通道

事件 topic `scheduler:changed`（`plugin.json.contributes.provides`）在 `broadcast_changed` 中经三条通道投递，覆盖 `create` / `edit` / `enable` / `disable` / `trigger` / `start` / `start-failed` / `done` / `reschedule` / `restart` / `once-done` / `never-match` 等动作：

| Topic | 消息总线（插件间） | `emit_event`（前端 UI） | `broadcast_sync`（移动端） |
|-------|:---:|:---:|:---:|
| `scheduler:changed` | ✓ | ✓ | ✓（复用 SDK `SyncEvent::TaskScheduledChanged` 线协议） |

前端 `SchedulerPanelView` 订阅该事件后自动重载任务列表与选中任务的执行记录。

## 关键设计

- **时间基准**：WASM 无系统时钟、无时区数据。`cron` 做纯日历字符串运算（`YYYY-MM-DD HH:MM:SS`，字典序即时间序），tick 的 `now_local` 由宿主注入，执行记录时间戳由宿主 DB `datetime('now','localtime')` 计算。
- **不感知 DST**：DST 跳变日不存在的时刻（如 02:30）由宿主注入的 `now_local` 序列自然跳过，`next_after` 只沿字符串推进不回头，不做特殊补偿。
- **无 catch-up**：错过补跑明确排除。应用重启时残留 `waiting` / `running` 置 `missed`；`next_at` 已过期且超过宽限（120s）插入 `missed` 执行记录并推进到下一未来时刻。
- **cron 不重叠**：`tick` 检测到 `next_at` 到期的任务已有 `waiting` / `running` 执行时跳过本次触发（长任务不会与短间隔 cron 撞车）。
- **`next_at` 推进**：`cron` 触发完成后从旧 `next_at` 起算下一未来时刻（与运行耗时无关，长跑任务不漂移）；`manual` 触发不改变 `next_at`；`once` 成功自动停用，失败 / 超时照常排下一周期（成功前重试语义）。
- **命令包装按宿主平台**：Windows 下 `script` / `inline` 均经 `cmd /C`；unix 下 `script` 直接执行（需 shebang + 可执行位），`inline` 经 `sh -c`。
- **并发槽位**：`count_running < max_concurrency` 时提升 FIFO `waiting` 到 `running`；spawn 失败置 `failed` 但不阻塞后续调度。

## 依赖

- `bedcode-plugin-api`：桌面端插件 SDK（`packages/plugin-sdk-desktop/rust`，启用 `wasm` feature）
- `serde` / `serde_json` / `anyhow`
- 前端：`@binblink/plugin-sdk-desktop` / `vue ^3.4`
