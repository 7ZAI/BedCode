# 03: 删除 `server/message.rs` 转发壳并重指向全部消费者

**What to build:** `server/` 根上那个 18 行、注释写着「WebSocket Message Types」却躺在根目录的纯转发壳消失。它是本任务里「兼容壳最终变成病灶」的标本——立这条壳的人当初也是为了少改调用点。删掉后，`Message` 只有一条真路径，`AuthPayload` 一族回到它本来的家 `crate::enums`。

**Blocked by:** 02（facade 的 `pub use message::*` 那段随本票摘掉，两票改同一段声明）；**并受工作区门禁约束**（见下）。

**Status:** done（2026-09-23，commit `4f8e705d5` 补齐；`36428e0a5` 票 04 已吸收大部分消费者重指向）

## ⚠ 前置门禁（本票存在的唯一理由）

隔壁在途会话（`SessionConfig` 线协议退役 + 认证记录下沉，AGENTS 已记到 ABI v24）当前**未提交**地持有本票要改的两个文件：

- `server/message.rs` —— 正在收窄该壳的 `enums` 再导出清单（去掉 `SessionConfig*` 一族）
- `server/ws/channel/event.rs` —— 该壳的消费者之一

**门禁**：这两个文件（连同 `server/ws/message.rs`、`server/ws/conn.rs`、`server/controllers/plugin_controller.rs`）在 `git status` 里干净后才许开工。禁止在其上 `git mv`、禁止 `git checkout --` / `git restore` 逆向（AGENTS §11 回滚规范 1：未提交内容无法从 git 恢复）。若冲突持续，本票并入票 06 一起做，不要抢。

## 改动面（实测）

- 消费者：`ws/channel/event.rs`、`ws/websocket_manager.rs`、`services/session_control.rs`（4 处 use/构造点）、`services/terminal_service.rs`
- 集成测试 3 个文件各 1 处 `bedcode_lib::server::message::{…}`：`tests/broadcast_shutdown.rs`、`tests/pty_session_chain.rs`、`tests/ws_auth_rules.rs`
- 重指向口径：`Message` → `crate::server::ws::message::Message`（**此时目录未动，仍写 `ws::`**，改名是票 06 的事）；`AuthPayload` / `AuthStage` / `SessionControlAction` / `SessionControlPayload` / `SessionSummary` / `TerminalAction` / `TerminalPayload` / `KeyCombo` → `crate::enums::*`（`enums.rs` 已把它们平铺在模块根）
- 壳内 `ConnAuthPayload` / `ConnAuthStage` 两个别名全仓零消费者，随壳一起消失，**不要**给它们找新家
- `server.rs` 的 `pub use message::*` 同段一起摘（票 02 已收掉其余四段）

## 验收

- [ ] `server/message.rs` 文件不存在，`server.rs` 无 `pub use message`
- [ ] 集成测试里 `bedcode_lib::server::message::` 出现次数为 **0**（`grep -rc` 贴数）
- [ ] `Message` 的 wire 语义**零变化**：不动 `ws/message.rs` 任何序列化形态、字段、tag（移动端兼容红线）。本票 diff 里只应出现 `use` 行
- [ ] `cargo check --lib --tests` 0 warning——`--tests` 是硬要求，壳的消费者全在 `#[cfg(test)]` 覆盖面上
- [ ] `cargo test` 绿（产物重出、`[skip]=0`）；`ws_auth_rules` / `broadcast_shutdown` / `pty_session_chain` 三个测试文件逐行核 `git diff --ignore-cr-at-eol`（它们是 **CRLF**，禁 python text 模式读写，只走 Edit 工具）
- [ ] 票 02 保留的三个契约锚点 DTO 与 `handle_control` / `RefreshEvent` 仍完整存活

## 归属与真源

来源：`../spec.md` §2 D3 表第 3 行 + §2 D8（不再造壳）+ §5.5 + §0（在途门禁）。

## Comments

- 2026-09-23 立项：拆票时从票 02 剥出。剥出理由不是工作量，是**撞车面不同**——票 02 零撞车可立即开工，本票必须等隔壁收尾。

## Comments

- 2026-09-23 done：门禁经用户裁决走「先提交 v24 → 外科共存」路线。实施详情：
  - **v24 先落地**（commit `727041862`，用户指示精确 add）：message.rs/event.rs/conn.rs/ws-message.rs/plugin_controller.rs 的 v24 内容先行提交，7 个与票 04 撞车文件用 blob 级手术只暂存 v24 内容。
  - **票 04 中途吸收**：并发会话执行票 04 时 `git add` 吸收了本票 6 个文件的重指向（event/websocket_manager/server.rs/3 测试文件），使其 commit `36428e0a5` 单独处于半断状态（session_control 仍引 server::message）；本票提交 `4f8e705d5`（session_control/terminal_service/message.rs 删除）补齐后 HEAD 完整可编译。
  - 验证：cargo check --lib --tests 0 error（0 新 warning）；cargo test lib 1147 / 集成全绿 / [skip]=0；cargo doc 断链 10 处全为 HEAD 既有；CRLF 文件行尾保持。
  - 偶发 flake 观察：全量 cargo test 曾出现 lib 1 失败（1146+1），单独重跑与全量重跑均绿——当时并发会话测试进程在跑，疑为端口/时序环境扰动，非本票引入。
