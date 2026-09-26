# Spec：插件并发模型升级 —— 事件循环属主 + **按需**异步化

- **日期**：2026-09-26（2026-09-27 补 §12 实测与 P2/P3 退役）
- **状态**：**P0 门禁已过（2026-09-26）**——票 01–05 全部有结论（§11）；
  **P1 属主化已完成并验收（票 06，桌面端全量 903 + 插件 364 用例绿；默认 `call_model` 仍 `mutex`，切默认待真机复验）**；
  **A4 改判**：WIT 层 `async func` 不可用（票 04 No-Go），按需异步化拟走「宿主实现侧 async」路径（§5 前置门禁）；
  **P2/P3 退役（2026-09-27）**：新增决定实测（§12）证明「同实例串行的根因是实例级门，与 host import 是否 async 无关」
  ⇒ 两条阶段的收益承诺（解除慢调用堵死交互）在 wasmtime 48 上不可达，按「兑现不了就退役 + 如实写文档」处理
- **P4 收口（2026-09-27，票 09）**：②③④⑤ 逐个走查完成、**全部不立项**（C3✗ 决定性，②④ 另有 C4✗；
  无实施项 ⇒ 零生产代码改动）；逐项表见票 09「走查结论」，结论落 ADR 0029 决定 8；上游 issue 裁决 = 暂不开
- **P0 阶段新增约束（仍生效，已复核）**：挂起 guest task 阻塞同实例后续调用（票 02 发现、票 01 用官方产物复核确认）
  ⇒ **禁止**按「同实例并发执行」写实现；I2 表述固定为「不阻塞属主循环」（§3.2）
- **来源**：用户在 CM-async 探针（`.scratch/2026-09-26-wasmtime-cm-async-eval/spec.md`，P1 绿）后指令
  「将任务插件并发模型升级独立出来；当前的任务搁置，等待新的异步模型改造完成；**注意不是所有函数、所有 WIT 协议接口都异步化，按需改造**」
- **范围**：桌面端宿主内核的**插件调用模型** + 少量按需 async 化的原语 + 插件 SDK 的 async 能力
- **零业务**：本改造只动引擎/契约层，不引入任何产品语义（AGENTS §5.1 自检适用）

---

## 1. 背景：这不是性能问题，是结构性风险

现状每个插件实例**一把锁**：

```text
webview --plugin_invoke--> with_wasm_plugin_call
                             └─ run_guest_call
                                  ├─ spawn_blocking 线程
                                  ├─ wasm_plugin.lock()          ← Arc<Mutex<LoadedWasmPlugin>>
                                  └─ block_on_async(call_invoke) ← 全程持锁直到 guest 返回
```

因此：**一次慢的 guest 调用会把该插件的全部交互一起堵死。** 已确认的三处现实后果：

| 后果 | 出处 | 严重度 |
| --- | --- | --- |
| 非流式 `host-http.fetch` 在 import 栈内等网络往返（时长不受本仓库控制） | `wasm_core/host_api/http.rs:222` | 高：慢 HTTP → 输入/resize/`output.pull`/`ack` 全堵 |
| 终端输出只能前端定时轮询（洞 ① 无发布者，延迟地板 = 轮询间隔） | `TerminalPreview.vue` `pullTick` | 中：交互延迟 ≤50 ms（已被 F4/F5 补偿到 ≈0） |
| 「guest 挂起等某事件、同时处理其它命令」在经典模型下不可表达 | 同上根因 | 中：限制了插件的编程模型 |

CM-async 探针已证明**运行时能力可用**（挂起的 guest task 不再独占 store），且能力**已在我们的构建里**（`component-model-async` 在 wasmtime default features、`concurrency_support` 默认 true）。但探针同时证明**必须改 WIT**（linker 强制「async 标注 ↔ concurrent 注册」配对，见该 spec §2 F13）。

---

## 2. 目标 / 非目标

**目标**

- **G1** 消除「单次慢调用堵死整插件」的结构性风险（事件循环属主 + guest task）
- **G2** 插件可以表达「等某事件」的语义（async import）而不交还宿主控制权
- **G3** 与既有行为**等价**：同步插件（无 async import）在新模型下行为、错误、trap 语义不变
- **G4** 可灰度、可回退（双模型并存）

**非目标**

- **N1 不追求插件更快**——async 化会略微增加单次调用开销（每次调用一个 task），换来的是**不阻塞**；只在「有等待」的调用上启用（§4 判据）
- **N2 不全量异步化**——见 §4，这是本 spec 的核心约束
- N3 不改业务语义、不新增产品能力（AGENTS §5.1）
- N4 不解决输出字节的 JSON 化成本（`data: number[]`，另一议题）
- N5 不动移动端契约（ADR 0018 独立）；仅按 ADR 0019 同步 WIT 副本

---

## 3. 宿主侧架构改造

### 3.1 目标形态

```text
每个插件实例 = 一个「事件循环属主」tokio 任务（唯一持 &mut Store 的地方）
  webview/Tauri IPC ──► 请求通道（入队）
  属主循环：
    ├─ 取出请求 → 用 call_concurrent 启动一个 guest task（不等完成）
    ├─ 同步 import 的 task 立即完成 → 结果回传调用方（oneshot）
    ├─ async import 的 task 挂起在宿主 future 上 → 属主继续处理下一条请求
    └─ 任务完成/失败 → 结算并通知（前端事件 / 调用方）
```

### 3.2 不变式（落地后必须逐条有测试/结构锁）

| ID | 不变式 |
| --- | --- |
| **I1** | 同一实例**至多一个**属主（Store 独占）；不存在第二个入口 |
| **I2** | 任何单次 guest 调用**不得阻塞属主**——要么立即完成，要么挂起成 task |
| **I3** | trap / panic / 重载 / 停用 `purge` 的语义在属主模型下**重新定义且 fail-visible**（不得静默吞掉半个任务）。**（票 02 已定稿）** ① **trap / panic = 整实例不可用**，与今天逐字等价（实测：task trap 同样污染 store，之后所有调用报 `cannot enter component instance`）；唯一恢复是「停属主 → 丢 store → 重新实例化」，仍由 `schedule_plugin_reload_after_trap` 限频调度；**禁止**「只丢那个 task」——wasmtime 不提供 task 句柄（#11833）。② trap 发生时**所有在等该实例的请求逐条显式失败**，不排队、不静默重试。③ 停用 / 退出一律**先丢 store 再做资源回收**；因在飞 task 而放弃 guest `on_shutdown`/`deactivate` 必须显性告警 + 计数。④ 请求方放弃等待**不取消**任务：副作用照常生效，返回值丢弃并计数 |
| **I4** | 命令的**启动顺序 = 入队顺序**（同实例串行语义保持，AGENTS 既有约定） |
| **I5** | 无 async import 的插件（当前全部存量插件）行为**逐字节等价**：返回值、错误串、trap 恢复、fuel/指标 |
| **I6** | 属主任务的生命周期与应用退出、插件停用、进程回收的顺序确定（无孤儿 task / 无持锁线程泄漏） |

### 3.3 与今天 `run_guest_call` 的差异

| 维度 | 现状 | 目标 |
| --- | --- | --- |
| 谁持 Store | 每个调用各自 `spawn_blocking` + 抢锁 | 单一属主任务 |
| 调用模型 | `call_invoke`（同步，等完成） | `call_concurrent`（启动 task，不等） |
| 长调用 | 持锁等待 | 挂起成 task，属主继续服务 |
| trap 恢复 | `catch_unwind` + 调度重载（每次调用独立） | 属主集中处理（需重新设计：任务失败是否连带整个实例重载） |
| 指标/燃料 | 每次调用 `track_call` / `refill_call_fuel` | task 级计量（探针已验证 fuel 在多 task 下可观测：10000000 → 9999961） |

---

## 4. **按需**异步化：判据与清单（核心约束）

### 4.1 判据（只有 C1 同时满足才 async 化）

| 判据 | 内容 | 反例（保持同步） |
| --- | --- | --- |
| **C1 必要** | 宿主实现必须在返回前**等待一个不由本次调用自身驱动**的外部事件（网络对端、PTY 输出、另一任务完成、时钟到期） | SQLite 查询、`sha256`、取内存锁（微秒级 futex）、读配置 |
| **C2 显著** | 等待时长**不可由本仓库控制**（外部 IO / 未知调度延迟） | 本地计算、进程内信号量 |
| **C3 有价值** | 等待期间调用方**确有其它工作**可做（并发交互、或另一条命令） | 单次启动后立即返回的调用（如 `host-process.run` 已返回 run-id + 事件回调） |
| **C4 不可替代** | 同步实现会**阻塞属主/整实例**（今天就是一把锁） | 有等价非等待替代路径（如流式 HTTP 已是「立即返回 + 事件」） |

**任一判据不满足 → 保持同步**。async 化的代价是真实的：WIT 变更（ABI bump）+ 手写 `func_wrap_concurrent` 注册（`bindgen!` 不生成，见 CM-async spec F6）+ guest 侧 async lowering + 插件 SDK 支持。

### 4.2 白名单：**明确不做** async 化（除非将来出现 C1 场景）

按「本仓库实测存在 `block_on_async` 但语义属本地」筛出（`host_api/*.rs` 计数：`peer 23 / database 13 / auth 12 / ws 11 / pty 7 / app 7 / api 7 / process 6 / platform 6 / fs 5 / storage 4 / …`，**计数高 ≠ 要 async**）：

| 原语 | 现状等待什么 | 判定 |
| --- | --- | --- |
| `host-storage` / `host-database` / `host-plugin-database` | 本地 SQLite（async 包装的同步调用） | ❌ 同步（C1✗） |
| `host-crypto` / `host-config` / `host-fs` / `host-log` / `host-mdns` / `host-platform` / `host-app` | 计算、文件、元数据、短查询、进程内服务句柄 | ❌ 同步（C1✗/C2✗） |
| `host-process.run` | **不等**（返回 run-id + `on-process-done` 事件） | ❌ 同步（已是正确的非等待形态） |
| `host-bus.publish` / `host-events.emit` | 不等（投递） | ❌ 同步 |
| `host-timer.register` | 不等（注册后由宿主 tick 回调） | ❌ 同步 |
| `host-pty.ring-fetch` | 不等（游标拉取，设计如此） | ❌ 同步（**拉取模型不变**） |
| `host-wsl-fs` / `unit-executor` | 本地路径/执行 | ❌ 同步 |

### 4.3 候选清单（按证据强度排序，逐个立项评估）

| 优先级 | 原语 | 等待什么 | 证据 | 备注 |
| --- | --- | --- | --- | --- |
| ~~**① 首批**~~ **已退役** | `host-http.fetch`（非流式） | 外部网络往返 | `http.rs:222` 在 import 栈内 `block_on_async`；C1✓C2✓ **C3✗（等待期调用方无活儿可干——实例级门，见 §12）C4✗（有等价非等待替代：`stream:true`）** | **退役（2026-09-27，票 08）**：「异步化 import」不能解除「慢 HTTP 堵死同插件其它交互」——同实例串行由实例级门保证；宿主实现侧 async 只让出宿主线程（mutex 模型下切换前后无差别）。流式分支已是正确形态 |
| ② 评估 | 「等 PTY 输出」新原语（若将来走拉取模型替代方案） | 引擎环出现新字节 | 需先决定是否替代 `output-ack` 的 P2 | **走查结论（2026-09-27，票 09）：不立项**——C4✗（等价非等待替代已存在：宿主主动 publish / 限频唤醒 = output-ack P2 原方案，不改 WIT/ABI）+ C3✗（实例级门）；guest 侧 `await` 前提亦不成立（A4）。重评条件见文末统一口径 |
| ③ 评估 | `host-peer`（传输完成/进度等待） | 对端网络与传输状态 | 22 处真实调用点（票 05 分类） | **走查结论（2026-09-27，票 09）：不立项**——`dial`（含重拨兜底）/ `list-shared-roots` / `browse-directory` 三个「真等外部」入口 C3✗；`resume-transfer` 的 redial 分支为混合形态（要 async 化须拆专用原语）暂不立项；其余维持同步 |
| ④ 评估 | `host-ws`（等待对端帧/连接事件） | 对端 | 11 处真实调用（1 真等外部 + 10 本地） | **走查结论（2026-09-27，票 09）：否决**——不存在「等下一帧」语义（帧走可选导出 `events-ws` 回调 + 状态事件走总线，不缓冲不重放），唯一等待点 `connect` 握手已有超时常量上界 ⇒ C4✗ |
| ⑤ 待定 | `host-auth` / `host-api-call`（跨插件互调） | 另一个插件实例的响应 | `host-auth` 12 命中 → 生产 5 处全本地 SQLite；`host-api-call` 生产 4 处（唯一真等待 = 等回复） | **走查结论（2026-09-27，票 09）：维持同步**——auth C1✗；等回复 C3✗/C4✗。**F13 两端配对结论**：两端一起 async 化同样收益 ≈0（实例级门）且不解决 A→B→A 环依赖 ⇒ 不做；5 s 超时兜底保留，环依赖根治归「非等待形态」（需用户单独立项） |

### 4.4 反模式（明令禁止）

- ❌ 「反正要改架构，顺便把所有接口都 async 化」——ABI 变更面与 SDK 破坏面会失控，且绝大多数调用**没有等待语义**（§4.2）
- ❌ 为了「看起来并发」把本地计算包成 async import
- ❌ 一个 import 半 async 半 sync（同一原语内两种等待形态混用）——语义与可观测性都会变复杂

---

## 5. WIT / ABI / SDK 流程

> **前置门禁（票 04 结论，2026-09-26）**：WIT 层 `async func` 标注在当前锁定组合
> （wit-bindgen 0.60.0 + wasmtime 48.0.3 + wasm32-wasip3）下**不可用**（async-lifted 导出入口
> 即 abort，见 §11 第 4 项）。**W1 / W2 在门禁解除前不得启动**；「按需异步化」在落地时走
> **宿主实现侧 async（`func_wrap_async`，WIT 签名保持同步）**——该路径不需 WIT/ABI/SDK 变更，
> 实证见 `.scratch/2026-09-25-wasip3-host-api-optimization/issues/01`。W4（手写
> `func_wrap_concurrent`）只对 `async func` 有意义，同门禁。
>
> **第二道门禁（2026-09-27 实测，§12）**：即便走「宿主实现侧 async」，**收益也不成立**——
> import 挂起期间同实例显式调用零进展（实例级门），故 async 化只把「占住宿主线程」换成
> 「属主/调用方等同一个实例门」。⇒ **在实例级并发进入可用之前，任何「按需 async 化」提案
> 都必须先过 §12 的边界锁**，否则不立项。

| 步骤 | 内容 | 依据 |
| --- | --- | --- |
| W1 | 逐个函数加 `async func` 标注（**函数级**，不是 interface 级） | WIT 组件模型 |
| W2 | **ABI bump**（导入签名变更 = 组件契约变更；旧产物缺 import 会在实例化期 fail-visible） | ADR 0019 |
| W3 | 双端 WIT 副本同步（移动端**不实现** async 标注，但副本需一致） | ADR 0019 / 0022 |
| W4 | 宿主为该函数**手写** `Linker::func_wrap_concurrent` 注册（生成代码不覆盖） | CM-async spec F6 |
| W5 | SDK 增加 async 绑定生成路径（`wit-bindgen` `async: true`），并保留默认同步路径 | CM-async spec F7 / A4 |
| W6 | CHANGELOG 双语条目 + 旧产物重建提示（点明按哪个版本重建） | AGENTS §5.1.4 / §8 |

> **重要**：async 标注是**函数级**的，ABI 仍需 bump；**不要**为了「不 bump」而把 async 化藏在旧接口里（如新增一个 `*-async` 平行函数）——那会让接口面翻倍且语义重复，评审时按 §4.1 判据逐个论证。

---

## 6. 分阶段与门禁

| 阶段 | 内容 | 门禁 |
| --- | --- | --- |
| **P0 评估** | A1–A4 前置复测（见 §11）+ 架构改造量估计 + 本 spec 评审 | ✅ **已过（2026-09-26）**：票 01（A1 重入不可复现 + A2' 官方复核确认）、票 02（取消/trap 语义 + I3 定稿）、票 03（改造面审计 + 属主设计 + 改造量 ≈1800-2200 行）、票 04（A4 No-Go + 旧根因证伪）、票 05（peer 22 处分类：真等外部仅 3 纯 + 1 混合）全部有结论并写回本 spec；ADR 由票 10 落地 |
| **P1 属主化** | 改宿主调用模型（`run_guest_call` → 事件循环属主），**不改任何 WIT** | I1–I6 逐条有测试/结构锁；桌面端 `cargo test` 全量绿；**存量插件行为等价**（plugin 集成测试全绿） |
| ~~**P2 SDK 能力**~~ **退役（2026-09-27）** | 插件 SDK 的 async 绑定生成 + 一个最小 async 原语夹具（仅测试用） | ~~夹具插件能 await 宿主信号且同期处理其它命令~~ → 前提随 A4 消失（guest 侧 async 不可用），且改判路径（宿主实现侧 async）不需要 SDK 改动 ⇒ 票 07 退役 |
| ~~**P3 首个按需 async 化**~~ **退役（2026-09-27）** | `host-http.fetch`（非流式）改 async WIT + concurrent 注册 | ~~「慢 HTTP 不再堵输入」集成测试~~ → 实测（§12）证明该门禁不可达 ⇒ 票 08 退役，零生产代码改动 |
| **P4 评估其余候选** | §4.3 的 ②③④⑤ 逐个走 §4.1 判据 | ✅ **已完成（2026-09-27，票 09）**：走查完毕、**全部不立项**（C3✗ 决定性，②④ 另有 C4✗）——无通过项 ⇒ 无实施、零生产代码改动；逐项表见票 09「走查结论」，结论落 ADR 0029 决定 8。重评条件 = ADR 0029 决定 6 三条任一 + §12 边界锁转红 |

灰度：`call_model` 开关（`mutex`（现状） / `event-loop`（新）），默认先 `mutex`；P1 验收通过后切默认并保留一版回退窗口。

---

## 7. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| **A1 未判定**：async-lifted 导出在某些构造下 task 体重入一次 | P0 阶段用上游 `crates/test-programs/src/bin/async_*` 复测；未澄清前不写任何 async 原语 |
| **A2 取消缺口**：`call_concurrent` 的 task 只能靠**丢 store** 取消（#11833 未实现） | ✅ 票 02 已结：实测「丢 store」干净（挂起中的宿主 future 被 drop、无 panic），但 guest 等待之后的代码不执行；三条路径里停用/退出必须**先丢 store**，trap→重载沿用换 map 条目。写进 I3 |
| **A2' 挂起任务阻塞同实例新调用**（票 02 新发现；**2026-09-27 在「宿主实现侧 async」形态上再确认，见 §12**） | 一个 task 真停在 Pending 的宿主 future 上时，所在 component 实例保持 `do_not_enter`，同实例后续调用被无限期推迟（放行后才补上）——**直接威胁 G1/I2**。票 01 复核（官方产物同样如此，推迟非死锁）后 I2 已改为「不阻塞属主循环」；§12 进一步证明它**也堵住了「按需 async 化」的全部用户可见收益** |
| 每个调用一个 task 的开销 | 仅 §4.1 判据命中的原语 async 化；其余保持同步（task 立即完成，开销可测） |
| trap 语义在属主模型下变形（一个 task trap 是否等于整实例不可用） | ✅ 票 02 实测：**等于**——task trap 同样污染整 store（之后所有调用 `cannot enter component instance`），经典模型的「整体重载」恢复路径原样成立。I3 按此定稿 + 结构锁；任何「静默降级」视为回归 |
| fuel / 指标口径变化（现在按调用计） | P1 同时改 `track_call` 口径并加结构锁（当前调用数指标是观测面，改动需同步文档） |
| 移动端被牵连 | ADR 0018：移动端独立契约，不跟演；WIT 副本同步即可（W3） |
| wasmtime 版本漂移 | 不升版本；本改造只用 48 已有能力（CM-async spec F5） |

---

## 8. 验收标准

- A1 I1–I6 每条有对应测试或结构锁（不接受「实现里自然满足」）
- A2 存量插件（无 async import）行为等价：插件侧 `cargo test` + 宿主 `wasm_flow` 集成 + 桌面端 `cargo test` 全量绿
- ~~A3 P3 的「慢 HTTP 不阻塞交互」集成测试：在一个正在等网络的 fetch 期间，终端输入/resize 仍能完成（可测的时延上界）~~
  ——**该验收在 wasmtime 48 上不可达（§12 实测）**，随 P3 退役；替代口径 = §12 的边界锁（同实例串行确实存在，且与 import 是否 async 无关）
- A4 灰度开关双向可切；回退后行为与今天一致
- A5 CHANGELOG 双语 + ADR（含 async 化判据的记录，避免下一个人「顺便全改」）
- A6 §11 的 A1–A4 全部有结论（无论通过/不通过）

---

## 9. 文档与知识库同步

- `docs/adr/`：新增《插件并发模型：事件循环属主 + 按需 async 化》ADR（含 §4 判据与白名单 +
  **§12 的实例级门结论**：为什么「按需 async 化」在本运行时上不产生收益、重新评估的条件）
- ~~`docs/knowledge/plugin-development-checklist.md`：新增一节「需要等待的原语怎么写」~~
  → **改口径（2026-09-27）**：不写「怎么写 async import」（WIT async 不可用、宿主实现侧 async 收益不成立）；
  改为**边界说明**：本运行时下同实例串行不可绕开（§12），需要「不等待」的能力时走**非等待形态**
  （立即返回句柄 + 事件回调，参考 `host-process.run` 与流式 `fetch`）
- `bedcode-desktop/docs/code-map.md`：`host_api` 调用模型变化
- `AGENTS.md` §5.2 四闸门处补一句「调用模型：事件循环属主（async 化按需，不全量）」

---

## 10. 与 output-ack-backpressure spec 的关系

`output-ack` 的 **P2（唤醒推送）搁置**，等本改造完成：

- P1.5 已交付部分（双水位驻留 + ack + 节奏对齐 + 水位诊断面）**保留不变**，它不依赖并发模型
- ~~P2 若在新模型下做，形态会变简单：guest 可以自己 `await`「环有新字节」，宿主不必主动 publish~~
  → **该形态已随 A4/§12 出局（2026-09-27）**：guest 侧 async 在当前工具链不可用，且宿主实现侧 async
  不产生「同实例继续干活」的收益；output-ack P2 只能按「宿主主动 publish / 限频唤醒」的原方案做（不改 WIT/ABI）
- 在本改造完成前，输出链继续用 F4/F5 的启发式（交互延迟 ≈0，代价是空闲 4 次/秒 invoke）
- 若本改造被无限期搁置，P2 仍可按原方案（不改 WIT/ABI）落地——**两条路不互斥**，只是优先级下降

---

## 11. 前置待办（P0 必须先做）

| # | 事项 | 出处 | 阻塞谁 |
| --- | --- | --- | --- |
| 1 | ~~**A1 复测**~~ ✅ **已结**（2026-09-26，票 01）：重入不可复现（探针判别矩阵 D1-D8 全不重入 + 官方产物静态对照），原始记录疑为「函数体固有 2 处调用」误读；无 P2/P3 影响。另 A2' 复核：挂起任务阻塞同实例新调用是 wasmtime 事件循环性质（官方产物同样如此，推迟非死锁），I2/G1 表述需修正 | CM-async spec §5.3 | P2/P3 |
| 2 | ~~**A2 取消语义**~~ ✅ **已结**（2026-09-26，票 02）：三条恢复路径语义 + 丢 store 接线点 + I3 定稿，见 `issues/02-p0-a2-cancellation-semantics.md` | 同上 | P1（另产出一条 A2' 风险，回流票 01 复核） |
| 3 | ~~**A3 架构量**~~ ✅ **已结**（2026-09-26，票 03）：全调用点枚举完成（10 处封装调用 + 1 处直锁 `call_plugin_capability_export` + 3 处能力转发嵌套 + 4 处元数据/装配点）；属主任务设计（闭集 `GuestOp`、oneshot 结算、`biased` 调度保 I4、停机顺序、门面双分支）与 `call_model` 开关接线定稿；改造量 ≈1800-2200 行 / 11 生产文件 / **零 WIT 改动**。两条新增硬事实：① 今天每次调用已是 CM-async task（`call_async` → `call_async_concurrent`，`concurrency_support` 默认 true），属主化 = 短命作用域换常驻作用域；② 能力转发/互调是「guest 栈内嵌套调另一实例」，P1 只能加超时兜底（根治属 P4 候选⑤），I2 因此表述为「不阻塞属主循环」而非「不阻塞任何等待」 | 同上 | P1（输入齐全，可开工） |
| 4 | ~~**A4 工具链**：`wit-bindgen` `async: true` 在本仓库 nightly 下能否稳定出组件~~ ✅ **已结**（2026-09-26，票 04）：**No-Go** —— guest 组件构建本身没问题（wit-bindgen 0.60 + wasm32-wasip3 出件成功、含 `context-get-0`/`task-return`），但**任何 async-lifted 导出一进入即 abort**（`async_support.rs:560 context_get().is_null()`），连「不含 await 的 async 导出」也一样；`concurrent`/`call_async` 两种调用形态同样失败。**旧根因假设（wasmtime `debug_assertions` 哨兵）已证伪**（profile 覆盖单独重编 wasmtime 后仍 abort）；`ctx-read` 诊断显示槽值是非零**指针状脏值**（同步线程 `0x100000` / 入口线程 `0xffcf0`），非哨兵。⇒ **W1/W2（WIT `async func` + ABI bump）不得启动**；按需异步化改走**宿主实现侧 async（`func_wrap_async`，WIT 保持同步）**，该路径 2026-09-25 已实证可用且零 ABI 变更。未收敛差异（官方 `async_round_trip_*` 为何在 CI 绿）已列为后续任务，见票 04 §3/§6 | 同上 | P2/P3（路径改判已落地） |
| 5 | ~~`host_api/*.rs` 中 23 处 `block_on_async`（peer）逐个分类~~ ✅ **已结**（2026-09-26，票 05）：**口径校正——真实调用点 22 处**（第 23 次命中是 `use` 行）。分类结果：**「真等外部」仅 3 纯 + 1 混合** —— `dial`（含重拨兜底）/ `list-shared-roots` / `browse-directory`（均为「拨号 + 对端应答」往返），`resume-transfer` 红色分支混合（主路径本地 ⇒ 暂不立项，避免「半 async 半 sync」）；其余 18 处是本地表/通道/DB/FS（其中目录递归 2 处 + `create_dir_all` 1 处是本地阻塞，C1✗ 但影响 I2 表述，记 ADR） | 本 spec §4 | P4（候选 ③ 输入已备） |

---

## 12. 补充实测：实例级门堵住「按需 async 化」的收益（2026-09-27）

**动机**：A4 把「按需异步化」改判为宿主实现侧 async（§5 前置门禁）后，其收益承诺（解除慢 HTTP
堵死同插件交互）尚未验证。落地前先测。

**用例**：`bedcode-desktop/src-tauri/src/wasm_core/manager/runtime/tests/p3_async_host_import.rs`
的 `test_p3_async_import_pending_second_explicit_call_same_instance`
（`cd bedcode-desktop/src-tauri && ~/.cargo/bin/cargo test --lib test_p3_async_import_pending_second -- --nocapture`）；
夹具 `packages/plugin-p3-async-host-import-test`（测试专用 world，生产 WIT/ABI 零变更）。

**形态**（刻意与生产属主循环同形）：常驻 `run_concurrent` 作用域内，① 用
`start_call_concurrent` 启动 #1（其 import 由 `func_wrap_async` 注册、真 `.await` 等宿主信号）；
② 在 #1 挂起期间用 `start_call_concurrent` 启动同实例 #2；③ 外部任务在 1200 ms 后放行 #1。
**不设任何人工锁**（既有 P0 用例用 `Arc<TokioMutex<..>>` 串行，测不到 wasmtime 自身的门）。

**实测输出**：

```text
[probe] host invoke 进入: mode=wait   at 0ms    (active=0)
[probe] host invoke 进入: mode=normal at 1201ms (active=0)
[probe] B 第二条结算 1201ms：Ok("normal-complete:solo")
```

**结论**：

1. **同实例第二条显式调用零进展**——连它的宿主 import 都要等 #1 放行那一刻才被进入（1201 ms）。
   即 A2' 的实例级门在「宿主实现侧 async」形态上同样成立：**同实例串行与 import 是否 async 无关**。
2. ⇒ 「按需 async 化」在 wasmtime 48 上的用户可见收益 ≈ 0：mutex 模型下切换前后无差别（仍持实例锁），
   event-loop 模型下只省掉一次 `block_in_place`（不占 tokio worker），但命令仍要等。
   这条直接决定 **票 07 / 票 08 退役**（§6）。
3. **属主停摆（同轮追加实测；修正首轮的误读）**：`test_p3_async_import_suspension_stalls_owner_closure`
   测量——闭包先启动 #1（挂起），随后 `sleep(20ms)`，释放由**闭包外**任务在 600 ms 执行：实测该
   sleep 在 **600 ms（= 放行时刻）**才完成 ⇒ **fiber 挂起期间属主闭包完全不被调度**，属主循环停摆
   到该次调用结束。（首轮把「释放动作写在闭包内」，于是自锁成 15 s 无进展，被误读为「外部 await
   不被唤醒」——现已被本用例纠正；写法上的教训：释放/唤醒驱动必须在闭包外。）
   **生产含义**：`host-*` import 一旦 async 化，**event-loop 模型的属主循环也一并停摆**（与 mutex
   模型持锁等待等价）⇒ P3 在两种模型下都不产生「继续服务其它请求」的机会，这是 P3 退役的
   **第二重证据**。当前生产 import 全同步 ⇒ 不触发。
4. **边界锁**：上述断言已固化（同实例第二条调用耗时 ≥ 放行时刻 − 200 ms）。**该锁转红 = 运行时
   支持实例级并发进入**，届时 §4.3 候选与票 07/08 需重新评估。

**与 bench G2 的关系**：`.scratch/2026-09-26-wasm-bridge-bench/` 的 G2（慢调用在途 → 同实例 nop
延迟 167×）测的是 mutex 模型的实例锁；本节的实例级门说明**切到 event-loop 也不能让它降到 ~1×**
（同实例仍串行）——G2 的验收口径需按本节点修正（改测「不同实例互不阻塞」或先等实例级并发可用）。

**边界与待办**：属主停摆（第 3 条）已实测并固化为边界锁；是否为此给 wasmtime 上游开 issue
**已定夺（2026-09-27，ADR 0029 决定 8）：暂不开**——以边界锁 + ADR 重评条件记账，等复现证据更
充分再考虑。在 §5 第二道门禁生效的前提下，event-loop 模型对「全同步 import 插件」的可用性
不受影响（当前生产即如此）。
