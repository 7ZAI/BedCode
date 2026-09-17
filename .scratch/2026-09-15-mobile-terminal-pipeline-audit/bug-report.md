---
name: mobile-terminal-pipeline-audit
description: 移动端「PTY → 终端显示」全链路排查报告（滑动/输入/格式三类症状）
metadata:
  type: issue
  severity: P0
  status: fixed
  labels: [ready-for-human]
---

# 移动端终端全链路排查（滑动 / 输入 / 格式）

排查范围：桌面端 PTY 读取 → 会话输出队列 → 背压水位 → TB v3 帧编码 → WS 转发 →
移动端 Rust `terminal_link`（缓存/ack/重连）→ 前端 `terminalBuffer` store → 写合并器
→ xterm 渲染 → 触摸滚动。

**症状**：运行过程中 ① 无法滑动到底部显示 ② 输入无反应 ③ 输出格式出现空行间隔。

---

## 0. 结论速览

三个症状可由**同一条缺陷链**解释，触发点是一个「无状态迁移守卫的 running 广播」：

```
桌面端广播 session status=running（重复/无害重播）
  → useMobileConnection.onSyncSessionStatusChanged 无迁移守卫调用 markSessionRunning
      → store.subscribed = false（输入出口被关）
      → store.lastRenderedOffset = null（历史游标归零）
  → subscribeSession() 因 subscribed=false 再次发起 terminal_subscribe
      → Rust TerminalLinkManager::subscribe 幂等「静默 return」（链路仍在运行，不 emit 任何事件）
          → 前端永远收不到 terminal-state → subscribed 永久为 false（自愈路径不存在）
  → spliceHistory 以 from=0 拉全量历史，但页面未清屏
      → 历史被「叠加」写在已有画面上（且 minOffset>0 时起点切在半条转义序列中间）
          → 视觉表现为重复内容 / 错行 / 空行间隔
  → sendInput 因 subscribed=false 恒 false → 输入无反应
```

次要但独立的成因见 §2（TUI 手势判定、cellHeight、桌面端带洞合帧、尺寸仲裁）。

---

## 1. 全链路分段核查结论

| # | 环节 | 结论 |
| --- | --- | --- |
| 1 | 桌面 PTY 读（`pty_reader.rs`） | 字节零加工（无 CRLF 转换 / lossy / 按行拆分），无问题；`is_waiting` 恒 false（协议 WAITING 位形同废弃） |
| 2 | 桌面会话输出队列（`session_output.rs`） | offset 按会话连续分配正确；**存在订阅占位无锁窗口（重复字节）**、**超时/pending 溢出丢事件（制造字节洞）**、**unacked 记账与是否有订阅者无关** |
| 3 | 桌面背压（64KB 暂停 / 8KB 恢复） | 记账口径正确；因移动端 Rust 侧为「收帧即 ack」，实际上不会因 UI 未渲染而死锁 |
| 4 | 桌面 TB v3 编码 | 与移动端解析器布局完全一致（16 字节头），无问题 |
| 5 | 桌面 batch/realtime 转发 | **`OutputBuffer::append` 不校验事件连续性**，带洞事件被合并成一帧 → 帧声明区间与 payload 不符 |
| 6 | 移动端 Rust `terminal_link` | 帧解析、LRU、ack 节流正确；**`SessionCache::snapshot` 跨洞静默拼接**（洞被当作连续字节供给前端） |
| 7 | 前端 store `terminalBuffer` | **`subscribed` 信念无法收敛（P0）**、`ackRendered` 依赖 `lastRenderedOffset!==null`、`markSubscribed` 为死代码 |
| 8 | 前端写管线 `writeCoalescer` | 重入/flush/兜底定时器逻辑经逐行推演无丢数据问题 |
| 9 | 触摸滚动 `useTerminalScroll` | **`cellHeight===0` 时滚动完全失效**；`autoFollowScroll` 陈旧标志会吞掉一次真实滚动事件 |
| 10 | TUI 兼容 `useTuiCompat` | **DECSET 嗅探只支持单参数序列**，且只嗅 1006 不校验 1000/1002/1003 |
| 11 | 终端尺寸仲裁（`TerminalView.queueResize`） | 80x24 跳过 / 同尺寸抑制 / needsConfirmation 不应用 → PTY 与移动端网格长期不一致 |

---

## 2. 缺陷清单

### P0-1 `markSessionRunning` 无迁移守卫 + Rust 订阅幂等静默 → 输入永久失效

`useMobileConnection.ts`（实际位置 `onSyncSessionStatusChanged`，无 `old_status !== new_status` 判断）

```typescript
    if (data.new_status === 'running') {
        const bufferStore = useTerminalBufferStore()
        bufferStore.markSessionRunning(data.session_id)
    }
```

`terminalBuffer.ts`

```737:751:bedcode-mobile/src/stores/terminalBuffer.ts
  /** 标记会话恢复运行：重新订阅（Rust 重建链路） */
  function markSessionRunning(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = false
      buffer.subscribed = false
      buffer.phase = 'idle'
      buffer.subscribing = false
      buffer.lastRenderedOffset = null
```

`terminal_link.rs`

```567:581:bedcode-mobile/src-tauri/src/terminal_link.rs
        let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(existing) = links.get(&session_id) {
            if !existing.stopped.load(Ordering::SeqCst) {
                return; // 已在运行
            }
        }
```

**触发条件**：任一「重复的 status=running 广播」（会话模式切换、插件同步、列表刷新重播、桌面端重发状态等），链路本身并未停止。

**后果**：

1. `subscribed=false` 且**无法再被置回 true**；`subscribeSession` 只在 `buffer.subscribed` 为真时短路，否则调用 `terminal_subscribe`，而 Rust 侧链路存活时**静默 return，不 emit 任何 `terminal-state`**，前端只能靠状态事件置真 → 永久假。
2. `sendInput` 直接返回 false（`terminalBuffer.ts:842-852`）→ 输入无反应。
3. `subscribeWithRetry` 进入 3s 无限重试循环（`buffer.subscribing` 恒 true）。
4. `lastRenderedOffset=null` → `spliceHistory` 以 `from=0` 全量拉历史，**页面未清屏** → 历史叠加在现有画面上（`minOffset>0` 时起点还切在半条转义序列中间）→ 格式错乱/空行。
5. `markSubscribed`（store 里唯一的补置真入口）全仓无调用者 → 无自愈路径。

**修复方向**：Rust `subscribe` 幂等分支也 `emit_state("subscribed")`（或 `terminal_subscribe` 直接返回当前 phase/snapshot）；前端把 `subscribed` 由 `phase ∈ {history,live}` 派生而非独立布尔；`onSyncSessionStatusChanged` 加迁移守卫；`markSessionRunning` 不要无条件清游标（区分「链路已停止」与「仅状态重播」）。

---

### P0-2 移动端网格与 PTY 尺寸长期不一致（格式错乱的独立成因）

```1547:1563:bedcode-mobile/src/views/TerminalView.vue
async function queueResize(cols: number, rows: number, force = false) {
  if (cols <= 0 || rows <= 0) return
  if (isMockSession(sessionId.value)) return
  const sid = sessionId.value
  if (!sid) return
  if (cols === 80 && rows === 24) return
  if (!force && rejectedSize && rejectedSize.cols === cols && rejectedSize.rows === rows) return
```

- 真实网格恰为 80x24 的设备**永远不会同步尺寸**；
- 用户曾拒绝一次覆盖（`rejectedSize`）后，同尺寸后续请求被永久抑制（旋转回来也不重发）；
- `needsConfirmation` 时 PTY 保持**对方的**尺寸（`isCanonicalRenderer=false`）。

后果：TUI 应用按 `cols_desktop × rows_desktop` 做绝对定位与换行，移动端按自己的网格渲染 → 行折叠 / 定位错位 / 大片空白带（“空行间隔”）；alt-screen 无 scrollback → “无法滑动到底部”。

---

### P1-1 `cellHeight === 0` 时触摸滚动整体失效

```141:148:bedcode-mobile/src/composables/useTerminalScroll.ts
  function computeCellHeight(): number {
    if (!terminalRef.value?.element) return 0
    const viewport = terminalRef.value.element.querySelector('.xterm-viewport') as HTMLElement
    if (viewport && terminalRef.value.rows > 0) {
      return viewport.clientHeight / terminalRef.value.rows
    }
    return 0
  }
```

```355:360:bedcode-mobile/src/composables/useTerminalScroll.ts
    if (!terminalRef.value || cellHeight.value <= 0) return

    const touch = e.touches[0]
```

`cellHeight` 只在 `setupViewportScroll()`（`initTerminal` 内 `setTimeout(50)` 调一次）与 `onResize`（仅 xterm **真实 resize** 时触发）赋值。若布局未就绪时测得 0，而初始预估网格恰好等于 fit 后网格（不产生 resize），则 `cellHeight` 永久为 0 → `onTouchMove`/`onTouchEnd` 直接 return → 滚动/惯性/长按选择全部失效，且无重试兜底。

---

### P1-2 TUI 嗅探漏判 → alt-screen 下无法滚动

```78:79:bedcode-mobile/src/composables/useTuiCompat.ts
/** DECSET 模式序列匹配：ESC [ ? <数字> <h|l> */
const DECSET_QUESTION_RE = /\x1b\[\?(\d+)([hl])/g
```

- 只匹配**单参数**序列；`\x1b[?1000;1006h`（多参数合并写法）完全不匹配 → `sniffer.enabled` 恒 false → `isTuiMode` 恒 false → 手势走 `scrollToLine`，而 alt-screen 无 scrollback → “滑不动”。
- 反向漏判：只嗅 1006（SGR 编码格式）不校验 1000/1002/1003（是否启用鼠标上报）→ 应用只置 1006 或后续关掉鼠标上报时会长期停留在 TUI 模式，手势持续发被忽略的 SGR 滚轮事件，同样“滑不动”。

---

### P1-3 桌面端把「带洞事件」合并成一帧（帧区间与负载不符）

```81:89:bedcode-desktop/src-tauri/src/server/ws/terminal_ws/forward.rs
    fn append(&mut self, event: &crate::session::OutputEvent) {
        if self.data.is_empty() {
            self.start_offset = event.start_offset;
        }
        // 始终更新 end_offset 为最新事件区间末
        self.end_offset = event.end_offset();
        self.data.extend_from_slice(&event.data);
        self.last_is_waiting = event.is_waiting;
    }
```

`append` 不校验 `event.start_offset == self.end_offset`，`flush()` 却按 `start_offset + data.len()` 声明区间。一旦上游丢过事件（`session_output.rs` 的 2s 超时丢弃 / pending 溢出 / 流代数门控丢帧），缓冲区会把 `[100,110)` 与 `[130,140)` 拼成一帧 `start=100, len=20`。

消费端（移动端 `deliverFrame` 的跨帧裁剪 `subarray(overlap)` 与缺口重拼接）全部按「区间=负载」假设运算 → **静默丢字节 / 渲染错位**；移动端按 `start>cursor` 判缺口 → 触发清屏重播（画面跳动 + 空白间隔）。

另注：`session_output.rs:471-481` 的 `unacked_bytes` 记账在**没有任何订阅者**时也照常累加，超出 FIFO 容量后冻结记账。

---

### P1-4 移动端 Rust 缓存跨洞静默拼接

```190:206:bedcode-mobile/src-tauri/src/terminal_link.rs
    fn snapshot(&self, from: u64) -> (u64, u64, u64, Vec<u8>) {
        let from = from.max(self.head);
        let mut out = Vec::new();
        for entry in &self.entries {
            if entry.end <= from {
                continue;
            }
            ...
                out.extend_from_slice(&entry.data[lo..]);
```

不同 `start` 之间若有洞（上游丢帧），`snapshot` 会把洞后内容直接接在洞前内容之后返回，但 `snapshotOffset/tail` 仍是原始 offset 空间 → 前端 `lastRenderedOffset` 直接跳到 tail，**ack 释放了从未渲染的字节**，渲染内容少一段（可能切在半条 ANSI 序列中间 → 脏屏）。

---

### P2 级问题（一致性/健壮性）

1. `ackRendered` 依赖 `lastRenderedOffset !== null`（`terminalBuffer.ts:662-668`）：任何「页面在但游标为 null」的窗口期不回 ack（现由上层的收帧式 ack 兜底，语义脆弱）。
2. `markSubscribed` / `resetMissingStrikes` 为无调用者的死代码。
3. `registerRealtimeHandler` 忽略 `terminal.onWriteParsed` 的 disposable（同一 terminal 重复注册会叠加监听）。
4. `TerminalView.handleInputSubmit/handleInputExecute` 在 `!isConnected || !isSessionActive` 时**静默 no-op**；`isSessionActive` 依赖 `activeSessions`，直接进终端页时该列表可能为空 → 输入框 disabled。
5. 桌面 `handle_session_input` 的 `self.bound_session.clone().unwrap()`（`terminal_ws.rs:1017`）异常时序下 panic；`terminal_service.rs:41` 对特殊键字节走 `String::from_utf8_lossy`（当前按键集合恰好合法，但位于字节级路径上）。
6. `session_output.rs:706-711`：占位订阅者插入（写锁释放）与快照读锁之间存在无锁窗口，窗口内事件会被**同时**收进历史段与 pending → 重复字节。
7. 断线重连期间 `subscribe_ok` 与历史二进制帧分属两条 actor 邮箱入队路径，无强顺序保证；历史帧若先到会被判为 live 段推送前端（`terminal_link.rs:406-430`）。

---

## 3. 建议的验证手段

| 缺陷 | 验证 |
| --- | --- |
| P0-1 | 终端页正常收输出时人为重播一次 `SyncSessionStatusChanged(running)`，观察 `terminalBuffer state` 日志是否 `phase -> idle` 且此后不再出现 `subscribed`；再看 `sendInput: session not subscribed` warn |
| P0-2 | 日志对照前端 `send resize to PTY` 与桌面端 resize 裁决日志；确认 PTY 尺寸是否等于 `onResize` 值 |
| P1-1 | 真机打点 `cellHeight` |
| P1-2 | 抓应用启动原始字节，确认是否出现 `\x1b[?1000;1006h` 形式 |
| P1-3 | 桌面 `output ack released unacked accounting` 与移动端 `frame stats ... gap` 对账；注入丢事件后断言帧区间 |

## 4. 修复记录（2026-09-15 已实施）

| 缺陷 | 修复落点 | 手段 |
| --- | --- | --- |
| P0-1 | `terminal_link.rs::TerminalLinkManager::subscribe` | 幂等分支不再静默 return，改为 `emit_state("resubscribed")` 补发当前状态 |
| P0-1 | `terminalBuffer.ts` | 新增 `applyLinkState` / `reconcileState`（`terminal_get_state` 主动对账，只前进不回退）；`subscribeSession` 调用成功后对账；`ensureBuffer` 的 `subscribed` 由 phase 派生；`markSessionRunning` 仅在实际「已停止」时复位 |
| P0-1 | `useMobileConnection.ts` | `SyncSessionStatusChanged(running)` 加迁移守卫（仅未跟踪/已停止会话走恢复路径） |
| P0-1 | `TerminalView.vue` | 输入三分支（submit/execute/specialKey）在未连接/非活跃时显式提示；onMounted 兜底拉取 `loadActiveSessions` |
| P0-2 | `TerminalView.vue` | 新增 `gridCalibrated`（字体度量就绪标志），取代「cols===80 && rows===24 就跳过」；刷新按钮清空 `rejectedSize`（用户显式意图可重新走尺寸仲裁） |
| P1-1 | `useTerminalScroll.ts` | 新增 `ensureCellHeight()`（手势入口补算）+ `computeCellHeight` 首行行盒回退，消除 `cellHeight===0` 导致的滚动/惯性/选择全失效 |
| P1-2 | `useTuiCompat.ts` | DECSET 正则支持多参数（`ESC[?1000;1006h`）；新增上报模式（9/1000/1001/1002/1003）跟踪，观察过上报开关后以其真实开关为准（修反向漏判） |
| P1-3 | `forward.rs` | 新增 `OutputBuffer::is_contiguous_with`，带洞/重叠事件先 flush 再起批，保证帧头区间与负载一致 |
| P1-4 | `terminal_link.rs` + store | 缓存登记字节洞 `SessionCache::gaps`/`has_gap`；`terminal_get_history` 返回 `gapDetected`；前端检出后清屏 + 锚定重播 + 一次性提示 |
| P2 | 多处 | `subscribe_ok` 前二进制帧只入缓存（`subscribe_ack` 闸门）；`drain_pending(snapshot_offset)` 按快照边界去重；`handle_session_input` 去 `unwrap`；特殊键 UTF-8 显式校验；`onWriteParsed` disposable 释放；历史截断/跨洞用户提示 |

### 验证证据（本次实际运行）

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 移动端前端 | `cd bedcode-mobile && pnpm run test:run` | 50 files / **457 passed** |
| 移动端 Rust | `cd bedcode-mobile/src-tauri && cargo test` | lib **308 passed** + 各集成套件全绿（含新增 5 条字节洞用例） |
| 桌面端 Rust | `cd bedcode-desktop/src-tauri && cargo test` | lib **802 passed** + 各集成套件全绿（含新增 3 条带洞切批/去重用例） |
| Lint | 根目录 `pnpm exec eslint .` | **0 error**（119 条既有 warning，非门禁） |
| 变异自检 | 反转 `markSessionRunning` 停止判定 / 禁用带洞切批 | 两条新增用例分别失败（`expected false to be true` / `left: 1, right: 2`），已还原 |

### 遗留与说明

- `cargo fmt --check` 在两个 crate 上均有**既有**不合规（如 `terminal_link.rs` 的 `Outbound` 枚举、`forward.rs` 既有断行），非本次引入；按最小改动未做全文件重排。
- 上游丢帧（`session_output.rs` 的发送超时丢弃 / pending 溢出）本身**未消除**：本次使其不再产生「错位帧」与「静默跨洞渲染」，而是显式可观测（日志 + 用户提示 + 清屏重播）。彻底零丢帧需要重做背压模型，另开票。

