# 06: WS 加密退役与清理（P5 / M5+M6）

**What to build:** 退役移动端 WS 帧级链路加密（桌面 `TrafficChannel::WsPlugin => false`，
插件端点帧永不加解密），保留 HTTP 信封加密；清理任务面旧前缀、文档口径与 `waiting_input`
字面量漂移。

**Blocked by:** 03、05 — 两条插件端点连接（事件 / 终端）重写落地后，其加密上装才能安全移除

**Status:** done（2026-09-26 实施完成，单测全绿；集成测试待票 07 统一运行）

## 验收标准

- [x] **WS 子开关退役**：`useLinkEncryption.ts` 删除 `encryptWsTerminal` / `encryptWsEvent` 字段与
      `ws-terminal` / `ws-event` 通道（`LinkCryptoChannel` 收窄为 `'http'`）；设置页 `linkChannelRows`
      只留 HTTP 行；i18n 双文件删除 `linkEncryptWsTerminal` / `linkEncryptWsEvent`（实现：直接删除，Comments 记录）。
- [x] `src/services/linkCrypto.ts` **保留 HTTP 通道实现与测试**（16 例全绿）；`install_event_crypto` 随
      事件连接重构删除（manager.rs 死代码段一并删 `extract_crypto_echo` / `EVENT_WS_AUTH_TIMEOUT_MS`；
      票 03 已去掉协商，本票确认零残留）。
- [x] HTTP 信封加密（`LinkEncryption` 的 `http` 子开关）行为不变：`encryptHttp` 保留、`setChannel('http')`
      保留、`isChannelEncryptionActive('http')` 判定不变；Rust `is_http_encryption_active` 保留。
- [x] **行为/结构锁**：`event_ws` / `terminal_link` 两插件端点连接零 crypto 引用；`isChannelEncryptionActive`
      WS 通道生产零调用（既有）；`WsClient` 加解密管道物理删除（`link_crypto` 字段 / `install_link_crypto` /
      `LINK_CRYPTO_CHANNEL_EVENT` / receive·send 双向 4 处分支全删）；Rust grep 锁
      `ws_link_crypto_has_no_production_residue` + TS 结构锁（票 06 描述组），均变异自检通过。
- [x] **任务面前缀**（B1 收尾）：票 04 已完成 `useHttpApi.ts` 14 处 → `com.bedcode.terminal-session`，
      本票核对零残留（仅一处历史中文注释提及 auto-task，URL 全为新前缀）。
- [x] **会话状态字面量核对**：统一到桌面 wire `waitingInput`——`useMobileConnection.ts` 3 处（316/891/919）+
      `SessionsView.vue` 1 处 + `SessionCard.vue` 5 处；`waiting_input` 零残留 + 用例锁（TS 结构锁断言
      三文件含 `waitingInput` 且不含 `'waiting_input'`）。任务状态面（`task_status` = snake_case，如
      `waiting_input` / `in_progress`）不受影响、保持正确。
- [x] **i18n**：加密开关文案退役（zh-CN / en 双删 key，命名跟随既有分组）；无残留引用。
- [x] **文档跟演**：`bedcode-mobile/docs/code-map.md`（connection 描述补 event_ws / WS 加密退役、handler
      /enums 描述修正、链路加密行更新）；`docs/knowledge/mobile-desktop-auth.md` §3.2 重写（插件端点
      双通道 + 极简认证 + 不重放对账 + 加密退役）；根 `AGENTS.md` §9 补「移动端 WS 面硬切后的调用口径」；
      两端 CHANGELOG 登记破坏性变更与收敛。
- [x] 单测：`useLinkEncryption.test.ts` 12 例（含新结构锁组）绿、`linkCrypto.test.ts` 16 例绿、
      `useMobileConnection.test.ts` 文件不存在（对应验证命令为过时路径；会话行为由 integration 覆盖，票 07 跑）、
      Rust `session::http` 13 例（含新结构锁）绿、`ws_client` 4 例绿；eslint 0 error（改动文件）。

## 边界与不做

- 不退役 HTTP 信封加密。
- 不改桌面端任何加密实现（`WsPlugin => false` 已是终态）。
- **本票不运行集成测试**（见 `README.md`）。

## 验证（单测）

```bash
cd bedcode-mobile && pnpm exec vitest run src/__tests__/useLinkEncryption.test.ts
cd bedcode-mobile && pnpm exec vitest run src/__tests__/linkCrypto.test.ts
cd bedcode-mobile && pnpm exec vitest run src/__tests__/useMobileConnection.test.ts
pnpm exec eslint .    # 仓库根，0 error
```

## 集成测试（待票 07 运行）

- 加密开启/关闭下插件端点连接无加密异常；HTTP 信封加密回归（strict/pinning）。
  登记用例：
- `bedcode-mobile/src-tauri/tests/ws_protocol_integration.rs`：既有 `scenario_*` 全组保持绿
  （本票未动 legacy 场景；删除 `LINK_CRYPTO_CHANNEL_EVENT` 后 `ws_client` 收/发帧路径变了，
  场景 `http_auth` / legacy 认证首帧不涉加密，预期无回归）。可选新增用例
  `scenario_ws_frames_never_encrypted`：建连后断言首帧为裸 JSON（`{"type":"auth"…}`）而非加密信封。
- `bedcode-mobile/src/__tests__/integration/`：HTTP 信封加密开关切换（strict 语义、pinning）经
  `useLinkEncryption` 状态 → Rust `is_http_encryption_active` 的回归（候选 `connection-flow.test.ts` 扩展）。

## Comments

### 实现选择：WS 子开关**直接删除**（非置灰）

理由：桌面 `TrafficChannel::WsPlugin => false` 已是终态，插件端点帧永不加解密；保留灰开关会以
「还能开」误导用户，且 `set_link_crypto_context` 的 ws 参数已无接收语义。删除面：
`useLinkEncryption.ts`（字段/通道/类型/推送参数）、`ConnectionSettingsView.vue`（linkChannelRows 两行）、
i18n 双文件（两 key）、Rust `LinkCryptoContext.encrypt_ws_event`、`is_event_encryption_active`、
命令 `encrypt_ws_event` 参数、`http_proxy_flow.rs` 构造点、`EVENT_WS_AUTH_TIMEOUT_MS` +
`extract_crypto_echo` + `install_event_crypto`（manager.rs）、`WsClient` 加解密管道（字段 1 + 方法 2 +
收发 4 分支 + import）、`LINK_CRYPTO_CHANNEL_EVENT` 常量。

保留：`linkCrypto.ts` WS 通道实现与 `linkCrypto.test.ts`（库自由契约，独立于上层退役）、
`ClientWsCrypto` crate 定义（HTTP 信封加密共用 `bedcode-link-crypto`；http_proxy 零改动）。

### 结构锁（变异自检通过）

- Rust `session::http::tests::ws_link_crypto_has_no_production_residue`：扫 src/ 下 .rs 禁 8 个 WS 加密
  符号；变异（ws_client.rs 注入 `LINK_CRYPTO_CHANNEL_EVENT` 字符串常量）→ 转红 → 还原回绿。
- TS 结构锁（`useLinkEncryption.test.ts`「结构锁」describe）：useLinkEncryption / 设置页 / i18n 双文件
  无 ws 字段与通道字面量；useMobileConnection / SessionsView / SessionCard 无 `'waiting_input'` 且含
  `waitingInput`（变异：把 SessionCard 一处改回 snake_case 即红）。

### 与会话状态字面量相关的既有正确面（勿误改）

任务状态（`task_status`）wire 是 snake_case（`waiting_input` / `in_progress` / `asking`）——
`SessionCard.taskStatusLabel` 与 `handler/plugin_event.rs` 的 task_status 断言保持 snake_case；
本次只统一**会话状态**（`session.status`）为桌面 wire `waitingInput`。

### 遗留噪音（与改动无关）

`useLinkEncryption.test.ts` pin 落地组跑测时出现 `[LinkEncryption] sync to native failed (non-fatal)`
stderr：`persist()` 内 fire-and-forget 的 `syncLinkCryptoContextToNative` 在 mock 未设置 `mockResolvedValue`
的测试里异步触发，vitest worker 对动态 import 的 mock 竞态所致；non-fatal 且断言全绿，改动前同款。
