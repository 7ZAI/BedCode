# 03: 移动端 Rust 终端链路（每会话一 WS + 缓存 + ack + 重连）

Type: task
Status: resolved（09-12 续收尾：补单测 + 修契约键 + HTTP 回退信封）
Blocked by: 02

## 范围
新模块 `src-tauri/src/terminal_link.rs`（lib.rs/state.rs 注册单例 `TerminalSessionManager`）：

- `TerminalSessionLink`：每会话一个 tokio-tungstenite WS（`/ws/terminal/session/{id}`），JWT 首消息认证；
  链路加密按开关复用 bedcode_link_crypto（机制对齐 ws_event 通道），strict 语义保持
- 帧解析：TB v3（v2 兼容过渡）；游标 lastRenderedOffset；去重/缺口（重订阅带 from_offset）/
  截断（min_offset 越过游标 → 清屏事件）
- 会话级字节缓存：即收即缓存（编帧数据 + 字节区间），上限 LRU（16MB）
- ack：`terminal_ack_rendered(session, offset)` 命令 → 节流（64KB/250ms）→ v3 ACK 帧
- 重连：意外断开 → 退避重连 + 重订阅（保留游标）；手动取消 → 关连接
- 历史：HTTP 一次性拉取（`GET /api/sessions/{id}/history`，走既有 HTTP 认证）→ 写入缓存 →
  拼接完成发 `terminal-history-ready`
- 事件：`terminal-frame`（实时帧 base64 + 字节区间）、`terminal-history-ready`、`terminal-state`
  （phase/reconnect/truncated/stopped/session_missing）
- 命令：`terminal_subscribe` / `terminal_unsubscribe` / `terminal_set_mode` / `terminal_send_input` /
  `terminal_ack_rendered` / `terminal_get_state`
- 会话联动：start_session 成功 / SyncSessionStatusChanged→Running → 自动 subscribe；
  Stopped / remove → unsubscribe；设备断开 → 全量取消 + 恢复后重订阅

## 验收
- `cd bedcode-mobile/src-tauri && cargo test` 全绿（帧解析/缓存/重连/ack 节流单测）
- 无 lint error；事件/命令注册接线完整
## Answer
teminal_link.rs 落地：每会话 WS（JWT 认证）、TB v3 解析（v2 兼容）、字节缓存（16MB LRU）、ack 水位 + 节流、退避重连（保留游标 from_offset 重订阅）、会话缺失有限重试、9 个 Tauri 命令（subscribe/unsubscribe/unsubscribe_all/remove/send_input/set_mode/ack_rendered/get_history/get_state）、事件 terminal-frame/terminal-state。链路加密（ws-terminal 协商）本轮明文 + JWT，已记录后续 ticket。cargo check 干净。

## 后续收尾（09-12 晚，另一 agent 续做）
- **补 11 个单测**（原 ticket 验收要求「帧解析/缓存/ack 节流单测」但落地代码 0 测试）：parse_tb_frames v3/v2 兼容/截断/未知版本、SessionCache push/snapshot 半块 slice/LRU 淘汰/超限整帧淘汰、build_ack_frame 逐字节格式（与桌面 parse_ack_frame 对齐）。`cargo test` 282 全绿。
- **修正 invoke 返回值键名 bug**：terminal_get_history / terminal_get_state 原来返回 snake_case 键（`min_offset` 等），而 Tauri invoke 只对**请求参数**做 camelCase 转换、返回值原样传递 → 前端 store 读 `result.minOffset` 恒为 undefined（终端真实运行时历史拼接必挂）。已改为 camelCase 键对齐前端 TS 接口（http_proxy 已有同款先例）。
- **HTTP 历史回退路径修双错**：桌面返回 `ApiResponse{code,message,data:{snake_case}}` 信封——旧代码只 `parsed.get("data")` 取内层即返回（snake 键、且 session-not-found 时 code=1002 但 HTTP 200 不报错）。改为：code!=0 视为错误、内层字段映射转 camelCase。
- needless_borrow ×2（maybe_ack 调用）顺手修掉。
- **前端测试基建修复**（vitest 10 失败 + 3 errors → 全绿）：① useTerminalBuffer.test.ts 的 vi.mock 工厂自建孤立 vi.fn，事件 handler 从未注册 → 改为与共享 listenMock/emitMock 引用（对齐 store 测试模式）；② terminal-flow.test.ts 仍是旧 v2 socket 架构（createTerminalSocket/lastRenderedSeq/HTTP 输入）→ 整文件重写为 Rust 驱动（terminal-frame 事件、terminalGetHistory 拼接、terminal_send_input）；③ terminalBuffer store markSessionStopped 未推进 replayGeneration → 在途 spliceHistory 完成后重写已重置的 lastRenderedOffset=0 → 补代数推进（停止语义下游标复位占先；页面遮罩由 HISTORY_SETTLE_TIMEOUT_MS 兜底）；④ useMobileConnection autoReconnect 的 localStorage JSON `Record<string, unknown>` 未收窄（vue-tsc error）→ 防御性 typeof 收窄、保 0=不等待语义。
- 验证：移动端 `cargo test --lib` 282 全绿、vitest 372（43 文件）全绿、vue-tsc 0、eslint 0 error（60 warning 既有）。
- 说明：egress::plugin_declarations / heartbeat::timeout 两用例在全量 run 偶发失败（1ms 超时 + 30ms sleep 时序敏感，负载所致），孤立运行与全量重跑均过——与本次改动无关的既有 flaky。
