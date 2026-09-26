# 插件并发模型：事件循环属主 + 按需 async 化（含实例级门边界）

## 背景

每个 WASM 插件实例在此之前只有**一把锁**：`webview --plugin_invoke--> with_wasm_plugin_call`
→ `run_guest_call`（`spawn_blocking` 线程）→ `wasm_plugin.lock()` → `block_on_async(call_invoke)`
全程持锁直到 guest 返回。于是**一次慢的 guest 调用会把该插件的全部交互一起堵死**，已确认的现实
后果包括：非流式 `host-http.fetch` 在 import 栈内等网络往返（时长不受本仓库控制，`host_api/http.rs`）、
终端输出只能靠前端定时轮询、以及「guest 挂起等某事件、同时处理其它命令」在经典模型下不可表达。

专项 spec：`.scratch/2026-09-26-plugin-concurrency-model/spec.md`（含 §12 补充实测）。
目标（spec §2）：G1 消除「单次慢调用堵死整插件」的结构性风险；G2 让插件能表达「等某事件」；
G3 与既有行为等价；G4 可灰度可回退。

## 决定

1. **调用模型 = 每插件实例一个装配条目，两种实现可灰度切换**。装配表
   `wasm_plugins: Arc<RwLock<HashMap<String, Arc<WasmInstanceEntry>>>>` 是宿主侧**唯一**入口，
   `WasmInstanceEntry { meta, call_model, slot }`，`slot: InstanceSlot::{ Mutex(Arc<Mutex<LoadedWasmPlugin>>), Owner(OwnerHandle) }`。
   门面 `PluginHost::call_guest`（异步）/ `call_guest_blocking`（同步桥，供 bus / process / task 与能力转发），
   闭集 `GuestOp` → `GuestReply` / `GuestCallFailure`。
   - `mutex`：原实现逐字搬移（`dispatch_mutex_op` 逐条委派既有导出方法）⇒ **存量行为逐字节等价**；
   - `event-loop`：每实例一个常驻 `run_concurrent` 属主循环（唯一持 `&mut Store` 的地方），
     `start_call_concurrent` 启动 guest task、oneshot 结算、`select! { biased }` + 单点 start
     保证「启动顺序 = 入队顺序」。
   - **灰度与回退**：`CoreConfig.call_model`（`mutex`（默认，回退窗口）/ `event-loop`，`wasm-core.json`
     可按名覆盖，非法值加载即报错）；**实例级快照**（建实例时读一次），reload 即切换。
2. **按需异步化判据 C1–C4（只有四条同时满足才 async 化）**：
   C1 必要（必须等一个**不由本次调用自身驱动**的外部事件）；C2 显著（等待时长不可由本仓库控制）；
   C3 有价值（等待期调用方确有其它工作可做）；C4 不可替代（同步实现会阻塞属主/整实例，且无非等待替代路径）。
   **明确不同步化的白名单**（storage / database / plugin-database / crypto / config / fs / log / mdns /
   platform / app / pty ring-fetch / process.run / bus / events / timer）；**反模式**：全量 async 化、
   本地计算包 async、同一原语半 async 半 sync。
3. **WIT / ABI 边界依据（CM-async 探针 F13–F15）**：WIT `async func` 标注与宿主 `func_wrap_concurrent`
   注册**强制配对**（F13）；`bindgen!` **不生成** concurrent 注册（F6/F14，`async | store` 模式生成的仍是
   `func_wrap_async`）；async 组件工程细节（F15）。**当前锁定工具链下 WIT 层 `async func` 不可用**
   （A4，票 04：async-lifted 导出入口即 abort `context_get().is_null()`，与是否 await 无关），
   故按需异步化在落地时只能走**宿主实现侧 async**（`func_wrap_async` + 真 `.await`，WIT 签名保持同步、
   **零 ABI 变更**，实证见 `.scratch/2026-09-25-wasip3-host-api-optimization/issues/01`）。
4. **实例级门与属主停摆 —— 按需异步化在 wasmtime 48 上不立项（2026-09-27 实测，决定性）**：
   spec §12 的两条实测（`runtime/tests/p3_async_host_import.rs`，生产属主形态、无人工锁）：
   - **实例级门**：一个 task 停在宿主 async import 上时，**同实例其它显式调用零进展**——第二条调用的
     宿主实现要到第一条放行那一刻才被进入（放行 1200 ms，实测进入 1200 ms）。同实例串行由运行时
     实例级门保证，**与 import 是否 async 无关**（A2′ 在「宿主实现侧 async」形态上再确认）。
   - **属主停摆**：该挂起期间**属主闭包完全不被调度**——闭包内 `sleep(20 ms)` 实测到放行时刻（600 ms）
     才完成。⇒ event-loop 模型也不会因为 import 变 async 而多出「服务其它请求」的机会。
   ⇒ 「异步化 import」既不解除同实例串行、也不给属主留下空档，其收益仅剩「等待期不占一条宿主执行线程」
   （mutex 模型下切换前后无差别）⇒ **P2（SDK async 绑定）/ P3（`host-http.fetch` 异步化）退役，
   不实施，零生产代码改动**；两条实测断言固化为**边界锁**。
5. **P0 判定记录（结论出处）**：A1 手写 async 组件「重入」**不可复现**（票 01：判别矩阵 D1–D8 +
   官方产物对照，原始记录疑为函数体固有 2 处调用误读）；A2 取消语义（票 02：`call_concurrent` 的 task
   **只能靠丢 store 取消**（wasmtime #11833 未实现），挂起中的宿主 future 随 store 干净 drop、无 panic，
   但 guest 等待之后的代码不执行；**task trap 污染整 store**，之后所有调用 `cannot enter component
   instance` ⇒ trap = 整实例不可用，恢复 = 停属主 → 丢 store → 重建）；A3 架构量（票 03：≈1800–2200 行 /
   11 生产文件 / 零 WIT 改动；10 处封装调用 + 1 处直锁 + 3 处能力转发嵌套 + 4 处元数据点）；
   A4 = 上条第 3 点；peer `block_on_async` 分类（票 05：22 处真实调用点，「真等外部」仅 3 纯 + 1 混合）。
   落地不变式 I1–I6（spec §3.2）逐条有测试或结构锁：I1 同实例至多一个属主（装配条目是唯一 Store 入口）/
   I2 单次调用不阻塞属主循环 / I3 trap·panic·重载·停用语义 fail-visible（含停用先丢 store、跳过 guest
   `on_shutdown` 必须计数 + warn）/ I4 启动顺序 = 入队顺序 / I5 无 async import 的存量插件行为逐字节等价 /
   I6 属主生命周期确定（无孤儿任务）。
6. **重新评估条件（边界锁转红即重评）**：① 实例级门放开（同实例并发进入可用）；② 属主停摆消失
   （挂起期间属主仍被调度）；③ wit-bindgen / wasmtime 修好 async-lifted 导出（A4 的未收敛差异定位清楚）。
   任一满足时，spec §4.1 的 C1–C4 需在**新事实**下重走一遍判据。
7. **与 output-ack P2 的关系**：`output-ack-backpressure` 的 P2（宿主唤醒推送）不能按「guest 自己
   `await` 环有新字节」实现（guest 侧 async 出局、宿主实现侧 async 无收益），只能按原方案
   「宿主主动 publish / 限频唤醒，不改 WIT/ABI」。
8. **P4 候选走查结论（2026-09-27，票 09）：②③④⑤ 全部不立项**。四项逐个走 §4.1 的 C1–C4
   （逐项表与代码级证据见 `.scratch/2026-09-26-plugin-concurrency-model/issues/09-p4-candidate-primitives.md`
   「走查结论」）：
   - **② 「等 PTY 输出」新原语**：C4✗——已有等价非等待替代（宿主主动 publish / 限频唤醒，
     即 output-ack P2 原方案，不改 WIT/ABI；前端游标 pull + 自适应节流已把交互延迟补偿到 ≈0），
     且 guest 侧 `await` 前提不成立（A4）；
   - **③ `host-peer`**（票 05：22 处真实调用点）：`dial`（含重拨兜底）/ `list-shared-roots` /
     `browse-directory` 三个「真等外部」入口 C3✗（实例级门）；`resume-transfer` 的 redial 分支是
     混合形态（主路径本地 + 分支真等外部），要 async 化必须拆专用原语，暂不立项；其余维持同步；
   - **④ `host-websocket`**（11 处真实调用 = 1 处真等外部 + 10 处本地）：否决——唯一等待点
     `connect` 握手已有超时常量上界，且现有形态**已是**「立即返回句柄 + 事件/订阅」（状态事件走
     属主私有 topic、消息帧走可选导出 `events-ws` 回调，WIT 无「等下一帧」原语）⇒ C4✗；
   - **⑤ `host-auth` / `host-api-call`**：`host-auth` 生产 5 处全为本地 SQLite（C1✗）；`host-api-call`
     生产 4 处唯一真等待 = 等回复，C3✗/C4✗ —— **F13 链路两端配对结论：两端一起 async 化同样
     收益 ≈0（实例级门）且不解决 A→B→A 环依赖 ⇒ 维持同步**（5 s 超时兜底保留）；环依赖根治
     归「非等待形态」（Considered Options 末项，需用户单独立项）。
   **统一重评条件**：决定 6 的 ①/②/③ 任一满足（或 §12 边界锁转红）时，②③④⑤ 重走 C1–C4。
   **上游 issue 裁决**：暂不开——以边界锁 + 本 ADR 重评条件记账，等复现证据更充分再考虑。

## Considered Options

- **保持现状（一把锁）**：实现零风险，但结构性尖角保留（慢调用堵死整插件；插件无法表达「等事件」）。
  已否决——但 `mutex` 分支作为**回退窗口**保留在装配条目里。
- **WIT 层 `async func` 全量 / 按需标注**：语义最干净（guest 可 await、可取消），但当前工具链下不可用
  （A4），且触发 ABI bump / SDK 发布 / 双端副本 / 旧产物 fail-visible 全流程。**否决（本轮）**。
- **宿主实现侧 async（`func_wrap_async`）**：零契约成本，确实能归还宿主线程；但实测（第 4 条）证明
  用户可见收益 ≈ 0。**否决（不立项）**。
- **非等待形态**（立即返回句柄 + 完成事件，参考 `host-process.run` 与流式 `fetch`）：天然不等待、
  不触发实例级门，是「慢操作不堵实例」的**唯一可行方向**；代价是契约与调用方改造。
  **登记为候选，需用户裁决后单独立项**（不在本 ADR 实施）。

## Consequences

- 正向：慢调用不再堵死**其它插件**（各实例独立属主/锁）；`event-loop` 与 `mutex` 双模型可灰度、可回退；
  trap / 停用 / 重载语义在属主模型下重新定义且 fail-visible；能力转发端口收窄（`CapabilityTarget`）
  使 `host_api` 对 manager 类型依赖清零。
- 边界（必须随结论一起读）：**同实例串行不可绕开**（实例级门，与 async 无关）；
  **按需 async 化不立项**（收益不成立）；**能力转发 / 互调仍是「guest 栈内嵌套调另一实例」**——
  当前只加 5 s 超时兜底把环依赖从「永久死锁」降级为「有界失败」（根治属 spec §4.3 候选 ⑤，前置条件
  同第 6 条）；`event-loop` 实例停用即丢 store ⇒ guest `on_shutdown` / `deactivate` **不再执行**
  （显性跳过 + `owner_cleanup_skipped` 计数 + warn，是 spec I3③ 的既定取舍）。
- 观测口径：燃料仍是**实例级预算**（续费点差值记账），调用计时器随 guest task 存活（start → finish）。
- 默认值现状：`call_model` 默认仍 `mutex`（回退窗口）；切默认需真机复验（终端输入/resize/输出拉取、
  停用→启用、trap 自愈、传输与 AI 面）。

## 修订记录

- 2026-09-27：新增（P0 结论 + P1 落地 + §12 实测的合并记录）。同批退役票 07（SDK async 绑定）与
  票 08（`host-http.fetch` 异步化），依据见「决定」第 4 条。
- 2026-09-27（第二轮）：新增决定 8 —— P4 候选 ②③④⑤ 走查结论（全部不立项，逐项证据与重评条件
  见该条）；上游 issue 裁决 = 暂不开；票 09 收口（零生产代码改动）。
