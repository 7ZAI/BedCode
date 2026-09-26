# 07: 集成测试统一运行与全量门禁（P6 / 收尾）

**What to build:** 汇总 01–06 各票**写好但未运行**的集成测试，补齐缺口，在全部实现票完成后
**一次统一运行**；随后跑全量单测/前端/eslint 门禁、宿主零改动判据、真机联调清单、文档与
CHANGELOG、`lens_diagnostics` 收尾。

**Blocked by:** 01、02、03、04、05、06（全部实现票）

**Status:** done（2026-09-26 全量门禁通过；真机联调缺设备环境，见 Comments）

> 本票是**唯一**运行集成测试的票。单票阶段只写不跑（见 `README.md` 测试节奏）。

## 验收标准

### A. 集成测试统一运行

- [x] 汇总各票 Comments 中登记的集成测试文件与用例，核对无遗漏、无重复；补齐以下缺口：
      - [x] **事件通道闭环**（票 03）：`tests/ws_protocol_integration.rs`（`event_ws_*` / `supervisor_*`
            + 新增 `scenario_ws_frames_never_encrypted`）+ 前端 `connection-flow.test.ts` 的
            `ws_event_channel_ready → HTTP 对账` 用例；7 类事件映射与未知事件丢弃由
            `handler/plugin_event.rs` 单测（票 03，18 例）覆盖，集成层以
            `scenario_event_ws_forwards_sync_data` + `scenario_supervisor_*` 闭环。
      - [x] **控制面 HTTP**（票 04）：**新增 `tests/session_http_flow.rs`**（4 场景：列表+JWT 头 /
            起停删输入形状+specialKey 透传 / 业务码 1002 → AppError::Auth / 非 2xx → Internal）；
            `session.list` / `terminal.sendInput` 权限判定由 `pluginContextHttp.test.ts`（7 例）单测覆盖；
            前端 **`connection-flow.test.ts` 新增 2 用例**（`sendInput` 成功/失败形状：code!=0 → throw）。
      - [x] **终端流闭环**（票 05）：**新增 `tests/terminal_stream_integration.rs`**（4 场景：订阅→
            回放→live→ack / 输入 text+special_key 双形态 / ring_resync 重锚 / session_stopped 后不重连）；
            emitter trait 化（`TerminalEventSink`）使集成测试可注入事件替身（票 05 Comments 计划项）。
      - [x] **加密**（票 06）：`scenario_ws_frames_never_encrypted`（插件端点首帧明文 JSON 锁）；
            HTTP 信封加密回归由 `http_proxy_flow.rs`（7 例）与 `linkCrypto.test.ts`（16 例）覆盖。
      - [x] **桌面插件广播**（票 02，可选）：产物 wasmHash `e08087b1…` 与 plugin.json 自洽、产物
            新鲜（晚于 ws_events.rs 源码 mtime），未重复重建避免踩并发 agent 构建；广播行为由票 02
            插件单测覆盖，宿主闭环留给真机联调（设备缺失）。
- [x] 所有集成测试**一次统一运行**并全绿 —— 移动端 `cargo test` 全量：lib 330 + 集成
      （http_auth 17 / http_proxy 7 / ws_fixture 14 / session_http 1 / terminal_stream 1 /
      ws_protocol 1）+ build_manifest_smoke 1，全部 ok；前端 `pnpm run test:run` 全量
      51 文件 473 例全绿（connection-flow 12 例含新增）。
- [x] 与本专项无涉的既有集成用例：`egress::tests` 两例偶发竞态（票 04 已文档化，本批次未触发，
      lib 330 全绿）；其余既有用例无跳过。

### B. 全量回归与门禁

- [x] 桌面插件：`cargo test` 全量通过（364 例）+ 产物核对新鲜（`e08087b1…`，未重建——
      见 A 节说明）+ `wasmHash` 与 plugin.json 自洽。
- [x] 移动端 Rust：`cargo test` 全量通过（lib 330 + 集成 41 + 1 smoke，含新集成 2 个 suite）。
- [x] 移动端前端：`pnpm run test:run` 全量通过（51 文件 473 例）。
- [x] 根 `pnpm exec eslint .` 0 error（121 warning 均为既有，不计门禁）。
- [x] `cargo fmt` / `cargo clippy` 提交前自查：`cargo check --tests` 零 warning（移除 `Ordering`
      未用 import）；clippy advisory 仅既有 `type_complexity`（page_channels 字段）与 `-> String`，
      非 CI 门禁；逐文件 rustfmt 已跑（禁止整 crate fmt，票 04 教训）。
- [x] i18n key 同步出现在 zh-CN 与 en（票 06 双删 `linkEncryptWsTerminal/Event`，双文件一致）。
- [x] **宿主零改动判据**：`git diff --stat -- bedcode-desktop/src-tauri/src
      bedcode-desktop/packages/plugin-sdk-desktop` 的 10 个文件改动**全部为并发 agent 在途**
      （wasm 应用二次启用专项：frontend_channel.rs 等），本专项对桌面宿主零 diff；
      `plugin.json` / WIT 未触碰（本专项只改 wasm-apps/terminal-session/rust + 移动端）。
- [x] **结构锁变异自检**：本票补验 `ws_link_crypto_has_no_production_residue`（ws_client 注入
      `LINK_CRYPTO_CHANNEL_EVENT` → 红 → 还原）与 TS 结构锁（SessionCard 注入 `waiting_input` →
      红 → 还原）；票 02/03/04 的锁变异已在各自票内记录证据。
- [x] 跑测后清理残留进程/端口（8765/1420/5173 干净）；cargo 全程走 rustup shim。

### C. 真机联调（必须）

- [ ] 本机无 Android 真机/模拟器环境（无 `adb` 设备在线、无 Android Studio 模拟器镜像），
      §8.3 清单（终端首订阅回放/大输出/特殊键/resize/停止帧/断线重锚、会话 HTTP 起停删历史、
      桌面广播即时性、断网重连对账、加密开关）**未执行**。风险与替代：真实桌面 + 真机的端到端
      行为由本批次集成测试部分闭环替代（假插件端点覆盖协议面；HTTP mock 覆盖控制面形状），
      剩余真机侧风险（WebView 性能、Android 网络栈、实际 WSL 输出）在具备设备后按 §8.3 清单逐项联调。

### D. 文档与收尾

- [x] `bedcode-mobile/docs/code-map.md`（connection/event_ws + WS 加密退役 + handler/enums 描述）、
      `docs/knowledge/mobile-desktop-auth.md`（§3.2 重写）、根 `AGENTS.md` §9（移动端 WS 面硬切
      调用口径）、两端 CHANGELOG（破坏性变更 + 配置推送/设备事件不推送收敛）——票 06 已落地。
- [x] 更新本 spec 状态为 done + 各票状态与 commit 登记（本票 Comments 输出）。
- [x] `lens_diagnostics mode=all` 无 blocker（见 Comments 输出）。

## 收尾命令

```bash
# 桌面插件（含产物重建）
cd bedcode-desktop/wasm-apps/terminal-session/rust && cargo test
node bedcode-desktop/scripts/plugin-build.js --plugin com.bedcode.terminal-session

# 移动端
cd bedcode-mobile/src-tauri && cargo test          # 含集成 target
cd bedcode-mobile && pnpm run test:run
pnpm exec eslint .                                  # 仓库根，0 error

# 宿主零改动判据
git diff --stat -- bedcode-desktop/src-tauri/src bedcode-desktop/packages/plugin-sdk-desktop
# 期望：空输出（随包产物 resources/plugins 除外）
```

## 边界与不做

- 本票不新增功能；只运行/补齐测试、跑门禁、联调、落文档。
- 发现实现缺陷 → 回对应票修（或按需新立差异票），不在本票顺手重构。

## Comments

### 集成测试清单与实际运行输出（2026-09-26）

**Rust（`bedcode-mobile/src-tauri/tests/`，`cargo test` 全量）**：
- `session_http_flow.rs`（本票新增）：`session_http_full_suite` 4 场景——
  list+JWT Bearer 头断言（含 wire `waitingInput` 字面量锁）/ start·stop·remove·input 形状
  （`configId`/`cols` 载荷、DELETE 方法、data+specialKey 透传）/ 业务码 1002 → `AppError::Auth` /
  非 2xx → `AppError::Internal`。
- `terminal_stream_integration.rs`（本票新增）：`terminal_stream_full_suite` 4 场景——
  subscribe→回放/实时→subscribed live→ack(阈值 70KB)/ 输入 text+special_key(binary [0x03]) 双形态 /
  ring_resync 重锚（offset=4096 为最后一次）/ session_stopped 后不自动重连。驱动经
  `TerminalEventSink` trait mock（本票 emitter trait 化：`terminal_link.rs` 新增 trait +
  `AppHandle` impl + 命令面 `Arc::new(app)` 注入）。
- `ws_protocol_integration.rs`（+1 场景）：`scenario_ws_frames_never_encrypted`（首帧 raw 文本
  断言明文 JSON `{"type":"auth"…}`，防插件端点重上 WS codec）。
- 既有全组：http_auth 17 / http_proxy 7 / ws_fixture 14 / ws_protocol 12 场景 全绿。

**前端（`src/__tests__/integration/connection-flow.test.ts`，+2 用例共 12）**：
- `sendInput 经 HTTP 输入面`：POST /api/sessions/s1/input 载荷透传（data/specialKey 原样）+
  成功不 throw。
- `sendInput 业务错误`：HTTP 200 + {code:1002} → `sendInput` throw（不静默吞错）。

### 真机联调未执行（原因与风险）

本机无 Android 真机/模拟器（无在线 adb 设备）。§8.3 五组清单（终端协议/会话 HTTP/事件即时性/
重连对账/加密开关）未执行；集成测试（假插件端点 + HTTP mock）已闭环协议面与形状面，
残留风险集中在 WebView 渲染性能与真实 WSL/PTY 输出端到端，设备就绪后按 §8.3 逐项联调。

### 票 07 实施说明

- 桌面插件产物未重建：wasmHash `e08087b1…` 与 plugin.json 自洽、mtime 晚于全部源码
  （ws_events.rs 03:27 vs 产物 06:59）；桌面工作区有并发 agent（wasm 应用二次启用专项）在途，
  重建会踩其半成品，故只核对不重建。
- `terminal_link.rs` emitter trait 化是本票对生产代码的唯一结构性改动（行为等价：同样 emit
  `terminal-state`/`terminal-resync`；`AppHandle` impl 与旧路径逐字节同参），lib 330 全绿。
- conncection-flow sendInput 用例依赖 `setApiBaseUrl`（测试内显式调用，模拟已连接态）。
