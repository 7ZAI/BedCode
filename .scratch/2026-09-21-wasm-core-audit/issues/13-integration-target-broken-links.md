# 13: 五个集成测试 target 的退役符号断链清理

**What to build:** `bedcode-desktop/src-tauri/tests/` 下五个集成 target 恢复到能编译并跑绿，
或在其语义已被 in-tree 用例覆盖时**显式删除并记明被谁覆盖**。目标不是「凑绿」，而是让
AGENTS §10 要求的 `cargo test`（全 target）重新成为一个真实门禁——现在它编译不过，
任何人只看 `cargo test --lib` 就得到半瞎的绿灯。

**Blocked by:** 无（纯测试面，不改生产代码）

**Status:** in-progress（2026-09-22：4/5 target 绿；`http_auth_biometric` 卡在一条**真实生产漂移**，
待用户裁决——见「实施记录」§3）

## 现状（2026-09-22 逐 target 实测：`cargo check --test <name>`）

| target | 行数 / 行尾 | 编译错误 | 断链符号 |
| --- | --- | --- | --- |
| `ws_session_route` | 342 / CRLF | 3 | `server::services::pairing_service`、`AppContextBuilder::pairing_service`、`utils::auth::QrTokenManager` |
| `pty_session_chain` | 665 / **LF** | 5 | 上三条 + `SessionManager::from_database`、`SessionManager::restart_session` |
| `ws_auth_rules` | 482 / CRLF | 4 | `pairing_service`（模块 + builder）、`QrTokenManager`、`SessionManager::from_database` |
| `http_auth_biometric` | 552 / CRLF | 4 | 同上（`pairing` 引用最密：37 处） |
| `broadcast_shutdown` | 680 / CRLF | 3 | `pairing_service`（模块 + builder）、`QrTokenManager` |

合计 2721 行测试代码不可编译。`git grep <符号> -- src-tauri/src` 全空 ⇒ 符号确实不存在。

## 断链来源（两批退役，非测试本身的问题）

1. **配对 / QR 的宿主降级实现整体退役**（2026-09-21）：`utils/auth/auth_center.rs` 的配对 / QR 桥接、
   `PairingService`、`QrTokenManager`、`utils/auth/pairing.rs` 与 `AppContext` 装配链全部删除
   （AGENTS §8「配对 / QR 的宿主降级实现已整体退役」）。这些 target 还在按「宿主能签配对码」写测试。
2. **host-session v21 收敛**：`create` / `restart` 从内核删除，创建与重启编排归插件
   （`restart_session`、`SessionManager::from_database` 随批退役）。

现役替代路径（改写时照这个走，**不要**在宿主侧找回归符号）：配对码 / QR 的编排与签发在
`plugins/terminal-session/rust/src/pairing/`，密钥托管与记录面留宿主（`host-auth` secret-store、
`pairings` / `connection_history` 表）；会话创建经 `com.bedcode.terminal-session` 的
`session-create` 编排（`utils/session_create_bridge.rs`，插件必需、无宿主降级）。

## 验收

- [x] `cargo check --lib --tests` 零错误（实测 0 error）；`[ ]` `cargo test`（全 target，非 `--lib`）跑绿**未达成**：
      4/5 绿（`ws_session_route` / `pty_session_chain` / `ws_auth_rules` / `broadcast_shutdown`），
      `http_auth_biometric` 卡在真实漂移（见实施记录 §3），待用户裁决
- [x] 逐 target 先做**归属判断**并写在票面 Comments：
      ① 语义已被 in-tree 用例覆盖（如 `src/plugin/manager/wasm_runtime/tests/session_e2e.rs`
      的会话闭环、`host_impl/tests/pty.rs` 的属主矩阵）→ 允许删除，但必须点名「被哪条用例覆盖」；
      ② 覆盖不到（HTTP/WS 认证规则面、广播关停时序）→ 改写为经插件路径驱动；
      禁止「注释掉断言」式修复（五个 target 全判 ② 并保留，见实施记录 §1）
- [x] 认证类 target（`ws_auth_rules` / `http_auth_biometric`）改写后必须保住原命题：
      未认证 / 错 token / 过期 / 首消息超时一律拒（AGENTS §8：WS/HTTP 接入必须过认证与过滤链），
      不得因为配对签发换人就削弱断言强度（四条 gate 与 T1–T10 逐条保留；T3 转红正是**没有**
      削弱——它抓住了一个真实漂移）
- [x] 不碰生产代码：若发现断链暴露了真实的功能缺口（例如某条认证规则在插件退役后**确实**无人测），
      停下来向用户报告，不在本票顺手补实现（本票已停下报告，见实施记录 §3）
- [x] 行尾纪律：`ws_session_route` / `ws_auth_rules` / `http_auth_biometric` / `broadcast_shutdown`
      是 CRLF，**只有 `pty_session_chain` 是 LF**。改写用 Edit 工具；必须用脚本时
      `open(..., newline='')` + 显式 `\r\n`，改完核 `git diff --ignore-cr-at-eol`
      只剩目标行、CR 计数 == 行数。整 crate `cargo fmt` 禁止
      （实测：4 个 CRLF 文件改后 `CR 行数 == 总行数`；`pty_session_chain` CR=0）

## 门禁跑法（本票专用）

```bash
cd bedcode-desktop/src-tauri
for t in ws_session_route pty_session_chain ws_auth_rules http_auth_biometric broadcast_shutdown; do
  cargo check --test "$t"   # 逐个收敛，别等全量
done
cargo test                  # 全 target；跑前按 AGENTS §3 重出插件产物，核 [skip] 计数为 0
```

## 为什么值得单独一票（登记出处）

同一事实在四处只作为「顺带登记」出现，没有一票接手：
`issues/01` §既有红、`issues/02` 末条、`issues/04`（本票面 L115 附近）、
`handoff-2026-09-22.md` §6，以及根 `CHANGELOG.md`「Tests & Quality」第一条
（原文：*Cleaning them up is a separate item, deliberately not folded into an audit ticket*）。
用户 2026-09-22 定档：单独立票。

## 实施记录（2026-09-22）

**状态：4/5 target 恢复绿；`http_auth_biometric` 卡在一条真实生产漂移（见 §3），待用户裁决。**
`cargo check --lib --tests` 已 0 error；本轮未改任何生产代码（`git status` 里 `src/**` 三条
`pty.rs` / `pty/pty_process.rs` / `session/session_manager.rs` 全是**对侧在途**）。

### 1. 逐 target 归属判断（验收①）

| target | 判断 | 落地方式 |
| --- | --- | --- |
| `ws_session_route` | ② 覆盖不到：`/ws/terminal/session/{id}` 的认证 + 订阅 + TB v3 帧 + `session_stopped` 只有它端到端 | 保留并修复：只摘已退役符号（`PairingService` / `QrTokenManager` / builder 两方法）。它本就用宿主 `JwtService` 自签令牌，**无配对依赖** |
| `pty_session_chain` | ② 覆盖不到：真实 PTY 端到端 + 未认证拒绝 + 重启后输出管理器重注册回归 | 改写：配对夹具改经插件驱动；会话创建/重启改**内核执行端**（受限项见 §2） |
| `ws_auth_rules` | ② 覆盖不到：首消息认证四条 gate 只有它端到端（`conn.rs::authenticate_jwt` 与 10s 超时无 lib 单测，`server/ws/channel/*.rs` 只有纯函数契约） | 改写：`/api/auth/*` 改经真实插件驱动；四条断言原样保留 |
| `http_auth_biometric` | ② 覆盖不到：`biometric-challenge/verify/bind` 端到端唯一（插件 `auth_http/mod.rs` 无测试） | 改写：改经真实插件驱动；T1–T10 十步断言原样保留 → **T3 一条红**（§3） |
| `broadcast_shutdown` | ② 覆盖不到：广播排除发送者 / 断链清理 / 停机后拒连 / 无 ERROR 日志（`websocket_manager.rs`、`supervisor.rs` 均无单测） | 改写：配对令牌改经真实插件驱动；场景 1–4 断言原样保留 |

「上游覆盖」清单（对照用，不替代本 target）：TB v3 帧编码 `server/ws/terminal_ws/forward.rs`（3 例）、
控制帧 `control_frame.rs`、订阅者 `subscriber.rs`（约 14 例）、认证分派纯函数
`server/ws/channel/terminal.rs`、广播目标选择 `ws/registry.rs`；会话闭环 / PTY 原语 / 插件 WS 端点
在 `plugin/manager/wasm_runtime/tests/{session_e2e,pty_e2e,ws_e2e}.rs`（lib 单测，`tests/` 无法复用其脚手架）。

### 2. 一处受限：会话创建在集成测试内无法经插件（已按同入口等价改写）

`StartSession` 的编排（配置真源 → launch spec → 命名唯一化）v21 起在会话中心插件，读的是**插件私有库**；
`tests/` 是独立 crate，`WasmHostContext.plugin_db_root` 是**私有字段**（仅 crate 内 `#[cfg(test)]` 可注入）
→ 无头上下文 `get_or_create_plugin_db` 必然报 `plugin database unavailable (no app_handle)`，
插件编排在集成测试里无法驱动。故 `pty_session_chain` 的创建 / 重启改为直接驱动**内核执行端**
`SessionManager::create_session_from_spec`（= 插件算出 spec 后经 host-session `create-with-spec` 到达的同一入口），
restart 按插件同一步骤 `remove_session_with_source` + 同 id 重建；WS 控制通道仍覆盖
`ListSessions` / `StopSession` / `RemoveSession` 与未认证拒绝（内核路径）。**不是功能缺口**：
插件侧编排有 lib 用例（`session_e2e.rs`，含私有库注入），见 §1 表末。

### 3. 阻塞项：连接历史「认证方式 / 结果」大小写漂移（真实缺陷，本票未修）

现象：`http_auth_biometric` T3 断言 `auth_method == "biometric" && result == "success"` 失败；
实测库里存的是 `method="BIOMETRIC" result="SUCCESS"`。

证据链：

- 内核取值真源：`src-tauri/src/db/models.rs` 的 `connection_method::{PAIRING_CODE,QR,BIOMETRIC,JWT}` 与
  `connection_result::{SUCCESS,FAILED}` **全为小写**，`ConnectionHistory` 文档注释写明取值集合。
- 写入面：`plugins/terminal-session/rust/src/auth_http/mod.rs` 共 9 处字面量**全大写**
  （行 200 / 230 / 259 / 279 / 302 / 326 / 358 / 363，`"PAIRING_CODE"` / `"QR"` / `"BIOMETRIC"` / `"JWT"` ×
  `"SUCCESS"` / `"FAILED"`）；宿主原语 `host_impl/auth.rs::auth_connection_history_record` 与
  `db/operations.rs::record_connection_event` **不归一化**（原样落库）。
- 消费面（用户可见）：插件自己的前端 `plugins/terminal-session/src/composables/useConnectionHistory.ts`
  的 `METHOD_KEY_SUFFIX` 只认小写（未命中兜底 `unknown`），成功计数按 `e.result === 'success'` 判定
  → 现状下连接历史页认证方式显示「未知」、成功/失败计数全错。

处理：验收③要求「发现真实功能缺口停下来向用户报告，不在本票顺手补实现」→ 已停下报告。
若不修，本 target 恒红；若修（改插件 9 处字面量为小写或抽常量），断言可原样转绿。

### 4. 一次性诊断（已还原，用于枚举 T4–T10）

把 T3 那条断言临时改为大小写不敏感 + 打印实际值后重跑：`[DIAG] history: method="BIOMETRIC" result="SUCCESS"`、
`test result: ok` ⇒ **T4–T10 全部通过，唯一失败就是大小写**。诊断补丁与备份文件均已删除，
交付文件仍是原严格断言。

### 5. 门禁实测（2026-09-22 本机）

- `cargo check --lib --tests`：**0 error**
- `cargo test --test <t>` 逐个：`ws_session_route` ok(0.07s) / `pty_session_chain` ok(0.44s) /
  `ws_auth_rules` ok(14.14s) / `broadcast_shutdown` ok(1.41s) / `http_auth_biometric` **FAILED**(0.18s，见 §3)
- 前置：`resources/plugins/desktop/com.bedcode.terminal-session/` 产物存在（今日本机构建产物）；
  产物缺失时四个 target **显性失败**并提示重建命令（`node scripts/plugin-build.js --plugin terminal-session`），
  **不静默 `[skip]`**
- 全 target `cargo test` 未跑：对侧「PTY 业务下沉」线在途（期间 lib 红过两次，已等其恢复后取数），
  且 http target 未绿
- 行尾：4 个 CRLF 文件改后 `CR 行数 == 总行数`；`pty_session_chain` CR=0

## Comments

- 2026-09-22 立项（票 05 收尾时经用户确认「新立清理票」）。
- 并发提醒：`utils/auth/`、`session/`、`server/` 是对侧「终端下沉 / 插件改名」线的活跃改动区，
  开工前先看 `git log --oneline -10` 与 `git status`，避免与对侧在途改动撞在同一批接线文件上；
  同 worktree 双线互卷已有先例（票 05 的 5 个文件被对侧 `5b008eb5c` 一起提交，见 `issues/05` 实施记录第 9 条）。
