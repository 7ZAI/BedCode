//! 宿主能力面（core-host-api，移动 fork 形态）——移动 16 域 host-* 原语
//!
//! 批次 2（票 17）：16 域实现 + 移动运行时绑定自宿主
//! `plugin/wasm_runtime{,/host_impl}` 迁入（真源转移完成）：
//! - [`ports`]：宿主引擎端口（auth / egress / peer / mdns / 平台桥——
//!   「离宿主无法实现」的引擎调用唯一入口，宿主装配实现）
//! - [`context`]：WasmHostContext（宿主上下文 + 端口注入点）
//! - [`http_engine`]：插件 HTTP 执行引擎（reqwest；egress/token 经端口）
//! - [`sql_guard`]：表名前缀护栏 + 列转换（自宿主 wasm_host.rs 迁入）
//! - 16 域实现位于 `manager::runtime::host_impl`（与组件绑定同侧，
//!   形状与迁移前宿主布局同构）
//!
//! **禁止**在本目录重建桌面独有域（api-call / timer / process / app / pty /
//! task / crypto）——WIT 是单一事实来源（移动 WIT v17 无这些 import），
//! lib.rs 反向锁盯删面清单。

pub mod context;
pub mod http_engine;
pub mod ports;
pub mod sql_guard;

pub use context::WasmHostContext;
pub use ports::{
    AuthEnginePort, ConnectionEnginePort, FsAuthGate, FsAuthOp, HostEnginePorts, PrimaryTarget,
    SafIoPort, UnimplementedPorts, WsReconnectPolicyPort, PORT_NOT_WIRED,
};
