# 02: 桌面插件补广播（P1 / D1）

**What to build:** 在 `terminal-session` 插件**既有事件收口点**向 `session-control` 端点全体
客户端广播业务事件，帧壳 `{"type":"event","event":"<name>","payload":{...}}`，恢复移动端
`ws_sync_*` 事件源。新增唯一广播出口 `ws_events.rs`。

**桌面改动限缩**：仅 `wasm-apps/terminal-session/rust/src/**` 追加约 1 个广播模块 + 8 个调用点 +
产物重建。**宿主内核、WIT/SDK/ABI、权限词汇、`plugin.json`、HTTP 面零 diff**；不新增 WS 端点。

**Blocked by:** 无（本票定稿帧壳，解锁票 03）

**Status:** done（2026-09-26 实施完成，待票 07 集成运行）

## 验收标准

- [x] 新增 `rust/src/ws_events.rs`：唯一广播出口
      `pub fn broadcast_event(host: &WasmHost, event_name: &str, payload: &serde_json::Value)`；
      帧壳 `{"type":"event","event":...,"payload":...}`（`event_frame()` 唯一出处）。
- [x] **端点反查**：新增 `endpoint_id_for_path("session-control")`，经 `WasmHost.ws_list_endpoints()`
      建 path→`EndpointEntry{id, clientCount}` 缓存（与 `lib.rs::resolve_endpoint_path` 的
      id→path 单向缓存互补，**未改** `lib.rs` 既有缓存）；反查核心是纯函数
      `parse_endpoints()` / `endpoint_entry_from_listing()`（native 可测）。
      缓存在 `ws:client-connect` / `ws:client-disconnect` 时失效（`invalidate_endpoint_cache()`，
      接线 `lib.rs::on_message`），避免「缓存记 0 但客户端已连上」的永久静默。
- [x] **零客户端早退**：`clientCount == 0` 直接返回（无客户端路径零宿主调用、不计错、不打 warn）
      ——判据收口为纯函数 `broadcast_target()`。
- [x] **失败口径**：`ws_broadcast_text` 返回成功数（0 合法）；`Err`（端点缺失/权限）→
      `log_warn` 留痕（`delivery_note()` 产文本），**绝不影响业务返回值**（事件是旁路）。
      另：无端点/零客户端路径**静默返回**，连 debug 都不打。
- [x] **7 事件接入**（spec §3.1 表，接入既有收口点，不新增事件语义）：

      | 事件 | 接入位置 | 载荷（snake_case，自足，无凭据） |
      | --- | --- | --- |
      | `session:created` | `session/events.rs::publish()` | `{session:<snake_case SessionSummary>, source_device}` |
      | `session:stopped` | 同上 | `{session_id, session_name, source_device}` |
      | `session:removed` | 同上 | `{session_id, session_name, source_device}` |
      | `task:status-changed` | `task/state.rs` 三个 emit 点（interrupted / dispatched / hook 状态） | `{session_id, task_status, task_reason?, task_questions?}` |
      | `session:mode-changed` | `task/state.rs::set_auto_mode` 与 `task/scheduled.rs::handle_session_created` | `{session_id, auto_approve, auto_execute}` |
      | `task:queue-changed` | `task/queue.rs::broadcast_queue_changed()` | 现有 bus 形 `{session_id, queue_count, action, task_id, status}` |
      | `task:scheduled-changed` | `task/scheduled.rs::broadcast_scheduled_changed()` | 现有形 `{job_id, status, action}` |

- [x] `task:status-changed` 合并载荷收口为 `task/state.rs` 内私有
      `broadcast_task_status(host, session_id, status, reason, questions)`，三个点调用
      （bus 无 reason/questions、emit 无 snake_case → 单点合并，保证载荷形状唯一）；
      可选字段缺席即不出现键（空串 reason / null questions 同样缺席，不伪造空值）。
- [x] 广播是 `publish()` / `broadcast_queue_changed()` / `broadcast_scheduled_changed()` 的
      **第三通道**，载荷取 **bus 形（snake_case）**，不引入第三套键名
      （queue / scheduled 三通道共用同一 `payload` 变量，`session:mode-changed` 的 bus 与
      广播共用 `session_mode_payload()`，emit 保留桌面前端 camelCase 兼容键不动）。
- [x] **结构锁**：`ws_broadcast_text(` / `ws_broadcast_binary(` 在实现段仅允许出现在
      `ws_events.rs`（本模块实现段恰好 1 处）；调用点数钉死（session 1 / queue 1 /
      scheduled 2 / state.rs `broadcast_task_status` 1 定义 + 3 调用 + `broadcast_event` 2）；
      载荷键为 snake_case（拒绝 camelCase 混入）。
- [x] **不新增**权限位、端点、`plugin.json` 条目、SDK 常量（`plugin.json` 零 diff，
      构建期 `generateManifest` 报告「已是最新」）。
- [x] 单测：18 项全绿（`cd wasm-apps/terminal-session/rust && cargo test ws_events`；
      全量 `cargo test` 348 passed / 0 failed）——帧壳形状、端点反查（含无端点 → None、
      畸形清单不 panic）、零客户端早退、7 事件载荷形状、`task:status-changed` 含/不含
      reason、questions 两态、失败留痕不 panic、结构锁。
- [x] 产物重建并核 `wasmHash`：`node bedcode-desktop/scripts/plugin-build.js --plugin
      com.bedcode.terminal-session`（wasmHash =
      `55d3a3e60c6df89c1e7c3630ee4e3270e8dcc721b7f439daf664e26827197ce3`）。
      注：`src-tauri/resources/plugins/**` 被 `.gitignore` 忽略，产物更新不进 git。
- [x] **宿主零改动判据**（本票收尾）：本票改动文件仅
      `wasm-apps/terminal-session/rust/src/{ws_events.rs(新), lib.rs, session/events.rs,
      task/{state,queue,scheduled}.rs}`；`plugin.json` 零 diff；
      `bedcode-desktop/src-tauri/src/**` 的既有 diff 全部来自本票开工**之前**的在途改动
      （lib.rs / host.rs / api_bridge.rs / component.rs / frontend_channel.rs，本票未触碰）。

## 变异自检（2026-09-26）

| 变异 | 结果 |
| --- | --- |
| `task/queue.rs` 旁路加一次 `HostWebsocket::ws_broadcast_text` | `broadcast_outlet_is_single` 转红 ✅ |
| `task/scheduled.rs` 删掉一处 `ws_events::broadcast_event` | `broadcast_call_points_are_pinned` 转红 ✅ |
| `broadcast_target` 判据 `client_count > 0` → `>= 0` | `broadcast_target_skips_endpoint_without_clients` 转红 ✅ |
| `task_status_payload` 去掉 `reason.filter(非空)` | `task_status_payload_treats_empty_reason_and_null_questions_as_absent` 转红 ✅ |

## Comments

1. **发现但未接的第 4 个 `task:status-changed` emit 点**：`task/state.rs::create_task_from_input`
   （原 `:501` bus / `:509` emit，`taskReason = "User submitted input"`，由
   `handle_submitted_input` 触发——**含移动端经 HTTP `/api/sessions/input` 提交的输入**）。
   本票按验收标准「state 4（3 状态 + 1 模式）」的钉数**未接**，故移动端自己提交输入产生的
   `in_progress` 不会经广播回推（移动端只能靠重连对账或 HTTP 回包感知）。
   是否补接（点数 4→5）请裁决；补接只需在该点加一行 `broadcast_task_status(...)` 并把
   结构锁的 `broadcast_task_status(` 计数改为 5。
2. **WIT 现状说明**：收尾 `git diff` 中 `packages/plugin-sdk-desktop/rust/wit/bedcode.wit`
   出现 65 行 diff（ABI v31 / v27-v28 文档记账更新），经核**不是本票写入**（构建链无写 WIT
   的脚本，文件 mtime 与本票构建不同源），属并行线的在途改动；本票对 WIT / SDK / 宿主零写入。
3. **集成测试（待票 07 运行）**——本票未写、未跑，候选：
   - `bedcode-desktop/src-tauri/tests/pty_session_chain.rs::plugin_event_reaches_session_control_client`
     （client-connect + auth 后收到 `{"type":"event","event":"session:created",...}`）；
   - `bedcode-desktop/src-tauri/tests/pty_session_chain.rs::plugin_event_is_silent_without_clients`
     （无客户端时创建会话不报错、无帧）。

## 边界与不做

- 不接移动端不消费的事件：配置增删改、`device:connected/disconnected`、`session:restarted`、
  `task:preset-changed`（spec §2.2）。
- 不做事件重放/断点补投；不做发送端过滤（`source_device` 已下发，消费端幂等）。
- 不新增 WS 端点、不改对等网络面。
- **本票不运行集成测试**（见 `README.md`）。

## 验证（单测）

```bash
cd bedcode-desktop/wasm-apps/terminal-session/rust && cargo test
# 过滤示例
cd bedcode-desktop/wasm-apps/terminal-session/rust && cargo test ws_events
node bedcode-desktop/scripts/plugin-build.js --plugin com.bedcode.terminal-session
git diff --stat -- bedcode-desktop/src-tauri/src bedcode-desktop/packages/plugin-sdk-desktop   # 期望空
```

## 集成测试（待票 07 运行）

- 可选：在既有桌面插件端点夹具上扩一条推送断言——`client-connect` 后收到事件、无客户端时静默
  （宿主零改动，优先扩 `pty_session_chain` 类夹具）。文件与用例名写入本票 Comments。
