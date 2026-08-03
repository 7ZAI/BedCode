//! WASM ABI 契约 — 单一事实来源
//!
//! 宿主（wasmtime Linker 注册）与插件（`wasm_entry!` 导出 / `WasmHost` 导入）
//! 共同引用本模块的名称常量与签名表。
//!
//! # ABI 约定
//!
//! - 所有 host functions 注册在 [`NAMESPACE`] 命名空间下
//! - 字符串参数以 (ptr, len) 对传递，指向 WASM 线性内存
//! - 返回 (ptr, len) 对的函数通过 out_ptr 输出参数传递结果
//!   （[`RESULT_PAIR_SIZE`] 字节: ptr:u32 + len:u32，小端序），而非 Rust 元组返回值
//!   （C ABI 多值返回不跨工具链稳定）
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
/// - v1: 初始版本（21 个 host functions + 14 个插件导出，元组返回）
/// - v2: 新增插件状态上报（`host_mark_plugin_error`），修复
///   on_app_startup/on_app_shutdown 导出名漂移（宿主曾用错误名字查找）
/// - v3: 结果传递改为 out_ptr（对齐桌面端），新增 `__bedcode_abi_version` /
///   `__bedcode_deallocate`，消除元组返回的 FFI-safe 警告与线性内存泄漏
pub const ABI_VERSION: u32 = 3;

/// 插件导出函数名（`wasm_entry!` 宏生成，宿主调用）
pub mod export {
    /// 内存分配器 — 宿主写入字符串前分配 len 字节
    pub const ALLOCATE: &str = "__bedcode_allocate";
    /// 内存回收器 — 释放 `__bedcode_allocate` / `wasm_alloc_string` 分配的内存
    pub const DEALLOCATE: &str = "__bedcode_deallocate";
    /// ABI 版本协商
    pub const ABI_VERSION: &str = "__bedcode_abi_version";
    /// 返回 manifest JSON（结果写入 out_ptr）
    pub const MANIFEST: &str = "__bedcode_manifest";
    /// 激活插件（返回 0 成功，非 0 失败）
    pub const ACTIVATE: &str = "__bedcode_activate";
    /// 停用插件（返回 0 成功，非 0 失败）
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
    /// 认证成功回调（移动端特有：WebSocket 认证成功后触发）
    pub const ON_AUTH_SUCCESS: &str = "__bedcode_on_auth_success";
    /// 连接断开回调（移动端特有：携带断开原因）
    pub const ON_DISCONNECT: &str = "__bedcode_on_disconnect";
    /// 会话创建回调（移动端特有）
    pub const ON_SESSION_CREATED: &str = "__bedcode_on_session_created";
    /// 会话停止回调（移动端特有）
    pub const ON_SESSION_STOPPED: &str = "__bedcode_on_session_stopped";
    /// 接收消息总线消息（可选导出，返回 0 成功，非 0 失败）
    pub const ON_BUS_MESSAGE: &str = "__bedcode_on_bus_message";
}

/// 宿主导入函数名（宿主在 Linker 中注册，`WasmHost` 调用）
pub mod import {
    /// 存储：获取值
    pub const STORAGE_GET: &str = "host_storage_get";
    /// 存储：设置值
    pub const STORAGE_SET: &str = "host_storage_set";
    /// 存储：删除值
    pub const STORAGE_DELETE: &str = "host_storage_delete";
    /// 数据库：执行 SQL（返回受影响行数）
    pub const DB_EXECUTE: &str = "host_db_execute";
    /// 数据库：查询 SQL
    pub const DB_QUERY: &str = "host_db_query";
    /// 终端：发送输入
    pub const TERMINAL_SEND: &str = "host_terminal_send";
    /// 会话：列出所有
    pub const SESSION_LIST: &str = "host_session_list";
    /// 会话：获取单个
    pub const SESSION_GET: &str = "host_session_get";
    /// 事件：向前端发送 Tauri 事件（无返回值）
    pub const EMIT_EVENT: &str = "host_emit_event";
    /// HTTP 代理：发起请求
    pub const HTTP_FETCH: &str = "host_http_fetch";
    /// 日志：info
    pub const LOG_INFO: &str = "host_log_info";
    /// 日志：debug
    pub const LOG_DEBUG: &str = "host_log_debug";
    /// 日志：warn
    pub const LOG_WARN: &str = "host_log_warn";
    /// 日志：error
    pub const LOG_ERROR: &str = "host_log_error";
    /// 通知：发送系统通知
    pub const NOTIFY: &str = "host_notify";
    /// 文件系统：读取文件
    pub const FS_READ: &str = "host_fs_read";
    /// 文件系统：写入文件
    pub const FS_WRITE: &str = "host_fs_write";
    /// 文件系统：复制文件
    pub const FS_COPY: &str = "host_fs_copy";
    /// 消息总线：发布消息
    pub const BUS_PUBLISH: &str = "host_bus_publish";
    /// 消息总线：订阅 topic
    pub const BUS_SUBSCRIBE: &str = "host_bus_subscribe";
    /// 消息总线：取消订阅
    pub const BUS_UNSUBSCRIBE: &str = "host_bus_unsubscribe";
    /// 插件状态：标记插件为错误状态（宿主置 Error + 持久化未启用 + 通知前端）
    pub const MARK_PLUGIN_ERROR: &str = "host_mark_plugin_error";
}

/// 宿主导入函数签名表 — (名称, 参数个数, 返回值个数)
///
/// 宿主侧测试遍历此表，校验 Linker 实际注册签名一致。
/// v3 起返回 (ptr, len) 对的函数改为 out_ptr 输出参数 + i32 状态码。
pub const HOST_FN_SIGNATURES: &[(&str, usize, usize)] = &[
    (import::STORAGE_GET, 3, 1),
    (import::STORAGE_SET, 4, 1),
    (import::STORAGE_DELETE, 2, 1),
    (import::DB_EXECUTE, 2, 1),
    (import::DB_QUERY, 3, 1),
    (import::TERMINAL_SEND, 4, 1),
    (import::SESSION_LIST, 1, 1),
    (import::SESSION_GET, 3, 1),
    (import::EMIT_EVENT, 4, 0),
    (import::HTTP_FETCH, 3, 1),
    (import::LOG_INFO, 2, 0),
    (import::LOG_DEBUG, 2, 0),
    (import::LOG_WARN, 2, 0),
    (import::LOG_ERROR, 2, 0),
    (import::NOTIFY, 4, 1),
    (import::FS_READ, 3, 1),
    (import::FS_WRITE, 4, 1),
    (import::FS_COPY, 4, 1),
    (import::BUS_PUBLISH, 4, 1),
    (import::BUS_SUBSCRIBE, 2, 1),
    (import::BUS_UNSUBSCRIBE, 2, 1),
    (import::MARK_PLUGIN_ERROR, 2, 0),
];

/// 插件导出函数签名表 — (名称, 参数个数, 返回值个数)
///
/// 供宿主加载 WASM 模块后做导出签名校验（v3 起结果传递走 out_ptr）
pub const PLUGIN_EXPORT_SIGNATURES: &[(&str, usize, usize)] = &[
    (export::ALLOCATE, 1, 1),
    (export::DEALLOCATE, 2, 0),
    (export::ABI_VERSION, 0, 1),
    (export::MANIFEST, 1, 0),
    (export::ACTIVATE, 0, 1),
    (export::DEACTIVATE, 0, 1),
    (export::INVOKE_COMMAND, 5, 0),
    (export::ON_TERMINAL_INPUT, 5, 0),
    (export::ON_TERMINAL_OUTPUT, 5, 0),
    (export::ON_STARTUP, 0, 0),
    (export::ON_SHUTDOWN, 0, 0),
    (export::ON_AUTH_SUCCESS, 0, 0),
    (export::ON_DISCONNECT, 2, 0),
    (export::ON_SESSION_CREATED, 2, 0),
    (export::ON_SESSION_STOPPED, 2, 0),
    (export::ON_BUS_MESSAGE, 7, 1),
];
