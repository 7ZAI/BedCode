# 04: 移动端控制面迁 HTTP + 旧信封退役（P3 / M3）

**What to build:** 会话起停删、会话列表/配置加载、终端输入、插件 API（`session.list` /
`terminal.sendInput`）从 WS `Message` 信封迁到桌面 HTTP 面（URL 与响应形状已由桌面保证不变）；
消费者清零后退役旧信封协议死代码。

**Blocked by:** 03 — 事件通道先行（信封退役需其消费者先从 `event_ws` 清零）

**Status:** done（2026-09-26 实施完成；集成测试在票 07 统一运行通过）

## 验收标准

- [x] 调用面迁移（spec §3.4 表）：

      | 现状（WS 信封） | 终态 | 状态 |
      | --- | --- | --- |
      | `session.rs::SessionManager::{start,stop,remove}_session` | `POST /api/sessions/start`、`POST /api/sessions/{id}/stop`、`DELETE /api/sessions/{id}/remove`（`session/http.rs`，JWT Bearer + `parse_envelope`） | ✅ |
      | `commands/session.rs::ws_load_sessions` | `GET /api/sessions`（前端已用 `httpListSessions`，命令删除） | ✅ |
      | `commands/session.rs::ws_load_session_configs` | `GET /api/configs`（前端已用 `httpListConfigs`，命令删除） | ✅ |
      | `commands/terminal.rs::ws_send_input_async` | `POST /api/sessions/{id}/input`（`{data, specialKey}`，桌面支持 specialKey 翻译）；命令删除，`useTuiCompat` / `plugin/context` 改 `httpSendSessionInput` | ✅ |
      | `plugin/context.ts::session.list` | HTTP 列表（`httpListSessions`；`session.list` 权限判定不变） | ✅ |
      | `plugin/context.ts::terminal.sendInput` | HTTP `/api/sessions/{id}/input`（`httpSendSessionInput`；`terminal.sendInput` 权限判定不变） | ✅ |
      | `commands/session.rs::get_terminal_ws_info` | 删除（旧前端直连路径无消费者；票 05 终端流 URL 由 `terminal_link` 自建） | ✅ |

- [x] 错误语义沿用桌面口径：HTTP 200 + `{code:1002,message}` → 按既有 `parse_envelope` 规则映射
      `AppError::Auth`/`Internal`（`session/http.rs::parse_ok_envelope` 对无 data 成功信封单独处理；
      单测覆盖成功/业务码/畸形三态）；resize 的 `NeedsConfirmation → force 覆盖` 路径不变
      （前端 `useTerminalResize` 零改动）。
- [x] **旧信封消费者清零**：`Message` 裁剪为 5 变体（Auth/SessionControl/Error/ServerClosed/Ack，
      `ws_protocol_integration` legacy 场景所需）；`Terminal`/`SessionConfig`/`ClientDisconnected`/
      `SessionEvent`/`SyncData` 变体与 `enums/sync.rs` 删除；`connection/request.rs` 仅留 `AuthRequest`
      （`SessionRequest`/`TerminalRequest`/`ConfigRequest`/`ResponseParser` 删除）；
      `commands/terminal.rs` 整文件删除（ws_send_input_async/ws_send_message/ws_send_and_wait/
      ws_resize_terminal 全部退役）；`handler/plugin_event.rs` 事件转发链保留。
- [x] `AuthRequest::reauthenticate*` 残留调用面删除/停用（B3）：`reauthenticate_with_crypto` 删除，
      生产路径零 `AuthRequest` 使用（仅集成测试 `connect_and_pair`）；认证 HTTP 链路 `/api/auth/*` 零改动。
- [x] 任务面旧前缀迁移（B1）：`useHttpApi.ts` 的 `com.bedcode.auto-task` → `com.bedcode.terminal-session`
      （14 处；桌面插件任务域路由注册为 `/api/plugin/com.bedcode.terminal-session/<path>`，
      无 legacy 别名，迁移后不再依赖桌面兜底）。
- [x] **结构锁**：`Message::` 信封在 WS 生产路径零使用；`rg` 兜底旧命令名（`ws_load_sessions` /
      `ws_send_input_async` / `get_terminal_ws_info`）与旧路径常量零生产命中——落锁：
      `session::http::tests::retired_envelope_command_names_have_no_production_hits`（扫全仓 src/）+
      `migrated_control_plane_has_no_envelope_usage`（session/commands/host_impl 实现段无信封引用）。
- [x] 单测：命令面返回形状、失败语义（`code!=0` → `AppError`）、`session.list`/`terminal.sendInput`
      权限判定不变、迁移后无信封残留（`session/http.rs` 12 例 + `pluginContextHttp.test.ts` 7 例 +
      既有 permission.test.ts）。

## 边界与不做

- 不改桌面 HTTP 面（URL/形状已冻结）。
- 不做终端流协议重写（票 05）：`terminal_link.rs` 与其 `WS_TERMINAL_SESSION_PATH` 常量保留（票 05 重写时退役）。
- 加密退役与 i18n 清理（票 06）。
- **本票不运行集成测试**（见 `README.md`）。

## 验证（单测）

```bash
cd bedcode-mobile/src-tauri && cargo test   # lib 347 + 集成 17/7/14/1/1 全绿
cd bedcode-mobile && pnpm run test:run      # 50 文件 474 例全绿
pnpm exec eslint .                          # 0 error
```

## 集成测试（待票 07 运行）

- 基于票 01 夹具 / 真实桌面 HTTP：会话列表、启动、停止、删除、输入（含 `specialKey`）走 HTTP 的成功与失败形状。
- 用例：`session.list` 权限判定 + HTTP 回包；`terminal.sendInput` 控制字符经 `specialKey` 翻译。
  文件与用例名写入本票 Comments（**本票只写不跑**）。

---

## Comments

### get_terminal_ws_info 去留决策（票 04）

**删除**。理由：① 前端零消费者（终端页订阅走 `terminal_link` Rust 侧链路，不经该命令）；② 票 05
重写 `terminal_link` 协议层时会自建新端点 URL（`WS_TERMINAL_SESSION_PATH` 常量随之退役），本票
保留该常量供票 05 过渡；③ 旧路径 `/ws/terminal/session/{id}` 桌面端已 404，返回旧 URL 属误导。

### 删除清单（票 04）

| 项 | 位置 | 处置 |
| --- | --- | --- |
| `ws_load_sessions` / `ws_load_session_configs` / `ws_join_session` / `get_terminal_ws_info` | `commands/session.rs` + `lib.rs` + `commands.rs` + `useMobileCommands.ts` | 命令删除 |
| `ws_send_input_async` / `ws_send_message` / `ws_send_and_wait` / `ws_resize_terminal` | `commands/terminal.rs`（整文件） | 命令删除 |
| `Message::{Terminal,SessionConfig,ClientDisconnected,SessionEvent,SyncData}` | `model/message.rs` | 变体删除（裁剪为 5） |
| `enums/sync.rs`（`SyncPayload`） | `enums/` | 文件删除 |
| `SessionConfigAction/Payload`、`TerminalAction/Payload`、`SubscribeMode`、`SessionControlAction::{SessionList,ResizeSession,JoinSession,LeaveSession,SessionChanged}` | `enums/control.rs` | 类型/变体删除 |
| `SessionRequest`/`TerminalRequest`/`ConfigRequest`/`ResponseParser`、`AuthRequest::reauthenticate_with_crypto` | `connection/request.rs` | 删除（仅留 `AuthRequest::reauthenticate` 供集成测试） |
| `timeouts::{TERMINAL_SUBSCRIBE,CONFIG,DEFAULT}` | `connection/request.rs` | 删除（`SESSION_CONTROL` 保留供 session/http.rs） |
| 前端 WS 信封 wrapper（wsLoadSessions/wsStartSession/wsStopSession/wsRemoveSession/wsSendInput/wsResizeTerminal/wsLoadSessionConfigs/wsSendMessage/wsSendAndWait/getTerminalWsInfo） | `useMobileCommands.ts` | 删除（会话/输入改 `useHttpApi`） |
| `useTuiCompat` 输入 | `useTuiCompat.ts` | `wsSendInput` → `httpSendSessionInput` |
| `plugin/context.ts` 会话 API | `context.ts` | `session.list`/`terminal.sendInput` → HTTP（权限判定不变） |
| `host_terminal_send`（WASM 插件终端输入） | `plugin/wasm_runtime/host_impl/terminal.rs` | WS → HTTP（`session/http.rs`） |
| 任务面前缀 | `useHttpApi.ts` | `com.bedcode.auto-task` → `com.bedcode.terminal-session`（14 处） |

**保留**：`Message::{Auth,SessionControl,Error,ServerClosed,Ack}` + `AuthRequest`（`ws_protocol_integration`
legacy 场景：WS 首消息 JWT 认证 / 请求-响应匹配 / token 注入断言）；`router/event.rs` 事件转发链；
`handler/{auth,system}.rs`（legacy 路由）；`WS_TERMINAL_SESSION_PATH` 常量（票 05 过渡）；
`connection/manager.rs` 的 `extract_crypto_echo`/`install_event_crypto` 死代码（票 06 加密退役一并删）。

### SessionManager 保留决策

按验收表第 1 行「`SessionManager::{start,stop,remove}_session` → HTTP」保留抽象（`session/http.rs`
直连 reqwest + JWT Bearer + `parse_envelope`，与 `AuthHttpClient` 同构），命令层 `ws_start/stop/remove_session`
继续薄包；`ws_disconnect` 的「停止活跃会话」保留（活跃会话恒 None——前端起停已走 HTTP 直连，
该块为天然 no-op，不删除以最小改动）。`send_input`/`input_tx` 无消费者删除。

### 票 07 用例候选（本票只写不跑）

- `bedcode-mobile/src-tauri/tests/session_http_flow.rs`（候选）：起本地 HTTP mock（actix，同
  `http_auth_flow`）→ `SessionManager` 起停删全链路 + 业务码 1002 → `AppError::Auth` + 非 2xx →
  `AppError::Internal` + JWT Authorization 头断言。
- `bedcode-mobile/src/__tests__/integration/connection-flow.test.ts` 扩展（候选）：`httpSendSessionInput`
  经 `useMobileConnection.sendInput` 成功/失败形状（`code!=0` → throw）。
- 真机/夹具闭环：桌面 `session-control` 事件 + HTTP 会话操作互操作（清单 §8.3 会话项）。

### 收尾备注（票 07 注意）

- `egress::tests`（`l1_desktop_target_allowed` / `plugin_declarations_registered_and_removed`）在全量
  `cargo test` 下偶发红：两用例互改全局 `policy()` 单例而无串行（同 `http_proxy_flow` 文档化的
  desktop_targets 共享态竞态）。单跑 `cargo test egress::tests` 恒绿；与本票无涉（egress 零改动），
  但票 07 全量门禁遇此红可先单跑确认。
- 实施过程中误触了整 crate `rustfmt`（经 lib.rs 递归 mod）+ 一个 `git stash` 循环事故，已全部
  恢复：16 个无关文件回退 HEAD、wasip3 专项在途 `component.rs` 恢复 CRLF（内容无损，diff 回到
  20+/8-）、4 个 stash 全部清空（含另一 agent 在途的 plugin-contract.test.ts / PluginTitleBarItems.vue
  已还原）。教训：**格式化只允许逐文件 `rustfmt <file>`，禁止经 `cargo fmt`/`lib.rs` 递归全仓**；
  **禁止对含他人在途改动的文件做 stash push/pop 循环**。
