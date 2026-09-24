//! Protocol Wire Shapes —— 跨端线协议形状的中立域
//!
//! 本域**只放**两类东西：
//! 1. **跨端 wire 契约**：宿主与前端 / 移动端共同消费的 serde 形状（有人跨进程读它的
//!    JSON，字段名与大小写即协议）；
//! 2. **引擎级转义**：线协议要求的按键 → 字节映射（当前仍住 `enums/special_key.rs`，
//!    其归属由并发专项 `.scratch/2026-09-24-host-crypto-business-downsink` 票 06 裁决，
//!    本域不预先挪位置）。
//!
//! **为什么单独成域**（票 02，会话引擎下沉 P1-b 后续）：这批类型原先和「内核会话登记
//! 实现」同住 `session/`，而该目录整体退役在即（票 11）。协议形状必须活过实现删除，
//! 否则删目录会顺手删掉移动端与前端仍在读的契约定义。
//!
//! 红线：
//! - 新增类型必须是**对外契约**——宿主内部实现细节留在各自模块，禁止把「方便」当理由搬进来；
//! - 本域只做形状与机械透传，**不解释业务语义**（AGENTS §5 无业务内核）；
//! - 改形状即改线协议，必须两端同步评估（AGENTS §9）。
//!
//! 与插件真源的关系：会话事实真源在 `com.bedcode.terminal-session` 登记域，本域是它的
//! **对外形状**；两侧逐字段对齐由 `wasm_core/manager/runtime/tests/session_e2e.rs` 的
//! 网关读取面对齐锁 + 本域各文件的形状锁用例共同守住。

pub mod session;

pub use session::{
    task_fields_from_slot, RendererSource, ResizeOutcome, SessionInfo, SessionInfoView, SessionStatus,
    SessionType,
};
