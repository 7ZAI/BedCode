# 票 05 — 场景 4：终端流 WS 闭环（真实 PTY 字节到达移动端）

**状态**：resolved · 2026-09-30
**类型**：task

## 落点

`cross-end-tests/tests/terminal_ws_flow.rs`（6 条契约 C-001…C-006）。

## 这是本方案最致命盲区的闭合

此前两套 mock 的夹缝正是这一段：桌面侧对面是通用 tungstenite 客户端（自己按理解
造 `input` 帧），移动侧对面是 `tests/support/mock_plugin_ws.rs` 假端点（自己按文档
造输出帧）。**「桌面插件 ring-fetch 拉到的真实 PTY 字节 → 移动端 ingest 门控 →
页面 Channel」从未被两端同时跑过。**

## 覆盖

| 契约 | 覆盖 |
|---|---|
| C-001 | 移动端 HTTP `start_session` → 桌面真实插件建会话（真 bash PTY） |
| C-002 | 移动端读到的列表含该会话且 `status=running` |
| C-003 | 链路首帧 auth + subscribe → 桌面回 `subscribed` → 移动端相位进 `live`；页面订阅态保持（否则 ingest 门控会把输出全丢） |
| C-004 | **核心**：移动端 HTTP 写入 `echo MARKER` → 真实 PTY 输出字节经 WS 到达移动端页面 Channel，且以裸字节帧形态（`frame_count > 0`） |
| C-005 | `stop_session` → 桌面真停 → 链路收到 `session_stopped` → 相位回落 `idle`，事件携带同一 sessionId（防张冠李戴的恒真断言） |
| C-006 | `remove_session` → 列表不再含该会话 |

## 关键实现细节

- **页面级 Channel 必须先于链路建立**：`ingest_output` 的门控 =
  `subscribe_ack`（已收 subscribed）**且** `frontend_subscribed`（页面在前台），
  否则字节被计入 dropped 而不推给页面。`page_subscribe` 在 `subscribe` 之前调用。
- 记录替身 `OutputRecorder` 承接 `InvokeResponseBody::Raw`（生产推给 WebView 的
  形态），非 Raw 形态也显性留痕而非静默丢弃。

## 验证

`cargo test --test terminal_ws_flow` → 1 passed（1.1s）。
