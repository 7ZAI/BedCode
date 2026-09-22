# 02: server 根死码清理（8 项中的 7 项）+ facade 收窄到单行

**What to build:** 把 `server/` 根目录上「名字像服务、实际没人调用」的六块死肉切掉，并把 `server.rs` 那个「谁都能从 `server::` 裸捞一把」的 glob 再导出面收成一个符号。做完后 `server/` 根只剩真正有归属的文件，票 04/05/06 搬的就是干净地基——**先删再搬**，避免把死码搬进新目录后再开第二票删同一条路径。

**Blocked by:** 无（可以立即开工，与票 01 并行）。**与在途会话零文件重叠**——这是本票刻意只含 7 项、把第 8 项（`message.rs` 壳）剥给票 03 的唯一理由。

**Status:** done（2026-09-23 commit `580507975`，dev）

## 本票删除清单（7 项，各条判据独立复核后才许删）

| 项 | 判据 |
| --- | --- |
| `server/client_info.rs`（整文件） | 仅 `server.rs` 的 `pub use client_info::ClientInfo` + `services/session_sub.rs` 形参 + 自身测试引用；已被 `ws/session.rs::WsSession` 与 `ws/registry.rs` 取代 |
| `server/services/session_sub.rs`（整文件） | `subscribe_session` / `unsubscribe_session` 全仓零调用者。⚠ `ws/channel/terminal.rs` 命中的 `handle_session_subscribe` 是同名私有方法，**不是**消费者 |
| `server/middleware/cors.rs`（整文件） | `cors_config()` 全仓零调用者；实际 CORS 在 `app.rs` 内联构造 `Cors::default()` |
| `server/dtos/auth_dto.rs`（整文件） | 11 个结构体逐条 `grep -rn "\bX\b" src/ tests/ \| grep -v auth_dto.rs` 各 0 命中（本 spec 立项时已实测）；开工时重跑一遍贴结果 |
| `server/dtos/plugin_dto.rs`（整文件） | 4 行注释、零 `pub` 项 |
| `connection_types::PairingCodeGeneratedEvent` + 该文件的 `pub use crate::enums::{AuthPayload, AuthStage}` | 前者全仓从未构造；后者与 `message.rs` 的再导出重复 |
| `dtos/git_dto.rs::GitCheckoutRequest` | 仅自身定义 |

**⚠ 勿删（本票最容易犯的错）**：`services/session_control.rs` 的 `handle_control` 与 `RefreshEvent` **看着像死码但不是**——它们没有外部调用者，却是活口 `handle_control_message` 内部路径（`:187` 调用、`:196/:208` 构造），且 `RefreshEvent` 是 `sessions-refresh` Tauri 事件的 payload、前端在监听。「零外部消费者即删」在这里会直接删崩功能。

**⚠ 保留（勿顺手删）**：`dtos/{config,file,git}_dto.rs` 的其余结构体生产零消费者，但它们是 `gateway.rs` 的 `business_endpoint_shapes_are_locked_for_dual_track` 与 `wasm_runtime/tests/session_e2e.rs` 两条**形状契约锚点**的消费者——删了等于拆双端协议锁。

## facade 收窄（`server.rs`）

现状 `:21-25` 五段 `pub use`。实测这五段的**裸符号消费者（`crate::server::X` 形式，扫 `src/` + `tests/`）只有 2 处，全是 `DeviceConnectionInfo`**（`commands.rs`）。故收成一行：

- 删 `pub use crate::enums::control::SessionControlAction`、`pub use client_info::ClientInfo`、`pub use message::*`（此段随票 03 的壳删除才生效——本票只摘 `client_info` 与 `SessionControlAction` 两段，`message::*` 留到票 03）、`pub use filter::{…}`；
- 留 **显式单项** `pub use connection_types::DeviceConnectionInfo;`（此处暂不加 `websocket::` 前缀，那是票 06 的事），**不再用 `*`**——glob 正是「谁都能从 `server::` 捞一把」的成因。
- 判据口径：按**裸符号**消费者为 0 即删该段；模块路径形式（`server::filter::…`）不受 facade 影响、照常可用，别把它当消费者留下来。

## 验收

- [ ] 7 项各自附判据（重跑的引用扫描命令 + 空输出），不接受「看着像死码」
- [ ] `handle_control` / `RefreshEvent` 与三个契约锚点 DTO 文件**完整存活**，`git diff` 里不得出现它们
- [ ] `server.rs` facade 收窄后 `commands.rs` 两处 `crate::server::DeviceConnectionInfo` 仍编译通过
- [ ] `cargo check --lib --tests` **0 warning**——必须带 `--tests`：`--lib` 不编译 `#[cfg(test)]`，删掉的 `client_info` 自身测试与集成测试引用**完全不报**（仓库既有踩坑）
- [ ] `cargo test` 绿（跑前重出插件产物，报数注明 `[skip]=0`）
- [ ] 零前端改动 → `pnpm run test:run` / `pnpm exec eslint .` 不适用，在 commit message 里写明理由
- [ ] 若 `dtos/session_dto.rs`（**行尾混合**：cr=71 / lines=93）出现在 diff 里，核 `git diff --ignore-cr-at-eol` 只剩目标行——整文件换行重排即回退

## 归属与真源

来源：`../spec.md` §2 D3 + D8、§5.5、§5.6。`server.rs` 声明段的最终归属规则见票 05 / 票 06。

## Comments

- 2026-09-23 立项：来源 spec §2 D3。拆票时从 8 项减为 7 项，第 8 项（`server/message.rs` 壳）因与在途会话撞文件剥入票 03。
- 2026-09-23 done：7 项死码删除各附全仓引用扫描判据；facade 收窄为单行 `DeviceConnectionInfo`。
  验证：`cargo check --lib --tests` 0 error、`cargo test` lib 1147 通过 / 0 失败 / 0 `[skip]`、
  集成测试全绿、`cargo doc` 零新增 broken intra-doc link（既有断链为 HEAD 状态，票 04/09 处理）。
  行尾：5 个 CRLF 编辑文件逐一核 `cr==lines`。
  **实施期发现**：删除 `connection_types` 的 enums 再导出后，`server/message.rs` 的
  `ConnAuthPayload/ConnAuthStage/PairingCodeGeneratedEvent` 三引用悬空（编译强制），
  本票顺带修剪其 connection_types 再导出块（仅该块；壳本体留票 03）。
  提交时 message.rs 用 `git apply --cached` 拆分 hunk：本票 hunk 已提交，在途
  SessionConfig 线 hunk（enums 块）留在工作区待对侧提交。
  **残留登记**：`tests/broadcast_shutdown.rs:296` 与 `tests/ws_auth_rules.rs:261`
  注释提及已删的 `dtos/auth_dto.rs` 路径（纯注释不编译），这两个测试文件是票 03
  的撞车面，留给票 03 顺带修。
  **并发观察**：gateway.rs 有并发会话编辑中（票 01 自校准前置，含一次 rustc
  生命周期修复），本提交未含 gateway.rs。
