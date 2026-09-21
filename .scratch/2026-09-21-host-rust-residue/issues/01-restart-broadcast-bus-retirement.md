# 01: 重启广播总线死链清理（`SessionRestartEvent` 通道整体退役）

**What to build:** 删除内核「会话重启」广播通道的**全部**残余——v21 起内核不再有重启
执行器（`host-session.restart` 删除），前端 `session-restarted` 事件改由 `com.bedcode.session`
插件在 Created 生命周期之后经 `host-events.emit` 补发，故内核这条通道**已无发送方**，
是一整条死链。

**决策依据:** ADR 0022「v21 收敛退役」节（重启编排归插件、宿主只留 `remove` /
`create-with-spec`）；`.scratch/2026-09-21-host-rust-residue/spec.md` 批次四。

**Blocked by:** 无（独立死代码清理，可与票 03/04 并行）

**Status:** done（2026-09-21）

## 待删清单（grep 锚点已核实，均无发送方）

- [x] `src-tauri/src/session/event_bus.rs`：`DefaultSessionEventBus.restart_tx` 字段与初始化、
      `restart_sender()`、`SessionEvent::Restarted` 变体与 `publish` 中的对应分支、trait
      `SessionEventBus` 的 `restart_sender()`
- [x] `src-tauri/src/session/session_manager.rs`：`restart_tx()`、`subscribe_restart()`、
      `use crate::session::{... SessionRestartEvent ...}` 导入
- [x] `src-tauri/src/session/session_event.rs`：`SessionRestartEvent` 结构体
- [x] `src-tauri/src/session.rs`：`SessionRestartEvent` 再导出
- [x] `src-tauri/src/events/forwarder.rs`：`forward_restart_events()` 及其 spawn 调用点
- [x] `src-tauri/src/system/constants/event.rs`：`SESSION_RESTARTED` 常量
- [x] `src-tauri/src/system/config.rs`：`channels.restart_broadcast_capacity` 六处
      （props 描述表 / props 键清单 / 结构体字段 / 默认值 / `parse_value` / 序列化输出）

## 验收

- [x] `grep -rn "restart_tx\|subscribe_restart\|SessionRestartEvent\|SESSION_RESTARTED\|restart_broadcast_capacity" src-tauri/src` 为空
- [x] 前端 `session-restarted` 行为不变：由插件 `actions.rs::flush_pending_restart` 在
      Created 之后补发（载荷 camelCase `{oldSessionId,newSessionId,sessionName}`）；本票**不动插件侧**
- [x] 桌面 `cargo test --lib` 全绿（1088/0）；无残留进程

## Comments

### ③ 实施记录（2026-09-21）

- 七处锚点全部删除，`grep` 复核为空；`DefaultSessionEventBus` 只剩 `status_tx` / `event_tx`
  两个channel（`event_bus.rs` 头注释与 `forwarder.rs` 类型注释改写为「重启事件归插件补发」）。
- 配置键退役口径按 ② 执行：`AppConfig::from_properties` 是「已知键解析 + 未知键忽略」，
  旧 `config.properties` 里的 `channels.restart_broadcast_capacity` 被静默忽略（无报错、无迁移）；
  CHANGELOG 已记「配置项移除」。
- 门禁：桌面 `cargo test --lib` 1088/0（含本批与并行 in-flight 的 host-task 指标用例）；
  前端零改动（`session-restarted` 无宿主前端消费者，`useSessionStatusListener` 已在票 05 批次删除）。

## Comments

### ① 为什么不是「顺手清掉」而是独立票

它牵动**配置 schema**（`channels.restart_broadcast_capacity` 是用户可见配置项，拆 props 表 /
默认值 / 序列化三处），与「删两个原语」不是同一风险等级；批次四刻意留给独立票，避免把
配置面变更混进 ABI 变更里。

### ② 配置键退役口径

旧配置文件中残留的 `channels.restart_broadcast_capacity` 应被**忽略而非报错**（配置读取是
「已知键解析 + 未知键忽略」语义）。票内需确认 `system/config.rs` 的解析路径对未知键的处理，
并在 CHANGELOG 记「配置项移除」。
