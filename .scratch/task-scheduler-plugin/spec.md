# 计划任务调度插件（com.bedcode.scheduler）实现规格

Status: ready-for-agent

> 本规格由 grilling 会话（grill-with-docs + domain-modeling）全程决策汇编而成，实现者无需再做重大决策。领域词汇见 `CONTEXT.md`「计划任务 (Task Scheduler)」「调度任务 (Scheduled Task)」词条。

## 1. Problem Statement

BedCode 现有的「定时自动任务」面向 agent 会话（一次性 trigger_at + prompts 注入），无法满足通用场景：**用户希望在 AI agent 会话中直接让 agent 创建/删除定时任务、自定义调度执行的 shell 脚本**（如"每天早上 9 点跑备份脚本"）。这需要一个与 agent 无关的通用调度框架：

- **计划任务插件**（仅桌面端）：cron 周期表达式触发，执行 shell 脚本/内联命令
- **CLI（bedtask）**：管理主入口，面向 AI agent（agent 写 shell → 注册 cron → 管理任务），随插件包分发、由插件控制安装/配置/卸载
- **插件互调机制**：插件之间互相调用只能通过插件自身声明的对外 api（宿主注册表门禁），限制插件之间胡乱调用——该机制与计划任务插件同时落地，计划任务插件是第一个实现者

## 2. 范围

**In scope**：
- 计划任务插件（桌面端 WASM）：调度引擎 + 任务/执行记录数据模型 + 状态机 + HTTP 端点 + 事件广播
- 宿主新能力：`host-process`（spawn/kill/输出落盘）+ 调度 tick 注入本地时间
- CLI `bedtask`（独立 Rust bin，随插件包分发，插件控制安装/卸载，本地 HTTP 调桌面端）
- 插件互调机制：manifest `api` 声明 + 宿主注册表校验 + JSON-RPC 2.0 消息形状 + SDK `#[plugin_api]` 宏
- 桌面端极简只读查看面板（任务列表 + 最近执行 + 日志）

**Out of scope**（已决排除）：
- 移动端（仅桌面端）
- OS 计划任务委托（`run_outside_app` 不做；桌面端"关闭"= 托盘驻留语义，真正退出后错过不补跑）
- UI CRUD 表单（CRUD 全走 CLI）
- 互调机制的调用方授权（层 2）、api 版本化
- 对 auto-task「定时自动任务」的迁移（保持并存不动）
- 错过补跑（catch-up）、限速、任务依赖编排

## 3. 术语

| 词 | 定义 |
|---|---|
| 计划任务 (Task Scheduler) | 插件 `com.bedcode.scheduler`，通用调度框架 |
| 调度任务 (Scheduled Task) | 单条任务条目：cron + 执行内容 + 执行环境 + 启停开关 |
| 执行记录 (Execution) | 每次触发产生的记录（succeeded/failed/timeout/missed），与任务定义分离 |
| 定时自动任务 (Scheduled Auto Task) | 现有 auto-task 插件功能（一次性 + agent prompt），与本插件并存区隔 |

完整定义与 _Avoid_ 见 `CONTEXT.md`。

## 4. 总体架构

```
┌─ AI agent 会话 ─────────────┐        ┌─ 桌面端进程（托盘驻留）──────────────┐
│  bedtask add --cron ...     │  HTTP  │ 网关(localhost 免 token) → 计划任务插件(WASM)
│  bedtask list / remove ...  │ ─────▶ │   ├─ 调度引擎：tick(1s, 注入本地时间) + next_at 匹配
└─────────────────────────────┘        │   ├─ cron 6 段解析器（自写，纯函数）
                                       │   └─ host-process 能力：spawn/kill/输出落盘
┌─ 其他插件 ──────┐   host-bus   │                                     │
│ ApiClient       │ ───────────▶ │  注册表校验(bedcode.api.*) → 目标插件 │
└─────────────────┘              └──────────────────────────────────────┘
```

- **CLI**：薄客户端，localhost HTTP → 桌面端网关 `/api/plugin/com.bedcode.scheduler/...`，复用 agent hooks 的免 token 本地放行；桌面端未运行时报错
- **调度**：宿主 timer（1s，沿用 `timer_register`）回调 tick，注入 `now_utc` + `now_local`；插件查 `next_at <= now_local AND enabled`
- **执行**：插件经 `host-process` 请求宿主 spawn 进程（WASM 无法直接 spawn）；宿主异步执行、完成后事件回调插件
- **互调**：插件 A publish `bedcode.api.<plugin-id>.<method>` → 宿主校验目标已声明 → 目标插件收到 JSON-RPC 2.0 请求 → 响应 publish 回 `bedcode.api.reply.<caller-id>.<request-id>`

## 5. 调度引擎

### 5.1 cron 表达式（统一 schedule 字段）

- **6 段**：`秒 分 时 日 月 周`，支持标准 cron 语法（`*`、数字、`-`、`,`、`/`、`?` 可选与 `*` 同义）
- **基准：本地时区**（宿主 tick 注入 `now_local` 字符串，规避 WASM 无时钟 + DST 偏移漂移）
- **解析器自写**（零依赖、纯函数、可单测）：`parse(schedule) -> CronSpec`、`next_after(spec, now_local) -> Option<String>`
- **一次性时刻** = cron 特例（5 段定值 + `*`）；`once` 标记的任务触发成功后自动停用（enabled=false），不重复触发
- **DST 行为**：以本地时间字符串序列计算；DST 跳变日不存在的时刻（如 2:30）跳过该次触发，不做特殊补偿

### 5.2 触发与错过

- tick 到点：`SELECT * FROM scheduled_jobs WHERE enabled=1 AND next_at <= now_local`（按 next_at 排序）
- 触发：插入执行记录（waiting）→ 并发槽位空出时置 running → `host-process` run
- 完成后计算 `next_at = next_after(...)` 写回任务定义
- **错过**：桌面端退出期间到期。activate 时恢复检查：`next_at` 已过且超过宽限（120s，沿用现有 scheduled 语义）→ 插入 missed 执行记录，任务定义继续排下一周期（`next_at` 推进到下一个未来时刻）
- **排队**：同一时刻触发数 > 并发上限（默认 3）时排队（执行记录 waiting）；应用重启时 waiting 行置 missed
- **手动触发**（`bedtask run`）：立即执行一次，不改变 next_at

### 5.3 执行记录保留

封顶 500 条，超限删最旧（参照 file-transfer HistoryStore::trim_to_cap 模式）。

## 6. 数据模型（插件独立 DB）

```sql
CREATE TABLE scheduled_jobs (
    id          TEXT PRIMARY KEY,          -- lower(hex(randomblob(16)))
    name        TEXT,                       -- 可选，显示名
    schedule    TEXT NOT NULL,              -- cron 6 段表达式（本地时区）
    exec_type   TEXT NOT NULL,              -- 'script' | 'inline'
    exec_value  TEXT NOT NULL,              -- 脚本路径 | 内联命令字符串
    cwd         TEXT,                       -- 工作目录，默认脚本目录/用户主目录
    env         TEXT,                       -- JSON 对象 {K: V}，附加环境变量
    timeout_sec INTEGER NOT NULL DEFAULT 600,
    enabled     INTEGER NOT NULL DEFAULT 1,
    once        INTEGER NOT NULL DEFAULT 0, -- 触发成功后自动停用
    next_at     TEXT,                       -- 下次触发（本地时间字符串）
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE job_executions (
    exec_id     TEXT PRIMARY KEY,
    job_id      TEXT NOT NULL,
    status      TEXT NOT NULL,              -- waiting|running|succeeded|failed|timeout|missed
    trigger     TEXT NOT NULL,              -- 'cron' | 'manual'
    started_at  TEXT,
    finished_at TEXT,
    exit_code   INTEGER,
    output_path TEXT                        -- stdout/stderr 落盘文件路径
);
```

**任务定义状态机**：无生命周期状态机——任务定义持续存活（enabled 维度 + 删除），终态概念只存在于执行记录。`missed` 是执行记录的属性，不是任务定义的终态。

**执行记录状态机**：

```
waiting ──(槽位空出)──▶ running ──(exit 0)──▶ succeeded
                        running ──(exit ≠0)──▶ failed
                        running ──(超时 kill)─▶ timeout
waiting ──(应用重启)──▶ missed
(activate 时错过检查直接插入 missed)
```

## 7. 执行引擎（host-process 宿主能力）

### 7.1 WIT 扩展（bedcode.wit 新增 interface）

```wit
interface host-process {
    /// 启动进程（异步执行），返回 run-id；完成后宿主经事件回调插件
    run: func(request-json: string) -> result<run-id, string>;
    /// 终止进程（超时/取消）
    kill: func(run-id: string) -> result<_, string>;
}
```

- request-json：`{ command, args, cwd, env, timeout_ms, output_path }`
- **异步 + 事件回调**（沿用 `session_create` → Created 事件模式）：宿主 spawn 后立即返回 run-id；进程结束（正常/异常/超时 kill）后宿主分发执行完成事件（含 exit_code / timeout 标记）
- **输出落盘**：宿主侧 Rust 边跑边写 stdout/stderr 到插件指定的 `output_path`（不经 WASM 内存，大输出无内存风险）
- 宿主实现：`tokio::process::Command`，超时 `tokio::time::timeout` + kill（进程组，含子进程）

### 7.2 权限与审计

- 新增 `PERMISSION_PROCESS` 权限键，插件 manifest 声明，宿主 `check_permission` 门禁（与现有权限模型一致）
- **安装即信任**：不做逐脚本白名单（无人值守 cron 场景无法弹窗确认）
- **审计日志**：每次执行全量记录——命令、参数、cwd、env、触发方式、时间、结果（DB 执行记录 + 输出文件）；可审计、可追溯

### 7.3 并发与超时

- 并发上限默认 3（可配），超出排队（waiting）
- 超时默认 10 分钟（`--timeout` 可配），超时 kill + 标 timeout

## 8. CLI（bedtask）

### 8.1 分发与生命周期（随插件包，由插件控制）

- 插件包结构：`plugin.json` + wasm 组件 + `cli/bedtask(.exe)`（构建发布时随插件打包）
- **on_activate 安装**：复制 CLI 到统一用户目录（Windows `%LOCALAPPDATA%\com.bedcode.app\bin\`，macOS/Linux `~/.bedcode/bin/`）并注册 PATH
  - Windows：用户级注册表 `HKCU\Environment\Path`（免管理员）+ 广播 `WM_SETTINGCHANGE`（新终端/agent 生效）
  - macOS/Linux：`~/.local/bin/` symlink
  - 幂等：重复激活不产生重复条目；升级时覆盖
- **on_deactivate 卸载**：删除文件 + 移除仅本插件添加的 PATH 条目（保留用户原有项）；插件停用 = CLI 不可用

### 8.2 通信

- localhost HTTP → 桌面端网关（端口 `config_get(NetworkPort)`，默认 8765），复用 agent hooks 免 token 本地放行
- 桌面端未运行：CLI 报 `desktop not running`

### 8.3 命令集

```
bedtask add --cron "<6段表达式>" (--script <path> | --exec "<command>")
           [--name <name>] [--cwd <dir>] [--env K=V,...] [--timeout <sec>] [--once]
bedtask list                        # 任务定义 + 状态（enabled/next_at/最近执行）
bedtask show <id>
bedtask remove <id>
bedtask edit <id> [--cron ...] [--exec ...] [--name ...] [--timeout ...] [--once/--no-once]
bedtask enable <id> | bedtask disable <id>
bedtask run <id>                    # 手动立即触发一次（不改变 next_at）
bedtask logs <id> [--limit N]       # 最近执行记录 + 输出文件路径
```

- 输出：人类可读默认 + `--json`（agent 友好，机器可解析）

### 8.4 插件 HTTP 端点（CLI 与互调共用）

`task-scheduler/...` 前缀：`add` / `list` / `remove` / `show` / `edit` / `enable` / `disable` / `run` / `logs`，语义与 CLI 命令对等；参数校验、错误码规范与现有端点一致（http_response 模式）。

## 9. 插件互调机制（宿主注册表 + JSON-RPC 2.0 + SDK 宏）

### 9.1 manifest 声明（门）

```json
{
  "id": "com.bedcode.scheduler",
  "api": [
    "com.bedcode.scheduler.add",
    "com.bedcode.scheduler.remove",
    "com.bedcode.scheduler.list",
    "com.bedcode.scheduler.show",
    "com.bedcode.scheduler.run",
    "com.bedcode.scheduler.logs"
  ]
}
```

### 9.2 宿主注册表与校验

- 插件加载/激活时登记 manifest 声明的 api 清单
- `bus_publish` 校验：topic 前缀 `bedcode.api.` 的互调消息，**目标 api 名必须命中某已激活插件声明过的清单**，否则拒绝 + 告警日志
- 普通广播 topic（`filesrv:peer_changed` 等既有约定）不校验，保持向后兼容
- 门禁层级：**层 1**（只校验目标存在，不校验调用方、不做版本化）

### 9.3 消息形状（JSON-RPC 2.0 over host-bus）

- 请求 topic：`bedcode.api.<plugin-id>.<method>`；payload：`{ jsonrpc: "2.0", id, method, params }`
- 响应 topic：`bedcode.api.reply.<caller-plugin-id>.<request-id>`；payload：`{ jsonrpc: "2.0", id, result | error }`
- 调用方订阅自身响应 topic；SDK 管理配对生命周期（correlation id 生成、超时默认 10s、错误传播）

### 9.4 SDK `#[plugin_api]` 宏

- **实现方**：`impl ScheduleApi for MyPlugin`（trait 方法签名即 API），宏生成消息分派（JSON-RPC 请求解析 → 调方法 → 回响应）
- **调用方**：`ApiClient::<ScheduleApi>::new("com.bedcode.scheduler")`，宏生成类型化调用（构造请求 → publish → 收响应 → 反序列化）；SDK 里没有 client 的 API 在编译期不可调
- **防漂移**：构建脚本比对宏生成的 api 清单与 manifest `api` 字段，不一致构建失败

### 9.5 计划任务插件声明的对外 API

`schedule.add / remove / list / show / run / logs`（与 CLI/HTTP 端点对等，供其他插件互调）。

## 10. 桌面端只读面板

- 任务列表（name/schedule/enabled/next_at/最近执行摘要）+ 最近执行记录 + 日志文件路径查看（只读，无 CRUD 表单）
- 数据源：`list` / `logs` 端点；实时刷新：复用 `emit_event` 三通道广播（`EVENT_TASK_SCHEDULED_CHANGED` 模式：broadcast_sync + bus + emit_event）

## 11. 安全模型

| 面 | 机制 |
|---|---|
| 脚本执行 | `host-process` 权限声明 + 安装即信任；全量审计日志（命令/参数/时间/结果） |
| CLI | localhost 网关免 token 本地放行（同机可信，与 agent hooks 同路径） |
| 互调 | 宿主注册表门禁（层 1）：未声明的 `bedcode.api.*` publish 拒绝 |
| 插件包 | CLI 随插件分发，PATH 注册/移除仅限本插件条目 |

## 12. 实施拆分（issue 计划）

- **01 宿主能力扩展**：WIT `host-process` interface + 本地时间注入（tick 参数或 host-clock）+ 权限键 + 宿主实现（tokio spawn/kill/输出落盘/完成事件）+ 测试
- **02 调度引擎**：cron 6 段解析器（纯函数 + 单测）+ 数据模型/建表 + tick 触发/错过恢复/排队 + 状态机 + HTTP 端点 + 事件广播
- **03 CLI**：Rust bin + 插件包结构（cli/ 目录）+ 激活安装/停用卸载生命周期（PATH 注册跨平台）+ 命令集实现 + `--json` 输出
- **04 互调机制**：manifest `api` 字段 + 宿主注册表登记/校验（bus_publish 门禁）+ JSON-RPC 2.0 消息约定 + SDK `#[plugin_api]` 宏（实现方分派 + 调用方 client）+ 构建期防漂移比对 + 测试插件验证
- **05 只读面板**：桌面端 Vue 组件 + 事件订阅 + 端点接线
- **06 端到端验证**：CLI 全命令集冒烟、并发/超时/错过场景、互调测试插件、卸载清理验证

## 13. 设计细节标注（实现时注意）

- WASM 无系统时钟：所有时间字符串由宿主注入/DB 计算，插件不做任何本地时间运算
- `next_at` 全部以本地时间字符串存储与比较（字典序可比，格式统一 `YYYY-MM-DD HH:MM:SS`）
- 错过恢复只在 activate 时执行一次（幂等）
- `bedtask run` 手动触发与 cron 触发共用执行记录表（trigger 字段区分）
- 排队行（waiting）重启置 missed：activate 恢复时一并处理
