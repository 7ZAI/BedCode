---
name: terminal-output-pipeline-optimization
description: 移动端 PTY 输出链路审计发现的优化项和待实施方案
metadata:
  type: project
---

# 移动端 PTY 输出链路优化

## 已完成

### 1. subscribeSession 未重置 lastIndexRef（已修复）

**问题**：重连/重新激活时 `subscribeSession` 用 `start_seq=None` 从头接收历史，但 `lastIndexRef` 保留旧值，历史事件被 `index <= lastIndexRef` 过滤。

**修复**：`subscribeSession()` 开头重置 `lastIndexRef = -1`

**Why**: 重连后历史输出缺失，只显示当前一屏内容
**How to apply**: 所有 `subscribeSession` 调用路径自动受益

### 2. subscribeSession 无防重入保护（已修复）

**问题**：`isConnected` watch 和 `isSessionActive` watch 可并发调用 `subscribeSession`，导致重复 `wsJoinSession` + 监听器替换。

**修复**：`subscribeSession()` 开头检查 `isSubscribing.value`，已订阅中则跳过

**Why**: 并发订阅导致后端双重订阅和输出重复
**How to apply**: 所有 watch 触发路径自动受益

### 3. onActivated 在 isSubscribing 时跳过终端恢复（已修复）

**问题**：停用期间 watch 触发 `subscribeSession`，用户切回时 `onActivated` 检测到 `isSubscribing=true` 直接 return，跳过 `clearTextureAtlas()` + `refreshTerminal()`。

**修复**：`isSubscribing` 时仍执行渲染恢复，只跳过订阅

**Why**: 终端画面白屏或纹理损坏无法恢复
**How to apply**: 所有 onActivated 路径自动受益

### 4. Rust 层冗余 Base64 解码 + UTF-8 lossy 转换（已修复）

**问题**：`event.rs` 中 `MobileEvent::Output` 携带 Base64 data → Rust 层解码为 bytes → `String::from_utf8_lossy` → 再通过 `app.emit` 传给前端。做了编解码的"往返运动"，且 `from_utf8_lossy` 会损坏非 UTF-8 字节。

**修复**：Rust 层直接传递 `data_base64` 字符串到前端，前端用 `atob()` 解码为 `Uint8Array` 传给 `xterm.write()`

**Why**: 消除高频路径上的冗余编解码，避免 UTF-8 lossy 数据损坏，xterm.write(Uint8Array) 比 write(string) 更高效
**How to apply**: 后续如有新终端组件，直接使用 `data_base64` 字段

## 已完成优化（续）

### 5. 前端全局监听器替代 per-component 监听（已实施）

**现状**：每个 TerminalView 实例都 `listen('ws_output', ...)`，N 个实例 = N 个监听器，每个事件被处理 N 次（N-1 次被 session_id 过滤丢弃）。

**方案**：在 `useTerminalOutput` composable 中创建单一全局 `ws_output` 监听器，用 `Map<sessionId, OutputHandler>` 分发到对应 xterm 实例。

**实施**：
- 新建 `useTerminalOutput.ts` composable，维护全局 `handlerMap`
- `registerHandler(sessionId, { onOutput })` / `unregisterHandler(sessionId)` API
- `TerminalView.createOutputListener()` 改为调用 `registerHandler`
- `outputListenerRef` 从 `UnlistenFn | null` 改为 `boolean`（标记是否已注册）
- 所有处理器注销后自动关闭全局监听器释放资源

**Why**: 减少 N-1 倍无效回调执行和 Tauri IPC 事件分发开销
**How to apply**: TerminalView 的 onMounted/onUnmounted/onActivated/onDeactivated 自动管理注册/注销

### 6. 历史回放按需获取（已实施）

**现状**：每次 `subscribeSession` 用 `start_seq=None` 全量回放，后端 `UnifiedOutputQueue` 可能存 50000 条事件，但 xterm scrollback 只有 5000 行。大量数据写入后被 scrollback 丢弃，浪费带宽和 CPU。

**方案**：
- 断线重连时，如果 `lastIndexRef >= 0`，使用 `start_seq = lastIndexRef + 1` 增量获取
- 首次订阅 / 会话切换仍用 `start_seq=None` 全量回放
- 前端通过 `SubscribeResult.minSeq` 检测数据覆盖，自动回退到全量回放

**实施**：
- `wsJoinSession` 改为调用 `ws_subscribe_session`（支持 `startSeq` 参数）
- `ws_subscribe_session` 命令返回 `SubscribeResult { minSeq, maxSeq, historyCount }`
- `subscribeSession` 根据 `lastIndexRef` 计算增量同步起点
- 增量同步回退：`minSeq > startSeq` 时清空 xterm + 重置 `lastIndexRef`

**Why**: 断线重连时避免全量回放，只获取缺失部分，减少带宽和渲染时间
**How to apply**: 所有 `subscribeSession` 调用路径自动受益，无需额外配置

### 7. OutputBuffer 合并消息增加 end_index（已实施）

**现状**：`OutputBuffer` 合并多条事件后只保留 `start_index`，前端 `lastIndexRef` 设为 `start_index`。如果将来启用增量同步（`start_seq=N`），可能出现 index 不连续导致去重误判。

**方案**：`OutputBuffer.flush()` 时在消息中增加 `end_index` 字段，前端用 `end_index` 更新 `lastIndexRef`。

**实施**：
- `TerminalAction::Output` 增加 `end_index: Option<usize>` 字段（`serde(skip_serializing_if = "Option::is_none")`）
- `OutputBuffer` 增加 `end_index` 跟踪，`flush` 时在合并多条事件（`end_index > start_index`）时附带
- `MobileEvent::Output` 增加 `end_index: Option<u64>` 字段
- 前端 `lastIndexRef` 使用 `end_index ?? index` 精确更新去重游标

**Why**: 为增量同步铺路，使去重逻辑更精确，单条事件 end_index=None 不影响现有行为
**How to apply**: 后续实施 start_seq 增量同步时，前端可用 end_index 准确计算断点位置

### 8. 历史回放批量写入优化（评估后收益有限，暂不实施）

**现状**：前端每收到一个 `ws_output` 事件就调用 `terminal.write()`。历史回放时后端快速发送多条消息，每条 `write` 都触发 xterm 解析和渲染。

**评估**：
- 后端 `OutputBuffer` 已在 30ms 间隔内合并多条事件，前端收到的每个 `ws_output` 已是"一批"
- xterm.js 的 `write()` 内部有异步渲染机制，连续多次 `write()` 会在同一个 refresh cycle 中合并渲染
- 进一步在前端做批处理（收集所有历史再一次性 `write`）的收益很小，且增加复杂度

**结论**：当前架构已足够高效，暂不实施

### 9. WebSocket Binary 消息替代 Text

**现状**：PTY 输出数据经过 bytes → Base64 → JSON Text 的编码链路。Base64 增加 33% 体积。

**方案**：使用 WebSocket Binary 消息直接传输原始 bytes，避免 Base64 编码/解码。

**收益**：减少 33% 传输体积，省去 Base64 编解码 CPU 开销。

**风险**：需要重新设计消息协议（Binary 消息无法携带 JSON 元数据如 session_id、index）；需要处理消息分帧和路由；改动范围大，影响 desktop 和 mobile 两端的 WS 层。

### 10. 背压 ack 双门控死锁——「显示一点就卡住」（已修复，2026-09-10）

**症状**：移动端终端只显示开头一些输出后永久卡住，后续输出不再出现。

**根因**（服务端 `SessionOutputManager` + 移动端双重门控叠加）：

1. 服务端背压水位**按会话整体记账**（每产出事件 +bytes，`unacked_bytes`），超 64KB 高位水即**暂停该会话 PTY 读取**（作用于所有订阅者），仅 ack 能把水位降到 8KB 恢复
2. 服务端 `GlobalOutputManager::ack` 有 **正统渲染端门控**：非正统端（current canonical 之外的订阅者）的 ack 直接丢弃
3. 移动端 `TerminalView` 的 `shouldAck` 又门控 `isCanonicalRenderer`（仅 resize 返回 applied 才为 true）→ 非正统时**根本不发 ack**

**死锁场景**：桌面端启动会话（初始正统 = Desktop）、手机观看——手机 resize 触发 needsConfirmation（或用户拒绝/80x24 skip/HTTP 失败）→ `isCanonicalRenderer=false` → 手机永不 ack；桌面顺向没在看（用户在手机上操作）也不 ack → `unacked_bytes` 触及 64KB → **会话 PTY 读整体暂停且仅 ack 能恢复 → 永久卡死**。历史回放不记账，故症状是"历史/开头能显示，实时输出到 64KB 后戛然而止"。

**修复**（双端，backward compatible）：

- **服务端** `session_output.rs::GlobalOutputManager::ack`：去掉正统门控，任何已认证订阅端的 ack 都推进记账（`_source` 保留签名）。`unacked` 是共享流量水位而非尺寸裁决依据，观看端确认即代表字节被消化；多端并发时慢端 ack 只落后不拖垮快端
- **移动端** `useTerminalBuffer.ts` / `TerminalView.vue`：`registerRealtimeHandler` 删除 `shouldAck` 参数，`onWriteParsed` 触发即无条件回发 ack（该事件本身就证明本端在消费渲染管线；mock/未连接时 `socket.ackRendered` 内部空转安全）。`isCanonicalRenderer` 保留（resize 覆盖弹窗 UI 仍用）

**回归护栏**：`test_ack_from_mobile_observer_releases_backpressure`（服务端，移动端身份 ack 推经水位恢复）。验证：cargo session_output 28 绿 / forward 15 / control_frame 13 / terminal_ws 28；移动端 vitest 360 绿。

**遗留观察**：超高速持续输出下（xterm write 队列常满导致 onWriteParsed 稀疏），ack 节奏可能退化为服务端暂停/恢复锯齿——主修复后实机烟测，若抖动明显再调水位或 ack 节流参数。

### 11. 运行中「滑动失效 / 输入无反应 / 格式空行」链路修复（2026-09-15）

**症状**：运行过程中 ① 无法滑动到底部显示 ② 输入无反应 ③ 输出格式出现空行间隔。

**根因（一条链，P0）**：桌面端重复广播 `status=running`（无状态迁移）→ `onSyncSessionStatusChanged` 无守卫调用 `markSessionRunning`（清零 `subscribed` 与 `lastRenderedOffset`）→ 前端再调 `terminal_subscribe`，而 **Rust 侧幂等订阅静默 return 不发状态事件** → `subscribed` 永久为假（`markSubscribed` 无调用者，无自愈路径）→ 输入被 `sendInput` 门控拒绝、订阅重试 3s 空转；同时 `lastRenderedOffset=null` 令历史以 `from=0` 全量重播并**叠加**在已有画面上（`minOffset>0` 时起点还切在半条转义序列中间）→ 重复内容/错行/空行。

**修复**：

- `terminal_link.rs`：幂等 `subscribe` 分支改为 `emit_state("resubscribed")` 补发当前状态
- `stores/terminalBuffer.ts`：新增 `reconcileState`/`applyLinkState`（`terminal_get_state` 主动对账，phase 只前进不回退）；`ensureBuffer` 的 `subscribed` 由 phase 派生；`markSessionRunning` 仅在实际「已停止」时复位
- `useMobileConnection.ts`：`running` 广播加迁移守卫（仅未跟踪/已停止会话走恢复）
- 附带修复：网格未校准时的 80×24 跳过改为 `gridCalibrated` 标志（真实 80×24 设备不再永久不同步 PTY）；`ensureCellHeight` 兜底（cellHeight=0 时滚动/惯性/选择静默全失效）；DECSET 嗅探支持多参数 `ESC[?1000;1006h` 并跟踪上报模式开关；桌面 `OutputBuffer` 带洞事件先 flush 再起批（帧区间与负载必须一致）；缓存字节洞显式上报（`gapDetected`）→ 清屏锚定重播 + 用户提示；`subscribe_ok` 闸门前二进制帧只入缓存

**Why**：状态信念不可自愈 + 破坏性重置是「输入无反应」的根因；区间错位/跨洞拼接是「格式空行」的根因；TUI 判定漏判与行高为 0 是「滑不动」的根因
**How to apply**：新增前端链路状态时必须保证「事件 + 主动对账」双通道收敛；任何「重置订阅信念/渲染游标」的操作都要先确认真实状态迁移

### 12. 行尾区右侧抖动 → 静态裁切取代逐帧 clip-path 补丁（2026-09-15）

**症状**：终端渲染区右侧出现抖动（TUI 重绘时行尾色块左右摆动）。原止血补丁
（`utils/terminalRowClip.ts`，MutationObserver + 逐 span 写/清 clip-path）反而加剧抖动。

**根因（抖动 = 补丁机制，而非裁切本身）**：

1. CJK advance 与 2 格宽存在亚像素偏差，xterm DOM 渲染器按真实 advance 流式排版 →
   误差逐字符累积，行尾**背景填充盒**被推出网格右界 7~22px（TUI 用「背景色 + 空格」
   铺面板时必现）；满行末字墨迹也溢出「数 px」
2. 旧补丁按「溢出量 > 1px」逐帧计算 `inset(0 <溢出>px 0 0)`：漂移量在容差阈值两侧
   振荡 → clip-path 反复写/清 → 合成层/重绘抖动；且每次扫描含
   `getBoundingClientRect()` 强制布局（单次全量 ≈10ms，120Hz 帧预算仅 8.3ms）

**新方案（已落地，纯 CSS 静态裁切）**：`terminal.css`

- 保留 `.xterm-rows > div { overflow: visible !important }`——解除 xterm 内联硬裁，
  末字墨迹可落墨进行尾余量区
- 新增 **静态** `clip-path: inset(0 -6px 0 0)`：行界裁切统一右扩 6px
  （= `TERMINAL_SCROLLBAR_GUTTER_PX`）
  - 末字超格数 px 落在 6px 内 → 完整可见
  - 背景盒超出 6px 的部分被裁掉，**裁切线为常量**、不随内容/重绘变化 → 色块不摆动
  - 6px 落在 gutter 区内，视觉等价「面板铺到滚动条线」，其余余量区仍是主题底色
- 零逐帧样式写入、无观察器、无强制布局、不新增合成层 → 机制上不可能再振荡
- 旧 JS 模块与单测已删除（git 历史可回溯；后续如真机仍不满意，见下方备选）

**为何不用其它方案**：

| 方案 | 否决原因 |
| --- | --- |
| 恢复行级原生 `overflow:hidden`（去掉 override） | 末字墨迹被硬裁右半，回退到已修的老问题 |
| 行级 mask 渐隐（`linear-gradient` 在行盒内） | 渐隐区落在网格内 → **整列字符被减淡**，正常文本也受影响 |
| 单层静态 mask 挂在容器上（渐隐只跨余量区） | 需 JS 提供网格右界 px 变量；mask 可能促使 `.xterm` 成为合成层，与「will-change:auto 防重影」约束冲突（实证风险高，留作 A/B 备选） |
| WebGL/canvas 渲染器（逐格绘制无漂移） | 移动端 SwiftShader 软渲下全屏重绘闪烁/显存膨胀未解（`USE_WEBGL_RENDERER` 注释） |

**How to apply**：任何针对行尾漂移的补丁都必须是**常量裁切**；禁止在渲染循环里读写
行/span 几何或反复写 clip-path（阈值振荡必然抖动）。若需更强的视觉收敛（把残余
6px 也藏掉），优先按上表 mask 方案做真机 A/B，再决定是否引入合成层代价。

### 13. 背压 ack 空闲滞留 → PTY 读永久暂停（滑动失效 / 输入无回显，2026-09-17）

**症状**：TUI 会话运行中（输出洪峰期）向上滚动查看后 ① 无法滑回底部（应用对滚轮
无响应）② 输入无回显；切换会话或重新进页面也不恢复，只有换一个会话才「正常」。
另见会话重进时出现 4MB 级历史全量重播。

**取证**（真机 dev 日志 + 桌面端 runtime 日志，双端时间轴对齐）：

- 桌面端 `session_output`：`18:29:42.377 paused unacked=71653` → 同秒 ack 释放 55825
  → `unacked=19923`（仍在低水位 8192 之上）→ **此后至日志结束再无 resume**（PTY 读停摆）
- 移动端 Rust `terminal_link`：最后一次 `ack frame sent` 为 `acked=4084650`；此后缓存尾
  推进到 `4104573`（差 19923，与桌面端 unacked 完全相等），但**不再有 ack 帧发出**
- 移动端前端：重进页面时 `history splice done cursor=4104573`，仅有一条
  `render ack #1 offset=-`（onWriteParsed 触发时游标尚未推进 → 被 `lastRenderedOffset
  === null` 短路）

**根因（双向死锁）**：ack 回发节流为「64KB 阈值 + 250ms 空闲兜底」，但该判据原先
**只在「收帧」与「前端渲染 ack」两个事件里求值**，没有任何定时机制：

1. 洪峰期最后一批字节（此处 19923B）在距上次回发 <250ms 内到达 → 判定「保留待发」
2. 桌面端随即因 `unacked > 64KB 高水位` 暂停 PTY 读 → 不再有新帧
3. 消费端还在等帧来触发下一次 ack 判定、生产端在等 ack 才恢复读 → 永久互等

叠加因素：前端渲染 ack 只在 `onWriteParsed` 里发出，而历史重播的写入回调早于游标
推进，回放字节因此不产生任何渲染 ack（该路径同样无法打破死锁）。

**修复**：

- `src-tauri/src/terminal_link.rs`：连接循环新增 ack 空闲轮询分支
  （`ACK_IDLE_TICK_MS = ACK_MAX_IDLE_MS / 2`，`MissedTickBehavior::Delay`），周期性调用
  节流判定，使「无新帧到达」时积压 ack 也能在空闲窗口内回发；判定逻辑抽为纯函数
  `should_send_ack` 并显式排除 `pending == 0`（否则定时器会每 250ms 回发空 ack）
- `src/stores/terminalBuffer.ts`：`spliceHistory` 推进游标后立即 `ackRendered`
  （历史段确已写入 xterm），不再依赖 `onWriteParsed` 的触发时序

**Why**：单向依赖「事件触发」的节流器在上游被自身背压暂停时必然自锁；消费端的
确认机制必须自带时间兜底，否则生产端等待的正是消费端等不到的触发条件。
**How to apply**：任何「消费端回发确认 / 生产端按确认放行」的反馈环，确认发送都不
得只依赖数据到达事件——必须叠加空闲定时兜底；缺省 `pending == 0` 短路防止空转流量。

**回归护栏**：`should_send_ack_*` 5 例（阈值即发 / 空闲窗口到期必发 / 窗口未到保留 /
`pending==0` 短路 / 子阈值积压永不滞留）+ `ack_idle_tick_is_within_idle_window`
（轮询间隔必须落在空闲窗口内）。验证：`cargo test terminal_link` 23 绿；
移动端 `pnpm run test:run` 452 绿。

**遗留观察**：`ingest_frame` 每次收帧都把 ack 水位同步到缓存游标（背压锚点在 Rust
缓存而非前端渲染），故前端渲染 ack 实际只起「触发回发」作用；若后续要回归真正的
渲染侧背压，需把水位语义与触发机制一并重构（另开 ADR/spec）。

### 14. 终端链路拆「两段订阅 + 两段背压」（2026-09-17）

**背景**：原设计只有一段订阅语义被明确（Rust ↔ 桌面端），前端与 Rust 之间靠
「注册/注销 handler + set_mode(realtime/batch)」隐含表达，两个问题：

1. 段2 没有订阅边界——`terminal-frame` 事件在页面关闭时**照常逐帧推送**（前端只是
   丢弃，IPC 与 base64 编码开销纯空转）
2. 段2 没有背压——`acked` 在收帧时就同步到缓存游标，前端渲染速度对上游没有任何
   反馈；WebView 跟不上（解码 + xterm 解析 + 渲染）时事件仍全速灌入

**改造**：

| | 段1 Rust ↔ 桌面端 | 段2 前端 ↔ Rust |
| --- | --- | --- |
| 生命周期 | 会话 WS 连接成功 → 订阅；会话停止/设备断开 → 取消 | 进入终端页 → 订阅；退出 → 取消 |
| 命令 | `terminal_subscribe` / `terminal_unsubscribe` | `terminal_page_subscribe` / `terminal_page_unsubscribe` |
| 订阅态载体 | 链路对象（每会话一个 WS） | 管理器持有的 `Arc<AtomicBool>`（链路重建后沿用） |
| 产物 | 会话级字节缓存（真源，16MB LRU），与页面无关始终收帧 | `terminal-frame` 事件推送开关 |
| 背压水位 | `acked` = 缓存游标（收帧即消化） | `frontend_rendered` = 前端渲染游标（`terminal_ack_rendered`） |
| 水位阈值 | 桌面端高 64KB / 低 8KB（暂停 PTY 读） | 高 1MB / 低 256KB（停推事件） |
| 越界行为 | 数据留内核管道（读线程恢复即自然续读） | 字节留 Rust 缓存 → **恢复时按缓存连续段一次性补推** |

**关键设计点**：

- **段2 订阅态必须挂在管理器上**：链路的生命周期是「会话存续」，页面的生命周期是
  「进出终端页」，两者正交。链路会因会话停止/断开被重建（新建 `TerminalLink`
  实例），订阅态若挂在链路对象上，会话重启后会静默失联。以 `Arc<AtomicBool>` 由
  管理器持有、链路共享引用即可沿用
- **段2 越界必须补推**（与段1 的本质差异）：段1 的数据停在内核管道，PTY 读线程恢复
  后自然续读，消费端不需要额外动作；段2 的数据停在**缓存**里，对消费者不可见——
  它既不会收到「帧首越过游标」的缺口帧，也不会主动重新拉取历史。若不补推，一旦
  输出恰在暂停期结束，前端将永久停在陈旧画面。补推按缓存的**连续段**切分
  （`contiguous_runs`），跨洞不合并——合成帧会让消费端按帧头区间推导的负载与真实
  负载错位（转义序列接在半途），按段切分后消费端按既有缺口自愈路径处理即可
- **段2 基线换代**：进入页面时重置 `frontend_rendered = 当前缓存游标` 并清暂停态
  ——沿用上一代的低水位会在与当前游标的差值上算出巨大假窗口，一进页面就停推（空屏）；
  新消费者随后自行 `terminal_get_history` 拼接，其私有游标覆盖 [基线, tail)
- **段2 水位上界钳制**：`terminal_ack_rendered` 的 offset 必须 `min(cursor)`——前端
  不可能渲染未收到的字节，越界值会把窗口算小（背压永不触发）且让补推起点错位

**验证**：`cargo test --lib` 321 绿（新增段2 门控四条件、水位滞回、连续段切分 ×3、
管理器订阅态隔离与持久性等 6 例）；移动端 `pnpm run test:run` 454 绿（新增段2 订阅
与页面进出配对、页面进出不触发段1 两例）。

**后续观察**：段2 高位水 1MB 为估值（对应「前端落后约一秒渲染量」）；若真机出现
暂停/恢复抖动（日志 `seg2 push backpressure state changed` 频繁），按 ack 节奏
（`onWriteParsed` ~60Hz）实测调整水位即可，机制不变。

### 15. 段1 语义变更：桌面端改为「每订阅者拉取游标」（2026-09-17）

**背景**：桌面端 PTY 输出链路从「单生产者广播（源侧背压）」重构为「单生产者环形缓存
+ 每订阅者拉取游标」（`.scratch/2026-09-17-pty-pull-subscribers/spec.md`）。对移动端
的段1 语义有三处直接变化：

| | 旧（推送 + 会话级共享水位） | 新（拉取 + 每订阅者私有窗口） |
| --- | --- | --- |
| 背压归属 | 会话级共享 `unacked`（任何一端的 ack 都能降水位）→ 超 64KB **暂停整个会话的 PTY 读** | 每订阅者私有 `acked_offset`；窗口（`next − acked`）越高位水只**停发该订阅者**，源产出与其他订阅者不受影响 |
| 窗口水位 | 高 64KB / 低 8KB（源侧滞回） | 高 128KB / 低 64KB（订阅者执行体滞回，`terminal.subscriber_*`） |
| ack 阈值 | 64KB / 250ms | 不变（64KB / 250ms）；关系 `ack(64) ≤ low(64) < high(128)` 且 `high − ack ≤ low` → 一次 ack 即解锁 |

**移动端适配**：

- **段1 截断显式化（M3，已落地）**：桌面端新增 `resync {min_offset, snapshot_offset}`
  控制帧（订阅者游标早于环驻留起点时下发，随后从 `min_offset` 连续重播）。
  移动端 `terminal_link.rs::apply_resync` 重锚缓存（`SessionCache::reset_to`）与
  水位（cursor/acked/frontend_rendered = min_offset）、置 `resynced`（此后重播帧
  直达前端，不再等 history_end——它不会到来）并立即回发一次 ack 让桌面端窗口归零解锁；
  同时发 `terminal-resync` 事件让前端清屏 + 游标重锚 + 一次性提示
  （`stores/terminalBuffer.ts::onResyncEvent`）。既有「缺口 → forceReplay →
  getHistory → minOffset 越界 → 清屏」间接路径**保留为兜底**（老端/信号缺失不退化）
- **段1 有损化（设计取舍）**：源不再暂停 ⇒ 极慢订阅者在环窗口耗尽后会被截断
  （走上面的 resync 自愈）。环 50MB 足以吸收正常抖动；收益是「任何单个消费者都不能
  冻结链路」——这是本次重构最重要的取舍
- **段1 水位预算上界**：`ack 阈值 ≤ 桌面低位水` 是硬约束（桌面端启动时校验，
  见 `TerminalConfig::subscriber_budget_violation`）；上界另受**本端 WS 接收缓冲**约束
  （水位必须低于接收缓冲，否则溢出丢消息）。禁单独抬高桌面水位或移动端 ack 阈值

**M4/M6 结论**：保持 ack 阈值 64KB（不放宽）——桌面窗口提到 128KB 后
「一次 ack 即解锁」关系已成立，放宽到 256KB 反而要求桌面高位水同步抬到 ≥320KB
（更接近 WebKit/WS 缓冲上界），收益（少发 ack 帧）不足以抵消风险。三段容量预算表
见 `docs/knowledge/pty-output-pipeline.md` §1.6。

### 16. 键盘避让连带 ±1 列漂移 → 整缓冲重排 + 多余整屏重绘（2026-09-18）

**症状**：会话有较长历史输出时，每次键盘弹出/收起（避让）后终端都像被"刷新"了一下；
历史短时几乎无感，长历史时肉眼可见。

**取证**（真机 dev 日志，双源时间轴对齐）：

- 原生 `[EdgeToEdge] WindowInsets Keyboard:true(822)` / `false(0)` 与前端 `onResize` 严格一一对应：
  弹出 `97x31`、收起 `96x52`——**列在 96↔97 之间翻转**（DPR 口径下应恒为 96）
- 每次键盘事件各触发一次 `send resize to PTY`
- 日志中无 `[TerminalView] fit:` 行 → 确认走 ResizeObserver → `applyResize` 路径（非 fit 轮询路径）

**根因（三层叠加）**：

1. 键盘避让 = 根容器高度收缩 → 行数变化 → `term.resize()`。xterm 内部 resize 本身必然整屏重绘
   （`RenderService.handleResize → _fullRefresh`），这一步无法避免（可见行确实变了）
2. `shouldApplyGridResize` 的「行变化立即生效」是提前 `return true` 的短路，把本应裁掉的 ±1 列
   测量漂移**一并写进网格**；而 xterm `Buffer._reflow` 以「列是否变化」为唯一开关
   （`if (this._cols === newCols) return`），列一变即走 `reflowLarger/reflowSmaller` 遍历并重写
   **整个 scrollback**（移动端 10000 行；`_isReflowEnabled` 依赖 `_hasScrollback`，故仅 normal
   buffer 的长时间历史付这份成本）→ 历史越长，每次键盘避让越重
3. `scheduleAtlasPreheat` 无条件排一次 700ms 后的整屏 refresh——它只服务 WebGL 字符图集，
   而移动端 `USE_WEBGL_RENDERER = false`（DOM 渲染器无图集），属纯多余的第二次全屏重绘

**修复**：

- `utils/terminalResizePolicy.ts`：`shouldApplyGridResize`（布尔）→ `resolveGridResize`（返回目标
  网格）：列偏差 ≤1 保持当前列、>1 采用目标列；行任意变化立即生效；结果与当前网格一致返回 null
  （调用方不 resize）。语义与原实现的触发条件一致，只把「被钳制的漂移」挡在写入值之外
- `composables/terminal/useTerminalRenderer.ts`：`scheduleAtlasPreheat` 门控 webglAddon 非空
  （对齐桌面端同款"无图集即 no-op"语义）
- 同批把 `TerminalView.vue` 按域拆分为 `composables/terminal/`
  （terminalKernel + 渲染器 / resize / 键盘避让 / 订阅），组件退化为编排层；
  范式参考桌面端 `composables/terminal/`

**Why**：列漂移是测量口径差（DPR 换算 + 每次 resize 后 xterm 重测字体的 subpixel 偏差），
钳制它本来就是策略本意；行变化必须生效（网格与容器高度不一致会露顶部空带或裁掉末行）。
二者必须在"写入值"层面分离，否则容差形同虚设。

**How to apply**：任何「容差钳制」都要先回答「钳制的是触发条件还是写入值」——触发条件放宽、
写入值照抄目标，会把本应丢弃的漂移重新引入（本例即每次键盘避让白付一次整缓冲重排）。

**回归护栏**：`resolveGridResize` 12 例（列 ±1 且行不变 / 列 ±1 且行变化保持当前列 / 行双向 /
列偏差 >1 / 完全一致 / 常量）。验证：移动端 `pnpm run test:run` 461 绿、
`vue-tsc --noEmit` 0 错、根 `pnpm exec eslint .` 0 error。

**遗留观察**：列 96↔97 的输入侧来源（容器 `clientWidth` 变化还是 xterm 重测的 `css.cell.width`
漂移）未最终定位；本次按"不写进网格"处理即消除其代价（reflow）。需继续追查时在 `applyDprFit`
打点 `containerWidth / cellWidth / targetCols`（已保留钳制后的 `applyDprFit:` debug 日志可对照）。

### 17. 段2 背压死锁：长历史拼接后「输入无回显」（2026-09-18）

**症状**：进入有长历史的会话 → 历史显示正常 → **输入字符终端无回显**。

**根因链条**（真机日志 `android-dev.2026-09-18.log`，Rust 段2 状态与前端 ack 双源时间轴对齐）：

1. `terminal_page_subscribe` 后，历史段冲刷使段2 未渲染窗口越过高位水 → `seg2_paused=true`
   （日志仅此一条状态变更）
2. 前端拼接 16MB 历史耗时约 4s，期间 `cursor` 涨到 ~36.8MB；拼接完成时 ack(34.1MB)（`snapshotOffset`）
3. 旧 `seg2_drain()` 要求窗口**已回落到低位水**才补投 → 此刻窗口仍 2.7MB → 直接 return：
   不补投、也不解除暂停
4. 消费端推进窗口的唯一手段是**收到帧**，而帧被暂停 → 前端再无帧、再无 ack → 永久停推。
   `frames_live=0` 恰好印证「从未 emit 过任何帧」（历史走 `terminal_get_history` 命令，不受段2 门控）

时间轴：`02:46:38.106 paused=true` → `02:46:45.551 history splice start` →
`02:46:49.736 render ack #1` + `history splice done: cursor=34104984, flushedLiveFrames=0 (0B)` →
之后 `frames_live=0` 持续到日志结束。

**修复**（`src-tauri/src/terminal_link.rs`）：

- 新增常量 `SEG2_ACK_PUSH_CHUNK_BYTES = SEG2_LOW_WATER_BYTES`：单次补投上限，防一次灌数 MB
  淹没 IPC 与 WebView 解析（正是段2 背压要防的事）
- 新增纯函数 `ack_backlog_push_range(rendered_before, rendered_after, cursor, max_bytes)`：
  **ack 推进**才补投、按 `cursor` 收口、按额度切片
- `seg2_drain(max_bytes)` 重写为 **ack 驱动**：无条件解除暂停 → 按字节额度切片补投
  `contiguous_runs(frontend_rendered)` → 推完按 `unrendered > SEG2_HIGH_WATER_BYTES` 重估暂停态
  （仍超则继续暂停，等下一次 ack 再解锁——投递节奏由消费端掌控）
- `seg2_mark_rendered()` 用纯函数决定是否补投；新增 debug 日志
  `seg2 backlog pushed on render ack`（from_offset / drained_bytes / unrendered_bytes / paused）

**Why**：段2 的窗口只能由「消费端收到帧」推动（`frontend_rendered` 单调不减、`cursor` 单调增
⇒ 滞回下沿在真实运行中永不触发）。把补投条件写成「先降回低位水」必然互等：消费端等帧、
生产端等窗口回落。补投必须由 ack 驱动，才能既不淹没 WebView 又不空等。

**回归护栏**：`ack_backlog_push_requires_advance_and_backlog`（正例：ack 推进且有滞留 → 给出
`[from, from+max)`；反例：无推进 / 无滞留 / `max_bytes=0` → `None`）；`cargo test --lib terminal_link`
33 绿。`seg2_paused_after` 及其单测**刻意保留**（它决定新帧是否直推），注释已写明
「回落低位水恢复」分支在新实现下不可达。

**待办**：真机复测——长历史会话进入后输入应即时回显，日志应出现若干条
`seg2 backlog pushed on render ack` 并最终 `paused=false`。

### 18. 段2 帧出口改 Tauri IPC Channel（TB v3 二进制 Raw 帧，2026-09-18）

**背景**：段2 输出帧此前走全局 `terminal-frame` 事件（`emit` + base64 载荷），开销 =
事件全局广播 + JSON 序列化 + base64 编解码（+33% 体积）。

**分工（已定，勿再摇摆）**：**帧（高频大负载）走页面级 Channel + TB v3 二进制；状态/重锚
（低频）保留全局事件**。与桌面端 `subscribe_terminal_channel` 同构；差异是移动端把控制面留在
事件里（`terminal-state` / `terminal-resync` 是全局状态机，页面卸载后仍需收敛 UI）。

**TB v3 帧布局（两端同源，改布局必须两端同步）**：
`magic 'T','B'(2) + version 3(1) + flags(1，数据帧 0x00 / ACK 0x02) + start_offset(8 LE) + len(4 LE) + payload`，
`end = start + len`。一条 Channel 消息**可承载多帧**（补投切片只落在字节边界），故前端按 16B
帧头循环解析而非「一条消息一帧」——与段1 wire 协议同一口径，帧语义只留一份真源。

**实现要点**：

- **Rust**（`terminal_link.rs`）：`emit_frame` 改为 `channel.send(InvokeResponseBody::Raw(encode_data_frame(..)))`；
  发送失败即「消费端已离去」→ 记 warn + 就地清空槽位。`seg2_channel`
  （`Arc<Mutex<Option<Channel<InvokeResponseBody>>>>`）与订阅态同由**管理器**持有 → 链路重建沿用
  （避免「链路重建即静默失联」）；`page_subscribe(session_id, channel)` 登记、`page_unsubscribe`
  与 `remove` 一并清空；锁序固定 `page_channels → consumers → links`（注释已写明）
- **前端**（`stores/terminalBuffer.ts`）：`markPageEntered` 新建 `Channel<ArrayBuffer>`、**先挂
  `onmessage` 再 invoke**（invoke 在途时到达的帧不会因缺回调而丢）；`markPageLeft` **先把
  `onmessage` 换成空操作**再 `terminal_page_unsubscribe`；`onChannelMessage` 按 16B 头逐帧解析
  （magic / 版本 / 长度越界即告警停止解析，交缺口自愈路径重拼接），负载用 `slice()` 切开与整条
  消息视图的关联；已删除 `terminal-frame` 监听（`terminal-state` / `terminal-resync` 保留）
- **为何不能 `channel.onmessage = null`**：真实 `Channel` 的 `onmessage` 类型不可空，其调度处直接
  `this.#onmessage.call(...)`——置 null 会让「取消订阅瞬间的在途帧」抛 TypeError（vue-tsc 也报错）；
  空操作回调语义等价且安全（真实实现构造时就以 `onmessage || (() => {})` 兜底）
- **测试接缝**：`src/__tests__/setup.ts` 提供全局 `@tauri-apps/api/core` 替身（真实 `Channel` 构造
  依赖 WebView 注入的 `__TAURI_INTERNALS__.transformCallback`）；store / 集成测试从
  `terminal_page_subscribe` 的调用参数取回通道，投喂真实 TB v3 字节（测试内断言
  `end − start` 必须等于负载长度，即用例本身充当协议校验）

**取舍**：省掉 base64 与 JSON，负载 −33% 且免解析；per-page 通道无全局广播开销。代价是输出帧
不再能经事件名旁路消费（当前无此需求），调试期需从 Channel 统计日志
（`[terminalBuffer] frame stats`）而非事件名过滤。页面卸载后 Rust 发送失败自动清槽——订阅态与
通道生命周期严格绑页面，无需额外握手。

**验证**：移动端 `pnpm run test:run` 461 绿、`vue-tsc --noEmit` 0 错、根 `pnpm exec eslint .` 0 error；
Rust `cargo test --lib terminal_link` 33 绿。

**待办**：真机复测（Rust 与前端必须同版本）。
