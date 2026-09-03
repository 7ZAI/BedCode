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
