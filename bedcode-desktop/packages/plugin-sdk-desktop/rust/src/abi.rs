//! WASM ABI 契约 — 单一事实来源
//!
//! 宿主（wasmtime Linker 注册）与插件（`wasm_entry!` 导出 / `WasmHost` 导入）
//! 共同引用本模块的名称常量与签名表。
//!
//! # ABI 约定
//!
//! - 所有 host functions 注册在 [`NAMESPACE`] 命名空间下
//! - 字符串参数以 (ptr, len) 对传递，指向 WASM 线性内存
//! - 返回 (ptr, len) 对的函数通过 out_ptr 输出参数写入
//!   （[`RESULT_PAIR_SIZE`] 字节: ptr:u32 + len:u32，小端序），
//!   而非 Rust 元组返回值（C ABI 不支持多值返回）
//! - 其他函数用 i32 状态码返回（0 成功，-1 失败）
//!
//! # 版本演进
//!
//! 新增/变更 host function 或导出函数时递增 [`ABI_VERSION`]，
//! 并同步更新 [`HOST_FN_SIGNATURES`] / [`PLUGIN_EXPORT_SIGNATURES`]。
//! 宿主侧测试会校验 Linker 实际注册签名与 [`HOST_FN_SIGNATURES`] 一致，
//! 漂移在测试期即暴露。

/// WASM import 命名空间 — 所有 host functions 注册于此
pub const NAMESPACE: &str = "bedcode";

/// WASM 线性内存导出名
pub const MEMORY: &str = "memory";

/// out_ptr 输出参数大小（字节）：ptr:u32 + len:u32
pub const RESULT_PAIR_SIZE: usize = 8;

/// 当前 ABI 版本
///
/// - v1: 初始版本（27 个 host functions + 11 个插件导出）
/// - v2: 新增 4 个参数绑定 SQL host functions（*_params），消灭插件侧手写转义
pub const ABI_VERSION: u32 = 2;

/// 插件导出函数名（`wasm_entry!` 宏生成，宿主调用）
pub mod export {
    /// 内存分配器 — 宿主写入字符串前分配 len 字节
    pub const ALLOCATE: &str = "__bedcode_allocate";
    /// 返回 manifest JSON（写入 out_ptr）
    pub const MANIFEST: &str = "__bedcode_manifest";
    /// 激活插件
    pub const ACTIVATE: &str = "__bedcode_activate";
    /// 停用插件
    pub const DEACTIVATE: &str = "__bedcode_deactivate";
    /// 调用自定义命令（结果写入 out_ptr）
    pub const INVOKE_COMMAND: &str = "__bedcode_invoke_command";
    /// 终端输入处理（结果写入 out_ptr，(0,0) 表示不修改）
    pub const ON_TERMINAL_INPUT: &str = "__bedcode_on_terminal_input";
    /// 终端输出处理（结果写入 out_ptr，(0,0) 表示不修改）
    pub const ON_TERMINAL_OUTPUT: &str = "__bedcode_on_terminal_output";
    /// 应用启动完成回调
    pub const ON_STARTUP: &str = "__bedcode_on_startup";
    /// 应用即将关闭回调
    pub const ON_SHUTDOWN: &str = "__bedcode_on_shutdown";
    /// 接收消息总线消息（可选导出）
    pub const ON_MESSAGE: &str = "__bedcode_on_message";
    /// 接收会话生命周期事件（可选导出）
    pub const ON_SESSION_LIFECYCLE: &str = "__bedcode_on_session_lifecycle";
    /// ABI 版本协商（v2 起导出；缺失视为 v1 兼容插件）
    pub const ABI_VERSION: &str = "__bedcode_abi_version";
    /// 内存回收器（v2 起导出；缺失时宿主跳过回收，退化为 v1 行为）
    pub const DEALLOCATE: &str = "__bedcode_deallocate";
}

/// 宿主导入函数名（宿主在 Linker 中注册，`WasmHost` 调用）
pub mod import {
    // === Storage ===
    /// 存储：获取值（out_ptr 输出）
    pub const STORAGE_GET: &str = "host_storage_get";
    /// 存储：设置值
    pub const STORAGE_SET: &str = "host_storage_set";
    /// 存储：删除值
    pub const STORAGE_DELETE: &str = "host_storage_delete";

    // === Database（主库，表名前缀校验） ===
    /// 数据库：执行 SQL（返回受影响行数）
    pub const DB_EXECUTE: &str = "host_db_execute";
    /// 数据库：查询 SQL（out_ptr 输出）
    pub const DB_QUERY: &str = "host_db_query";
    /// 数据库：执行 SQL 参数绑定版（params 为 JSON 数组字符串）
    pub const DB_EXECUTE_PARAMS: &str = "host_db_execute_params";
    /// 数据库：查询 SQL 参数绑定版（out_ptr 输出）
    pub const DB_QUERY_PARAMS: &str = "host_db_query_params";

    // === Plugin Database（插件独立库，无前缀校验） ===
    /// 插件独立数据库：执行 SQL（返回受影响行数）
    pub const PLUGIN_DB_EXECUTE: &str = "host_plugin_db_execute";
    /// 插件独立数据库：查询 SQL（out_ptr 输出）
    pub const PLUGIN_DB_QUERY: &str = "host_plugin_db_query";
    /// 插件独立数据库：执行 SQL 参数绑定版（params 为 JSON 数组字符串）
    pub const PLUGIN_DB_EXECUTE_PARAMS: &str = "host_plugin_db_execute_params";
    /// 插件独立数据库：查询 SQL 参数绑定版（out_ptr 输出）
    pub const PLUGIN_DB_QUERY_PARAMS: &str = "host_plugin_db_query_params";

    // === Terminal ===
    /// 终端：发送输入
    pub const TERMINAL_SEND: &str = "host_terminal_send";

    // === Session ===
    /// 会话：列出所有（out_ptr 输出）
    pub const SESSION_LIST: &str = "host_session_list";
    /// 会话：获取单个（out_ptr 输出）
    pub const SESSION_GET: &str = "host_session_get";
    /// 会话配置：列出所有（out_ptr 输出）
    pub const SESSION_CONFIG_LIST: &str = "host_session_config_list";
    /// 会话生命周期：注册监听器
    pub const SESSION_LIFECYCLE_REGISTER: &str = "host_session_lifecycle_register";

    // === Event / Broadcast ===
    /// 事件：向前端发送 Tauri 事件（无返回值）
    pub const EMIT_EVENT: &str = "host_emit_event";
    /// 广播：同步事件到所有客户端（无返回值）
    pub const BROADCAST_SYNC: &str = "host_broadcast_sync";
    /// 通知：发送系统通知
    pub const NOTIFY: &str = "host_notify";

    // === HTTP ===
    /// HTTP 代理：发起请求（out_ptr 输出）
    pub const HTTP_FETCH: &str = "host_http_fetch";

    // === File System ===
    /// 文件系统：读取文件（out_ptr 输出）
    pub const FS_READ: &str = "host_fs_read";
    /// 文件系统：写入文件
    pub const FS_WRITE: &str = "host_fs_write";
    /// 文件系统：复制文件
    pub const FS_COPY: &str = "host_fs_copy";

    // === Config ===
    /// 配置：读取白名单配置项（out_ptr 输出）
    pub const CONFIG_GET: &str = "host_config_get";

    // === Logging ===
    /// 日志：info（无返回值）
    pub const LOG_INFO: &str = "host_log_info";
    /// 日志：debug（无返回值）
    pub const LOG_DEBUG: &str = "host_log_debug";
    /// 日志：warn（无返回值）
    pub const LOG_WARN: &str = "host_log_warn";
    /// 日志：error（无返回值）
    pub const LOG_ERROR: &str = "host_log_error";

    // === Message Bus ===
    /// 消息总线：发布消息
    pub const BUS_PUBLISH: &str = "host_bus_publish";
    /// 消息总线：订阅 topic
    pub const BUS_SUBSCRIBE: &str = "host_bus_subscribe";
    /// 消息总线：取消订阅
    pub const BUS_UNSUBSCRIBE: &str = "host_bus_unsubscribe";
}

/// 宿主导入函数签名表 — (名称, 参数个数, 返回值个数)
///
/// 宿主侧测试遍历此表，校验 Linker 实际注册签名一致
pub const HOST_FN_SIGNATURES: &[(&str, usize, usize)] = &[
    (import::STORAGE_GET, 3, 1),
    (import::STORAGE_SET, 4, 1),
    (import::STORAGE_DELETE, 2, 1),
    (import::DB_EXECUTE, 2, 1),
    (import::DB_QUERY, 3, 1),
    (import::DB_EXECUTE_PARAMS, 4, 1),
    (import::DB_QUERY_PARAMS, 5, 1),
    (import::PLUGIN_DB_EXECUTE, 2, 1),
    (import::PLUGIN_DB_QUERY, 3, 1),
    (import::PLUGIN_DB_EXECUTE_PARAMS, 4, 1),
    (import::PLUGIN_DB_QUERY_PARAMS, 5, 1),
    (import::TERMINAL_SEND, 4, 1),
    (import::SESSION_LIST, 1, 1),
    (import::SESSION_GET, 3, 1),
    (import::SESSION_CONFIG_LIST, 1, 1),
    (import::SESSION_LIFECYCLE_REGISTER, 0, 1),
    (import::EMIT_EVENT, 4, 0),
    (import::BROADCAST_SYNC, 2, 0),
    (import::NOTIFY, 4, 1),
    (import::HTTP_FETCH, 3, 1),
    (import::FS_READ, 3, 1),
    (import::FS_WRITE, 4, 1),
    (import::FS_COPY, 4, 1),
    (import::CONFIG_GET, 3, 1),
    (import::LOG_INFO, 2, 0),
    (import::LOG_DEBUG, 2, 0),
    (import::LOG_WARN, 2, 0),
    (import::LOG_ERROR, 2, 0),
    (import::BUS_PUBLISH, 4, 1),
    (import::BUS_SUBSCRIBE, 2, 1),
    (import::BUS_UNSUBSCRIBE, 2, 1),
];

/// 插件导出函数签名表 — (名称, 参数个数, 返回值个数)
///
/// 宿主侧 ABI 验证测试（`test_wasm_export_signatures`）遍历此表，
/// 校验测试插件 WASM 模块的导出签名一致
pub const PLUGIN_EXPORT_SIGNATURES: &[(&str, usize, usize)] = &[
    (export::ALLOCATE, 1, 1),
    (export::MANIFEST, 1, 0),
    (export::ACTIVATE, 0, 1),
    (export::DEACTIVATE, 0, 1),
    (export::INVOKE_COMMAND, 5, 0),
    (export::ON_TERMINAL_INPUT, 5, 0),
    (export::ON_TERMINAL_OUTPUT, 5, 0),
    (export::ON_STARTUP, 0, 0),
    (export::ON_SHUTDOWN, 0, 0),
    (export::ON_MESSAGE, 6, 1),
    (export::ON_SESSION_LIFECYCLE, 2, 1),
    (export::ABI_VERSION, 0, 1),
    (export::DEALLOCATE, 2, 0),
];
