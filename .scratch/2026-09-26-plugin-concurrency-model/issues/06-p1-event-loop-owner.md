# 06 — P1：事件循环属主化（宿主调用模型改造，不改 WIT）

**Type:** task
**Spec:** `../spec.md`（§3 架构改造 + §6 阶段 P1 + §7 风险 A2）
**Blocked by:** 02（取消语义设计）、03（架构审计设计）；另含 **P0 门禁**：01–05 全部有结论（结论写回 spec §11，ADR 由票 10 落地）——未过门禁不进入 P1（spec §6）
**Status:** done（2026-09-26）——P1 全部落地并通过验收（全量 903 + 插件 364 用例绿）；
**默认 `call_model` 仍为 `mutex`**（切默认待真机复验，见 `## Progress` 第 6 步）

**What to build:** 把宿主的插件调用模型从「每调用一把实例锁 + `spawn_blocking` + `block_on_async`」改为「**每插件实例一个事件循环属主任务**（唯一持 `&mut Store` 的地方）」。**不改任何 WIT**（I5：无 async import 的存量插件逐字节等价）。这是本 spec 的 P1 阶段，是 P2/P3 的地基。

目标形态（spec §3.1）：

```text
webview/IPC ─► 请求通道（入队）
属主循环（持 &mut Store）：
  ├─ 取出请求 → call_concurrent 启动 guest task（不等完成）
  ├─ 同步 import 的 task 立即完成 → 结果回传（oneshot）
  ├─ async import 的 task 挂起 → 属主继续处理下一条
  └─ 完成/失败 → 结算并通知
```

**不变式（spec §3.2，每条必须有测试或结构锁，不接受「实现里自然满足」）：**

| ID | 不变式 | 落地形态（建议） |
| --- | --- | --- |
| **I1** | 同一实例至多一个属主；不存在第二个 Store 入口 | 结构锁：`&mut Store` 只在属主任务内触达；第二进入路径编译期消除或运行期显性拒 |
| **I2** | 任何单次 guest 调用不阻塞属主——立即完成或挂起成 task | 挂起模拟测试：挂住 guest task 时另一命令可完成。⚠ **票 02 实测 A2'：wasmtime 48 上挂起 task 会阻塞同实例新调用**，此条验收前先看票 01 的 A2' 复核结论（可能需降级为「不阻塞属主」而非「不阻塞同实例」） |
| **I3** | trap/panic/重载/停用的语义重新定义且 fail-visible（不得静默吞掉半个任务） | **票 02 已定稿**（`issues/02-p0-a2-cancellation-semantics.md` §3 + spec §3.2）：① task trap = 整实例不可用，恢复 = 停属主→丢 store→重建；② trap 时在等请求逐条显式失败；③ 停用/退出先丢 store 再回收，跳过 guest `on_shutdown` 必告警+计数；④ 放弃等待 ≠ 取消 |
| **I4** | 启动顺序 = 入队顺序（同实例串行语义保持） | 乱序场景测试：并发入队 N 条，完成序 = 入队序（同步任务） |
| **I5** | 无 async import 插件行为逐字节等价：返回值、错误串、trap 恢复、fuel/指标 | 存量 4 个 wasm-app（terminal-session / file-transfer / ai-chatbox / agent-hub）契约测试 + 宿主 wasm_flow 集成全绿，断言不改 |
| **I6** | 属主任务生命周期与应用退出/停用/进程回收顺序确定（无孤儿 task / 无持锁线程泄漏） | 停用即发消息→属主退出 → 丢 store（按票 02 设计）；进程退出路径测试 |

**灰度（spec §6）**：`call_model` 开关（`mutex`（现状）/ `event-loop`（新），接线点按票 03 审计）；**默认先 `mutex`**；本票验收通过后切默认 `event-loop` 并保留一版回退窗口（开关双向可切，回退后行为与今天一致——spec §8 A4）。

**实施约束：**

- **零业务**（AGENTS §5.1 自检：宿主侧改动，三问裁决 + 自检 6 问）；不动 WIT/ABI/SDK/插件。
- 任务级计量：`track_call` / `refill_call_fuel` 口径随属主化调整（spec §3.3），口径变化同步改监测面与文档。
- fuel 多 task 可观测性已由探针验证（10000000 → 9999961），可直接复用其测试形状。
- 两段式测试：开发中跑针对性单测（`cargo test <前缀>`），本票收尾跑全量（桌面端 `cargo test` 全绿 + 插件集成测试 + 宿主 wasm_flow）。
- 完成定义：验收 A1（I1–I6 每条有测试/结构锁）、A2（存量等价全绿）、A4（灰度双向可切）。

**Out of scope:**

- 不改 WIT、不 bump ABI、不 async 化任何原语（P3 的事）。
- 不做 SDK（票 07）、不做第一个 async 原语（票 08）。
- 不做候选原语评估（票 09）。

**Acceptance（spec §8 A1/A2/A4）：**

- [x] I1–I6 逐条有测试或结构锁（对照表见 `## Progress` 第 6 步「I1–I6 逐条落地对照」）。
- [x] `call_model` 开关双向可切；`mutex` 模式下行为与今天一致（两模型对照用例 + 默认值保持
      mutex 的回退窗口；切默认待真机复验）。
- [x] 存量插件行为等价：`wasm-apps/terminal-session` 契约测试 364 用例 + 宿主 wasm_flow /
      session_e2e 集成 + 桌面端 `cargo test` 全量 903 绿。
- [x] trap/purge/停用语义按票 02 设计落地，fail-visible 有测试（`owner_cleanup_skipped`
      计数 + warn + 两模型对照）。
- [x] fuel/指标口径完成调整并有结构锁（`refill_fuel(` / `let _timer = timer;` 断言 +
      `calls_total` 用例；燃料仍是**实例级**预算，非 per-task 累加——写入 code-map 与 ADR）。
- [x] 桌面端 `cargo test`（含集成 target）全量通过；无残留进程 / 端口（8765 / 1420 / 5173 无监听）。
      `lens_diagnostics mode=all` 未跑（需启动 app；本次为纯宿主内核改动，真机复验时一并执行）。

## 关联证据

- 现状调用链：`src-tauri/src/wasm_core/manager/host/commands.rs:137/166`（`run_guest_call` / `with_wasm_plugin_call`）、`component.rs:1442`（`block_on_async(call_invoke)`）、`runtime.rs:307`（`wasm_component_model_async(true)`）
- 探针结论：`.scratch/2026-09-26-wasmtime-cm-async-eval/probe/`（P1 挂起 task 不独占 store；P2 run_concurrent 退出后状态为空；P3 多 task fuel 可观测）
- 既有测试形态参照：`manager/runtime/tests/engine_limits.rs`、`manager/runtime/tests/p3_async_host_import.rs`（host-side async 探针，注意它解决的是「让出宿主线程」，**不等于**本票的实例锁解耦——见票 04 的交叉结论）
- AGENTS §5.1（宿主红线）、§10 完成定义

---

## Progress（2026-09-26，施工顺序 §7 第 1-2 步已落地并验证）

**Status: in-progress**——第 1-2 步 done（编译绿 + 属主用例 5/5 + 宿主 lib 全量回归），第 3-6 步待续。

### 第 1 步：Store/Instance 与元数据拆分（零行为变化）

- `manager/runtime/component.rs`：新增 `InstanceMeta`（plugin_id / preopened_dirs /
  exported_capabilities / created_at）与 `OptionalExports`（4 个可选导出句柄快照，Copy）；
  `LoadedWasmPlugin` 字段组由「三元组散字段」收敛为 `meta + instance + store`，
  `Drop` 生命周期日志改读 `meta`（日志逐字不变）。
- 新增属主化访问器（作用域内拿不到 `&mut self`，必须在进入前取出）：
  `instance_handle()` / `store_mut()` / `metrics()` / `fuel_spec()` / `optional_exports()`
  / `meta()`。`exported_capabilities()`、`preopened_dirs()` 改为 delegate 到 meta（调用方零改动）。
- `manager/capability.rs`：`EXPORT_STORAGE_{GET,SET,DELETE}` 提为 `pub(crate)`（属主 op 闭集复用）。
- 回归：`cargo test --lib` 全量（见下）。

### 第 2 步：`manager/host/owner.rs`（属主任务本体，~700 行）+ 直接驱动测试

- 闭集 `GuestOp`（15 op：世界导出 11 + 能力转发 4）/ `OpKind`（Copy，日志与文案口径单点）/
  `GuestReply`（8 形态，保留 WIT `result<T,string>` 内层）/ `OwnerOutcome`（Done / Failed / Trap）。
- `OwnerHandle`：`call()`（异步门面，队列满 / 渠道关闭 / 实例终止一律**立即显性 Err**）、
  `call_blocking()`（`block_on_async` 同步桥，供 bus/process/task/能力转发四处）、
  `is_alive()` / `stats()` / `stop()`（等属主退出 ⇒ **store 已 drop** 才返回，超时 abort 兜底）。
- `spawn_owner`：整块 `LoadedWasmPlugin` 移入属主任务（**I1 by construction**：宿主侧不再有第二个
  Store 入口）；`catch_unwind` 包任务体 ⇒ 宿主函数 panic 与 trap 同路收敛到 `OwnerFailureSink`。
- `owner_body`：常驻 `run_concurrent` 作用域 + `select! { biased }` + **单点 start**（I4）；
  `start_op` 内做燃料续费（续费点差值记账，票 03 §3 口径）→ `get_typed_func` →
  `start_call_concurrent`；完成解释（`interpret`）后交属主循环结算。
- `settle`：Done → 应答 + 生命周期记账（ActivateOk / Deactivate；Failed 记 ActivateFail）；
  Trap → 该请求按 op 文案 Err + `stats.traps` + 属主退出（实例级失败交 sink）。
- 退出统一结算：在等请求逐条显式 Err（I3②）+ 通道内未 start 的请求同样显式 Err，
  「放弃等待」计数与日志（I3④，任务不取消）。

**新增硬事实（施工中发现，写回票 03/02 的结论需知）**：

1. **guest task trap 从 `run_concurrent` 顶层 `Err` 冒出**，不是从 `finish_call_concurrent` 返回
   （后者是短命作用域 `call_async` 的形态）。⇒ 属主循环会被整个中断，trap 请求的应答只能走
   「退出统一结算」路径。为保住 I5 的**错误串逐字等价**，实现按「trap 归属最早的在飞请求」处理
   （mutex 模型下调用串行，单在飞是常态），其余在等请求收显式「instance failed (trap): …」文案。
2. **在飞 future 只能借用 `&Accessor`**：wasmtime 48 未提供自持 accessor 的公开构造
   （`clone_for_spawn` 私有、`Accessor` 无公开 `Clone`）。⇒ 在飞表（`FuturesUnordered`）必须留在
   `run_concurrent` 作用域内，只有「在等请求表」（`id → oneshot`）留在作用域外（供退出统一结算）。
   结构锁 `owner_i4_structural_lock` 钉死 start 单点与 `biased`。

**已覆盖的不变式（`manager/runtime/tests/owner_e2e.rs`，5 用例，真实夹具组件驱动）**：

| 不变式 | 用例 |
| --- | --- |
| I3① trap = 整实例不可用 + 后续调用显性失败 | `owner_trap_poisons_instance_and_reports_failure` |
| I3② 在等请求逐条显式失败（含 sink 上报 "trap"） | 同上（+ 退出结算路径） |
| I4 完成序 = 入队序 + 结构锁 | `owner_serves_commands_and_preserves_enqueue_order`、`owner_i4_structural_lock` |
| I6 停止语义（store 已 drop / 幂等 / 后续调用显性失败） | `owner_stop_is_prompt_and_later_calls_fail_explicitly` |
| 有界失败（队列满立即显性 Err，不丢已入队请求） | `owner_queue_full_is_explicit_failure` |
| 指标口径（`calls_total` 随 task 递增） | `owner_serves_commands_and_preserves_enqueue_order` |

### 第 3 步：装配条目 + 统一门面 + 调用点切换（done，2026-09-26）

- **装配条目**（`manager/host.rs`）：`WasmInstanceEntry { meta, call_model, slot }` +
  `enum InstanceSlot { Mutex(Arc<Mutex<LoadedWasmPlugin>>), Owner(OwnerHandle) }`；
  装配表类型改 `Arc<RwLock<HashMap<String, Arc<WasmInstanceEntry>>>>`（I1：Store 只能
  存在于 slot 内，宿主侧无第二个入口）。宿主侧只读元数据一律走 `entry.meta()`
  （认证中心候选、系统组件能力装配、预打开目录漂移判定——不再锁实例）。
- **门面**：`PluginHost::call_guest(op)`（异步）+ `call_guest_blocking(op)`（同步桥，
  供 bus / process / task 四处派发）。`mutex` 分支 = 原 `run_guest_call` 逐字搬移
  （`spawn_blocking` + ambient + `catch_unwind`）+ 按 `OpKind::recovers_after_failure`
  复刻原 `with_wasm_plugin_call` 的通知/重载调度（**I5 by construction**：op 分派
  `dispatch_mutex_op` 逐条委派既有导出方法，错误串不变）；`event-loop` 分支 = 属主队列
  + oneshot。`with_wasm_plugin_call` / `run_guest_call` / `call_plugin_capability_export`
  三个旧入口删除；`get_wasm_plugin`（直锁访问器）删除。
- **调用点切换（10 处 + 4 处元数据）**：`commands.rs`（命令面）、`services.rs` 四派发
  （bus / ws 帧 / process done / task event；`AtomicBool` 旁路去掉）、`activation.rs`
  四生命周期（activate / on_startup / on_shutdown / deactivate）、`auth_center.rs`
  （`PluginHost::call_auth_policy` 取代泛型直锁调用）。
- **灰度开关**：`CoreConfig.call_model`（`mutex`（默认，回退窗口）/ `event-loop`，
  `wasm-core.json` 可按名覆盖，非法值加载即报错）；**实例级快照**（建实例时读一次，
  存进装配条目），`rebuild_wasm_instance` 按当前配置重建 = 「reload 即切换」。
- **失败回报端口**：`HostOwnerFailureSink`（持 `Weak<PluginHost>`，避免宿主自引用强环）
  在 `PluginHost::new` 中先绑定再实例化——因此 **`PluginHost::new` 现返回 `Arc<Self>`**，
  实例化路径（启动扫描 / zip 安装）整体后移到 `Arc` 就绪之后
  （`instantiate_scanned_wasm_plugins`）。
- **`event-loop` 实例的再激活必须重建**：停用即丢 store（I3③）⇒ `activate_plugin`
  phase 2 先查 `entry.owner_stopped()`，为真则 `rebuild_wasm_instance`（与预打开目录
  漂移判定合并为一次，避免连续重建）。
- **能力转发的层级约束被顺手修好**：`CapabilityProvider` / `CapabilityTarget` 两个窄端口
  移除了 host_api 对 manager 类型的**最后一处**引用（原 `Arc<Mutex<LoadedWasmPlugin>>`），
  转发方不再知道调用模型。

### 第 4-5 步：停机顺序 + 能力转发超时（done，2026-09-26）

- **停机顺序（I3③ / I6）**：`deactivate_plugin_inner` 第一步停属主丢 store，再做既有资源
  回收；`event-loop` 实例的 guest `on_shutdown` / `deactivate` **显性跳过 + 计数**
  （`owner_cleanup_skipped`，warn 日志含 `skipped` 字段，前端呈现留待按需）；
  `deactivate_all` 末尾 `shutdown_all_owners` 兜底；`rebuild_wasm_instance` /
  卸载先停旧属主再动 map；`OwnerHandle::drop` 仍保留 Stop + abort 兜底。
- **能力转发超时**：`CAPABILITY_FORWARD_TIMEOUT = 5s`，在端口实现内 `tokio::time::timeout`
  包住转发调用 ⇒ 环依赖（A→B→A）从「永久死锁」降级为「有界失败 + 能力回落宿主原语」。

### 第 6 步：验收（done，2026-09-26）

**新增/修改测试**

- 两模型对照 `manager/host/tests/instance_call_model_test.rs`（4 用例）：命令往返 /
  停用→再激活（属主模型走重建）/ trap 文案与失败可见性 / 结构锁（装配形态 + 属主启动单点 +
  能力导出无直锁调用）；模型经 `install_instance(..., model)` 显式装配，不依赖全局配置
  （两模型并行安全）。夹具组件字节落盘（`materialize_fixture_wasm`）——重建路径从磁盘加载。
- `config.rs`：`call_model` 默认 mutex / 按名解析 / 非法值报错（灰度开关 fail-visible）。
- 既有测试适配：`PluginHost::new` 返回 `Arc<Self>`（commands/services/scaffold/scan_dedup/
  集成测试 5 处 + bench）；实例注入改走 `install_instance` / `instantiate_wasm_plugin`；
  系统组件用例的能力导出调用改走装配条目门面。

**验证结果（2026-09-26）**

| 门 | 命令 | 结果 |
| --- | --- | --- |
| 桌面端全量（含集成 target + doctest） | `cargo test` | **903 passed / 0 failed**（lib 63.8 s）+ 集成 target 全绿（broadcast_shutdown / pty_session_chain / ws_auth_rules / http_auth_biometric / link_crypto_http / server_integration / wasm_bridge_bench(1 ignored) / build_manifest_smoke） |
| 宿主测试域（含 wasm_flow / system_component 真实产物闭环） | `cargo test --lib "wasm_core::manager::host::tests"` | 65 passed / 0 failed |
| 属主任务直驱 | `cargo test --lib owner_e2e`（含于全量） | 5 passed / 0 failed |
| 存量插件行为等价（A2） | `cd wasm-apps/terminal-session/rust && cargo test --offline` | **364 passed / 0 failed** |
| 编译零新增告警 | `cargo check --profile test --lib` | 本次改动文件 0 告警 |

**默认开关仍为 `mutex`**：切 `event-loop` 是配置一行（`wasm-core.json` 的 `call_model`，或
`CoreConfig::default` 的取值）。切默认前建议真机复验一轮（§6 灰度：P1 验收通过后切默认并保留
一版回退窗口；工作区惯例 = 行为变更由用户复验后落默认）。**真机复验要点**：终端会话输入/resize
与输出拉取、插件停用→启用、trap 自愈、文件传输与 AI 聊天面。

**I1–I6 逐条落地对照（验收 A1）**

| 不变式 | 测试 / 结构锁 |
| --- | --- |
| **I1** 同实例至多一个属主（无第二 Store 入口） | `instance_call_model_test::instance_slot_structural_lock`（装配表类型只存条目 / `InstanceSlot` 双形态 / 属主启动单点 / 命令面与能力转发无直锁）；`owner.rs::spawn_owner` 移入整块 `LoadedWasmPlugin`（by construction） |
| **I2** 单次调用不阻塞属主循环 | `owner_e2e::owner_serves_commands_and_preserves_enqueue_order`（入队即 start，不等待完成）+ `owner_queue_full_is_explicit_failure`（有界失败）；边界表述见下「已知边界」 |
| **I3①** trap/panic = 整实例不可用 | `owner_e2e::owner_trap_poisons_instance_and_reports_failure`、`instance_call_model_test::trap_error_text_and_post_trap_failure_match_across_models`（两模型同文案 + trap 后显性失败） |
| **I3②** 在等请求逐条显式失败 | 同上 + `owner.rs::owner_body` 退出统一结算（含通道内未 start 请求） |
| **I3③** 停用/退出先丢 store 再回收；跳过 guest 清理显性计数 | `deactivate_then_reactivate_works_in_both_call_models`（`owner_cleanup_skipped` 计数 1 / mutex 计 0）+ `activation.rs` 停机顺序注释与实现 |
| **I3④** 放弃等待 ≠ 取消 | `OwnerStats.detached` 计数 + `owner.rs::send_reply` 的 debug 日志 |
| **I4** 启动顺序 = 入队顺序 | `owner_e2e::owner_serves_commands_and_preserves_enqueue_order` + `owner_i4_structural_lock`（`biased` + 单点 start） |
| **I5** 存量插件行为等价 | 命令往返 / 生命周期 / 错误文案两模型对照 + 宿主 wasm_flow（真实产物）+ `cargo test` 全量 903 + 插件侧 364 用例；`mutex` 分支 = 原实现逐字搬移（`dispatch_mutex_op` 委派）+ 结构锁断言 |
| **I6** 属主生命周期确定（无孤儿任务） | `owner_e2e::owner_stop_is_prompt_and_later_calls_fail_explicitly`（stop 幂等、store 已 drop）+ `deactivate_all` 末尾 `shutdown_all_owners` 兜底 + `OwnerHandle::drop` 停止/abort 兜底 |
| fuel / 指标口径（spec §3.3） | `owner_e2e` 的 `calls_total` 断言 + `instance_slot_structural_lock` 的 `refill_fuel(` / `let _timer = timer;` 断言（续费在 start 前、计时器绑 task 生命周期） |

**已知边界（不在 P1 解决，写进 ADR/后续票）**

- 嵌套等待（能力转发 / 互调）仍占住调用方属主 —— P1 只加 5 s 超时兜底（P4 候选 ⑤）。
- `event-loop` 实例停用即丢 store ⇒ guest `on_shutdown`/`deactivate` 不再执行（计数 +
  warn 可见）。这是 spec I3③ 的既定取舍（宁可显性跳过，也不赌挂起 task 放行）。
- 同实例并发执行不可得（wasmtime 实例级 `do_not_enter`，票 01 A2'）⇒ I2 只承诺
  「不阻塞属主循环」，不承诺插件内真并发。