# Spec：终端输出 ack 背压 ——「迁移前机制」在当前架构内的适配

- **日期**：2026-09-26
- **状态**：**P1 已实施完成**（2026-09-26；按 §12.3 定案落地，验收证据见 §13）；
  **P1.5 已实施完成**（F5 节奏对齐 + P3a 水位诊断面，证据见 §14）；
  **P2 已实施完成**（2026-09-27，限频**唤醒**形态：宿主主动 publish 提示，数据面仍拉取，见 §9；
  阻塞条件「等插件并发模型升级」已由 ADR 0029 关闭）
- **决策来源**：用户指令「使用迁移前的机制适配当前的架构」+「不要改动宿主 / 不在宿主加业务代码」+「写成 spec 文档再开工」+「评审这几点请参考迁移前的实现」
- **范围**：`bedcode-desktop/wasm-apps/terminal-session`（插件 Rust + 插件前端）。**宿主零改动**。

---

## 1. 背景与事实基线

| 维度 | 迁移前（宿主时代） | 现在（terminal-session wasm app） |
| --- | --- | --- |
| 输出环 | 宿主内核 `UnifiedOutputQueue`（**会话级**；50 MB / 65536 块双限；TB v3 字节连续语义） | 宿主 `PtyRing`（**PTY 句柄级**；容量 = 插件 `ringBytes`，**插件当前未声明 → 宿主默认 256 KiB** / 上限 4 MiB） |
| 传输方向 | 宿主 **push**：桌面 Tauri Channel（`useTerminalOutputStreamChannel`）/ 移动 WS / HTTP 历史 | 桌面 **pull**（前端 100 ms↔500 ms 轮询）；移动插件 WS drain push；HTTP `session-history` 续拉 |
| 背压 | **per-subscriber `SubscriberHandle`**：私有 `acked_offset` + **双水位迟滞**（high 128 KiB / low 64 KiB）+ park 等待 + park 兜底轮询 200 ms + zombie 30 s 回收 | **无**：宿主环零等待（`PtyRing::push` 不感知消费者）；慢消费只有 `truncated` 兜底 |
| 客户端 ack | **桌面也有**：`useTerminalOutputStreamChannel` 按 `ACK_BYTES_THRESHOLD = 64 KiB` 累计 / `ACK_MAX_IDLE_MS = 250` 空闲兜底，经命令 `terminal_channel_ack` 回发；移动同语义（WS `{"type":"ack"}`） | **无** |
| 唤醒源 | 宿主主动推送 | **宿主不唤醒插件**（SDK 明文决策）；插件 `host-timer` 仅**秒级** |

证据：`git show 35b7fe663^:bedcode-desktop/src-tauri/src/session/session_output.rs`（`SubscriberHandle::window()` / `on_ack` / `park_count`）、
`.../system/config.rs`（默认值，见 §12）、`.../server/websocket/terminal_ws/subscriber.rs`（park 执行体）、
`git show f9849fd0b^:bedcode-desktop/src/composables/useTerminalOutputStreamChannel.ts`（桌面 ack 时点）、
现状：`src-tauri/src/pty/pty_ring.rs`、`wasm-apps/terminal-session/rust/src/output.rs`。

**结论**：不动宿主时「真 push」（毫秒唤醒）不可得（宿主唯一事件是 `pty:exit`；插件 timer 秒级）。
本次恢复**背压与消费驱动推进**（旧机制的另一半，形态按 §12 定案），交互体感用「输入后即时拉取」补偿。

---

## 2. 目标 / 非目标

**目标**：G1 恢复未确认窗口背压（**双水位迟滞**，对齐旧实现）；G2 水位可观测（P1：进出驻留日志 → **P1.5：诊断读面 + 驻留统计**）；G3 输入后即时拉取（体感补偿）；G4 语义不退化（`truncated`/resync、游标单调、慢消费自担）；**G5 节奏对齐**（P1.5：轮询 50/250 ms、单 tick 预算 64 KiB = 旧引擎）。

**非目标**：N1 宿主侧订阅者窗口（ADR 0022 红线）；N2 宿主 `pty:output` 事件（P2，待放行）；N3 毫秒 `host-timer`；N4 移动端行为变更。

---

## 3. 术语与不变量

- `pushed`：插件已下推的最大偏移（= 上次响应 `nextOffset`）。
- `acked`：前端**已交付给渲染管线**（`onData` 入队后推进，非"渲染完成"——对齐旧实现）的最大偏移。
- `unacked = pushed − acked`。

**不变量**：I1 水位单调；I2 ack 由前端交付推进并按阈值节流回发；I3 宿主零改动；I4 `fromOffset < acked` 必放行（防 resync 死锁）；I5 环语义不变（不阻塞 PTY 产出）。

---

## 4. 契约（插件命令面）

### 4.1 `session.output.pull`（扩展；向后兼容）

请求：`{ sessionId, fromOffset, maxBytes? }`

| 态 | 形状 |
| --- | --- |
| 追平 | `null` |
| 有数据 | `{ data, nextOffset, truncated, throttled: false, unacked }` |
| **抑制（新）** | `{ data: [], nextOffset: <原样回传 fromOffset>, truncated: false, throttled: true, unacked }` |

### 4.2 `session.output.ack`（新增）

请求 `{ sessionId, offset }` → `{ ok: true, offset }`；幂等 + 单调（I1）；会话销毁回收。

**前端回发节流（对齐迁移前）**：累计待 ack ≥ `ACK_BYTES_THRESHOLD = 64 KiB` **或** 距上次回发 ≥ `ACK_MAX_IDLE_MS = 250 ms` 且水位有推进 → 回发一次。

---

## 5. 裁决规则（双水位迟滞，对齐旧实现）

```text
decide_pull_gate(unacked, from_offset, acked, was_parked) ->
    if from_offset < acked            -> Allow    # I4（重锚不可抑制）
    if was_parked:  unacked <  LOW    -> Allow    # 迟滞下沿（退出驻留）
    else:           unacked >= HIGH   -> Throttle # 迟滞上沿（进入驻留）
    else                              -> Allow
```

默认值（**沿用旧实现**）：`HIGH = 128 KiB`、`LOW = 64 KiB`。
附加约束（旧配置校验同一口径）：`ACK 阈值(64 KiB) ≤ LOW < HIGH 且 HIGH − ACK ≤ LOW` ✓
**环容量约束（新增，因当前环更浅）**：`HIGH ≤ 环容量/2`。当前环默认 256 KiB → HIGH 128 KiB 正好取上界 ✓（若插件将来声明更小的 `ringBytes`，需同步下调水位）。

---

## 6. 前端协作契约

| ID | 契约 | 落点 |
| --- | --- | --- |
| **F1**（修订） | **交付即账 + 节流回发**：`onData` 入队后推进 `ackedThroughOffset = nextOffset`；按 64 KiB / 250 ms 节流发 `session.output.ack`（不再是"flush 完成后上报"——对齐迁移前 `useTerminalOutputStreamChannel`） | `TerminalPreview` |
| **F2** | 抑制退避：`throttled` → 不推进游标、不写入，退避 **200 ms**（对齐旧 `subscriber_park_poll_ms`）再拉 | `TerminalPreview.pullTick` |
| **F3** | resync 重锚：`truncated` → 清屏 + `ackedThroughOffset = minOffset`（对齐旧实现 resync 分支） | `TerminalPreview` |
| **F4** | 输入即时拉取：`terminal.onData` 写入后立即跑一轮 pull（对旧 push 语义的等价补偿） | `TerminalPreview.initTerminal` |
| **F5** | **（P1.5 已对齐）** 兜底轮询保持唯一唤醒源；快档/慢档 `100/500 ms` → **`50/250 ms`**（`ENGINE_POLL_FAST/IDLE`），单 tick 预算 8 批/128 KiB → **4 批/64 KiB**（`FETCH_BUDGET`）——只降延迟（100→50 ms）不动吞吐（预算/间隔等值），驻留态强制快档 | `terminalPullPolicy.ts`（纯函数）+ `TerminalPreview` |
| **F6**（新增） | **zombie 计时**：抑制持续时间累计 > `30 s` → 打一次 warn（"背压超时"），并按旧语义**继续拉取**（新架构无连接可回收，不强制断开） | `TerminalPreview` |

---

## 7. 失败模式与边界

| 场景 | 行为 | 兜底 |
| --- | --- | --- |
| 前端卡死/长期不 ack | 驻留（`throttled`）→ 数据滞留宿主环 → 环满淘汰 → `truncated` | 恢复后 resync；F6 zombie 日志 |
| ack 未发出 | 驻留下沿不满足 → 持续抑制 | 兜底轮询 + 幂等重发（水位单调） |
| 游标异常回退 | 放行（I4） | — |
| 抑制 + truncated 同时可能 | 先裁决（I4 放行）→ fetch 返回 truncated | resync |
| 会话销毁 | 水位/驻留态回收（`pump::forget` 接线到 close/remove） | — |
| 环容量 < HIGH（未来声明变更） | 驻留无意义（数据先被淘汰） | §5 约束 `HIGH ≤ 环/2`；接入时校验 |
| 首次拉取 | pushed=acked=0 → 放行 | — |

---

## 8. 验收标准

- A1 Rust 纯函数契约（≥8 条）：上沿进入驻留 / 下沿退出 / **区间内迟滞保持** / I4 放行 / 边界值 / 零水位。
- A2 水位单调 + `forget` 回收。
- A3 前端 5 条组件级强断言：F1（交付推 ack + 64 KiB/250 ms 节流）、F2（退避 200 ms、游标不动）、
  F3（resync 重锚同步 ack 水位）、F4（输入即时 pull）、F6（zombie 日志一次）。
- A4 变异自检 ≥4（上/下沿反转、去 I4 放行、ack 改回渲染完成、退避缺失）。
- A5 **宿主零改动判据**：本次改动集仅插件两文件 + 前端两文件（工作区既有并行线改动除外）。
- A6 回归：插件前端全量 + 插件 Rust native 全量。
- A7 真机复验（用户）：正常输出无感；慢消费场景出现驻留/退出日志；无 UI 回归。

---

## 9. 分阶段

- **P1（本 spec）**：插件 Rust（双水位驻留 + ack）+ 前端 F1–F6 + 测试/变异 + 产物重建（宿主零改动）。
- **P1.5（已落地）**：F5 节奏对齐（旧引擎 50/250 ms + 64 KiB tick 预算，抽出可单测的
  `terminalPullPolicy` 纯函数）+ **P3a 水位诊断面**（`session.output.watermarks` 读命令
  + 驻留/抑制/环淘汰计数 + 驻留退出时的一次快照日志）。
- **P2（已实施完成，2026-09-27）**：`host-pty` 新增 `pty:output` 引擎事件（限频）——**唤醒形态，非真 push**。
  **形态定案**：按 ADR 0029 §7（原方案，**不改 WIT/ABI**），宿主在输出环出现新字节时向属主私有 topic
  `<owner>::pty:output` 限频发布 `{ ptyId }`（同句柄 ≥50 ms），插件转成前端事件 `session:output-available`，
  命中即立刻拉一轮。**数据面仍是游标拉取**，`truncated` resync 与背压语义一字未变；提示**可丢**
  （无订阅 / 队列满 / 未激活 / 被限频合并，从不重放）⇒ 正确性兜底仍是前端 50/250 ms 节奏 + resync，
  事件只买延迟（空闲后首个字节：慢档 250 ms → ≈0）。**真 push（宿主推字节）不在本票内**：
  它需要宿主持 per-subscriber 窗口（ADR 0022 红线 N1），见下「未落地」栏。
  实现位置：宿主 `host_api/pty_output.rs`（写侧装饰器）+ SDK `PTY_OUTPUT` + 插件 `output.rs` /
  `TerminalPreview.vue`；落锁见 `host_api/tests/pty.rs`（事件名漂移锁 + 真 PTY 突发限频用例 + 裁决纯函数单测）。
  **2026-09-26 补充**：CM-async（wasmtime 48 的 `func_wrap_concurrent`）已实测可行（见
  `.scratch/2026-09-26-wasmtime-cm-async-eval/spec.md` §5，P1 绿：挂起的 guest task 不再独占
  store）——它也能解本票（guest 自己 await 等输出，宿主那次主动回调可省），但代价是
  **要改 WIT（把 import 声明为 `async func`）→ ABI bump** + 手写 linker 注册 + guest async 化 +
  宿主调用架构重写。故**本 spec 仍以 P2（不改 WIT/ABI）为当前推荐路线**，
  CM-async 作为中期独立立项。
- **P3b（挂起，无消费者）**：per-consumer 水位（多窗口）。当前每个会话**只有一个拉取消费者**（桌面
  单窗口 / 移动 WS drain 各自独立），引入 consumer 身份没有第二个消费者，**按最小改动与「不猜」原则不做**；
  真正需要它的场景（同一会话多窗口并行看）出现时再立项。

---

## 10. 风险与回退

| 风险 | 缓解 |
| --- | --- |
| R1 驻留震荡 | 双水位迟滞（128/64 KiB，间隔 64 KiB = 一个 ack 阈值）对齐旧实现经验值 |
| R2 环更浅（256 KiB vs 旧 50 MB）→ 驻留期间更易截断 | 约束 `HIGH ≤ 环/2`；必要时插件声明更大 `ringBytes`（声明面，宿主不夹取） |
| R3 ack 节流导致驻留判定滞后 | 250 ms 空闲兜底对齐旧实现；正常路径不触发驻留 |
| 回退 | 删 ack 调用 + 恢复无窗口 pull（老前端兼容：忽略 `throttled`/`unacked` 字段） |

---

## 11. 实施现状

**P1 已落地（2026-09-26，宿主零改动）**：

- `rust/src/output.rs`：双水位 `HIGH_WATER_BYTES=128 KiB` / `LOW_WATER_BYTES=64 KiB` +
  编译期不变量自检（`const _: () = { assert!… }`：`LOW < HIGH`、`HIGH − 64 KiB ≤ LOW`）；
  `decide_pull_gate` 增 `was_parked` 迟滞入参并显式收 `high`/`low`（保持纯函数，单测可注入任意窗口）；
  `pump` 增 `parked` 驻留态与 `set_parked`，模块改 `#[cfg(any(target_arch = "wasm32", test))]`
  使水位表可 native 单测；`pull_via_host` 增驻留进/出 `log_debug`；响应字段统一为 `unacked`
  （删掉上一轮的 `ackedOffset`）；新增 `forget_via_host`。
- `rust/src/session/mod.rs`：`note_removed`（重启路径经此）与 `close_via_pty` 接线
  `forget_via_host`——**§12.3 表格未列但 §7/§11 要求**，见 §13 偏差 ①。
- `TerminalPreview.vue`：F1 交付即账 + 64 KiB/250 ms 节流回发；F2 `throttled` 不推进游标/
  不写入 + 200 ms 退避；F3 resync 同步 ack 水位（**游标仍前进到 `next`**，置 `minOffset`
  会重复渲染——实施中被既有测试当场抓住）；F4 输入即时拉取 `kickOutputPull`；F6 30 s zombie
  单次告警并继续拉取。
- 单测：Rust 12 例（C1–C9 gate 契约 / W1–W3 水位单调·驻留态·forget 回收）；前端 7 例。
- `useTerminalWritePipeline.ts` **零改动**（如 §12.3 所料）。

**未落地（后续）**：P2 真 push（需改宿主 + WIT/ABI 流程）；P3b per-consumer 水位（无第二个消费者，见 §9）。

## 11.1 P1.5 实施现状（2026-09-26 续）

**F5 节奏对齐（旧引擎）**：

- 新增 `src/utils/terminal/terminalPullPolicy.ts`（纯函数 + 常量，对齐仓库既有
  `terminalResizePolicy` / `terminalRendererPolicy` 形态）：`OUTPUT_PULL_INTERVAL_MS = 50`、
  `OUTPUT_IDLE_INTERVAL_MS = 250`、`OUTPUT_IDLE_THRESHOLD = 5`、
  `OUTPUT_PULL_MAX_BATCHES = 4`（× 16 KiB = `FETCH_BUDGET` 64 KiB）、
  `nextIdleStreak()`、`decidePollIntervalMs()`。
- `TerminalPreview.vue` 改为消费该模块，并把原先「`idleStreak === 阈值` 时单向切慢档」
  的定时器提拉改成**幂等重排**（每次 tick 决策，间隔变化才重建 interval）。
- **顺带修正的行为不一致**：P1 代码在驻留态 `return` 前不再调整节奏，若驻留发生在慢档
  期间，抑制重试会滞留在慢档（与「驻留中保持快档」的注释相反）；现在 `parked` 是
  `decidePollIntervalMs` 的最高优先级分支。
- 取舍：快档 invoke 频率 10→20 次/秒（拉取模型固有成本；P2 真 push 才能免）。

**P3a 水位诊断面（G2 收尾）**：

- `output.rs`：`PumpCursor` 增四个无时钟计数（`park_count` / `unpark_count` /
  `throttled_pulls` / `truncated_count`）+ `PumpSnapshot` / `render_watermark_report()`
  （纯函数，native 可测）；新增 `session.output.watermarks`（**纯读、不建条目**）。
- `pull_via_host`：抑制分支计 `throttled_pulls`；`truncated` 响应计 `truncated_count`；
  `set_parked` 改为**仅在状态翻转时**累计进/出次数。
- `TerminalPreview.vue`：`clearParked()` 取一次快照打进 `info`（驻留时长 + 退出后
  `unacked` + 累计驻留/抑制/环淘汰次数）——**真机复验 A7 据此判定「背压是否真发生过」**，
  失败仅 `console.warn` 留痕，绝不影响拉取链路。
- **有意差异**：旧实现有 `parked_ms`（宿主进程内有 `Instant`），插件 **wasm32 无系统时钟**
  （`SystemTime::now()` 触发 unreachable trap，见 `task/queue.rs` 同款注释），故插件侧只存
  无时钟的计数/字节，**时长由前端计时**（它有 `Date.now`）写入日志。
- 同时修正 `lib.rs` 里 `session.output.ack` 的**过期注释**（写着「写入管线 flush 完成后上报
  真实消费水位」，与 §12.2 ③ 定案的「交付即账」不符）。

---

## 12. 评审定案（逐点对照迁移前实现）

> 依据：`git show 35b7fe663^` 的 `session_output.rs` / `config.rs` / `terminal_ws/subscriber.rs`，
> 与 `git show f9849fd0b^` 的 `useTerminalOutputStreamChannel.ts`。

### 12.1 迁移前机制的完整要素（原文证据）

| 要素 | 旧实现 | 证据 |
| --- | --- | --- |
| 窗口定义 | `window() = next_offset − acked_offset`（已发未确认字节） | `session_output.rs` `SubscriberHandle::window()` |
| **双水位迟滞** | 上沿 `subscriber_high_water_bytes = 128 KiB`（达即驻留）；下沿 `subscriber_low_water_bytes = 64 KiB`（驻留后降到其下才解除） | `config.rs` 默认值 + 字段注释「低位水（字节，滞回下沿）」 |
| 滞回约束 | `客户端 ack 阈值 ≤ 低位水 < 高位水` 且 `高位水 − ack 阈值 ≤ 低位水` | `TerminalConfig::subscriber_budget_violation` |
| 客户端 ack 阈值 | `CLIENT_ACK_BYTES_THRESHOLD = 64 KiB`（桌面 `useTerminalOutputStreamChannel` 与移动 `terminal_link` 必须一致） | `config.rs` 常量注释 |
| 桌面 ack 时点 | 帧**交付**（去重/裁剪/连续性校验后交给管线）即推进 `ackedThroughOffset`；按 `64 KiB 累计` 或 `250 ms 空闲` 节流回发 `terminal_channel_ack` | `useTerminalOutputStreamChannel.ts`（`ACK_BYTES_THRESHOLD` / `ACK_MAX_IDLE_MS` / `ackedThroughOffset`） |
| 驻留等待 | `engine_park_until_ack`（等 ack 唤醒；`park_poll_ms = 200` 兜底轮询防丢失唤醒） | `subscriber.rs`；`config.rs` 默认 `200` |
| 僵尸回收 | `subscriber_zombie_timeout_ms = 30_000`：窗口持续不降 → 回收该订阅者连接（`subscriber stalled`） | `subscriber.rs` + `config.rs` |
| 观测 | `park_count` / `parked_ms` / `truncated_count` + 结构化日志（`engine subscriber park entered/exited`，带 `lag_bytes`） | `SubscriberStats` |
| 引擎轮询双速 | `ENGINE_POLL_FAST_INTERVAL = 50 ms` / `IDLE = 250 ms` / `IDLE_THRESHOLD = 5` / `FETCH_BUDGET = 64 KiB` | `subscriber.rs` |
| resync | 重锚 `min_offset` 同时 `ackedThroughOffset = minOffset` | `useTerminalOutputStreamChannel.ts` resync 分支 |

### 12.2 逐点定案（对上一轮我提的 5 个问题）

| # | 我的原提案 | 旧实现依据 | **定案** |
| --- | --- | --- | --- |
| ① 抑制语义 | `throttled` + 游标不动（新造） | 旧是**阻塞 park**（等 ack 唤醒） | **保持新形态**（前端拉取命令不能长阻塞），但**补"驻留"的出口语义**：下沿迟滞解除 + 200 ms 兜底 + 30 s zombie（见 ③⑥） |
| ② 窗口默认值 | 512 KiB（拍脑袋） | **HIGH 128 KiB / LOW 64 KiB**，且 `HIGH ≤ LOW + ack(64 KiB)` | **改 128 KiB / 64 KiB**；另加"`HIGH ≤ 环容量/2`"约束（当前环 256 KiB ⇒ 128 KiB 恰好可行） |
| ③ ack 时点 | "flush 完成后上报真实消费"（我提的最贵方案） | 旧桌面是**交付即账**（帧交付给管线即推进），按 64 KiB / 250 ms 节流回发 | **改为交付即账 + 节流回发**——不再需要给写入管线加 `onFlushed` 钩子，工作量下降且与旧语义一致 |
| ④ 迟滞恢复 | 无（单阈值，易震荡） | **双水位迟滞**（旧有 low water 明确设计） | **补迟滞**：驻留后须 `unacked < LOW` 才恢复（`decide_pull_gate` 增 `was_parked`） |
| ⑤ zombie | 无 | 30 s 窗口不降 → 回收连接 | 新架构无连接可回收：**30 s 超时打 warn 并按旧语义继续拉取**（前端计时，F6） |
| ⑥ 退避间隔 | 250 ms | `park_poll_ms = 200` | **改 200 ms**（对齐旧值） |
| ⑦ 输入即时拉取 | 提案 | 旧为 push（输入后回显必达） | **保留**（push 语义的等价补偿） |
| ⑧ 真 push | P2 | 旧即宿主 push | **P2 登记**（不动宿主约束下不可做） |

### 12.3 定案导致的实现变更（待确认后执行）

| 文件 | 变更 |
| --- | --- |
| `rust/src/output.rs` | 常量 512 KiB → `HIGH_WATER_BYTES=128 KiB` / `LOW_WATER_BYTES=64 KiB`；`decide_pull_gate` 增 `was_parked` 入参（迟滞）；`pump` 水位表增 `parked` 态；单测 6 → ≥8（补"区间内迟滞保持"与下沿退出） |
| `rust/src/lib.rs` | 不变（`session.output.ack` 已注册） |
| `TerminalPreview.vue` | F1 交付即账 + 64 KiB/250 ms 节流回发；F2 退避 200 ms；F3 resync 同步 ack 水位；F4 输入即时 pull；F6 zombie 30 s warn |
| `useTerminalWritePipeline.ts` | **不再需要改动**（F1 走 onData 交付点，无需 `onFlushed`） |

### 12.4 与旧实现的**有意差异**（需用户知晓）

1. **等待形态**：旧=阻塞 park（任务可挂起）；新=`throttled` 空响应 + 前端退避（命令不阻塞）——因 WASM 命令由宿主同步调用，长阻塞会占调用线程。
2. **僵尸处理**：旧=回收连接；新=告警并继续（无连接可回收；且断开桌面渲染链会破坏 UI）。
3. **在途语义载体**：旧=`next_offset − acked_offset`（单调水位差）；新=同一差（前端交付推进 acked）——语义等价，只是 acked 的推进点从"WS 客户端收到"变为"前端交付管线"。
4. **环深度**：旧 50 MB vs 新 256 KiB（既有差异，非本次引入）⇒ 驻留期间更易触发 `truncated`，故加 `HIGH ≤ 环/2` 约束并在验收中观察。

---

## 13. P1 验收证据与偏差（2026-09-26）

### 13.1 验收对照

| 验收项 | 结果 | 证据 |
| --- | --- | --- |
| A1 Rust 纯函数契约（≥8 条） | ✅ 12 条 | C1 零水位 / C2 上沿−1 / C3 上沿进入驻留 / **C4 区间内迟滞保持** / C5 下沿退出 / C6 ack 前移恢复 / C7 I4 放行（驻留中亦然）/ C8 零窗口异常 / C9 常量不变量（含 `HIGH ≤ 环/2`） |
| A2 水位单调 + `forget` 回收 | ✅ 3 条 | W1 pushed/acked 单调 / W2 驻留态读写 / W3 `forget` 摘除后回零水位 |
| A3 前端 5 条强断言 | ✅ 7 例 | F1 首次回发 / F1 64 KiB 阈值 / F1 250 ms 空闲兜底 / F2 驻留退避 / F3 resync 游标+水位 / F4 输入即时拉取 / F6 zombie 告警一次 |
| A4 变异自检（≥4） | ✅ 7 项全被捕获 | 见 13.3 |
| A5 宿主零改动 | ✅ | 改动集仅 4 文件，全在 `wasm-apps/terminal-session/`（见 13.2 偏差 ①）；`src-tauri/src/**` 零改动 |
| A6 回归 | ✅ | 插件 Rust `cargo test` 360 passed；插件前端 `vitest run wasm-apps/terminal-session/src/__tests__` 20 files / 217 tests passed；`eslint .` 0 error（改动文件 0 warning）；wasm32-wasip3 release 构建通过并重投产物（wasmHash 已注入） |
| A7 真机复验 | ⏳ 待用户 | 正常输出无感 / 慢消费出现「进入驻留·退出驻留」日志 / 无 UI 回归 |

### 13.2 偏差（须知悉）

① **改动集比 §12.3 表格多一个文件**：新增 `rust/src/session/mod.rs`。原因是 §7/§11 要求的
`pump::forget` 接线到 close/remove 必须有落点，而 §12.3 表格只列了 `output.rs`（`lib.rs` 不变）。
实际接线：`note_removed`（`restart_via_host` 经此）+ `close_via_pty`。**这不是可选的**——重启会
换新 PTY 句柄、环偏移从 0 起，残留旧水位会让新环被误判为「已推送远超已确认」而永久驻留。

② **`deliveredOffset/ackedThroughOffset = minOffset` 的 resync 重锚赋值在当前流程下不可观测**：
紧随其后的交付分支必然把 `deliveredOffset` 覆盖为 `next`，两种写法产生的 ack 偏移完全相同。
首次变异自检（M7 删这三行）**未变红**——这是测试不够敏感，不是实现有错。处置：保留该赋值
（水位卫生，被淘汰前缀不应被计入未确认窗口），但把 M7 换成真正被守住的不变量
（resync 后游标必须前进到 `next`，`minOffset` 会导致同一段重复渲染），F3 用例相应加断言。

③ **ack 时点采用 spec §12.2 ③ 的「交付即账」**，未改成迁移前 `useTerminalOutputStreamChannel`
   的「`onWriteParsed` 后回发」渲染时点。由此产生的已知性质（**不构成缺陷，是拉取模型的固有
   结果**）：`pushed` 随前端拉取前进，未确认窗口常态 ≈ ack 节流滞后（≤ 64 KiB + 一 tick），
   128 KiB 上沿主要充当**安全上限**而非主流量控制。若要让它成为真正的流量控制，需走 §9 P2
   （宿主 `pty:output` 引擎事件 + 真 push），而 P2 需要改宿主。

### 13.3 变异自检记录（每项都「改坏 → 变红 → 还原 → 变绿」）

| # | 变异 | 捕获 |
| --- | --- | --- |
| M1 | 迟滞解除条件 `< low` 翻为 `> low`（上/下沿反转） | Rust C4 + C5 变红 |
| M2 | 删除 I4 `from_offset < acked` 放行分支 | Rust C7 + C8 变红 |
| M3 | 迟滞塌缩为单阈值（删 `was_parked` 分支） | Rust C4 变红 |
| M4 | `note_acked` 去掉单调（`.max()` → 直接赋值） | Rust W1 变红 |
| M5 | 删除 200 ms 抑制退避（`throttleBackoffUntil` 判断） | 前端 F2 变红 |
| M6 | ack 上报旧水位（`deliveredOffset` → `ackedThroughOffset`） | 前端 F1×3 + F3 变红 |
| M7 | resync 后游标置 `minOffset`（重复渲染） | 前端 F3 + 既有用例变红 |

---

## 14. P1.5 验收证据（2026-09-26 续）

### 14.1 验收对照

| 验收项 | 结果 | 证据 |
| --- | --- | --- |
| F5 节奏对齐 | ✅ | `terminalPullPolicy.ts`：50/250 ms、阈值 5、4 批 × 16 KiB = 64 KiB；组件接线测试 F5（260 ms 内 ≥3 轮新 pull） |
| 优先级契约（驻留 > 有余量 > 阈值） | ✅ | 策略单测 6 例（含「驻留中即使计数超阈值仍快档」这条冲突组合） |
| G2 诊断面 | ✅ | `session.output.watermarks`（纯读）+ 4 项观测计数 + 驻留退出 info 日志；组件测试 G2 |
| 观测计数正确性 | ✅ | Rust D1（仅翻转计数）· D2（`unacked` 自算 + 截断计数）· D3（过滤纯读、不泄漏其他会话）· D4（报告 JSON 全字段） |
| A5 宿主零改动 | ✅ | 改动集 5 文件全在 `wasm-apps/terminal-session/`（`output.rs` / `lib.rs` / `TerminalPreview.vue` / `terminalPullPolicy.ts`(新) / 2 个测试文件）；`src-tauri/src/**` 零改动 |
| A6 回归 | ✅ | 插件 Rust `cargo test` **364 passed**（360 → +4）；插件前端 `vitest run wasm-apps/terminal-session/src/__tests__` **21 files / 230 tests passed**（20/217 → +1/+13）；`eslint` 0 error；`node scripts/build.js`（wasm32-wasip3 release）通过并重投产物（wasmHash 已注入） |

### 14.2 P1.5 变异自检

| # | 变异 | 捕获 |
| --- | --- | --- |
| M8 | `decidePollIntervalMs` 删 `parked` 覆盖分支 | 策略单测「驻留态覆盖空闲退避」变红 |
| M9 | 阈值语义 `>=` 改 `>`（提前降档） | 策略单测「恰好达阈值」变红 |
| M13 | `nextIdleStreak` 清零逻辑写反（`hasMore ? idleStreak : …`） | 策略单测「有数据时清零」变红 |
| M12 | 删除驻留退出的水位快照副作用（`clearParked` 不取快照） | 组件测试 G2 变红（`expected +0 to be 1`） |
| M10 | `set_parked` 去掉「仅翻转计数」守卫 | Rust D1 变红 |
| M11 | `pump::snapshots` 去掉 `sessionId` 过滤 | Rust D3 变红 |

> **首轮 M11 未被捕获**（D3 当时只断言「查未知会话得空表」，恰好与其他并行用例的
> 清理时序重合而侥幸通过）。按「变异存活先怀疑测试不敏感」的纪律，D3 改为**双哨兵**
> （造 A/B 两行 → 指定过滤只回一行 + 不过滤两行都在 + 未知会话空且不建条目），
> 重建后 M11 被稳定捕获。

### 14.3 残余风险 / 未覆盖面

1. **`pull_via_host` 内的计数接线（`note_throttled` / `note_truncated`）只有编译期覆盖**：
   该函数 `#[cfg(target_arch = "wasm32")]`，native 单测不可达。已由 wasip3 release
   构建保证编译通过，**运行时行为靠 A7 真机复验观察**（驻留退出日志里的
   `抑制次数` / `环淘汰`）。
2. **F5 的 invoke 频率翻倍**（活跃输出期 20 次/秒）未做实机负载测量；P1.5 只保证
   吞吐上限不变（预算/间隔等值），未证明高频 invoke 对宿主 IPC 无感。
3. **背压仍以「交付即账」记账**（§13.2 ③）：128 KiB 上沿在拉取模型里主要是安全上限。
   真正的主流量控制需 P2（改宿主）。
