# PTY 输出全链路：桌面端 → 移动端/桌面端本地终端显示

> **当前架构（2026-10-15 核对）**：终端订阅协议**整体归桌面插件**
> `com.bedcode.terminal-session`（`bedcode-desktop/wasm-apps/terminal-session/rust/src/ws_terminal.rs`，
> 经 `/ws/plugin/com.bedcode.terminal-session/terminal` 端点直连，auth=jwt）；
> 移动端协议客户端归**同 id 的移动 wasm 应用**（`bedcode-mobile/wasm-apps/bedcode/rust/src/link.rs`）；
> 两端宿主只剩零解析窄转发（桌面转原生 WS 帧 / 移动 `host-terminal-stream` 转页面 Channel）。
>
> **输出帧 = 裸字节**：无帧头、无 per-frame offset、无事件数编码。字节连续性由 WS/TCP 帧序 +
> 每连接游标保证；缺口只经 `ring_resync` 显式表达；「重放世代切换」的清屏由 `terminal-resync`
> 事件承担。**TB v3 的 16B 帧头 / `start_offset` / 宿主 `terminal_link.rs` / 每订阅者合帧窗口
> 属于 2026-09 的旧宿主实现，已随票 05/12/15 退役**（见文末「历史」）。

---

## 全链路概览

```text
桌面端（宿主机制）                             桌面端（插件 = 协议真源）
PTY 进程输出
  ↓ (os pipe)
host-pty 引擎（输入即写、输出入环）
  · 输出环 = 按会话内存环（SESSION_PTY_RING_BYTES = 4 MiB；拉取模型唯一缓冲）
  · 新字节 → 属主私有通知 <owner>::pty:output（≥50ms 限频，只带句柄不带数据）
        │
        ├── 插件自己的前端（session.output.pull，双水位滞回）
        └── 桌面插件 ws_terminal.rs（WS 订阅连接）
              每连接独立游标 + 有界 drain（单次 ≤16 KiB ×≤8/轮）
              → 二进制帧 = 环原始字节（零 JSON）
              → text 控制帧：subscribed / ring_resync / session_stopped / error
              ← 入站：subscribe / unsubscribe / ack / resync / input / poll（text）
                       原始输入字节（binary）

移动端（宿主 = 传输面）                        移动端（插件 = 协议状态机）
host-websocket 客户端（连接/心跳/退避重连/jwt-auth 首帧代发）
  → ws:message 二进制信封（kind 字节 + 原始帧）→ 插件 link.rs
        │                                             │
        │  ack(64 KiB 阈值) / input / subscribe 帧 ←──┤
        │                                             ↓ host-terminal-stream.forward-output
        └──────── 宿主窄转发表（页面 Channel）←────────┘
                        ↓ Raw 字节（零 JSON / 零 base64）
                 前端 store（terminalBuffer）→ writeCoalescer(rAF) → xterm
```

---

## 1. 桌面端（插件 ring-fetch 拉取模型）

### 1.1 输出环与通知（宿主机制）

- `packages/bedcode-pty-engine`：PTY 输出入**按会话内存环**（默认宿主上限
  `PLUGIN_PTY_RING_BYTES` = 256 KiB，终端会话按 `SESSION_PTY_RING_BYTES` = 4 MiB
  声明——见 `launch.rs`，环是拉取模型下唯一的缓冲）。
- 新字节只发**通知**（`<owner>::pty:output`，≥50 ms 限频，payload 只有句柄）：
  消费者按自己的游标 `pty_ring_fetch(handle, from, ≤16 KiB)` 拉取，
  `truncated=true` 表示请求游标已被淘汰（缺口）。

### 1.2 插件输出泵（`ws_terminal.rs`）

- 每连接 `TerminalConnection { session_id, pty_id, cursor, watermark, mode }`；
  `subscribe` 会把该连接游标归零并从环驻留窗口**整段重放**（fresh subscribe 语义）。
- **有界 drain**：单轮最多 8 次 fetch（≈128 KiB），触发源三条——
  ① 入站帧（input / ack / poll；ack 自时钟为主驱动）；② `pty:output` 通知（`drain_session`）；
  ③ 1 s 调度 tick 兜底（`drain_all_on_tick`）。
- 环淘汰：先发 `ring_resync{offset, gapFrom}` **再**照发本块驻留数据（发送失败则游标不前进、
  下轮重发——重锚信号不得静默丢失）；退出路径的尾帧同样按截断补 `ring_resync`。

### 1.3 控制帧协议（插件定义；宿主不解析）

| 方向 | 帧 | 语义 |
| --- | --- | --- |
| C→S | `{"type":"subscribe","sessionId","mode":"live"\|"poll"}` | 订阅 + **整段重放**（游标归零、水位清零） |
| C→S | `{"type":"unsubscribe"}` | 退订（保留连接） |
| C→S | `{"type":"ack","offset":N}` | N = 客户端**本地累计字节**（见 1.4） |
| C→S | `{"type":"resync","offset":N}` | 客户端已清屏，从 N 续拉 |
| C→S | `{"type":"input","data":"..."}` / binary 原始字节 | 直写 PTY（文本 / 控制字符） |
| C→S | `{"type":"poll"}` | 主动拉取一轮 |
| S→C | `{"type":"subscribed","sessionId","mode"}` | 订阅达成；此后输出帧才是本订阅的流 |
| S→C | `{"type":"ring_resync","offset","gapFrom"}` | 环已淘汰：客户端清屏 + 重锚（`gapFrom` 为缺口起点，诊断用） |
| S→C | `{"type":"session_stopped","sessionId","reason","exitCode"?}` | 尾帧之后下发（帧序保证） |
| S→C | `{"type":"error","message"}` | 会话不存在 / 帧非法 / 未订阅输入 |

### 1.4 背压（交付水位 + 客户端本地计数换算）

- 水位常量与插件自己前端拉取路径**同源**（`output.rs`）：
  高位水 `HIGH_WATER_BYTES = 128 KiB` / 低位水 `LOW_WATER_BYTES = 64 KiB`（滞回）。
- 客户端 ack 的 `offset` 是**本地累计字节**（客户端在 `subscribed` / `ring_resync` 时归零），
  插件侧 `Watermark.base` 记「本地计数归零那一刻的环游标」，`acked = base + ack_local`
  （单调 max，旧 ack 天然忽略）。
- 抗记账错位：连续抑制 `PARK_MAX_CYCLES = 4` 轮仍无 ack 回落 → 强制放行一轮并 **warn**
  （背压的失败模式必须是「多发」而不是「永不发」）。

---

## 2. 移动端

### 2.1 插件协议客户端（`wasm-apps/bedcode/rust/src/link.rs`）

命令面（前端 `terminal-session.<cmd>`）：

| 命令 | 语义 |
| --- | --- |
| `prewarm` | **只建连**（宿主代发 auth），不发 subscribe 帧——进终端页前的传输面预热（走 subscribe 预热会白传一遍整段重放：页面通道尚未登记） |
| `subscribe` | fresh subscribe：建连（无链路时）或重发 subscribe 帧（有链路时）→ 桌面端整段重放 |
| `unsubscribe` / `unsubscribe-all` / `remove` | 关连接（显式 close 同时取消宿主 auto-reconnect）/ 清理 |
| `send-input` | 文本 + 特殊键（帧序即写入序；不支持键显性报错且不半投递） |
| `ack-rendered` | 前端渲染水位；插件按 `ACK_BYTES_THRESHOLD = 64 KiB` 节流回发 ack 帧 |
| `get-state` | 链路快照（phase/cursor/acked/stopped/subscribed）供前端对账 |

事件面（`plugin:com.bedcode.terminal-session:terminal-*`）：

- `terminal-state`：`{session_id, phase(idle/connecting/auth/live), cursor, detail}`，
  detail ∈ `subscribed_sent` / `subscribed` / `reconnecting` / `reconnect_scheduled` /
  `resync` / `stopped` / `session_missing` / `error` / `prewarmed`。
- `terminal-resync`：`{session_id, offset, reason}`；reason = `truncated`（环淘汰，数据有缺口）
  / `replay`（**重放世代切换**：重订阅 / 断线重连 / 会话重启，无丢失）。

关键行为：

- 门控：未收 `subscribed` 前的输出帧一律丢弃（旧流残留；重放覆盖）。
- **重放前清屏**：`subscribed` 到达时若 `page_content`（该会话已成功转发过输出）非空 ⇒
  先发 `terminal-resync{reason:"replay"}` 再让重放字节流出——否则新一轮整段重放叠加在旧屏幕上
  = 重复显示（会话启动竞态双订阅 / 断线重连 / 会话重启三条路径都会重放）。
- 断线重连由宿主承担（退避 baseMs 1000 / maxMs 30000，下限钳制在宿主）：`ws:open` 接管新句柄后
  **自动重订阅**（prewarm 未订阅过的链路不重订阅——没有订阅可恢复）。
- 会话不存在：`MAX_SESSION_MISSING_STRIKES = 3` 内在退避循环里重试，超限停止等外部恢复。
- 输出字节全程零 JSON：`on_ws_frame(binary)` → 门控 → `host-terminal-stream.forward-output`。

### 2.2 宿主窄转发（`bedcode-mobile/src-tauri/src/plugin/terminal_stream.rs`）

- 表：`session_id → Channel<InvokeResponseBody>`（`terminal_page_subscribe` 登记 /
  `terminal_page_unsubscribe` 注销，容量上限 64）。
- `forward-output`（WIT，权限位 `terminal:output`）只做空 id 校验 + 原样 `Channel::send(Raw)`：
  **零解析、零缓存、零 JSON**（ADR 0022 薄壳④）。
- 丢弃可观测：`forwarded_chunks/bytes`、`dropped_no_channel`、`dropped_send_failed`
  （`TerminalStreamGateway::stats()`）；无通道丢弃按「每段未订阅窗口首次」留 warn。
  页面未订阅期间的字节**不缓存**，由重进页面的重订阅重放补齐。

### 2.3 前端（`wasm-apps/bedcode/src/terminal/`）

- `store.ts`（`terminalBuffer`）：
  - 页面进入 `markPageEntered` 登记页面字节流（`registerRealtimeHandler` 复位本代屏幕状态）；
  - 裸字节按到达序直写（**无去重、无 offset 判定**——协议侧不提供 offset）；
  - **重放世代**：每个 `detail=subscribed` 记一代；第 2 代起若屏上有内容 ⇒ 先清屏再渲染新重放；
    首代不清（事件与字节是两条投递通道，首代即便事件晚到也不得吞掉重放头部）；
  - `terminal-resync`：清屏 + 本地计数归零；`reason=truncated` 才上报「历史截断」；
  - 订阅意图去重：`subscribing` / `reconnecting` / （`subscribed` 且本代已有内容）时不再发
    subscribe 帧——桌面端每次 subscribe 都整段重放，重复意图只带来重复传输 + 一次多余清屏。
- `useTerminalBuffer.ts`：`onWriteParsed → ackRendered`（渲染背压 ack）；`onClear` =
  `writeCoalescer.dispose()` + `terminal.clear()`；回放静止 250ms 补一次全量重绘。
- `utils/writeCoalescer.ts`：rAF 合并（同帧单次 `terminal.write`）、单块 ≤64 KiB、
  每 128 KiB 让出主线程一次、累积 ≥512 KiB 立即 flush、100 ms 兜底定时器；
  `dispose()` 递增**写入世代**（epoch）——在飞的 >64 KiB 分块写在让出点醒来后立即中止
  （清屏后旧世代剩余分块不得再上屏），并补调度期间新入队的数据。

---

## 3. 契约与不变量

| 项 | 结论 | 依赖/边界 |
| --- | --- | --- |
| 字节顺序 | 保序 | 单线程 wasm 插件 + **总线单消费者 FIFO**（`MessageBus` 的 `publish_tx`：发布序 = 入队序 = 投递序；旧「每 publish spawn 一个任务」实现在锁上竞争，曾出现 16 KiB 帧成对互换——回归锁 `bedcode-mobile/src-tauri/tests/terminal_plugin_pipe_e2e.rs` P-003）+ 每连接单游标 + writeCoalescer 重入守卫 |
| 字节连续 | 缺口只经 `ring_resync` 显式表达（+ 前端清屏重锚） | 环淘汰、页面未订阅丢弃、通道发送失败三条路径都会留缺口 |
| 重复 | 协议侧不承诺去重；前端按「重放世代」清屏保证**渲染**不重复 | 每次 subscribe 必整段重放；前端订阅去重避免重复传输 |
| 丢失 | ① 页面未订阅丢弃（无缓存，靠重进重放）② 通道发送失败丢弃并摘槽 ③ 环淘汰（有重锚） | 三类都有计数/日志；③ 是数据真丢（有 `gapFrom` 可诊断） |
| ack 水位 | `acked = base + ack_local`；客户端归零点与插件 `base` 重锚**一一对应** | 偏大=多发（安全），偏小由 `PARK_MAX_CYCLES` 逃生阀兜底 |
| 事件 vs 字节的投递序 | 设计依赖「同一宿主调用序 ⇒ 前端投递序」（跨 transport 未在无头环境验证） | 清屏事件先于新重放字节被 emit；writeCoalescer 的 rAF 缓冲收窄失败窗口 |

---

## 4. 测试覆盖（当前真源）

- **桌面插件**（`bedcode-desktop/wasm-apps/terminal-session`）：`ws_terminal.rs` 内联单测
  （水位换算/闸门/重锚/逃生阀/`ring_resync` 载荷）、`d10_contract_test.rs`（结构锁）。
  端到端：`bedcode-desktop/src-tauri/tests/pty_session_chain.rs`（真实 PTY + 插件命令面）；
  `tests/ws_e2e.rs` 的终端端点闭环用例受无头认证限制**当前被跳过**（`tests/support/mod.rs`
  的 `positive_auth_needs_dedicated_binary`）。
- **移动插件**：`rust/src/protocol.rs` 内联 native 单测（帧形状 / 输入计划 / ack 节流 / 错误分类）。
- **移动宿主**：`bedcode-mobile/src-tauri/tests/terminal_session_e2e.rs`（真实组件闭环：
  jwt-auth 代发 → subscribe 帧 → `forward-output` 权限门）、
  `plugin/terminal_stream.rs` 单测（丢弃计数 / 通道转发计数）。
- **移动前端（vitest）**：`terminal/__tests__/store.test.ts`（世代清屏 / 订阅去重 / resync reason /
  停止恢复）、`writeCoalescer.test.ts`（合并 / 让出 / 阈值 / **epoch 中止**）、
  `terminalFlow.test.ts`（Channel 裸字节 → xterm）、`useTerminalSubscription.test.ts`、`useTerminalBuffer.test.ts`。
- **跨端真实互连**：`cross-end-tests` 的终端场景（`terminal_ws_flow` / `terminal_output_pressure` /
  `terminal_resync_ordering`）**当前不编译且不在 CI**（陈旧：仍引用退役的宿主 `terminal_link`），
  恢复工作见仓库待办。

---

## 5. 历史（已退役实现，仅供追溯）

以下机制属于 2026-09 的**宿主实现**，在票 05（绑定归位）/票 12（协议迁插件）后退役，勿按旧文实现：

- 宿主 `bedcode-mobile/src-tauri/src/terminal_link.rs`（1,363 行：`TerminalLink` /
  `TerminalLinkManager` / `TerminalEventSink` / link_io 重连 / special-key 翻译）+ `terminal_*` 八命令；
- 宿主 `bedcode-server-websocket/terminal_ws/{subscriber,forward,control_frame}.rs` 与
  `UnifiedOutputQueue`（50 MB 环）、`encode_output_frame_v3`（16 B 帧头）、每订阅者合帧窗口
  （`flush_interval=30ms` / `max_buffer_size=64KB`）、订阅者私有 ack 水位；
- 前端旧 `useTerminalOutputStream(.ts/Channel)` 的 TB v3 解析 / `lastRenderedOffset` 对齐裁剪 /
  `resync` 控制帧消费；
- 移动宿主 `SessionCache`（16 MB LRU）与段1/段2 双水位（`SEG2_*`）——已被「插件门控 + 宿主窄转发」
  取代（页面级背压现由前端 ack + 桌面插件水位承担）。

历史材料：`.scratch/2026-09-12-mobile-ws-rust/spec.md`、`.scratch/2026-09-12-pty-byte-history/spec.md`、
`.scratch/2026-09-17-pty-pull-subscribers/spec.md`（对应退役实现的设计记录）；
`docs/knowledge/mobile-terminal-optimization-reference.md`（标注历史的优化参考）。

## 6. 关联文档

- `docs/diagrams/pty-output-flow-desktop.html` / `pty-output-flow-mobile.html`（交互式流程图，按旧架构绘制，待刷新）
- `docs/adr/0022-plugin-host-interface-primitive-boundary.md`（宿主/插件边界）
- `bedcode-mobile/docs/code-map.md`、`bedcode-desktop/docs/code-map.md`（目录级导航）
- AGENTS.md §9（协议两端同步部署）
