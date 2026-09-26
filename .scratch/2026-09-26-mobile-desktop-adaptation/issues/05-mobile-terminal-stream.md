# 05: 移动端终端流新协议重写（P4 / M4）

**What to build:** 终端流对齐 `terminal-session` 插件新协议（订阅 / 输入 / 流控 / 重锚 / 停止帧），
前端 TB v3 帧解析与 per-frame offset 缺口模型退役，改为「本地接收字节计数 + `ring_resync` 断点」，
live 门控从 `history_end` 改为收到 `subscribed`。

**Blocked by:** 01 — 假插件端点夹具；03 — 端点常量与极简认证口径

**Status:** done（单测全绿；集成测试待票 07 统一运行）

## 验收标准

- [x] **协议层重写**（spec §3.3 表，`src-tauri/src/terminal_link.rs`）：

      | 项 | 终态（已实现） |
      | --- | --- |
      | URL | `/ws/plugin/com.bedcode.terminal-session/terminal`（`WS_PLUGIN_TERMINAL_PATH`，旧 `WS_TERMINAL_SESSION_PATH` 已删） |
      | 握手 | `{"type":"auth","token"}`（不变） |
      | 订阅 | `{"type":"subscribe","sessionId":"<id>","mode":"live"}`；重订阅（页面挂载/重连/停止后重建）回包 `subscribed` 后发 `terminal-resync` 清屏重锚 |
      | 退订 | **关闭连接**（实现择一已锁定：Close，契约测试在票 07 锁定「离开页面必关」） |
      | 输入（可打印） | `{"type":"input","data":"<UTF-8 文本>"}`（serde_json 转义） |
      | 输入（控制字符/特殊键） | **binary 帧**：`KeyCombo::parse(name)` + `to_pty_bytes()` → 原始字节（`special_key_to_pty_bytes`） |
      | 流控 ack | `{"type":"ack","offset":<本地已渲染字节数>}` text 帧（64KB 阈值 + 250ms 空闲兜底 + 半空闲轮询保留） |
      | 追加拉取 | `{"type":"poll"}`（协议保留能力，`build_poll_frame` 单测锁形状；当前关闭连接策略下无生产调用） |
      | 输出帧 | **裸字节**（本地字节计数；无 per-frame offset、无 TB v3 16B 头） |
      | 重锚 | 收 `{"type":"ring_resync","offset":N}` → 清屏（`terminal-resync` 事件）+ 本地计数基准重置 |
      | 结束 | `{"type":"session_stopped","sessionId","reason","exitCode"?}` → phase=stopped + 关闭连接（尾帧在前，帧序保证） |
      | 错误 | `{"type":"error","message"}` → 留痕 + 状态事件；`会话不存在` 类（`classify_server_error` 按消息含「会话不存在」）按退避重连，超 `MAX_SESSION_MISSING_STRIKES`(3) 停止 |
      | 历史门控 | 收 `subscribed` 即进入 live（历史与实时同一条流）；8s 超时兜底（前端 HISTORY_SETTLE_TIMEOUT_MS）保留 |
      | 历史回补 | 订阅即回放环形窗口（fresh subscribe 语义）；HTTP 回退 `terminal_get_history`（HTTP 直取，无缓存）保留为能力，前端暂未接线 |
      | 重连 | 退避保留（500ms→8s）；重连后**重新订阅**（无续传语义）；重连前曾 live → 回包后清屏重锚（防重播重叠） |

- [x] **缓存与缺口语义重构**：本地计数仅用于 ack 水位（`cursor` 统计 + `frontend_rendered` ack offset）；
      `ring_resync` 是唯一重锚信号（清屏 + 基准重置）；无字节缓存（历史由订阅回放提供）——
      缺口不再静默拼接，只经 `ring_resync` 显性表达。
- [x] **流控参数语义保留**：`ACK_BYTES_THRESHOLD`(64KB) / `ACK_MAX_IDLE_MS`(250ms) 阈值与空闲兜底
      行为不变（`should_send_ack` 回归护栏测试全量保留）。
- [x] **尾帧与停止帧顺序**：`session_stopped` 到达前必须先消费已到达的输出帧（WS 帧序 +
      receive 循环按序处理；前端停止后字节丢弃）。前端集成测试「尾帧先渲染、停止后丢弃」锁定。
- [x] **慢客户端**：插件侧发送失败只停本人（游标不前进，下轮续拉）；移动端不假设「未收到即丢失」
      ——页面未订阅期间输出由桌面环窗口保留，重订阅回放补齐（`ingest_output` 门控丢弃 + 统计）。
- [x] **批量态**：离开终端页**退订/关闭**（`markPageLeft` → `terminal_unsubscribe` 关闭连接），
      不得后台常拉；`mode:"poll"` / `{"type":"poll"}` 为协议保留能力。
- [x] **前端跟演**：
      - `src/stores/terminalBuffer.ts`：TB v3 头解析 / `startOffset/endOffset` 缺口重拼 /
        spliceHistory / 跨帧裁剪 / 双速 setMode 全部删除；改为「裸字节按序渲染 + 本地字节计数 +
        `ring_resync` 断点（清屏 + 归零）」；`subscribed`（phase=live）为渲染与输入门控。
      - `src/composables/useTerminalBuffer.ts`：跟随 store 语义（无 writeParsed 拼接；replayDone 立即；
        unsubscribeSession = 关闭链路）。
      - `src/composables/terminal/useTerminalSubscription.ts`：live 门控 = 收 `subscribed`（phase=live），
        8s 超时兜底保留；重试两路径统一收敛守卫（已订阅不再重播，防清屏重刷闪烁）。
      - `useTerminalResize` / HTTP 输入路径不变。
- [x] 单测：订阅帧（shape + mode）、输入双形态（text 转义 / binary `to_pty_bytes`，覆盖 UI 全部
      特殊键）、ack/poll 帧、error 分类（会话不存在 vs 其它）、ack 节流回归、`ring_resync` 重锚必清屏
      （前端）、`session_stopped` 顺序（前端）、重连后重订阅（Rust 结构 + 前端）、`subscribed` 门控 +
      超时兜底（前端）；结构锁 `terminal_link_has_no_legacy_protocol_residue`。
- [x] **结构锁**：`terminal_link` WS 协议实现段零 `Message::` / `from_offset` / `history_end` /
      TB 帧头常量（自检测试锁定）；`WS_TERMINAL_SESSION_PATH` / `terminal_set_mode` 全仓零生产命中。

## 边界与不做

- 不改桌面插件终端协议（`ws_terminal.rs` 是事实源，移动端对齐）。
- 不改 resize / HTTP 输入路径。
- 加密退役（票 06）；控制面迁 HTTP（票 04）。
- **本票不运行 Rust 集成测试**（见 `README.md`；前端 vitest 集成 `src/__tests__/integration/terminal-flow.test.ts` 属
  前端单测范畴，随 `pnpm run test:run` 运行）。

## 验证（单测，已实际运行全绿）

```bash
cd bedcode-mobile/src-tauri && cargo test                        # lib 329 + 集成 17/7/14/1/1 全绿
cd bedcode-mobile/src-tauri && cargo test --lib terminal_link    # 15 例
cd bedcode-mobile && pnpm run test:run                           # 51 文件 469 例全绿
cd bedcode-mobile && pnpm exec vitest run src/__tests__/stores/terminalBuffer.test.ts                # 24 例
cd bedcode-mobile && pnpm exec vitest run src/__tests__/composables/useTerminalBuffer.test.ts        # 12 例
cd bedcode-mobile && pnpm exec vitest run src/__tests__/composables/terminal/useTerminalSubscription.test.ts  # 6 例
cd bedcode-mobile && pnpm exec vitest run src/__tests__/integration/terminal-flow.test.ts            # 5 例
cd /home/binblink/project/tauriProject/BedCode && pnpm exec eslint bedcode-mobile/src  # 0 error
```

## 集成测试（待票 07 运行）

- 基于票 01 `terminal` 端点夹具（`MockPluginWsServer` + `ENDPOINT_TERMINAL`）的协议闭环。
  文件与用例名见本票 Comments（**本票只写不跑**）。

---

## Comments

### 生命周期决策（模式选择与理由）

| 决策 | 选择 | 理由 |
| --- | --- | --- |
| 退订方式 | **关闭连接**（非 `{"type":"unsubscribe"}` 帧） | spec「实现择一」；插件 `client-disconnect` 清理订阅态，关闭即停推；少一个帧状态。票 07 集成测试锁定「离开页面必关」 |
| 页面生命周期 | 进入 = fresh subscribe（回放）；离开 = 关闭连接 | 「离开终端页必须退订/关闭，不得后台常拉」；桌面环窗口（256KB 默认）保留断开期间输出，重进重播回放补齐 |
| 重订阅 = 回放 | `terminal_subscribe` 命令 = **fresh subscribe 语义**（链路已运行时发 subscribe 帧，插件游标归零重播） | 新协议 subscribe 总是从 0 重放环窗口 → 页面挂载/预加载后进入/重连统一由重播提供历史，无需本地缓存 |
| 重播重叠 | 链路已 live 时再次 subscribe / 重连前曾 live / 停止后重建 → `subscribed` 回包后发 `terminal-resync`（前端清屏 + 归零） | 重播从环头起，与已在屏内容重叠即画面错乱 |
| 停止后台订阅 | 移除「会话启动即订阅」（startSession）与「onPaired 全量重建订阅」 | 后台常拉违例；进入终端页才建连，重播覆盖历史 |
| 段2 背压（Rust→前端） | **移除**（旧 1MB 水位/补投机制整体退役） | 新协议裸字节 + 插件有界 drain（≤128KB/轮）下 WebView 可跟；旧机制为 base64 解码开销设计。真机如有洪峰再按需加 |
| 无字节缓存 | 移除 `SessionCache`/gap/`terminal_get_history` 缓存优先 | 历史由订阅回放提供；HTTP 历史仅保留为「环窗口外/无流会话」能力（见下） |
| `terminal_set_mode` | **命令 + 前端调用删除**（`LinkMode` 仅留 subscribe 帧构造） | 新协议 mode 由 subscribe 的 `mode` 字段表达；页面进出 = 关/开连接，无独立 mode 帧 |
| `terminal_get_history` | **保留**为 Rust 命令（HTTP 直取，无缓存）；前端 wrapper + `TerminalHistoryResult` 删除 | spec §3.3「历史回补 HTTP 回退」能力落点；停止会话/环窗口外数据仍可显式拉取。前端暂未接线（停止会话继续显示空终端，与现状一致），票 07 不覆盖 |

### 删除 / 退役清单（票 05）

| 项 | 位置 | 处置 |
| --- | --- | --- |
| `SessionCache`（16MB LRU + gap 登记 + `contiguous_runs`） | `terminal_link.rs` | 删除（历史由订阅回放提供） |
| TB v3：`parse_tb_frames` / `encode_data_frame` / `build_ack_frame`(二进制) / `TB_FRAME_HEADER_LEN` 等常量 | `terminal_link.rs` | 删除（输出 = 裸字节；ack = JSON 帧） |
| 段2 背压：`seg2_paused_after` / `ack_backlog_push_range` / `seg2_drain` / `SEG2_*` 水位 | `terminal_link.rs` | 删除（无缓存无补投） |
| `LinkMode` 命令面：`terminal_set_mode` / `Outbound::SetMode` / `{"type":"mode"}` 帧 | `terminal_link.rs` + `lib.rs` + `useMobileCommands.ts` + store | 命令 + 调用删除 |
| `Outbound::Subscribe{from_offset}` `from_offset` 续传 | `terminal_link.rs` | 改为 fresh subscribe（无续传语义） |
| 前端 TB v3 解析：`onChannelMessage` 16B 头 / `OutputFrame{startOffset,endOffset,isWaiting}` | `stores/terminalBuffer.ts` | 删除，改裸字节直送 |
| 缺口/重拼：`GAP_RESPLICE_COOLDOWN_MS` / `scheduleGapRetry` / `forceReplay` / `waitForLive` / `spliceHistory` / `bufferedLive` / `historyPreparing` | `stores/terminalBuffer.ts` + `useTerminalBuffer.ts` + `TerminalView` | 删除（缺口号不再误报；回放随流） |
| `markSessionRunning` 内订阅 / `startSession` 订阅 / `onPaired` 全量重建 | store + `useMobileConnection.ts` | 移除（订阅由页面驱动） |
| `markPrepared` / `consumePrepared` / `preparedSessionId` | store | 删除（预加载只预热连接；挂载必 fresh subscribe） |
| `WS_TERMINAL_SESSION_PATH` | `system/constants/connection.rs` | 删除（新 `WS_PLUGIN_TERMINAL_PATH`） |
| `TerminalHistoryResult` / `terminalGetHistory`（前端） | `useMobileCommands.ts` | 删除（Rust 命令保留） |

### 前端门控细节

- **reg 与渲染入口**：`registerRealtimeHandler` 只登记通道 + 渲染入口（不再启动历史拼接）；
  `onReplayDone` 立即触发（门控以 subscribed 为准）。挂载后的 fresh subscribe（`subscribeWithRetry`）
  由 TerminalView 驱动。
- **resync 清屏门控**：`hasRenderedContent` —— 有在屏内容才 `onClear` + 一次性 `onTruncated`
  toast；空屏（首次挂载/prepare 后进入）清屏无意义跳过，避免误报「历史不完整」。
- **subscribeSession**：不再「已订阅早退」——总是 invoke（fresh subscribe）。调用方守卫：
  `subscribeWithRetry` 已订阅即停 + 重试两路径统一收敛守卫（`scheduleSubscribeRetry` 到期先查
  subscribed，已收敛不重播）；`markSessionRunning` 只复位不订阅。
- **停止帧**：`sessionStopped` → store `sessionStopped=true` + 计数归零；之后到达字节丢弃
  （Rust 侧也已 stopped 关连接）。
- **重试两路径统一**：失败路径与「订阅在途」路径共用 `scheduleSubscribeRetry`（3s），到期先过
  connected/active/subscribed 三守卫——修复「订阅在途路径重试触发二次重播闪烁」。

### 票 07 集成测试用例（Rust `src-tauri/tests/`，拟定文件 `terminal_stream_integration.rs`）

文件与用例（**本票只写不跑**，票 07 运行）：

- `terminal_stream_subscribe_replay_live_loop`：连 mock `ENDPOINT_TERMINAL` → auth → subscribe(live)
  → 断言夹具记录 subscribe 帧（sessionId/mode）→ 注入分片二进制（回放+实时）→ 订阅回包 `subscribed`
  →按序流出；输入 text/binary 双形态记录。
- `terminal_stream_input_dual_mode`：`terminal_send_input` text（UTF-8 无转义损失）与特殊键
  （ctrl_c → binary `[0x03]`）逐帧记录顺序。
- `terminal_stream_ring_resync_clears`：注入 `ring_resync` → 断言 `terminal-resync` 事件（offset）
  + 计数归零 + 后续字节从 0 续（前端侧 vitest 已锁清屏；Rust 侧锁事件与基准）。
- `terminal_stream_session_stopped_after_tail`：先注入尾帧二进制再 `session_stopped` → 尾帧先于
  停止状态被消费（事件序）。
- `terminal_stream_reconnect_resubscribes`：断连 → 夹具 `total_accepted` 递增 → 重新 auth +
  subscribe（fresh，无续传 from_offset 键）→ 若断连前曾 live，回包后 `terminal-resync` 触发。
- `terminal_stream_error_session_missing_backoff`：注入 `error{message:会话不存在}` → 退避重连
  重订阅；超 3 次 → `session_missing` 状态并停止。
- 前端侧（vitest `terminal-flow.test.ts` 已覆盖 raw 渲染/重锚清屏/停止序/输入回传；`useTerminalSubscription`
  已覆盖 subscribed 门控 + 超时兜底）——票 07 全量回归时一并过。