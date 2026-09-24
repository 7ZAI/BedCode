# 08: SessionStatus / SessionType 业务视图收窄

**What to build:** 收窄宿主侧的会话状态/类型归属：宿主只剩引擎事实（PTY 进程状态与线协议形状），会话业务状态机视图归插件（插件已有同形副本）。宿主不再以业务语义持有会话状态机，仅保留 PTY 进程生命周期状态（如关窗守卫按存活 PTY 计数、WS 传输视图）。`SessionType` 这类产品概念从宿主移除。业务语义类型在 `enums/` 的残留清零或逐项登记为「线协议形状」。

**验收要求入**：`enums/` 目录终态按 spec §4.2 三分类复核通过；宿主侧无业务语义会话状态机消费方（grep 断言），只剩 PTY 进程状态与线协议透传形状。

**Blocked by:** P1-b land（关窗守卫/会话网关以 P1-b 后基线为准）

**Status:** ✅ 完成（2026-09-24；业务下沉随对侧票 10/11 land，本票做线协议归位收口）

## 落地

- **业务下沉已由对侧票 10/11 实质完成**：`host-session` 整 interface 退役（ABI 26→27）、
  内核会话目录整删（宿主「零会话对象」）。宿主侧对 `SessionStatus`/`SessionType`
  **从不构造/推进状态机**（grep 断言：全部 `SessionStatus::X` 都来自插件线协议值的
  透传/判断/形状锁；PTY 存活用独立引擎枚举 `PtySessionStatus`）。
- **物理归位（本票收口）**：`SessionStatus`/`SessionType` 定义从 `enums/session.rs`
  移入 `protocol/session.rs`（线协议形状中立域，见 protocol.rs 头注释）；`enums.rs`
  保留 `pub use crate::protocol::session::{SessionStatus, SessionType}` 兼容 re-export，
  现有 6 处 `enums::SessionStatus` import 零改动；`enums/session.rs` 文件删除。
- `enums/` 目录终态：剩 `pty_status.rs`（引擎级）+ `auth/control/summary/sync`
  （线协议）+ `plugin`（SDK re-export）+ `special_key`（票 06 已下沉，文件残留待清），
  会话业务视图零残留在 enums/。
- 线协议 wire 形状零变更（`shape_lock_*` 用例保持绿）；移动端不依赖宿主 `enums::` 路径。
- 门禁：宿主 lib 全量 1055/0（protocol 11 + events 37 + session_e2e 11 重点验证）。

- [x] 宿主会话状态/类型业务视图清零：只保留 PTY 进程状态 + 线协议透传形状；grep 断言宿主无业务语义会话状态机消费方
- [x] 关窗守卫 / 会话巡检等探源判据与 P1-b 后插件登记域一致（不自造宿主会话状态）
- [x] `enums/` 目录三分类复核通过，业务语义类型零残留或按「线协议形状」登记
- [x] i18n / 协议标签（camelCase/snake_case wire 名）不因收窄破坏跨端