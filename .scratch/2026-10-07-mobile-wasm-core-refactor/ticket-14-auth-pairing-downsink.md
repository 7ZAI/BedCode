# 票 14 · 认证 / 配对编排下沉（移动端）

> 状态：**阶段 A 已完成（2026-10-08）**；**阶段 B 已实施（2026-10-08，用户指令「继续实施阶段 B」；
> 实施会话 = 阶段 A 同会话，07:36 的「接手会话校订」注是对当时在途状态的中间观察，已由本节
> §9 完整记录取代）。阶段 B 门禁结果见 §9.3。**
> 阶段 A：零 ABI 变更、零 WIT 变更、零跨端协议变更、零插件文件改动、零前端文件改动；
> 退役面**零消费者**（逐条核实见 §1）。阶段 B：**mobile ABI 15→16 纯增量**（v15 = 票 12，
> 先于本票落定），编排迁 `com.bedcode.terminal-session` 插件 auth 域，凭据零过境（C4）。

## 1. spec 票 14 与本阶段的边界

spec 票 14 原文：配对流程编排（QR / 配对码 / 生物挑战 UI 流）迁插件；设备身份文件 /
JWT 持有 / `AuthHttpClient` / 生物凭证绑定留宿主引擎（安全边界 C4）；
`ws_request_pairing` / `ws_verify_pairing_code` / `ws_authenticate_with_qr` 等命令面随编排收窄。

**动手前实测结论（不凭记忆）**：

| spec 条目 | 实测结论 | 本阶段动作 |
| --- | --- | --- |
| 本地配对码编排面（`connection/pairing_service.rs` + `auth/pairing.rs` + 4 命令 + `PAIRING_CODE_DIGITS`） | WS 握手时代遗留：移动端自认证 HTTP 化（spec §4.5 六端点）起**不再是配对码颁发方**，本面**零消费者**（前端 `src/` / 插件 / `cross-end-tests` 全仓零 invoke，仅有 `lib.rs` 注册项） | **退役删除**（不是下沉——没有可迁往的插件侧对等物，见 §3） |
| `ws_request_pairing` / `ws_verify_pairing_code` / `ws_authenticate_with_qr` / `ws_authenticate_with_biometric` | 前端活跃消费者（`src/composables/useMobileCommands.ts`、`__tests__/integration/pairing-flow.test.ts`） | **不动**：见 §5 阻塞项 |
| 设备身份 / JWT / `AuthHttpClient` / 生物凭证绑定 | 安全边界 C4，留宿主 | **不动**（反向断言钉住） |
| `AuthStatus` 状态机 | 前端 `ws_get_auth_status` 在消费 | **不动**：同上 |

## 2. 改动清单（7 文件：删 2 / 改 4 / 新增 1）

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 宿主编排 | `src-tauri/src/connection/pairing_service.rs` | **删除**（`PairingService`：生成 / 持有 / 校验并单次消费 / 清码 + pending 设备登记，113 行） |
| 宿主类型 | `src-tauri/src/auth/pairing.rs` | **删除**（`PairingCode` / `PendingDevice` / `PAIRING_CODE_TTL_SECS`，141 行） |
| 宿主模块 | `src-tauri/src/auth.rs` | 去 `pub mod pairing;` 与 `pub use pairing::{PairingCode, PendingDevice};`；模块头点名退役与现存职责 |
| 宿主模块 | `src-tauri/src/connection.rs` | 去 `pub mod pairing_service;` 与 `pub use pairing_service::PairingService;` |
| 宿主命令面 | `src-tauri/src/system/commands.rs` | 删 4 个本地配对码命令 + 失效 import（`PairingCode` / `PairingService` / `Arc` / `crate::Result`）；模块头补退役说明与真入口 |
| 宿主装配 | `src-tauri/src/lib.rs` | `invoke_handler!` 注销 4 项；删 `app.manage(PairingService)` 与 `use connection::PairingService;` |
| 常量 | `src-tauri/src/system/constants/auth.rs` | 删 `PAIRING_CODE_DIGITS`（唯一消费者是已删的 `auth/pairing.rs`） |
| 防回接锁 | `src-tauri/tests/retired_mobile_local_pairing_code_face_lock.rs`（新，4 例） | ① 全 `src/**/*.rs` 零退役符号（9 needle） ② `invoke_handler!` 内零 4 个退役注册项 ③ 前端零退役命令字面量 ④ 认证引擎面（5 needle）仍在——反向断言 |
| 文档 | `bedcode-mobile/docs/code-map.md` | 连接段 `pairing_service` 行改为退役注（含拦截锁路径）；模块树 `auth/` 行改述 |
| 文档 | 本票 + `spec.md` + 双语 CHANGELOG | 票文档、spec 状态、变更条目 |

## 3. 为什么是退役而不是「下沉到插件」

1. **移动端不是配对码颁发方**。现行链路（spec §4.5）：配对码由**桌面端**生成并展示，
   移动端只经 `AuthManager::request_pairing`（HTTP，让桌面端出码）与
   `verify_pairing_code`（HTTP，提交码）参与。`PairingService` 的「本地生成 6 位码 →
   TTL 60s 过期判定 → 校验成功即消费清除」是与真实链路**无关的第二份实现**，且与桌面
   端颁发面无同步通道——留着就是无同步通道的双真源（与票 10 的三个设置命令同款病灶）。
2. **判据命中明确**：`PairingCode` / `PendingDevice` = **B1**（宿主以产品名词定义数据结构）；
   `PairingService` 的码生命周期编排 = **B2**；6 位码 + 60s TTL = **B5**（宿主替上层决定
   业务默认值）。三者都在宿主，正是本专项要清的东西。
3. **没有可迁往的插件侧对等物**：「移动端自己出码给别人扫」这一产品流已不存在，
   迁进插件等于在插件里复活一个死流程。按 spec D6 合并口径「未用到的贡献点随迁即删，
   不许先留着」，处置是删。
4. **零消费者是实测结论**（§1 表格）：删后 `invoke_handler!` 少 4 项、少一个
   `app.manage` 状态，前端行为面完全等价。

## 4. 与桌面端的差异（点名）

| 语义 | 桌面 | 移动端（本阶段后） |
| --- | --- | --- |
| 配对码颁发 | 认证中心插件 `com.bedcode.terminal-session`（`auth_service` / `PairingService` / `QrTokenManager` 在插件内） | **宿主无颁发面**：只提交桌面端下发的码 |
| 本地配对码编排 | 宿主 `utils/auth/pairing.rs` 已随认证中心下沉退役（`docs/adr/0022` 桌面批次条目） | 同向：宿主 `auth/pairing.rs` 退役（本票） |
| 认证引擎 | 宿主剩链路身份原语（ADR 0033 后无设备入场密码学） | 宿主剩设备身份文件 / JWT 持有 / `AuthHttpClient` / 生物凭证绑定（C4） |

## 5. 阶段 B：阻塞项与选项（**已解除**——三条硬阻塞随票 12 落地消解，实施记录见 §9）

`ws_request_pairing` / `ws_verify_pairing_code` / `ws_authenticate_with_qr` /
`ws_authenticate_with_biometric` 的编排下沉**本阶段未做**，三条硬阻塞：

1. **目标插件不存在**：D3 / D6 定案的连接 / 会话 / 终端 / 自动任务四域合一 app
   `com.bedcode.terminal-session`（移动版）由**票 12** 创建；工作区实测
   `bedcode-mobile/plugins/` 仍只有 `ai-chatbox` / `auto-task` / `file-transfer` 三家，
   票 12 未开工。本票若自行建 app，会与票 12 / 票 16 争夺
   `plugin.json`（权限位取并集，须单人持有）的所有权。
2. **原语面未决**：编排要进插件，插件就得触达桌面 `/api/auth/*`。两条路都不干净：
   - 复用既有 `host-http:fetch` → JWT 会落进插件，违反 C4「JWT 持有留宿主引擎」与
     AGENTS §8「认证链路只走既有 auth 模块，禁止旁路」；
   - 新增移动 `host-auth` 域 → 占 **ABI 14→15 窗口**，与票 15（host-terminal /
     terminal-hooks 退役，同一窗口）冲突，且当前有并行会话正在改 WIT / `abi.rs` /
     `permission.rs` / `component.rs`（单点文件）。
3. **命令面先于插件退役 = 功能性回退**：这四个命令有活跃前端消费者，插件未就位就注销
   等于让配对流程直接不可用；而「保留命令面」又违背本票目标。

**建议选项（待裁决）**：① 先跑票 12 建 app 骨架（推荐，顺位与原 spec 一致），票 14 阶段 B
紧随其后；② 若坚持票 14 先行，则由本票同时建 app 骨架并**点名接管**票 12 / 16 的
`plugin.json` 所有权，两票改为「往已有 app 里加域」；③ 阶段 B 与票 15 合并进同一个 ABI
窗口，一次性把 `host-auth` 新增与 `host-terminal` / `terminal-hooks` 退役做完。

## 6. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| 宿主 `cargo check --lib` | ✅ 0 error 0 warning（首轮报 `system/commands.rs` 的 `crate::Result` 失效 import，已删） |
| 新锁 4 用例 | ✅ 4 passed / 0 failed |
| **变异自检 4/4** | ✅ ① 回接 `struct PairingService` + `struct PairingCode` → 第 1 例红 ② 回接命令函数 + `invoke_handler!` 注册（可编译形态）→ 第 1 + 第 2 例红 ③ 前端 `src/composables/useMobileCommands.ts` 加回 `generate_pairing_code` 字面量 → 第 3 例红 ④ `auth/manager.rs` 的 `request_pairing` 收 `pub(crate)`（同步改调用方保证可编译）→ 第 4 例红。四次均已还原并复跑全绿 |
| 锁自身缺陷修复 | ✅ 首轮第 2 例**恒真**：块检测写的是 `line.contains("invoke_handler!")`，而实际形态是 `.invoke_handler(tauri::generate_handler![`（宏名不带 `!`），永远进不去块——变异 ② 暴露后才转红。已改判据为 `invoke_handler(` / `generate_handler![`（`retired_mobile_peer_transfer_command_face_lock` 有同款写法，另立条目提醒） |
| 宿主 `cargo test --no-fail-fast` 全量 | ✅ lib **383 passed / 6 failed**（6 个全在并行会话在途的 `egress.rs`：`always_allow_tier_lands_audit_record` / `always_ask_tier_skips_records` / `default_tier_consults_records` / `deny_record_beats_always_allow` / `plugin_records_isolated` / `purge_plugin_clears_strategy_and_records`，与票 06–10 基线**同数同款**，本票零文件涉入）+ **14 个集成目标全绿**：新锁 4、发现投影 4 / 命令面 4 / 接收编排 2 / 发送编排 2 / ws 客户端域 2、`http_auth_flow` 17、`mock_plugin_ws_fixture` 14、`http_proxy_flow` 7、`build_manifest_smoke` 1 / `plugin_storage_db_backed_lock` 1 / `session_http_flow` 1 / `terminal_stream_integration` 1 / `ws_protocol_integration` 1 |
| 前端 vitest / 根 eslint | ⚠️ **未跑**：本阶段零前端文件改动（`src/composables/useMobileCommands.ts` 的变异已还原，`git diff` 零漂移），无 i18n key 增减 |
| `cross-end-tests` | ⚠️ **未跑**：本阶段不改跨端协议（HTTP / WS / QR / 认证 wire 均未动） |
| 插件 crate 测试 / wasm32 门禁 | ⚠️ **未跑**：零插件文件改动 |
| 真机双端互连 | ⚠️ **未跑**：留票 21 全量验收 |
| `rustfmt --check`（仅新锁文件，禁整 crate fmt） | ✅ clean（首轮报 `read_to_string(...).unwrap_or_else(...)` 折行，已按 rustfmt 格式化后复跑） |
| `cargo clippy --lib --tests` | ✅ 本票文件零新增告警 |

## 7. 执行记录

- 变异备份 `.dev-logs/t14-m1-{commands,lib,frontend,manager}.bak`（工作区内，不入版本库），
  还原后 `git diff` 逐文件确认零漂移；CRLF 文件（`auth.rs` /
  `system/constants/auth.rs`）用**字节模式** Python 脚本（`.dev-logs/t14_edit.py`）改，
  规避 heredoc 吞字与行尾漂移，改完 `bytes.decode('utf-8')` 校验。
- 全量测试日志：`.dev-logs/t14-host-test-all.log`（工作区内，不入版本库）；
  收尾已确认无残留 cargo / rustc 进程。
- 并行线提醒：工作区同时存在票 11（host-websocket）的在途改动（WIT / `abi.rs` /
  `permission.rs` / `component.rs` / `host_impl/ws.rs`），本阶段**未触碰**这些文件。

## 8. 下一票

**票 13 · 会话控制客户端迁插件**（`session/http.rs` + `commands/session.rs` 迁入
`com.bedcode.terminal-session`，复用本票与票 12 落定的 `host-connection` /
`host-websocket` / `host-auth` 地基）。

---

## 9. 阶段 B 实施记录（2026-10-08）

### 9.1 裁决与形状（三选项的最终落点）

用户指令「继续实施阶段 B」；采用 **② 的变体**：app 骨架由票 12 会话先行落定（阶段 B 开工时
实测 `plugins/terminal-session/` 已有终端域 + `host-connection`（ABI v15）），本票在其上
**追加 auth 域**并接管 `host-auth` ABI 窗——**v15→16 纯增量**（非原估 14→15）。`plugin.json`
的终端域条目归票 12、auth 域条目归本票，同文件分域追加。

**原语形状（C4 的关键决策）**：对齐票 12 `jwt-auth`「token 不落插件」先例，`host-auth` 5 原语
**只返回 accepted / rejected，凭据零过境**——认证成功后 JWT 由宿主 `apply_auth_success` 落地
（global token + 凭据表）；前端 localStorage 持久化镜像经**新增宿主窄读命令
`ws_get_auth_credentials`** 直接读引擎，不经插件（凭据持有与读取面都在宿主引擎）。
流程事件（`ws_pairing_request` / `ws_pairing_verified` / `ws_paired` / `ws_auth_failed`）
由插件经 host-events 广播，**事件名与载荷与退役前逐字一致 → 前端监听零改动**。

### 9.2 改动清单（15 文件：新增 4 / 修改 11）

| 层 | 文件 | 改动 |
| --- | --- | --- |
| WIT | `packages/plugin-sdk-mobile/rust/wit/bedcode.wit` | 新增 `host-auth` 接口（5 函数 + C4 边界注释）+ world import + abi 版本史 v16 |
| SDK | `packages/plugin-sdk-mobile/rust/src/abi.rs` | `ABI_VERSION` **15→16** + v16 版本史（纯增量） |
| SDK | `src/host/auth.rs`（新） | `HostAuth` trait（签名与 WIT 一一对应）+ trait 形状锚测试 |
| SDK | `src/host/mod.rs` | `pub mod auth` + re-export + `HostApi` 聚合 trait 双处补 `HostAuth` |
| SDK | `src/wasm_host.rs` | `impl HostAuth for WasmHost`（bindgen 自由函数转发） |
| SDK | `src/permission.rs` | 权限位 `auth`：常量 + `VALID_PERMISSIONS` + `PERMISSION_API_MAP` 三处 |
| 宿主投影 | `src-tauri/src/plugin/wasm_runtime/host_impl/auth.rs`（新） | 5 原语逻辑层（权限门 fail-closed + `block_on_async` + `plugin_id` 结构化日志） |
| 宿主投影 | `host_impl.rs` / `component.rs` | 模块声明 + re-export；`impl host_auth::Host` + `add_to_linker` |
| 宿主命令面 | `src-tauri/src/commands/auth.rs` | **注销 5 编排命令**（`ws_request_pairing` / `ws_verify_pairing_code` / `ws_authenticate_with_qr` / `ws_authenticate_with_biometric` / `ws_get_auth_status`——末者零消费者）+ **新增 `ws_get_auth_credentials`** 窄读；保留 `ws_authenticate`（重启 / 重连 JWT 换新）与生物凭证绑定三命令（C4） |
| 宿主装配 | `commands.rs` / `lib.rs` | re-export 与注册同步（注销 5 + 注册 1） |
| 宿主事件 | `src-tauri/src/router/event.rs` | 退役 3 个流程事件 helper（`emit_pairing_request` / `emit_pairing_verified` / `emit_auth_failed`，发射点随编排迁插件） |
| 插件 | `plugins/terminal-session/rust/src/auth.rs`（新） | 配对 / 认证编排域：流程顺序 + 事件发射（事件名逐字一致）+ 拒绝文案 + 激活期引擎事实对账 |
| 插件 | `rust/src/commands.rs` / `lib.rs` / `plugin.json` | 分派 4 命令（`terminal-session.request-pairing` / `verify-pairing-code` / `authenticate-with-qr` / `authenticate-with-biometric`）+ `mod auth` + activate 对账日志；manifest 权限位 +`auth`、contributes.commands +4 |
| 前端 | `src/composables/useMobileCommands.ts` | 4 编排包装改走 `plugin_invoke`（返回类型不变）；受理后经 `ws_get_auth_credentials` 取凭据；`wsGetAuthStatus` + `AuthState` 类型退役（零消费者） |
| 前端 | `src/composables/model.ts` / `useMobileConnection.ts` / `DevicesView.vue` / 监听 | `AuthState` 删除；调用方与事件监听**零改动** |
| 测试 | `src/__tests__/integration/{pairing-flow,connection-flow}.test.ts` / `fixtures/auth.ts` | mock 面换 `plugin_invoke` + `ws_get_auth_credentials`；断言改插件命令形状 |
| 防回接锁 | `src-tauri/tests/retired_mobile_auth_orchestration_command_face_lock.rs`（新，4 例） | ① 宿主零退役编排符号（8 needle，含 3 事件 helper） ② `invoke_handler!` 零 5 注册项 ③ 前端零退役命令字面量 ④ 反向断言：引擎 4 方法 + 投影 2 原语 + 窄读命令 + 插件编排域 5 needle + manifest `auth` 位 |

### 9.3 门禁结果

| 门禁 | 结果 |
| --- | --- |
| 宿主 `cargo check --lib` | ✅ 0 error 0 warning |
| 两把锁（阶段 A + 阶段 B） | ✅ 8 passed / 0 failed |
| **阶段 B 变异自检 3/3** | ✅ ① 回接命令函数 + 注册（可编译形态）→ 第 1 + 2 例红 ② 前端 `useMobileConnection.ts` 加回 `'ws_verify_pairing_code'` 字面量 → 第 3 例红 ③ 移除 plugin.json `auth` 权限位 → 第 4 例红（改名投影原语的首次尝试触发编译期 E0599 先拦——与票 10 同款结论）。全部还原复跑全绿 |
| 插件 crate native `cargo test` | ✅ 40 passed / 0 failed（首轮 3 处 `HostLog` trait 未在作用域编译红——auth.rs 补 import 修复；07:36 观察会话记录的「编译红」即此中间态） |
| **wasm32 真门禁**（`pnpm run build` = SDK CLI） | ✅ componentize 完成（490,464 bytes）；2 个 warning 均为票 12 在途文件的既有 `dead_code`（`keys.rs` KeyCombo），非本票文件 |
| 前端定向 vitest | ✅ pairing-flow + connection-flow 23 passed |
| **前端全量 vitest** | ✅ **65 文件 / 680 测试全绿**（含改写的两个集成测试） |
| 根 eslint（改动前端文件） | ✅ 0 error（3 warning 均为 `__tests__` ignore-pattern 提示，不计入） |
| 宿主 `cargo test --no-fail-fast` 全量 | 见日志 `.dev-logs/t14b-host-test-all.log`（收尾核对） |
| 插件 wasm 产物 | ⚠️ 已构建至 `rust/target/`，未随包分发——真机双端互连（配对 / QR / 生物三流）留票 21 全量验收 |
| `cross-end-tests` | ⚠️ 未跑：`host-auth` 是移动端单侧新增（对桌面 `/api/auth/*` 的 HTTP 调用 wire 零变化），无跨端契约变更 |
| ABI 同步五点 | ✅ SDK `abi.rs` + WIT 版本史 + 宿主 `component.rs` 接线 + 权限位四点（SDK 常量 / VALID / API_MAP / 宿主权限门）；`plugin-component-test` 夹具 `abi.version` 仍为 14（票 12 会话在途文件，v14 产物在 v16 宿主照常加载，不阻塞） |

### 9.4 执行事故记录（并行会话碰撞，点名）

1. **lib.rs 被并行写回覆盖一次**：票 12 会话以旧上下文写 `lib.rs`（terminal 退役）时，把我已
   注销的 5 个 auth 注册项带了回来（E0433 `__cmd__*` 编译期先拦）。重新应用注销后恢复。教训
   与 MEMORY 既有判据一致：单点文件（lib.rs / WIT / abi.rs / component.rs / plugin.json）必须
   「写前重读 + 写后复查」。
2. **07:36「接手会话校订」注**：另一会话观察到本票阶段 B 在途落盘（abi.rs 注释 / host_impl /
   插件 auth 域均为本会话产出），在票文档记了中间态（插件编译红 / 门禁未复跑）。该注已被
   §9 取代；未发现第二份 phase-B 实现（工作区唯一性核实过）。
3. 观察会话指出的「ABI 窗口实际为 15→16（非原估 14→15）」正确——v15 已被票 12 占用。
