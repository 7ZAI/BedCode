//! 宿主上下文与状态类型（移动 fork 形态，票 17）
//!
//! 形状真源 = 移动宿主 `plugin/wasm_runtime.rs`（WasmHostContext，批次 2 迁入
//! `manager::runtime` 后真源归 crate）。机制依赖（FsAuthChecker / MessageBus /
//! PluginStorage）随机制核 fork 进本 crate；宿主引擎调用（auth / egress /
//! peer / mdns 守护 / android 平台桥……）经 [`ports`] 注入——形状见
//! `host_api::ports`（票 17 §6.2，对齐桌面 CapabilityProvider 范式）。

use std::sync::{Arc, Mutex};

use crate::bus::MessageBus;
use crate::db::Database;
use crate::host_api::ports::{FsAuthGate, HostEnginePorts, UnimplementedPorts};
use crate::storage::PluginStorage;

/// 宿主上下文（注入到 WasmPluginState）
///
/// 移动端无 SessionManager 和 PermissionManager（权限门在域函数内经
/// granted_permissions 仲裁，形状与宿主 wasm_runtime.rs 逐字一致）
pub struct WasmHostContext {
    /// 数据库（批次 2b：底库 = crate Database wrapper——宿主主库连接经
    /// `Database::from_connection` 移交；std Mutex：host fn 为同步上下文，
    /// SQL 执行亦为同步操作，无需经 tokio 锁 + block_in_place/block_on 绕行）
    pub db: Arc<Mutex<Database>>,
    /// 插件 KV 存储
    pub storage: Arc<PluginStorage>,
    /// Tauri AppHandle（None 时无头/测试上下文，依赖前端事件的能力降级）
    pub app_handle: Option<Arc<tauri::AppHandle>>,
    /// 文件系统访问校验器（宿主授权闸门经 FsAuthGate 端口投影——
    /// 白名单 / 弹窗 / 持久授权真源留宿主）
    pub fs_auth: Arc<dyn FsAuthGate>,
    /// 消息总线
    pub message_bus: Arc<MessageBus>,
    /// 插件状态上报回调（`host_mark_plugin_error` 触发）
    ///
    /// 由 PluginManager 注入：置 Error 状态 + 持久化未启用 + 前端通知
    pub status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync>,
    /// 插件私有库连接表（host-plugin-database，票 05）：按 plugin_id 惰性打开，
    /// 属主分区；停用回收（purge）时摘除（Drop 关闭连接）。无头上下文内存库
    pub plugin_dbs: Arc<Mutex<std::collections::HashMap<String, Arc<Mutex<Database>>>>>,
    /// 宿主引擎端口（批次 2）：auth / egress / peer / mdns / 平台桥等
    /// 「离宿主无法实现」的引擎调用唯一入口
    pub ports: Arc<dyn HostEnginePorts>,
}

impl WasmHostContext {
    /// 创建宿主上下文
    ///
    /// `app_handle` 为 None 时（无头/测试上下文）依赖前端事件的宿主能力降级
    /// （对齐桌面端形态，见其 wasm_runtime.rs 同名字段注释）
    pub fn new(
        db: Arc<Mutex<Database>>,
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
        fs_auth: Arc<dyn FsAuthGate>,
        message_bus: Arc<MessageBus>,
        status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync>,
        ports: Arc<dyn HostEnginePorts>,
    ) -> Self {
        Self {
            db,
            storage,
            app_handle,
            fs_auth,
            message_bus,
            status_reporter,
            plugin_dbs: Arc::new(Mutex::new(std::collections::HashMap::new())),
            ports,
        }
    }

    /// 无头构造（测试夹具）：端口取 [`UnimplementedPorts`] 占位——
    /// 无头上下文命中引擎调用一律 fail-visible 拒绝
    pub fn new_headless(
        db: Arc<Mutex<Database>>,
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
        fs_auth: Arc<dyn FsAuthGate>,
        message_bus: Arc<MessageBus>,
        status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync>,
    ) -> Self {
        Self::new(db, storage, app_handle, fs_auth, message_bus, status_reporter, Arc::new(UnimplementedPorts))
    }
}
