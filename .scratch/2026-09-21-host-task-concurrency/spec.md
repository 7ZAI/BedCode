# host-task：宿主侧异步并发方案（WASM 插件调度 OS 线程）· 设计稿

> 状态：**已实施**（2026-09-21，票 A-D 一次性落地；ABI desktop v20，mobile 停 11 双端偏离）。
> 日 期：2026-09-21。
> 更新：2026-09-21 补充 §14「WASIp3 协作式线程演进路径」（用户指示纳入设计考虑）；同日实施（WIT/SDK 五同步点 + host-task 宿主执行器 + events-task 回调管道 + fixture 闭环 6 用例，宿主 lib 1090 测试全绿）。
> 关联：ADR 0022（裁剪线 / 双端偏离）、ADR 0017（互调）、`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`、票 03/04（文件浏览 / git 域已用 `host-process.run-sync` 同步阻塞先例）。

---

## 1. 背景与目标

WASM 插件（wasm32-wasip3 组件）无法创建 OS 线程；其全部宿主调用在当前 ABI 下是**同步阻塞**的——插件要并发做 50 个 `fs.stat` / 并行跑 3 条 `git diff`，只能串行排队，每个调用都占用 Store 执行线程。既有原语只覆盖了两类窄场景：

- `host-process.run-sync`：同步执行**单条**外部命令（百毫秒级，长命令禁用）；
- `host-process.run`：异步单进程 + `on-process-done` 回调（外部进程，非宿主线程工作）。

目标：给插件一组**通用**的宿主侧并发原语——插件提交带参数的「单元操作计划」，宿主在 OS 线程池上并发执行，过程与结果经插件公开的导出方法回调；不引入业务语义（ADR 0022 裁剪线），不改既有插件的迁移负担（纯增量）。

**一句话方案**：新增 `host-task` 宿主能力域——「单元操作 = 既有宿主原语的参数化投影，宿主线程池并发执行；回调 = 可选导出 `events-task#on-task-event`（events-ws 同款探测模式）」。插件代码永远是单线程执行域（单 Store 不可重入），被并发化的是**宿主侧工作**，不是插件代码。

---

## 2. 执行模型事实（方案的地基，全部为现状代码实证）

| # | 事实 | 出处 |
| --- | --- | --- |
| F1 | 每插件单实例：`wasm_plugins: RwLock<HashMap<plugin_id, Arc<Mutex<LoadedWasmPlugin>>>>`，所有宿主→插件导出调用经该 tokio Mutex **串行化** | `plugin/manager/host.rs:100` |
| F2 | wasmtime Store 不可重入：guest 执行期间不能再进同一实例——`host-session.create/restart` 因此被迫异步化（WIT 注释明示） | `wit/bedcode.wit` host-session |
| F3 | guest→host 调用同步阻塞在 Store 执行线程，宿主实现内部用 `block_on_async` 驱动异步资源（ambient runtime / block_in_place 双分支） | `wasm_runtime.rs:119` |
| F4 | 宿主→插件异步回调已有成熟路径：后台任务 → `PluginServices::dispatch_process_done` → `block_on_async` + `with_wasm_plugin_call`（拿 F1 的锁） | `host/services.rs:177` |
| F5 | 可选导出动态探测先例：`events-binary` / `events-ws` / `auth-policy` 单独 world 生成绑定、实例化后探测、未导出降级 `Ok(false)` 不影响加载 | `wasm_runtime.rs:1383`（`on_ws_frame`） |
| F6 | 句柄属主隔离范式：`pty-<uuid>` / `wsc-<uuid>` / `sess-<uuid>` 全部「仅属主可调 + 停用 purge_for_plugin」 | host_impl/pty.rs、ws.rs |
| F7 | 事件定向投递范式：owner 内嵌 topic（`pty:exit.<owner>` / `ws:open.<owner>`）或可选导出直投，宿主不缓冲、查询原语自愈 | WIT host-pty / host-websocket |
| F8 | 配额常量集中在 `system/constants/plugin.rs`（`PLUGIN_WS_MAX_CONNS_PER_PLUGIN` 等），宿主仲裁、超限 fail-visible | constants/plugin.rs |
| F9 | 后台任务统一 `spawn_with_error_boundary` 包装 | `system/error_boundary.rs` |

由 F1/F2 直接推出的**根本边界**：单个插件实例的回调天然串行（一把锁），并发收益只存在于「宿主替插件做的 work」上。任何「让插件代码并发跑」的方案（多实例、async WIT 全量改造）都是宿主大手术，见 §10 备选裁决。

---

## 3. 方案总览

```
插件 (WASM, 单线程执行域)
  │ ① submit(plan) / execute-batch(plan)          plan = { units:[{id,kind,params,…}], … }
  ▼
host-task 宿主实现（权限门 task:run + 配额仲裁 + 注册表登记 task-<uuid>）
  │ ② 单元操作分发到【专用 OS 线程池】（PLUGIN_TASK_POOL_THREADS，与 tokio blocking 池隔离）
  │    每单元 = 直调既有 host_impl 函数（同实现、同权限门、同 fs_auth，零新 DTO）
  ▼
宿主线线程并发执行（fs.read / fs.read-dir / fs.stat / fs.exists / fs.write /
                     process.run-sync / http.fetch …7 种 kind，可函数级追加）
  │ ③ 事件入每插件有界回调队列（started → progress* → terminal，终态优先）
  ▼
events-task#on-task-event 可选导出回调（F5 探测；未导出 → 丢弃 + warn + 计数）
  +  status(job-id) / list-jobs 查询原语自愈（F7 范式）
```

两档 API，共享同一单元模型与执行器：

- **同步档 `execute-batch`**：扇出 → join → 一次性返回全部单元结果。阻塞 Store 直至完成（与 `run-sync` 同款阻塞语义与代价），适合「几十个快操作并行」的同步路径（HTTP 命令面拿并行结果）。
- **异步档 `submit`**：立即返回 `task-<uuid>` 句柄，宿主线线程池执行，`on-task-event` 回调进度与终态；`cancel` 协作式取消。适合长任务（批量哈希、日志解析、多 git 命令）。

---

## 4. 契约设计（WIT v20 草案）

### 4.1 新 import：`host-task`（新 interface → ABI desktop 19 → 20，同 v16 host-pty 先例）

```wit
/// 宿主并发任务域（v20）：OS 线程池并发执行「单元操作计划」，零业务语义（ADR 0022）。
/// 单元操作 = 既有宿主原语的参数化投影：每种 kind 复用对应 host_impl 实现与
/// 权限门（task:run 之外还须持有单元自身的域权限），宿主不做任何编排解释。
/// 属主隔离：句柄 task-<uuid> 仅创建者插件可调；插件停用宿主回收其全部任务。
interface host-task {
    /// 同步批：并发执行 plan 全部单元并 join，返回 { results: [...] }。
    /// 阻塞 Store 至全部单元终态（或超时）——同 run-sync 的阻塞语义，仅限快操作。
    execute-batch: func(plan-json: string) -> result<string, string>;
    /// 异步任务：登记后立即返回 task-<uuid>；进度/终态经 events-task 回调。
    submit: func(plan-json: string) -> result<string, string>;
    /// 任务状态自愈快照（事件丢失后查询）：state / 计数器 / 终态 results（有界保留）
    status: func(job-id: string) -> result<option<string>, string>;
    /// 取消（协作式：正在执行的单元跑完或超时，未开始单元跳过）；返回是否命中
    cancel: func(job-id: string) -> result<bool, string>;
    /// 本插件在册任务清单（自愈快照）
    list-jobs: func() -> result<string, string>;
}
```

### 4.2 新可选导出：`events-task`（同 events-ws：独立 world `plugin-task` 仅供 SDK 生成绑定，宿主不实例化它、动态探测）

```wit
/// 观察型回调（同 events-ws）：无返回值；插件处理失败经 host-log 记录，
/// 宿主仅 trap 时 error! 并计数，不影响任务执行与其余投递。
interface events-task {
    /// event-json（camelCase）：
    /// { jobId, phase: "started"|"progress"|"completed"|"failed"|"cancelled",
    ///   doneUnits?, failedUnits?, result? }   // result 仅终态携带，同 execute-batch 返回
    on-task-event: func(event-json: string);
}
```

**回调通道裁决**：选可选导出而非消息总线 topic。理由：(a) SDK `wasm_entry!` 无条件导出默认空实现，不存在「activate 期忘订阅 → 事件永久丢失」的时序失败面（bus 路线有）；(b) 专用入口不与 `on-message` 的插件间消息语义混杂；(c) 投递路径与 events-ws 完全同构，复用探测/降级/计数基建。兜底自愈靠 `status` / `list-jobs`（F7）。

### 4.3 计划与结果形状（JSON 一律 camelCase，沿 WIT 惯例）

```jsonc
// plan（submit / execute-batch 共用）
{
  "units": [
    { "id": "u1", "kind": "process.run-sync", "params": { "command": "git", "args": ["diff"], "cwd": "…", "timeoutMs": 30000 } },
    { "id": "u2", "kind": "fs.stat", "params": { "path": "…" } }
  ],
  "maxConcurrency": 4,          // 可选，≤ 全局池线程数；缺省 = 池满即排队
  "jobTimeoutMs": 600000,       // 可选，缺省取常量；超时 → cancelled + 已完成单元结果保留
  "progress": { "everyUnits": 10, "everyMs": 500 }  // 可选，progress 事件节流
}

// execute-batch 返回 / submit 终态 result
{
  "jobId": "task-<uuid>",       // execute-batch 同样分配（status 可查）
  "results": [
    { "id": "u1", "ok": true,  "value": "{…该单元原语的原返回 JSON…}", "durationMs": 120 },
    { "id": "u2", "ok": false, "error": "fs error: …", "durationMs": 3 }
  ],
  "cancelled": false
}
```

要点：

- **单元 `params` = 对应宿主原语既有请求 JSON 原样内嵌**（`process.run-sync` 的 params 就是 run-sync 的 request-json），宿主零新 DTO 映射，语义单点在既有实现里；
- **fail-collect 不 fail-fast**：单元独立成败，失败不中断同批其余单元（编排判断归插件——业务决策不进内核）；
- 结果按 units 原顺序返回，`id` 由插件指定用于关联。

### 4.4 v20 单元操作 kind 初始集（全部映射既有 host_impl 函数）

| kind | 宿主实现复用 | 附加权限门 | 备注 |
| --- | --- | --- | --- |
| `fs.read` / `fs.read-dir` / `fs.stat` / `fs.exists` / `fs.write` | `host_impl/fs.rs` 同名函数 | `fs:read` / `fs:write` + fs_auth（见 §6 弹窗差异） | |
| `process.run-sync` | `host_impl/process.rs::process_run_sync` | `process:run` | 长命令在单元里也有 timeoutMs 兜底 |
| `http.fetch` | `host_impl/http.rs` | 沿用 host-http 既有门 | |

追加新 kind = 既有 interface 函数级追加（批次内不 bump，沿 v19 先例），新增时必须同批核对权限映射表。

---

## 5. 宿主侧实现设计

### 5.1 模块落位（沿内核五模块分工）

- **`wasm_runtime/host_impl/task.rs`**：WIT 绑定入口——`task:run` 权限门、plan 解析校验、配额仲裁、注册表登记、句柄寻址与属主校验（F6）；
- **`plugin/manager/task/`（新，core-task）**：`TaskRegistry`（`jobId → { owner, state, 计数器, 有界结果保留, 时间戳 }`，`Arc` 共享）、专用线程池、每插件回调队列、purge_for_plugin；
- **`PluginServices::dispatch_task_event`**：新 trait 方法，与 `dispatch_process_done` 同模式（F4）：`block_on_async` + `with_wasm_plugin_call` 调探测到的 `on-task-event`；
- **可选导出探测**：`wasm_runtime.rs` 增 `on_task_event` 探测句柄与降级路径（F5，探测 None → 事件丢弃 + 首次 warn + 计数）；
- **常量**：`system/constants/plugin.rs` 新增 §7 所列 `PLUGIN_TASK_*`；
- **指标**：任务数 / 池利用率 / 回调丢弃计数进 core-monitor MetricsRegistry。

### 5.2 专用线程池（不共用 tokio blocking 池的理由）

- 隔离：`spawn_blocking` 池与 PTY 读线程、WS 任务共享，插件批量单元可饿死它们；专用池（`PLUGIN_TASK_POOL_THREADS`，建议默认 8）独立队列 + 利用率可观测；
- 执行体：池线程为普通 std 线程，单元执行**直调 host_impl 同步函数**（其内部本就走 `block_on_async`，F3 的 ambient 分支正是为此存在），不经过 WASM、不触碰 Store——从机制上杜绝重入；
- 单元间并发度：`maxConcurrency` ≤ 池线程数，任务内用信号量仲裁；跨任务公平性由全局队列近似保证（v20 不做多任务公平调度承诺，文档明示）。

### 5.3 回调投递与背压

- 每插件一条**有界回调队列**（`PLUGIN_TASK_CALLBACK_QUEUE_DEPTH`，建议 64）+ 单消费派发任务（串行 = F1 的锁语义天然要求）；
- 入队顺序 = 事件顺序（started → progress → terminal），保证同任务事件有序；
- 溢出策略：progress 事件可丢（丢弃 + warn + `droppedEvents` 计数，status 可见，自愈同 F7）；terminal 事件优先入队；极端情况下 terminal 也丢则 error! 留痕，`status` 是唯一真源——文档明示「回调是尽力投递，status 是权威快照」。

### 5.4 生命周期

- **取消**：协作式——运行中单元跑完或超时，未开始单元置 skipped；`cancel` 与完成事件竞态时幂等（同 `process_kill` 尽力而为语义）；
- **停用回收**：`task::purge_for_plugin` 挂入 `host.rs::deactivate_plugin_inner`（紧邻 ws / pty / mdns purge）：cancel 全部在册任务、清注册表、清回调队列，只碰本人；
- **错误边界**：池任务与派发任务一律 `spawn_with_error_boundary`（F9）；单元 panic 按该单元失败收集，不拖垮任务与池；
- **急停**：`on-shutdown` 时 cancel 全任务并等池排空（有上限），沿 Graceful Shutdown ADR。

---

## 6. 安全与权限

- **双门结构**：`task:run` 管「占用宿主线程资源」这件事本身；每个单元**另过**其 kind 对应的既有域权限门（fs:read + fs_auth / process:run / http 门）。仅授 `task:run` 不授域权限的插件，所有单元都会失败——并发能力与数据访问能力解耦授权、解耦审计；
- **fs_auth 弹窗差异（行为契约，必须写进 WIT 注释）**：单元操作只做 fs_auth **已授权校验**，未授权路径直接 `Err`，**绝不从池线程触发用户弹窗**（弹窗会长时间占用池槽位并困惑用户）。需要新授权的路径，插件须在普通调用栈里先 `host-fs.request-auth`；
- **凭据红线不变**：单元结果不落日志明文（沿 §8）；宿主全量审计日志记 plan 摘要（kind / 单元数 / owner），不记 params 全文（process 命令行等敏感参数按 process 域现状口径）；
- **五同步点**（权限拆分纪律）：SDK 常量与 API 映射 / 打包 CLI / 前端合法集合 / `capability.rs::HOST_PRIMITIVE_CAPABILITIES` 清单（20 → 21 组）/ host_impl 权限门——`task:run` 上线时五处同步落。

---

## 7. 配额与资源上限（`PLUGIN_TASK_*`，宿主仲裁、超限 fail-visible，沿 F8）

| 常量（建议名） | 建议默认 | 含义 |
| --- | --- | --- |
| `PLUGIN_TASK_POOL_THREADS` | 8 | 全局池线程数（宿主配置面可调） |
| `PLUGIN_TASK_MAX_JOBS_PER_PLUGIN` | 4 | 每插件并发在册任务上限（含 running + queued） |
| `PLUGIN_TASK_MAX_UNITS_PER_PLAN` | 256 | 单计划单元数上限 |
| `PLUGIN_TASK_UNIT_RESULT_MAX_BYTES` | 1 MiB | 单元结果上限，超出截断 + `truncated` 标记（保护回调载荷与线性内存） |
| `PLUGIN_TASK_UNIT_TIMEOUT_MS`（缺省） | 600_000 | 单元缺省超时（与 process DEFAULT_TIMEOUT_MS 同档） |
| `PLUGIN_TASK_JOB_TIMEOUT_MS`（缺省） | 3_600_000 | 任务墙钟缺省超时 |
| `PLUGIN_TASK_CALLBACK_QUEUE_DEPTH` | 64 | 每插件回调队列深度 |
| `PLUGIN_TASK_STATUS_RESULTS_MAX` | 64 | status 终态结果保留条数上限（超出只留计数——大结果别靠 status 兜底） |

---

## 8. 重入与死锁纪律（红线，实现与 SDK 文档必须双写）

1. **池线程永不回调进插件**：回调只经 §5.3 的派发任务 + `with_wasm_plugin_call`（F4 模式），从机制上不触碰 guest 调用栈；
2. **插件禁止在 guest 调用栈内同步等待自己任务的事件**：回调需要 F1 的锁，而 guest 正持有它 → 自死锁。等待语义一律走 `execute-batch`（宿主侧 join，不经 Store）；异步任务的结果消费只能在事件回调 / 后续空闲调用里做；
3. **回调内再 submit 允许**（新调用、新拿锁，非嵌套），但受 §7 配额约束，SDK 文档提示避免「回调风暴」模式；
4. **`execute-batch` 阻塞上限**：单批单元数与单元超时上限（§7）决定最坏阻塞时长，SDK 文档沿用 run-sync 的「百毫秒～秒级适用」口径，长任务一律 `submit`。

---

## 9. 消费场景（真实性校验）

- **session 插件 git 域（票 03/04）**：现以 `run-sync` 串行跑 git 命令；`process.run-sync` 单元并发后，status 树并行取 diff/log/blame，HTTP 命令面同步拿全量结果；
- **file-transfer**：发送前并行 stat / 读取目录展开（`fs.*` 单元），替代串行循环；
- **agent-hub**：使用统计 / 会话日志解析 = 并行 `fs.read` 多文件 + `process.run-sync` 检测命令，CPU/IO 密集正中池化收益；
- **ai-chatbox 等纯网络插件**：无感（不申请 `task:run` 即零开销）。

---

## 10. 备选方案与裁决

| 备选 | 结论 | 理由 |
| --- | --- | --- |
| **A. wasmtime async WIT（component async + call_async 全量改造）** | 否（当前） | ABI 全量重铸 + SDK/双端/移动 47 分叉同步，成本极大；收益主要在 IO 等待（已有 block_on_async 覆盖）而非真并发；stable 工具链与全同步 ABI 现状冲突。**但其中间形态「WASIp3 协作式线程」是中期演进路径（§14）**——built-in 机制 + `std::thread` 透明，非全量 async 改造 |
| **B. 回调走消息总线 owner topic（`task:event.<owner>`）** | 否 | activate 期订阅时序失败面（F7：宿主不缓冲）；与插件间消息 `on-message` 语义混杂。已在 §4.2 详述 |
| **C. 扩展 host-process** | 否 | host-process 语义锚定「外部进程」；通用并发执行器是新关注域，混装会让两个域的权限/配额/审计都拧巴 |
| **D. 同插件多 Store 实例并行跑插件代码** | 否 | 身份/存储/权限/互调全按 plugin_id 单实例建模（F1），多实例是宿主结构性改造；且插件代码并发的收益场景（CPU 密集业务）本就该留在宿主原语或独立进程 |
| **E. 宿主内嵌脚本解释器执行「任意任务脚本」** | 否 | 编排引擎 = 业务语义进内核，直接违反 ADR 0022 裁剪线；本方案只做「无依赖单元的并发执行」，顺序编排留在插件 |
| **F. WASIp3 协作式线程（component-model built-in）** | 中期演进（非本票） | 见 §14：Rust wasm32-wasip3 `std::thread` 未来默认获得协作式支持，插件代码可表达逻辑并发（交错 ≠ 真并行）；工具链未就绪（当前 `std::thread` 仍 error）+ 依赖宿主 async store（A0-3）与关键原语 async 化，故不纳入当前实施。与 host-task 互补：协作式解决「并发编排」，host-task 解决「真并行工作」 |

---

## 11. 与既有范式对齐表（实现时逐条对照，防漂移）

| 维度 | 对齐先例 | 本方案取法 |
| --- | --- | --- |
| 新 interface 引入 | v16 host-pty | ABI 19→20，world 增 import；可选导出走独立 world（v14 events-ws 模式） |
| 异步回调 | host-process run / dispatch_process_done | `dispatch_task_event` 同模式；新导出 events-task 替代「往 events 里塞函数」（后者会破坏旧插件实例化） |
| 句柄与属主 | pty- / wsc- / sess- | `task-<uuid>` + 仅属主可调 + purge_for_plugin |
| 事件丢失自愈 | ws list-clients / pty is-running | `status` / `list-jobs` 快照原语 |
| 配额常量 | PLUGIN_WS_* / PLUGIN_PTY_* | §7 表 |
| 同步阻塞口径 | run-sync「百毫秒级可接受」 | execute-batch 同口径 + 上限常量 |
| 后台任务包装 | spawn_with_error_boundary | 池任务与派发任务全覆盖 |

---

## 12. 实施拆票建议（未实施；届时按此落票到 `.scratch/<task>/issues/`）

1. **票 A · 契约与 SDK**：WIT v20（host-task + events-task + world plugin-task）、`abi.rs` bump、SDK trait + `wasm_entry!` 默认导出、五同步点全落、双端偏离记录（mobile 停 11）；
2. **票 B · 宿主执行器**：`TaskRegistry` + 专用池 + `host_impl/task.rs`（权限双门 / 配额 / plan 校验 / 单元直调映射）+ 常量；
3. **票 C · 回调与回收**：events-task 探测与降级、`dispatch_task_event`、有界队列与背压、`purge_for_plugin` 接入 deactivate、shutdown 排空；
4. **票 D · 闭环验证与文档**：fixture 插件（沿 `packages/plugin-*-test` 模式，wasm32-wasip3）覆盖并发正确性 / 取消 / 超时 / 配额 / 未导出降级 / 停用回收；同步 code-map、AGENTS §7 清点、CHANGELOG。

## 13. 测试与验收要点（实施票 D 展开为矩阵）

- 并发正确性：N 单元全完成、结果按 id 关联、fail-collect（部分失败不中断）；
- 取消 / 超时：cancel 后未启动单元 skipped、运行中单元跑完、墙钟超时 → cancelled 且已完成结果保留；
- 配额：超 `MAX_JOBS_PER_PLUGIN` / `MAX_UNITS_PER_PLAN` fail-visible（错误可见，不静默降级）；
- 回调背压：队列溢出丢 progress、terminal 必达（或 error 留痕）、`droppedEvents` 计数与 status 一致；
- 未导出 events-task 的旧产物：加载不受影响、事件丢弃 + 首次 warn + 计数（F5 降级路径）；
- 停用回收：deactivate 后任务全取消、注册表清空、无回调投递残留；
- fs_auth：未授权路径在单元内 `Err` 且**无弹窗**（关键行为差异断言）；
- 死锁回归：guest 栈内 submit + 立即等待自身事件的反模式由 SDK 文档约束（不阻止 API），池路径压测不出现锁自死锁。

---

## 14. WASIp3 协作式线程演进路径（中期方向；用户 2026-09-21 指示纳入设计）

### 14.1 技术事实（2026-09 现状，外部证据）

- **协作式多线程是 Component Model 的 built-in**（位于 WASI 层之下，spec 以 🧵 emoji 标注 gated feature）：为 core wasm 提供创建/切换协作式线程的 built-in imports，构建在 Preview 3 已有的 async machinery 之上，顺序交错（sequentially-interleaved）、只在显式程序点切换，**非真并行**（fiber 式栈切换）
- **wasmtime 已合入实现**：PR #11751「Cooperative Multithreading」（2025-10-27 merged）。wasmtime 48 对该 built-in 的启用方式与 Config 开关**需实施前验证**（本稿未验证）
- **Rust 侧透明**：Rust 组件 `std::thread::spawn` 在工具链就绪后**默认**获得协作式支持（“A Rust component using `std::thread::spawn` gets cooperative thread support when it lands, with nothing special needed in WIT”）；**但当前 wasm32-wasip3 的 `std::thread` 仍返回 error**，属未来兼容性变更（compiler-team #1001 / rustc book）；LLVM 侧 PR #175800 进行中
- **时间线**：rustc book 明确「future release of Rust's wasm32-wasip3 target will support cooperative threading and `std::thread` APIs」——**工具链未就绪，这是本路径不纳入当前实施的决定性理由**

### 14.2 能力模型与边界（对照 §2 F1-F3）

| 能力 | 协作式线程给插件 | host-task（本稿）给插件 |
| --- | --- | --- |
| 插件代码并发表达 | ✅ 可 spawn 逻辑线程、并发 await 宿主调用 | ❌ 插件代码恒单线程，只能「提交计划」 |
| 真并行（多核利用） | ❌ 单物理线程交错，CPU 密集无收益 | ✅ 专用 OS 线程池真并行 |
| 切换点 | async 宿主调用（await 时 yield 让出 Store） | 不涉及（宿主侧执行） |
| 前提 | 宿主 async store（A0-3）+ 宿主原语 async 化（await 点） | 宿主 sync ABI 即可（现在可做） |
| 工具链 | 未就绪（wasip3 `std::thread` 当前 error） | 就绪 |

**核心洞察**：协作式线程是对 §2「根本边界」的破界路径——它让「插件代码并发跑」无需多 Store / 全量 async 改造，而是由 built-in 机制在**单 Store 内交错**。**但切换点必须落在 async 宿主调用上**：当前 ABI 的同步原语（`block_on_async` 桥）调用期间 Store 被阻塞，逻辑线程无法交错——因此本路径依赖宿主 async 化（A0-3），且**关键 IO 原语从「同步桥」改为「async 原语 await」**（fs / process / http 等协作式线程会等的原语；不需要全部原语 async 化，顺序编排仍走同步桥）。

### 14.3 与 host-task 的边界与演进（裁决）

- **互补不互斥**：
  - 协作式线程解决「并发编排」——并发 stat、多路 HTTP 并行等待、git 多命令并发跑（IO 等待密集），届时插件直接用 `std::thread` + 并发 await 原语写，比 execute-batch 计划 DSL 更自然；
  - host-task 保留解决「真并行工作」——CPU/IO 密集长任务（批量哈希、日志解析、大文件展开）需要 OS 线程池真并行，协作式线程给不了；
- **execute-batch 定位不变**：真并行扇出（CPU/IO 密集）是协作式线程不可替代的档；「几十个快操作并行」场景届时两条路并存（execute-batch 真并行 / std::thread 交错），由插件按资源语义选择；
- **submit / on-task-event 定位不变**：「宿主托管生命周期 + 配额 + 取消」仍是有价值的选择；协作式线程可用后插件也可用 std::thread + 直接 await 自管，两者并存；
- **对 host-task 契约的影响（现在就要留的余地）**：
  1. 单元 `params` = 既有原语请求 JSON 原样内嵌——协作式线程到来后插件可直接并发调这些原语，单元投影语义不变、零契约漂移；
  2. **不引入**「插件内并发执行模型」承诺：协作式线程是工具链 / built-in 能力，host-task 不为其改契约（避免把内核设计绑死在未定标准上）；
  3. 时间线：host-task（现在，本稿）→ 宿主 async 化 A0-3 + 关键原语 async 化（中期）→ rust wasip3 `std::thread` 落地（外部等待）→ SDK 模板/文档支持 std::thread 并发模式（届时另立设计票，评估 host-task 缩窄与 execute-batch 存留）。

### 14.4 不纳入本稿实施的理由（一句话）

工具链未就绪（Rust `std::thread` on wasip3 当前 error）+ 宿主 async 化未落地（A0-3 未实施）+ wasmtime 48 启用方式未验证——三项外部依赖均未满足；host-task 不依赖任何一项，先行落地为「真并行工作」提供并发，协作式线程作为中期演进路径记录于此，届时宿主 async 化完成、工具链就绪后另立设计票评估。
