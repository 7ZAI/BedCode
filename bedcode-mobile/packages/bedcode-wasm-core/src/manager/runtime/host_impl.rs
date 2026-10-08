//! WASM 宿主能力实现层（host 函数，从 wasm_runtime.rs 拆分）
//!
//! 各功能域与 WIT import 接口一一对应；共享辅助在 support.rs。
//! 由 wasm_runtime/component.rs 的 Host trait impl 直接调用（值传递逻辑层）。
//! core 形态的 func_wrap 胶水（Caller + (ptr,len) 内存搬运）已随 09 清理删除。

pub(super) mod auth;
pub(super) mod bus;
pub(super) mod config;
pub(super) mod connection;
pub(super) mod db;
pub(super) mod event;
pub(super) mod fs;
pub(super) mod http;
pub(super) mod notify;
pub(super) mod mdns;
pub(super) mod peer;
pub(super) mod platform;
pub(super) mod storage;
pub(super) mod support;
pub(super) mod terminal_stream;
pub(super) mod ws;

// 域内函数为 pub(crate)，显式 re-export 到 host_impl 层（component.rs 经
// `super::host_impl::xxx` 调用，不带子模块路径）
pub(crate) use auth::*;
pub(crate) use bus::*;
pub(crate) use config::*;
pub(crate) use connection::*;
pub(crate) use db::{
    db_execute, db_query, db_execute_params, db_query_params, db_execute_batch,
    plugin_db_execute, plugin_db_query, plugin_db_execute_params, plugin_db_query_params,
    plugin_db_execute_batch,
};
pub(crate) use event::*;
pub(crate) use fs::*;
pub(crate) use http::*;
pub(crate) use notify::*;
pub(crate) use mdns::*;
pub(crate) use peer::*;
pub(crate) use platform::*;
pub(crate) use storage::*;
pub(crate) use terminal_stream::*;
// 宿主身份广播登记原语（peer_net 引擎经垫片反向消费；批次 2b 提 pub）
pub use mdns::{register_host_service, stop_host_service};

// ws 域 5 原语以别名导出供 component.rs 接线（`purge_for_plugin` 名被下方聚合函数
// 占用，回收统一走聚合入口，无需单导出）
pub(crate) use ws::{ws_close, ws_connect, ws_is_connected, ws_send_binary, ws_send_text};

/// 停用回收聚合入口（插件管理器 `deactivate_inner` 唯一调用点；批次 2 起带
/// host_ctx——mdns 域停守护句柄需经端口取「已初始化」面）
///
/// 逐域回收：mDNS 浏览/广播双表 + WS 出站连接表（各域只碰本人句柄，宿主与
/// 它插件登记不受影响）。新域接入回收必须在此登记——漏登记 = 停用后连接泄漏
/// （ADR 0022 §5.1.3「停用可回收」硬要求）。
pub fn purge_for_plugin(host_ctx: &crate::host_api::WasmHostContext, plugin_id: &str) -> usize {
    let mut purged = mdns::purge_for_plugin(&host_ctx.ports, plugin_id);
    purged += ws::purge_for_plugin(plugin_id);
    // 票 12：终端输出流窄转发表随停用清空——输出流的唯一消费者是终端
    // 插件（单例 app），停用后无转发目标，残留表位只会让后续转发打到
    // 已失效的页面通道
    crate::terminal_stream_gateway::terminal_stream_gateway().clear_all();
    purged
}
