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
/// - v4: 新增通用文件服务能力（host functions `FILESRV_*` / `TRANSFER_*`
///   + 可选导出 `ON_UPLOAD_REQUEST` 上传策略钩子），与桌面端 ABI v5 同构，
///   见内网文件传输插件规格（移动端 ABI 基线从 v3 起算，故为 v4）
/// - v5: 新增 `host_config_get` 宿主配置读取能力（`AppDownloadsDir` 下载目录）
/// - v6: 新增批量传输批准协议（host functions `FILESRV_APPROVE_TRANSFER` /
///   `FILESRV_REJECT_TRANSFER` / `FILESRV_SET_APPROVAL_TIMEOUT` /
///   `FILESRV_CANCEL_RECEIVING` + 可选导出 `ON_TRANSFER_REQUEST` 批钩子），
///   见内网文件传输插件 v2 规格（接收策略 / 异步批量批准）
pub const ABI_VERSION: u32 = 6;

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
    /// 上传请求策略钩子（可选导出，v4 起；决定写入 out_ptr）
    ///
    /// 宿主在上传会话创建时调用一次（写任何字节前），2 秒超时；
    /// 缺失/超时/异常一律 fail-closed 拒绝上传。
    /// 与桌面端 SDK `abi::export::ON_UPLOAD_REQUEST` 同名同义
    pub const ON_UPLOAD_REQUEST: &str = "__bedcode_on_upload_request";
    /// 批量传输请求钩子（可选导出，v6 起；决定写入 out_ptr）
    ///
    /// 宿主在 POST /transfer-request 时调用一次（批级三路分流 allow/ask/deny）；
    /// 缺失/超时/异常一律 fail-closed 拒绝（deny）。
    /// 与桌面端 SDK `abi::export::ON_TRANSFER_REQUEST` 同名同义
    pub const ON_TRANSFER_REQUEST: &str = "__bedcode_on_transfer_request";
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
    /// 文件系统：检查文件是否存在（返回 i32: 1=存在, 0=不存在, -1=错误）
    pub const FS_EXISTS: &str = "host_fs_exists";
    /// 文件系统：删除文件（返回 i32: 0=成功，-1=失败；不存在视为成功）
    pub const FS_DELETE: &str = "host_fs_delete";
    /// 消息总线：发布消息
    pub const BUS_PUBLISH: &str = "host_bus_publish";
    /// 消息总线：订阅 topic
    pub const BUS_SUBSCRIBE: &str = "host_bus_subscribe";
    /// 消息总线：取消订阅
    pub const BUS_UNSUBSCRIBE: &str = "host_bus_unsubscribe";
    /// 插件状态：标记插件为错误状态（宿主置 Error + 持久化未启用 + 通知前端）
    pub const MARK_PLUGIN_ERROR: &str = "host_mark_plugin_error";

    // === File Service（v4） ===
    /// 文件服务：挂载（MountOptions JSON → out_ptr 输出 MountResult JSON）
    ///
    /// 与桌面端 SDK `abi::import::FILESRV_MOUNT` 同名同签名
    pub const FILESRV_MOUNT: &str = "host_filesrv_mount";
    /// 文件服务：卸载挂载点
    pub const FILESRV_UNMOUNT: &str = "host_filesrv_unmount";
    /// 文件服务：更新挂载点允许目录根
    pub const FILESRV_UPDATE_ROOTS: &str = "host_filesrv_update_roots";
    /// 文件服务：获取对端文件服务信息（out_ptr 输出）
    pub const FILESRV_GET_PEER: &str = "host_filesrv_get_peer";

    // === 批量传输批准（v6） ===
    /// 文件服务：批准传输批（接收端用户应答「接受全部」；批 pending → approved）
    pub const FILESRV_APPROVE_TRANSFER: &str = "host_filesrv_approve_transfer";
    /// 文件服务：拒绝传输批（接收端用户应答「拒绝全部」；批 pending → rejected）
    pub const FILESRV_REJECT_TRANSFER: &str = "host_filesrv_reject_transfer";
    /// 文件服务：设置批准超时（秒，10–600；仅 ask 策略生效，宿主 TTL 扫描用）
    pub const FILESRV_SET_APPROVAL_TIMEOUT: &str = "host_filesrv_set_approval_timeout";
    /// 文件服务：取消接收中的上传会话（接收端本地取消，session 级）
    pub const FILESRV_CANCEL_RECEIVING: &str = "host_filesrv_cancel_receiving";

    // === Transfer（v4） ===
    /// 传输引擎：启动传输任务（TransferRequest JSON → out_ptr 输出 task_id）
    pub const TRANSFER_START: &str = "host_transfer_start";
    /// 传输引擎：取消传输任务
    pub const TRANSFER_CANCEL: &str = "host_transfer_cancel";

    // === Config（v5） ===
    /// 配置：读取宿主配置项（key 字符串 → out_ptr 输出 value 字符串）
    pub const CONFIG_GET: &str = "host_config_get";
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
    (import::FS_EXISTS, 2, 1),
    (import::FS_DELETE, 2, 1),
    (import::BUS_PUBLISH, 4, 1),
    (import::BUS_SUBSCRIBE, 2, 1),
    (import::BUS_UNSUBSCRIBE, 2, 1),
    (import::MARK_PLUGIN_ERROR, 2, 0),
    (import::FILESRV_MOUNT, 3, 1),
    (import::FILESRV_UNMOUNT, 2, 1),
    (import::FILESRV_UPDATE_ROOTS, 4, 1),
    (import::FILESRV_GET_PEER, 3, 1),
    (import::FILESRV_APPROVE_TRANSFER, 2, 1),
    (import::FILESRV_REJECT_TRANSFER, 2, 1),
    (import::FILESRV_SET_APPROVAL_TIMEOUT, 3, 1),
    (import::FILESRV_CANCEL_RECEIVING, 2, 1),
    (import::TRANSFER_START, 3, 1),
    (import::TRANSFER_CANCEL, 2, 1),
    (import::CONFIG_GET, 3, 1),
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
    (export::ON_UPLOAD_REQUEST, 3, 1),
    (export::ON_TRANSFER_REQUEST, 3, 1),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_magic_constants() {
        // 锁死核心 ABI 常量：命名空间/内存导出名/out_ptr 大小/版本号均被
        // 宿主 Linker 与插件二进制双向引用，漂移会导致加载失败
        assert_eq!(NAMESPACE, "bedcode");
        assert_eq!(MEMORY, "memory");
        assert_eq!(RESULT_PAIR_SIZE, 8);
        assert_eq!(ABI_VERSION, 6);
    }

    #[test]
    fn test_export_names_are_abi_contract() {
        // 插件导出符号名：宿主按字面量在模块中查找，改名即失配 —— 全部锁死
        assert_eq!(export::ALLOCATE, "__bedcode_allocate");
        assert_eq!(export::DEALLOCATE, "__bedcode_deallocate");
        assert_eq!(export::ABI_VERSION, "__bedcode_abi_version");
        assert_eq!(export::MANIFEST, "__bedcode_manifest");
        assert_eq!(export::ACTIVATE, "__bedcode_activate");
        assert_eq!(export::DEACTIVATE, "__bedcode_deactivate");
        assert_eq!(export::INVOKE_COMMAND, "__bedcode_invoke_command");
        assert_eq!(export::ON_TERMINAL_INPUT, "__bedcode_on_terminal_input");
        assert_eq!(export::ON_TERMINAL_OUTPUT, "__bedcode_on_terminal_output");
        assert_eq!(export::ON_STARTUP, "__bedcode_on_startup");
        assert_eq!(export::ON_SHUTDOWN, "__bedcode_on_shutdown");
        // 移动端特有回调（WebSocket 认证/断开/会话生命周期）
        assert_eq!(export::ON_AUTH_SUCCESS, "__bedcode_on_auth_success");
        assert_eq!(export::ON_DISCONNECT, "__bedcode_on_disconnect");
        assert_eq!(export::ON_SESSION_CREATED, "__bedcode_on_session_created");
        assert_eq!(export::ON_SESSION_STOPPED, "__bedcode_on_session_stopped");
        assert_eq!(export::ON_BUS_MESSAGE, "__bedcode_on_bus_message");
        assert_eq!(export::ON_UPLOAD_REQUEST, "__bedcode_on_upload_request");
        assert_eq!(export::ON_TRANSFER_REQUEST, "__bedcode_on_transfer_request");
    }

    #[test]
    fn test_import_names_are_abi_contract() {
        // 宿主导入符号名：WasmHost 侧 extern 声明与宿主 Linker 注册按此字面量配对
        assert_eq!(import::STORAGE_GET, "host_storage_get");
        assert_eq!(import::STORAGE_SET, "host_storage_set");
        assert_eq!(import::STORAGE_DELETE, "host_storage_delete");
        assert_eq!(import::DB_EXECUTE, "host_db_execute");
        assert_eq!(import::DB_QUERY, "host_db_query");
        assert_eq!(import::TERMINAL_SEND, "host_terminal_send");
        assert_eq!(import::SESSION_LIST, "host_session_list");
        assert_eq!(import::SESSION_GET, "host_session_get");
        assert_eq!(import::EMIT_EVENT, "host_emit_event");
        assert_eq!(import::HTTP_FETCH, "host_http_fetch");
        assert_eq!(import::LOG_INFO, "host_log_info");
        assert_eq!(import::LOG_DEBUG, "host_log_debug");
        assert_eq!(import::LOG_WARN, "host_log_warn");
        assert_eq!(import::LOG_ERROR, "host_log_error");
        assert_eq!(import::NOTIFY, "host_notify");
        assert_eq!(import::FS_READ, "host_fs_read");
        assert_eq!(import::FS_WRITE, "host_fs_write");
        assert_eq!(import::FS_COPY, "host_fs_copy");
        assert_eq!(import::FS_EXISTS, "host_fs_exists");
        assert_eq!(import::FS_DELETE, "host_fs_delete");
        assert_eq!(import::BUS_PUBLISH, "host_bus_publish");
        assert_eq!(import::BUS_SUBSCRIBE, "host_bus_subscribe");
        assert_eq!(import::BUS_UNSUBSCRIBE, "host_bus_unsubscribe");
        assert_eq!(import::MARK_PLUGIN_ERROR, "host_mark_plugin_error");
        // v4 文件服务 / 传输
        assert_eq!(import::FILESRV_MOUNT, "host_filesrv_mount");
        assert_eq!(import::FILESRV_UNMOUNT, "host_filesrv_unmount");
        assert_eq!(import::FILESRV_UPDATE_ROOTS, "host_filesrv_update_roots");
        assert_eq!(import::FILESRV_GET_PEER, "host_filesrv_get_peer");
        // v6 批量传输批准
        assert_eq!(import::FILESRV_APPROVE_TRANSFER, "host_filesrv_approve_transfer");
        assert_eq!(import::FILESRV_REJECT_TRANSFER, "host_filesrv_reject_transfer");
        assert_eq!(import::FILESRV_SET_APPROVAL_TIMEOUT, "host_filesrv_set_approval_timeout");
        assert_eq!(import::FILESRV_CANCEL_RECEIVING, "host_filesrv_cancel_receiving");
        assert_eq!(import::TRANSFER_START, "host_transfer_start");
        assert_eq!(import::TRANSFER_CANCEL, "host_transfer_cancel");
        // v5 配置读取
        assert_eq!(import::CONFIG_GET, "host_config_get");
    }

    #[test]
    fn test_host_signature_table_contract() {
        // 宿主侧测试遍历此表校验 Linker 实际注册签名，漂移在测试期暴露；
        // 锁死总数与关键行（参数/返回个数 = 宿主 RegisterFunc 签名）
        assert_eq!(HOST_FN_SIGNATURES.len(), 35);
        // 无重复名称（宿主注册冲突会 panic）
        let mut names: Vec<&str> = HOST_FN_SIGNATURES.iter().map(|(n, _, _)| *n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 35);
        // 关键行锁死：out_ptr 函数 (ptr,len)+out_ptr = 3 参数，返回状态码
        assert_eq!(HOST_FN_SIGNATURES[0], (import::STORAGE_GET, 3, 1));
        assert_eq!(HOST_FN_SIGNATURES[1], (import::STORAGE_SET, 4, 1));
        assert_eq!(HOST_FN_SIGNATURES[8], (import::EMIT_EVENT, 4, 0));
        assert_eq!(HOST_FN_SIGNATURES[9], (import::HTTP_FETCH, 3, 1));
        assert_eq!(HOST_FN_SIGNATURES[18], (import::FS_EXISTS, 2, 1));
        assert_eq!(HOST_FN_SIGNATURES[23], (import::MARK_PLUGIN_ERROR, 2, 0));
        assert_eq!(HOST_FN_SIGNATURES[24], (import::FILESRV_MOUNT, 3, 1));
        // v6 批量传输批准（FILESRV_* 后追加）
        assert_eq!(HOST_FN_SIGNATURES[28], (import::FILESRV_APPROVE_TRANSFER, 2, 1));
        assert_eq!(HOST_FN_SIGNATURES[29], (import::FILESRV_REJECT_TRANSFER, 2, 1));
        assert_eq!(HOST_FN_SIGNATURES[30], (import::FILESRV_SET_APPROVAL_TIMEOUT, 3, 1));
        assert_eq!(HOST_FN_SIGNATURES[31], (import::FILESRV_CANCEL_RECEIVING, 2, 1));
        assert_eq!(HOST_FN_SIGNATURES[32], (import::TRANSFER_START, 3, 1));
        assert_eq!(HOST_FN_SIGNATURES[33], (import::TRANSFER_CANCEL, 2, 1));
        assert_eq!(HOST_FN_SIGNATURES[34], (import::CONFIG_GET, 3, 1));
    }

    #[test]
    fn test_plugin_export_signature_table_contract() {
        // 宿主加载 WASM 模块后按此表校验导出签名；锁死总数与关键行
        assert_eq!(PLUGIN_EXPORT_SIGNATURES.len(), 18);
        let mut names: Vec<&str> = PLUGIN_EXPORT_SIGNATURES.iter().map(|(n, _, _)| *n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 18);
        // 版本协商：无参数返回 i32；out_ptr 型导出：参数 + 1 个 out_ptr，无返回值
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[0], (export::ALLOCATE, 1, 1));
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[2], (export::ABI_VERSION, 0, 1));
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[6], (export::INVOKE_COMMAND, 5, 0));
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[9], (export::ON_STARTUP, 0, 0));
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[11], (export::ON_AUTH_SUCCESS, 0, 0));
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[12], (export::ON_DISCONNECT, 2, 0));
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[15], (export::ON_BUS_MESSAGE, 7, 1));
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[16], (export::ON_UPLOAD_REQUEST, 3, 1));
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[17], (export::ON_TRANSFER_REQUEST, 3, 1));
    }

    #[test]
    fn test_signature_tables_are_sorted_and_complete() {
        // 表按 import/export 模块声明顺序组织：新增 host function 必须同步
        // 追加到表尾（索引断言依赖此顺序，顺序变更会在此测试暴露）
        assert_eq!(HOST_FN_SIGNATURES[10].0, import::LOG_INFO);
        assert_eq!(HOST_FN_SIGNATURES[11].0, import::LOG_DEBUG);
        assert_eq!(HOST_FN_SIGNATURES[12].0, import::LOG_WARN);
        assert_eq!(HOST_FN_SIGNATURES[13].0, import::LOG_ERROR);
        assert_eq!(HOST_FN_SIGNATURES[14].0, import::NOTIFY);
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[3].0, export::MANIFEST);
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[4].0, export::ACTIVATE);
        assert_eq!(PLUGIN_EXPORT_SIGNATURES[5].0, export::DEACTIVATE);
    }
}
