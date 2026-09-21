# 01: 重启广播总线死链清理（`SessionRestartEvent` 通道整体退役）

**What to build:** 删除内核「会话重启」广播通道的**全部**残余——v21 起内核不再有重启
执行器（`host-session.restart` 删除），前端 `session-restarted` 事件改由 `com.bedcode.session`
插件在 Created 生命周期之后经 `host-events.emit` 补发，故内核这条通道**已无发送方**，
是一整条死链。

**决策依据:** ADR 0022「v21 收敛退役」节（重启编排归插件、宿主只留 `remove` /
`create-with-spec`）；`.scratch/2026-09-21-host-rust-residue/spec.md` 批次四。

**Blocked by:** 无（独立死代码清理，可与票 03/04 并行）

**Status:** ready-for-agent

## 待删清单（grep 锚点已核实，均无发送方）

- [ ] `src-tauri/src/session/event_bus.rs`：`DefaultSessionEventBus.restart_tx` 字段与初始化、
      `restart_sender()`、`SessionEvent::Restarted` 变体与 `publish` 中的对应分支、trait
      `SessionEventBus` 的 `restart_sender()`
- [ ] `src-tauri/src/session/session_manager.rs`：`restart_tx()`、`subscribe_restart()`、
      `use crate::session::{... SessionRestartEvent ...}` 导入
- [ ] `src-tauri/src/session/session_event.rs`：`SessionRestartEvent` 结构体
- [ ] `src-tauri/src/session.rs`：`SessionRestartEvent` 再导出
- [ ] `src-tauri/src/events/forwarder.rs`：`forward_restart_events()` 及其 spawn 调用点
- [ ] `src-tauri/src/system/constants/event.rs`：`SESSION_RESTARTED` 常量
- [ ] `src-tauri/src/system/config.rs`：`channels.restart_broadcast_capacity` 六处
      （props 描述表 / props 键清单 / 结构体字段 / 默认值 / `parse_value` / 序列化输出）

## 验收

- [ ] `grep -rn "restart_tx\|subscribe_restart\|SessionRestartEvent\|SESSION_RESTARTED\|restart_broadcast_capacity" src-tauri/src` 为空
- [ ] 前端 `session-restarted` 行为不变：由插件 `actions.rs::flush_pending_restart` 在
      Created 之后补发（载荷 camelCase `{oldSessionId,newSessionId,sessionName}`）；本票**不动插件侧**
- [ ] 桌面 `cargo test --lib` 全绿；无残留进程

## Comments

### ① 为什么不是「顺手清掉」而是独立票

它牵动**配置 schema**（`channels.restart_broadcast_capacity` 是用户可见配置项，拆 props 表 /
默认值 / 序列化三处），与「删两个原语」不是同一风险等级；批次四刻意留给独立票，避免把
配置面变更混进 ABI 变更里。

### ② 配置键退役口径

旧配置文件中残留的 `channels.restart_broadcast_capacity` 应被**忽略而非报错**（配置读取是
「已知键解析 + 未知键忽略」语义）。票内需确认 `system/config.rs` 的解析路径对未知键的处理，
并在 CHANGELOG 记「配置项移除」。
