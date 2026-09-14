# 统一 PTY 输出消费逻辑 — 架构方案（v2）

> 状态：P0–P4 全部已实现 · 2026-08-10
> 范围：桌面端 / 移动端 终端输出链路的统一抽象设计
>
> 实现进度：
> - ✅ P1：服务端契约（字节偏移 + 裁剪回放 + mode 裁决 + 本地 WS 二进制通道 `/ws/terminal/local`）
> - ✅ P2：桌面端迁移（`useTerminalOutputStream` + TerminalPreview 改造）＋残留清理（插件管线迁至 `on_output` 单点 D1、`useGlobalTerminal` / `usePtyOutput` / `ptyReplay` / invoke 历史拉取删除）
> - ✅ P3：移动端迁移（mode 裁决替换 `hasGap`/`minSeq>startSeq`、删 2MB 前端环形、文本协议新增字节偏移字段、连续违反自愈重订阅）
> - ✅ P4：清屏快照点（`\x1b[2J` 扫描，reset 从快照回放自洽帧）；本地通道短期一次性 token 加固（`get_local_ws_token` + 握手校验）
>
> v2 相对 v1 的两个核心变更（对应本轮需求）：
> 1. **桌面端输出通道改为本地 WS + 二进制帧**，直接获取 PTY 原始字节，取代 Tauri event（base64 JSON）+ invoke 历史拉取的双通道模式
> 2. **gap 补偿方案重做**：字节偏移游标 + 服务端裁决 mode + 单通道有序投递 + 帧级连续性校验。会话存活期间输出**严格连续**，中间零丢失，仅允许环形缓存上限之外的头部淘汰

## 1. 现状：为什么当前方案做不到"严格连续"

| 维度 | 桌面端（本地） | 移动端（远程） |
|------|---------------|---------------|
| 传输 | Tauri event `pty-output-{id}`（base64 JSON）+ `get_session_output_history` invoke | WS `ws_output`（base64 JSON） |
| 通道数 | **两条通道**（实时事件流 + 历史 invoke），靠 `lastReplayedIndex` 水位 + `pendingLiveEvents` 去重拼接 | 单通道，但靠客户端猜测 gap |
| 缺口处理 | 双通道竞态（快照与监听注册之间的输出可能既不在历史也不在事件流）；`broadcast` 通道 `Lagged` 时静默丢弃 | `minSeq > startSeq` → 清空全量重播（猜测式）；前端 2MB 环形 buffer 溢出置 `hasGap` |
| 游标粒度 | 事件号 index（无法表达"事件中间的断点"） | 事件号 index / end_index（同上） |

**中间丢数据的三个根因：**

1. **桌面端双通道竞态**：`PtyReader` 把同一份输出同时写入 broadcast（→ 桌面事件）与 `GlobalOutputManager`（→ 历史）。监听器注册、历史快照、实时到达三者之间没有统一序，窗口内的事件可能被两条通道同时覆盖（重复）或都不覆盖（丢失）。`pendingLiveEvents` 只兜住了"监听已注册、回放未完成"这一段，兜不住"监听尚未注册"的挂载竞态。
2. **broadcast 静默丢弃**：`FrontendOutputHandler` 消费慢时 `RecvError::Lagged` 只打日志，输出直接丢——这是真实存在的中间缺口。
3. **客户端猜测式补偿**：移动端 `minSeq > startSeq` 靠客户端推断"数据不可用"，推断错误（或推断过晚）就会产生缺口或重复；且事件号粒度无法表达"上次收到的最后一个字节在事件 N 中间"这种断点——重订阅时要么整事件重发（重复），要么从事件 N+1 开始（跳过 N 的后半段）。

**关键前提（v1 保留）**：事件 index 已是全局统一递增（`next_output_index()`），是天然的顺序坐标；但 v2 在此基础上引入**每会话字节偏移**作为游标——事件号只做顺序辅助，不再承担"续传坐标"职责。

## 2. 核心模型：字节偏移游标 + 服务端裁决

```
每个 PTY 会话 = 一条 OutputStream（单一真源）
  ├── 事件窗口（环形，只允许从头淘汰）    ← UnifiedOutputQueue 演进
  ├── 字节偏移空间：事件 N = [start_N, end_N)
  │      end_N == start_{N+1}（会话流 = 所有事件字节的连续拼接）
  └── 保留区间 [min_offset, max_offset)：环形只淘汰头部 → 保留后缀恒字节连续

消费者只做两件事：
  1. 持有一个字节游标 cursor（已渲染到的字节位置）
  2. subscribe(cursor) → 服务端裁决并回放，此后同一连接内历史与实时
     走同一条有序通道，客户端逐帧校验连续性
```

**subscribe(cursor) 的三种裁决（由服务端基于真源给出，客户端零猜测）：**

| 裁决 | 条件 | 服务端行为 | 客户端行为 |
|------|------|-----------|-----------|
| `incremental` | `min_offset <= cursor <= max_offset` | 回放 `(cursor, max_offset]`，**首个事件从头裁剪到 cursor**（字节级断点续传） | 直接渲染，无需清屏 |
| `reset` | `cursor < min_offset`（头部已被淘汰）或 `cursor > max_offset`（流已重建）或首次订阅 | 清屏，回放 `[min_offset, max_offset]` | 先 `terminal.clear()` 再渲染（头部缺失被需求允许） |
| （会话不存在） | — | 返回 `SESSION_NOT_FOUND` 错误 | 按会话生命周期处理 |

**核心原则（v1 保留并强化）：**

- 单一真源是服务端 PTY 输出流；"数据可用性"由服务端裁决（应答带 min/max + mode），消费者只做单边比对
- **历史与实时必须走同一条有序通道**：占位订阅 → 历史快照 → 排空 pending → 原子激活（`SessionOutputManager.subscribe` 现有实现 + 单测已覆盖），订阅切换时刻零丢失
- 允许丢最早（环形语义），丢的粒度由服务端窗口裁决；**中间严格连续**是硬性要求
- 游标语义 = "客户端已渲染到的字节位置"，跨重连/断线保留；屏幕状态与游标解耦，`reset` 时以清屏重建自洽帧

## 3. 服务端契约（Rust 侧一个实现，两个出口）

演进 `GlobalOutputManager` / `SessionOutputManager`，本地二进制通道与移动端 WS 共用同一订阅逻辑。

### 3.1 OutputEvent 扩展

```rust
pub struct OutputEvent {
    pub session_id: String,
    pub data: Vec<u8>,
    pub index: u64,             // 保留：全局顺序号（调试 + 移动端兼容字段）
    pub start_offset: u64,      // 新增：本事件在会话流中的起始字节偏移
    pub end_offset: u64,        // 新增：结束字节偏移（end == start + data.len()）
    pub timestamp: i64,
    pub is_waiting: bool,
}
```

偏移分配在 `SessionOutputManager.on_output` 单写者路径完成（持队列写锁，天然有序）：

```rust
let start = self.next_offset.load(Ordering::SeqCst);
let end = start + event.data.len() as u64;
event.start_offset = start;
event.end_offset = end;
self.next_offset.store(end, Ordering::SeqCst);
self.output_queue.write().await.push(event);
```

### 3.2 UnifiedOutputQueue 演进

- 淘汰最旧事件时同步推进 `min_offset`（= 新队首事件的 `start_offset`）；`max_offset` = 队尾事件的 `end_offset`
- `get_range(cursor) -> Vec<OutputEvent>`：返回 `end_offset > cursor` 的全部事件；若首个事件 `start_offset < cursor`，**裁剪其 `data` 到 `cursor` 起**并改写 `start_offset = cursor`——字节级断点续传，不重不漏
- 双重容量限制（条目数 + 总字节）保留不变

### 3.3 SubscribeResponse 扩展（字段追加，旧字段兼容）

```rust
pub struct SubscribeResponse {
    pub min_seq: u64, pub max_seq: u64, pub history_count: usize, // 保留
    pub mode: SubscribeMode,      // 新增：incremental | reset
    pub min_offset: u64,          // 新增
    pub max_offset: u64,          // 新增
}
```

### 3.4 插件输出管线迁移（决策点 D1，推荐方案）

桌面端现状：`FrontendOutputHandler.process_through_plugins` 在 **emit 到事件通道前**做插件 TerminalHandler 变换。桌面切到 WS 后该变换必须落在统一真源上，否则"历史存原始、实时发变换后"会破坏连续性契约。推荐：把变换移动到 `SessionOutputManager.on_output` 的 `queue.push` 之前——一个实现，所有出口（本地 WS / 移动端 WS / 历史回放）语义一致。影响面：移动端此后也会收到变换后的输出（语义统一，属预期变更）。

## 4. 桌面端本地 WS 二进制通道

### 4.1 传输实现选型：浏览器原生 WebSocket（已定，不用 Tauri 插件）

候选方案对比：

| 方案 | 二进制直通 | 代价 | 结论 |
|------|-----------|------|------|
| **浏览器原生 WebSocket**（WebView 内 JS 直连） | ✅ `event.data` 直接为 `Blob`/`ArrayBuffer`，零编码零中转 | 无；WebView2 / WKWebView / WebKitGTK 均原生支持，CSP 当前为 null | **采用** |
| 社区 `tauri-plugin-websocket`（Rust 端 tokio-tungstenite + IPC 转发） | ❌ Tauri v2 事件 payload 为 JSON 序列化，binary 消息只能退化为 base64（或字节数字数组），与现有事件通道形态无异 | 多一跳中转 + 一次拷贝 + 社区插件维护风险（官方插件仓库无 WS 插件） | 否决 |
| 自写 Rust 中转插件 | ❌ 同上，二进制到前端仍要 base64 / 数组；前端依旧 `listen` + 解码 | ~100 行插件 + 双端逻辑分叉 | 否决 |

否决插件的核心理由：二进制直通是本方案的硬性目标（省 33% 体积 + 免编解码 CPU），Tauri IPC 的 JSON 约束使任何插件中转都无法达成——等于绕回被否掉的"事件 + base64"形态。原生 WebSocket 唯一软肋是 WebView 网络栈受系统代理影响，缓解：WebView2 配置 `additionalBrowserArgs: ["--proxy-bypass-list=<-loopback>"]`（WKWebView 无此问题）；无需为此引入插件层。

### 4.2 端点与鉴权

- 新路由 `/ws/terminal/local`，`LocalTerminalWs` 在握手时校验 `req.peer_addr().ip().is_loopback()`（服务器绑 `0.0.0.0` 供移动端访问，本地通道必须显式限定环回），通过后**免 JWT**、直接标记 authenticated，复用 `TerminalWs` 的订阅处理（`handle_subscribe` / `handle_unsubscribe`）
- 可选加固：Tauri command `get_local_ws_token()` 签发短期一次性 token，握手后首条消息携带校验（防御"本机其他进程连本地端口"），v1 可不做，标为增强项
- 桌面端 WebView（WebView2 / WKWebView / WebKitGTK）原生支持浏览器 `WebSocket` API，无需新增 npm 依赖；`tauri.conf.json` 当前 `csp: null`，无需放宽；若将来收紧 CSP 需加 `connect-src ws://127.0.0.1:*`
- 一个连接只订阅一个会话（终端窗口 1:1），连接关闭即取消订阅（复用现有 `stopping()` 清理路径）

### 4.3 二进制帧格式（输出通道，客户端唯一数据来源）

```
偏移   长度  内容
0      2    magic 0x54 0x42 ("TB")
2      1    version = 1
3      1    flags（bit0 = is_waiting）
4      8    start_offset（u64 LE）
12     8    end_offset（u64 LE）
20     n    原始 PTY 字节 [start_offset, end_offset)
```

- 每条 WS 二进制消息 = 一个帧；服务端转发任务可把连续事件合并为一个帧（`start` 取首事件、`end` 取尾事件），减少帧数
- **连续性不变量（客户端校验）**：`frame.start_offset == cursor` 且 `frame.end_offset == cursor + payload.length`。不满足即违反不变量 → 打错误日志并按 `reset` 语义重订阅（幂等，服务端给正确答案；正常路径永不触发）
- 不使用 base64：原始字节直通，零编码开销（比 JSON+base64 省约 33% 体积 + 无解码 CPU）

### 4.4 控制消息（text JSON，复用现有 Message 协议）

- 客户端 → 服务端：`Message::Terminal(Subscribe { start_seq })` 语义不变，`start_seq` 字段复用为字节游标（`None`/`0` 视作首次订阅 → `reset` 全量）；`Unsubscribe` 同现有
- 服务端 → 客户端：`SubscribeResponse`（含 mode/min_offset/max_offset）+ `Error`，均为 text JSON，**先于**任何二进制回放帧到达（同一 TCP 连接天然有序）

### 4.5 连接生命周期

```
打开 TerminalPreview
  → invoke get_server_status 拿端口
  → new WebSocket(`ws://127.0.0.1:{port}/ws/terminal/local`)
  → onopen: 发 Subscribe(cursor)          // cursor 初始 null，之后持久
  → onmessage:
      text → subscribed{mode,...} | error
      binary → 校验连续性 → 交写入管线
  → onclose: 指数退避重连（1s/2s/5s 上限），cursor 保留 → 服务端裁剪续传
  → 组件卸载 / 会话停止删除：close，丢弃 cursor
```

## 5. 前端统一抽象：一个 composable + 两个 adapter

```
useTerminalOutputStream(sessionId, adapter)
  ├── cursor（字节偏移，内存态，跨重连保留）
  ├── onFrame(data, start, end)   // 内部：连续性校验 → rAF 合并 → DEC2026 → xterm
  ├── onReset()                   // mode=reset 时先清屏再回放
  └── adapter 接口：{ connect(cursor), onControl(cb), onFrame(cb), onClose(cb), disconnect() }

LocalBinaryAdapter   // 桌面端：/ws/terminal/local，二进制帧（v2 主线）
RemoteWsAdapter     // 移动端：ws_subscribe + ws_output（P3 迁移，兼容期字段不动）
```

### 5.1 TerminalPreview 迁移清单

**删除：**
- `usePtyOutput` 事件监听、`replayHistory()` invoke、`decodeBase64Bytes`
- `lastReplayedIndex` / `pendingLiveEvents` / `advanceWatermark` / `ptyReplay.ts`（水位去重逻辑整体消失——单通道后无重叠可言）
- `useGlobalTerminal` 5MB 本地第二份历史（服务端环形即真源；`SessionsConfigView` / `TerminalWindowView` 的 `initSessionCache` / `destroySessionCache` 调用同步移除）

**保留（渲染层，与数据层无关）：**
- rAF writeQueue / flush / 100ms 兜底、DEC 2026 包裹、`MAX_WRITE_CHUNK` 分块
- 滚动联动、`historyTruncated` toast（触发条件从 `minSeq > 0` 改为 `min_offset > 0`，i18n 文案微调）、清屏按钮（改为纯 UI 清屏，不动游标）

**新增：**
- `useTerminalOutputStream` + `LocalBinaryAdapter`（4.2 / 4.4）

### 5.2 移动端迁移（P3）

- `ws_subscribe_session` 返回值增加 mode/min_offset/max_offset（字段追加兼容）
- 删除 `hasGap` / `minSeq > startSeq` 猜测逻辑 → 按 mode 裁决：`incremental` 续传 / `reset` 清屏全量重播
- 前端 2MB 环形 buffer 移除，只留游标（内存 -2MB/会话）；`writeCoalescer` / rAF 写入管线保留
- 传输是否同步升级为二进制帧：需 Rust `ws_client` 增加 binary 接收能力 + 帧解析，属协议演进点，可与 mode 语义分批发版

## 6. 为什么这个方案保证"中间严格连续"

1. **环形只淘汰头部**：任意时刻保留区间 `[min_offset, max_offset)` 字节恒连续，服务端可回放任意 `cursor >= min_offset` 的完整后缀——不存在"中间缺了一段"的数据形态
2. **订阅切换零丢失**：占位 subscriber 在历史快照前已注册，窗口内事件要么进历史快照、要么进 pending；排空 pending（跳过与快照重叠部分）与原子激活在同一写锁内完成（现有实现 + 单测 `test_pending_covers_subscribe_gap` 等覆盖）
3. **历史与实时同通道**：快照回放与实时推送走同一条有序 mpsc，从根上消除桌面端"双通道竞态"与 broadcast `Lagged` 静默丢弃
4. **字节级断点续传**：游标落在事件中间时服务端裁剪首个回放事件到 cursor——不重复、不跳过，粒度小于任何 WS 帧/合并块
5. **客户端只守不猜**：`frame.start == cursor` 帧级校验作为不变量守护，异常即报错重订阅（幂等）；`mode` 由服务端告知，`hasGap` 类猜测逻辑全部删除

**允许的丢失（需求边界内）**：环形容量（条目数 + 总字节）之外的会话最早输出——`reset` 清屏后从 `min_offset` 起重放，用户可见头部截断（`historyTruncated` toast 提示）。

**不保证的边界**：会话关闭/销毁后不承诺（流重建 = 新坐标空间，客户端收 `SESSION_NOT_FOUND` 或 `reset` 后清屏，语义正确）。会话存活期间，从任意时刻开始订阅都能拿到从 `min_offset`（或 `cursor`）到当前的全部字节，中间无洞。

## 7. 分阶段路线图

| 阶段 | 内容 | 兼容性 | 状态 |
|------|------|--------|------|
| **P0** | 桌面端链路字节化（base64→Uint8Array）、服务器常驻、性能监控默认关闭 | — | ✅ 已完成 |
| **P1** | 服务端契约：字节偏移（OutputEvent + 队列 min_offset + 裁剪回放）、SubscribeResponse 增 mode/min/max offset、本地 WS 二进制通道（`/ws/terminal/local` 环回免 auth + 帧编码 + 合并转发） | 旧字段全兼容，移动端 WS 线上字段不动 | ✅ 已完成 |
| **P2** | 桌面端迁移：`useTerminalOutputStream` + TerminalPreview 改造；残留清理：插件管线迁至 `on_output` 单点（D1）、删 `useGlobalTerminal` / `usePtyOutput` / `ptyReplay` / invoke 历史拉取 | 桌面端内部重构 | ✅ 已完成 |
| **P3** | 移动端迁移：mode 裁决替换 `hasGap`/`minSeq>startSeq`、删 2MB 前端环形（服务端回放取代）、文本协议新增字节偏移字段（兼容默认）、连续性违反自愈重订阅 | 字段追加兼容（旧版服务端降级透传） | ✅ 已完成 |
| **P4** | 清屏快照点：服务端扫描 `\x1b[2J` 记录 offset，`reset` 时从快照回放得到完整自洽帧（全屏 TUI 语义更佳）；本地通道短期一次性 token 加固（`get_local_ws_token` command + 握手 query 校验） | 增强 | ✅ 已完成 |

## 8. 决策点

1. **游标坐标：字节偏移（已定）** —— 需求"中间严格连续"强制要求字节级断点续传与帧级连续性校验，事件号粒度无法表达事件内断点。事件号仅保留作顺序/调试/兼容字段。
2. **插件管线落点（D1）** —— 推荐迁至 `SessionOutputManager.on_output` 单点（统一真源原则）；代价是移动端也收到变换后输出。不推荐保留桌面专属变换（历史/实时不一致会破坏契约）。
3. **本地通道鉴权** —— 环回 IP 校验为必做基线；短期 token 为 P4 增强。
4. **`reset` 的清屏策略** —— 默认清屏 + 从 `min_offset` 重放（保证自洽帧，全屏 TUI 正确）；备选"不清屏、从 cursor 续"仅当能证明屏幕状态完全源于 `[0, cursor)` 时更优，作为未来优化，不在 v2 范围。
5. **本地第二份历史（5MB 全局缓存 + 2MB 移动端环形）** —— 契约成熟后整体移除（P2 / P3），期间保留兜底。

## 9. 非目标

- 不做跨消费者进度同步（本就无需同步）
- 不追求"字节级零丢失"——允许环形容量之外的头部淘汰（需求明确允许），但**中间不丢**是硬性契约
- 不保证会话关闭后的输出（流重建语义按 `reset` 处理）
- 移动端 WS 线上字段在 P3 前不动（仅新增 mode/min_offset/max_offset）
- 本地通道不做多会话复用（1 连接 1 会话，简化生命周期与鉴权模型）
