# 06: WS 面收进 `server/websocket/`（含 `ws` 改名与 `services/` 收编）

**What to build:** WS 那套的家一次住全：`ws/*` 改名进 `server/websocket/`，把根上遗留的 WS 专属件（`services/`、`connection_types.rs`）一起收编，`server/websocket.rs` 作模块入口。做完后 `crate::server::websocket::` 是 WS 的唯一入口，`server::websocket::services::` 这个名字不再骗人（它只服务 WS 通道）——**`http` 与 `websocket` 的零互依在本票结束时已经成立，只是还没有锁**（锁是票 08）。

**Blocked by:** 03（`message.rs` 壳先消失，本票才有余地把它消费者的路径写成最终形态）、04（`core` 路径定稿）、05（**仅为 `server.rs` 声明段的物理串行**，不是逻辑依赖——见下）。

**Status:** done（2026-09-23，commit 25e9ba3a1 与票 05 联合交付）

## 搬移面（`git mv`，内容尽量零改）

| 旧 | 新 | 备注 |
| --- | --- | --- |
| `server/ws.rs` | `server/websocket.rs` | 改写模块声明：新增 `services`、`connection_types` 两条（`routes` 留到票 07） |
| `server/ws/{conn,endpoint,message,registry,session,subscription,websocket_manager}.rs` | `server/websocket/` 同名 | 7 个平铺件 |
| `server/ws/channel.rs` + `channel/{terminal,event,plugin}.rs` | `server/websocket/` 同名同层 | `channel/*.rs` 的 `use super::super::conn::{…}` 解析目标不变 → **零改**，这是「整目录原样搬」的红利 |
| `server/ws/terminal_ws.rs` + `terminal_ws/{control_frame,forward,subscriber}.rs` | `server/websocket/` 同名同层 | |
| `server/services.rs` + `services/{session_control,terminal_service}.rs` | `server/websocket/` 同名同层 | **⚠ 勿删** `handle_control` / `RefreshEvent`（票 02 已说明它们是活内部路径）；`session_sub.rs` 已被票 02 删除 |
| `server/connection_types.rs` | `server/websocket/connection_types.rs` | 死项已被票 02 摘除，此处只剩 `DeviceConnectionEvent` / `DeviceConnectionInfo` |
| —— | `server/websocket/routes.rs` | 本票**不建**，票 07 建 |

## 为什么 `services/` 归 WS 而不是归 HTTP 或留在根

实测：`services/` 的**唯一**消费者是 WS 通道——`ws/channel/event.rs` 调 `terminal_service::handle_input` 与 `session_control::handle_control_message`、`ws/channel/terminal.rs` 调 `terminal_service::handle_input`；HTTP 侧对它零调用。它躺在根目录跟 HTTP 各模块平级，是「归属不可读」这条病灶的最大来源。

它的**内容**（会话控制、终端输入）其实是会话业务、不是 WS 传输原语——但那是「该不该继续往插件/`session` 下沉」的问题，属行为面重构，本票只搬不改（spec §9.1 已列为后续票，并入 `2026-09-20-host-business-decarriage` 线）。搬完在 `websocket/services.rs` 的模块注释里写明：**唯一消费者是 `websocket/channel/*`，归属应为会话业务，勿当作 WS 原语扩展**。

## 引用改写

- 外部 40 处 `crate::server::ws::` → `crate::server::websocket::`，分布：`plugin/.../host_impl/ws.rs`(24)、`events/sync_handler.rs`(6)、`plugin/.../tests/ws_e2e.rs`(5)、`commands.rs`(1)、`host_impl/session.rs`(1)、`lib.rs`(1)、`tests/session_e2e.rs`(1)、`session/session_output.rs`(1，文档注释)。
- `server/` 内 WS 侧自引用约 35 处前缀改名 + `services` / `connection_types` 三处改指 `crate::server::websocket::…`。
- **`WebSocketManager` 对 `core::app::start_http_server` 的调用保持原样**（现状是 WS 管理器持有承载 HTTP 的 actix 服务器）。这是本票刻意不修的倒挂（spec §9.2）：修它属行为面重构，会连带 `supervisor` 的启停链，与本任务 move-only 性质不兼容。**别顺手「正名」。**
- `crate::server::DeviceConnectionInfo` 的 facade 行改指 `websocket::connection_types::DeviceConnectionInfo`（仍是显式单项，glob 已在票 02 收掉）。

## `server.rs` 最终形态归本票

票 05 只允许增删它自己那五行；本票负责把声明段收成最终形状：`pub mod core; pub mod http; pub mod websocket;` + 单行 facade。**并行撞车处理**沿用票 05 的规则（plumbing 只提自己那几行、不 `git add -A`、禁 `git checkout --` 覆盖对侧未提交内容）。

## 验收

- [ ] `grep -rn "crate::server::ws\b\|server::ws::" src/ tests/` == **0**（改名彻底性的唯一硬判据）
- [ ] `server/` 根只剩 `server.rs` + `core/` + `http/` + `websocket/`；无 `mod.rs`
- [ ] **wire 协议零变化**：`websocket/message.rs`（1520 行 `Message` 枚举）、TB v3 帧、`/ws/event` 同步广播面在 `git diff` 里**只出现 `use` 行**；`serde(tag="type", content="payload")` 形态一字不动（移动端兼容红线）
- [ ] **HTTP 与 WS 零互依已成立**（本票结束时）：`grep -rn "server::http" src/server/websocket/` == 0 且 `grep -rn "server::websocket" src/server/http/` == 0。⚠ 例外只有 `core/app.rs`（组合物），它本票还不动
- [ ] `services/` 收编后其模块注释按上文写法落地（写清新归属与「勿当 WS 原语扩展」）
- [ ] 移动端零改动（`git status bedcode-mobile/` 里不应出现任何文件——本票的改名全在桌面 crate 内）
- [ ] `cargo check --lib --tests` 0 warning；`cargo test` 绿（产物重出、`[skip]=0`；`ws_e2e` / `pty_e2e` / `test_session_*` 闭环必须真跑不能 skip）
- [ ] `cargo doc --no-deps` 无新增 broken link（基线来自票 04）；`host_impl/ws.rs` 头部三处 `crate::server::ws::…` 文档路径同批改
- [ ] **CRLF 纪律**：`ws.rs`、`ws/session.rs`、`ws/websocket_manager.rs`(430/430)、`services/terminal_service.rs`、`connection_types.rs` 都是 CRLF；`websocket_manager.rs` 是本票「既 CRLF 又要改内容」的最大文件，逐行核 CR 数
- [ ] 与票 05 的 `server.rs` 分段改动核对：合并后声明段是最终形状，且票 05 那五行没被本票覆盖回旧路

## 归属与真源

来源：`../spec.md` §2 D1（改名，用户选定）+ D6（`services/` 归属）+ §3.2 WS 各行 + §4 外部 40 处 + §5.4。

## Comments

- 2026-09-23 立项。与票 05 并行的前提是 `server.rs` 归属预先划清（用户裁决）；本票额外挂 05 一条边，纯粹为把同一文件的改动物理串行。


## Comments

- 2026-09-23 立项。
- 2026-09-23 done：与票 05 联合提交（用户指示）。验收：crate::server::ws = 0、http↮websocket 双向 = 0、server/ 根只剩 server.rs + core/ + http/ + websocket/、wire 协议 message.rs 100% 零改搬移、CRLF 全保持、移动端零改动。
- services.rs 模块注释已写明归属（唯一消费者是 websocket/channel/*，勿当 WS 原语扩展）。
- server.rs 最终形态归本票落地：pub mod core; pub mod http; pub mod websocket; + 单行 facade。
- 偶发 flake：task_e2e 时序型 1 次失败，单测/全量重跑均绿。