//! WASM Host Functions 注册与共享辅助
//!
//! 本模块只负责：
//! 1. 将所有 Host Functions 注册到 Linker（名称引用 SDK `abi` 常量，单一事实来源）
//! 2. 提供跨域共享的辅助：线性内存读写（[`memory`]）、统一权限守卫（[`check_permission`]）
//!
//! 各 Host Function 的实现按功能域拆分到子模块，与 SDK `host/*` trait 一一对应：
//! storage / database / terminal / session / events / http / log / fs / config / bus / lifecycle
//!
//! 所有 Host Function 签名约定：
//! - 字符串参数以 (ptr, len) 对传递，指向 WASM 线性内存
//! - 返回 (ptr, len) 对的函数通过 out_ptr 输出参数写入（8 字节: ptr + len）
//! - 其他函数用 i32 状态码返回
//! - 宿主通过 Caller 访问 WasmPluginState 获取 plugin_id 和宿主能力

mod bus;
mod config;
mod database;
mod events;
mod fs;
mod http;
mod lifecycle;
mod log;
pub(super) mod memory;
mod session;
mod status;
mod storage;
mod terminal;
mod wsl_fs;

use crate::plugin::wasm_runtime::{WasmHostContext, WasmPluginState};
use bedcode_plugin_api::abi;
use wasmtime::Linker;

// ==================== Host Function Registration ====================

/// 注册所有 Host Functions 到 Linker
///
/// 所有函数注册在 `abi::NAMESPACE`（"bedcode"）命名空间下，
/// 函数名定义在 SDK `bedcode_plugin_api::abi` 模块（单一事实来源），
/// 宿主侧测试 `test_host_fn_registration_matches_abi` 会校验
/// 实际注册签名与 `abi::HOST_FN_SIGNATURES` 一致
pub(super) fn register_host_functions(linker: &mut Linker<WasmPluginState>) -> crate::Result<()> {
    /// 本地注册宏：统一命名空间与错误上下文，避免 27 处重复
    macro_rules! register {
        ($name:expr, $func:path) => {
            linker
                .func_wrap(abi::NAMESPACE, $name, $func)
                .map_err(|e| crate::AppError::Plugin(format!("Failed to register {}: {}", $name, e)))?;
        };
    }

    // 存储
    register!(abi::import::STORAGE_GET, storage::host_storage_get);
    register!(abi::import::STORAGE_SET, storage::host_storage_set);
    register!(abi::import::STORAGE_DELETE, storage::host_storage_delete);

    // 数据库（主库，表名前缀校验）
    register!(abi::import::DB_EXECUTE, database::host_db_execute);
    register!(abi::import::DB_QUERY, database::host_db_query);
    register!(abi::import::DB_EXECUTE_PARAMS, database::host_db_execute_params);
    register!(abi::import::DB_QUERY_PARAMS, database::host_db_query_params);

    // 插件独立数据库
    register!(abi::import::PLUGIN_DB_EXECUTE, database::host_plugin_db_execute);
    register!(abi::import::PLUGIN_DB_QUERY, database::host_plugin_db_query);
    register!(abi::import::PLUGIN_DB_EXECUTE_PARAMS, database::host_plugin_db_execute_params);
    register!(abi::import::PLUGIN_DB_QUERY_PARAMS, database::host_plugin_db_query_params);

    // 终端
    register!(abi::import::TERMINAL_SEND, terminal::host_terminal_send);

    // 会话
    register!(abi::import::SESSION_LIST, session::host_session_list);
    register!(abi::import::SESSION_GET, session::host_session_get);
    register!(abi::import::SESSION_CONFIG_LIST, session::host_session_config_list);
    register!(abi::import::SESSION_LIFECYCLE_REGISTER, lifecycle::host_session_lifecycle_register);
    register!(abi::import::SESSION_INPUT_REGISTER, lifecycle::host_session_input_register);

    // 事件 / 广播 / 通知
    register!(abi::import::EMIT_EVENT, events::host_emit_event);
    register!(abi::import::BROADCAST_SYNC, events::host_broadcast_sync);
    register!(abi::import::NOTIFY, events::host_notify);

    // HTTP 代理
    register!(abi::import::HTTP_FETCH, http::host_http_fetch);

    // 日志
    register!(abi::import::LOG_INFO, log::host_log_info);
    register!(abi::import::LOG_DEBUG, log::host_log_debug);
    register!(abi::import::LOG_WARN, log::host_log_warn);
    register!(abi::import::LOG_ERROR, log::host_log_error);

    // 插件状态
    register!(abi::import::MARK_PLUGIN_ERROR, status::host_mark_plugin_error);

    // 文件系统
    register!(abi::import::FS_READ, fs::host_fs_read);
    register!(abi::import::FS_WRITE, fs::host_fs_write);
    register!(abi::import::FS_COPY, fs::host_fs_copy);
    register!(abi::import::FS_DELETE, fs::host_fs_delete);

    // 配置读取
    register!(abi::import::CONFIG_GET, config::host_config_get);

    // 消息总线
    register!(abi::import::BUS_PUBLISH, bus::host_bus_publish);
    register!(abi::import::BUS_SUBSCRIBE, bus::host_bus_subscribe);
    register!(abi::import::BUS_UNSUBSCRIBE, bus::host_bus_unsubscribe);

    Ok(())
}

// ==================== Shared Guards ====================

/// 统一权限守卫
///
/// 校验通过返回 true；拒绝时记录结构化错误日志并返回 false，
/// 调用方据此 `return -1`。替换原先约 30 处重复的 check/log 三连。
pub(super) fn check_permission(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    permission: &str,
    api: &str,
) -> bool {
    if host_ctx.permission.check(plugin_id, permission) {
        true
    } else {
        tracing::error!(plugin_id = %plugin_id, permission = %permission, "{}: permission denied", api);
        false
    }
}
