//! 宿主引擎端口（批次 2 · 票 17 §6.2）——插件机制面与宿主引擎的唯一桥
//!
//! 运行时与 16 域迁入本 crate 后，所有「离宿主无法实现」的引擎调用不再直引
//! 宿主符号（`crate::state::get_auth_manager` / `crate::peer_net::*` /
//! `crate::egress::policy` / android_plugins 桥……），一律经本端口注入：
//!
//! - **引擎资产留宿主**：auth 引擎（C4 凭据）、egress 安全闸门（D5）、连接
//!   管理器、peer 四模块、mDNS 共享守护、WS 重连状态机、Android 平台桥
//!   （android_plugins / SAF）——端口只声明调用形状，实现由宿主装配
//!   （宿主 `plugin/host_ports.rs`）。
//! - **机制随 crate**：句柄表、属主仲裁、权限门、事件定向投递——域文件内，
//!   不经端口。
//! - **DTO 以原始值 / JSON 过界**：宿主侧类型（DialEndpoint、RemotePullFileDto、
//!   ConsentRequest…）不出宿主，域内只传原始值或 JSON 串，反序列化责任在
//!   端口实现方——契约形状与 WIT 边界一致（JSON 字符串过界先例）。
//!
//! 无头 / 测试上下文用 [`UnimplementedPorts`]（fail-visible：所有方法返回
//! 显性错误或空值，禁 panic）。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::error::Result;

// ==================== 子 trait：引擎对象面 ====================

/// 设备入场认证引擎面（C4：密码学与凭据留宿主，域内只投影调用结果）
///
/// 形状对齐宿主 `auth::manager::AuthManager` 的插件消费面五方法；
/// `get_credentials` 只投影存在性（凭据零过境）。
#[async_trait]
pub trait AuthEnginePort: Send + Sync {
    async fn request_pairing(&self) -> Result<()>;
    async fn verify_pairing_code(&self, code: &str) -> Result<bool>;
    async fn authenticate_with_qr(&self, token: &str) -> Result<bool>;
    async fn authenticate_with_biometric(&self) -> Result<bool>;
    /// 宿主当前是否持有认证凭据（JWT）——只投影存在性
    async fn has_credentials(&self) -> bool;
}

/// 主连接事实面（票 12 `host-connection.primary-target` 的引擎读数）
///
/// `Ok(None)` = 从未配置目标；`Ok(Some((target, connected)))` = 传输事实。
#[async_trait]
pub trait ConnectionEnginePort: Send + Sync {
    async fn primary_target(&self) -> Result<Option<(PrimaryTarget, bool)>>;
}

/// 主连接目标（引擎事实：最后配置的目标地址 / 端口）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryTarget {
    pub address: String,
    pub port: u16,
}

/// SAF 桥面（Android MediaStore / 保存对话框；宿主 `saf_io::SafIo` 的窄投影）
pub trait SafIoPort: Send + Sync {
    fn write_media_downloads(&self, src: &str, display_name: &str, mime_type: &str) -> std::result::Result<(), String>;
    fn save_to_document(&self, src: &str, suggested_name: &str, mime_type: &str) -> std::result::Result<(), String>;
}

/// WS 自动重连策略面（票 12 R1：全局退避单一事实源 `connection::reconnect`
/// 留宿主；crate 侧按策略推进重连循环）
#[async_trait]
pub trait WsReconnectPolicyPort: Send + Sync {
    /// 推进一轮排期（无限重试下恒 `Some`；防御性保留放弃分支）
    async fn start(&self) -> Option<()>;
    /// 当前轮次的退避延迟
    async fn get_delay(&self) -> Duration;
    /// 重连成功回报（重置退避状态）
    async fn on_success(&self);
}

/// 文件授权操作（crate 自有枚举；宿主实现方映射到宿主 FsOp——形状解耦，
/// 宿主 fs_auth 白名单 / 弹窗 / 持久授权真源全部留宿主）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsAuthOp {
    Read,
    Write,
}

/// 文件授权闸门面（宿主 fs_auth 的窄投影）
///
/// crate 侧只问「这个插件能不能读/写这个路径」；授权判定链（路径白名单 →
/// 插件白名单 → 弹窗授权）是宿主安全闸门（ADR 0022 §5.1.3 ②），真源在宿主。
#[async_trait]
pub trait FsAuthGate: Send + Sync {
    async fn check(&self, plugin_id: &str, path: &str, op: FsAuthOp) -> bool;
    async fn check_batch(&self, plugin_id: &str, paths: &[String], op: FsAuthOp) -> bool;
}

// ==================== 宿主引擎端口主 trait ====================

/// 宿主引擎端口（`WasmHostContext.ports` 注入；域函数唯一宿主调用面）
#[async_trait]
pub trait HostEnginePorts: Send + Sync {
    // ==================== egress（D5：授权策略整体留宿主） ====================

    /// 外网出口三层判定 + 需授权时弹窗回执，一次完成（decide + consent 编排
    /// 是宿主安全闸门的内聚实现，crate 侧只见「放行 / 拒绝（含错误码文本）」）
    async fn egress_check(&self, app: &tauri::AppHandle, url: &str, source: &str) -> std::result::Result<(), String>;

    /// 跳转重校验策略（302 → 内网 / 云元数据须过同源 / 桌面目标 / 私网链白名单）
    fn egress_redirect_policy(&self) -> reqwest::redirect::Policy;

    // ==================== auth / token（C4） ====================

    /// 认证引擎（None = 无头 / 未装配；域内 fail-visible 拒绝）
    fn auth_engine(&self) -> Option<Arc<dyn AuthEnginePort>>;

    /// 全局 JWT token（jwt-auth 首帧代发 / 宿主代注 Bearer；token 不落插件）
    fn global_token(&self) -> String;

    // ==================== connection ====================

    /// 主连接事实读数
    fn connection_engine(&self) -> Arc<dyn ConnectionEnginePort>;

    // ==================== ws 重连 ====================

    /// 重连退避策略（参数钳制已在域内完成，宿主实现只出策略对象）
    fn reconnect_policy(&self, max_retries: u32, base_ms: u64, max_ms: u64) -> Box<dyn WsReconnectPolicyPort>;

    /// 重连退避钳制边界 `(min_delay_ms, max_delay_ms)`（宿主全局常量的投影）
    fn reconnect_bounds(&self) -> (u64, u64);

    // ==================== mdns ====================

    /// 本机 peer 节点 ID（自播回显过滤；未启动 None）
    fn current_node_id(&self, app: &tauri::AppHandle) -> Option<String>;

    // ==================== peer：peer_net 引擎 ====================

    /// 拨号建连（返回引擎应答的 `status` 字段：`"connected"` / 其他）
    async fn peer_dial_endpoint(
        &self,
        app: &tauri::AppHandle,
        node_id: String,
        addr: String,
        port: u16,
    ) -> Result<String>;

    /// 断开对端（返回是否命中；`close` 统一资源关闭的 session 路由）
    async fn peer_disconnect(&self, app: &tauri::AppHandle, node_id: String) -> Result<bool>;

    /// 首连应答
    async fn peer_respond_consent(&self, app: &tauri::AppHandle, request_id: String, accepted: bool) -> Result<bool>;

    /// 信任列表（JSON 序列化由实现方完成）
    async fn peer_list_trusted(&self, app: &tauri::AppHandle) -> Result<String>;

    /// 撤销信任
    async fn peer_revoke_trusted(&self, app: &tauri::AppHandle, node_id: String) -> Result<bool>;

    /// 全量幂等替换引擎广播源（`[{ id, name, safTreeUri }]` JSON，camelCase）
    async fn peer_set_shared_roots(&self, app: &tauri::AppHandle, entries_json: String) -> Result<()>;

    /// 按需启动本机节点（幂等；caller = 属主）
    async fn peer_start_node(&self, app: &tauri::AppHandle, caller: &str) -> Result<bool>;

    /// 属主插件让节点下线（非属主拒绝）
    async fn peer_stop_node(&self, app: &tauri::AppHandle, caller: &str) -> Result<bool>;

    // ==================== peer：transfer / receive / remote ====================

    /// 取消发送批
    async fn peer_cancel_transfer(&self, app: &tauri::AppHandle, batch_id: String) -> Result<bool>;

    /// 取消/拒绝接收批（pending 即拒；`close` 统一资源关闭的接收侧路由）
    async fn peer_cancel_receiving(&self, app: &tauri::AppHandle, batch_id: String) -> Result<bool>;

    /// 发送一批文件（paths 已由域内完成双形态解析与并发脉冲字段拒绝）
    async fn peer_send_files_with_policy(
        &self,
        app: &tauri::AppHandle,
        node_id: String,
        paths: Vec<String>,
        force_encrypt: Option<bool>,
    ) -> Result<String>;

    /// 接收批应答
    async fn peer_respond_transfer(&self, app: &tauri::AppHandle, batch_id: String, accept: bool) -> Result<()>;

    /// 接收策略
    async fn peer_set_receive_policy(&self, app: &tauri::AppHandle, mode: String, timeout_secs: u64) -> Result<()>;

    /// 暂停发送批（返回是否命中）
    async fn peer_pause_transfer(&self, app: &tauri::AppHandle, batch_id: String) -> Result<bool>;

    /// 恢复发送批（返回是否命中）
    async fn peer_resume_transfer(&self, app: &tauri::AppHandle, batch_id: String) -> Result<bool>;

    /// 设置接收落点（None = 恢复默认）
    async fn peer_set_download_dir(&self, app: &tauri::AppHandle, path: Option<String>) -> Result<()>;

    /// 远端共享根列表（JSON 由实现方序列化）
    async fn peer_list_shared_roots(&self, app: &tauri::AppHandle, node_id: String) -> Result<String>;

    /// 远端目录浏览（JSON 由实现方序列化）
    async fn peer_browse_directory(
        &self,
        app: &tauri::AppHandle,
        node_id: String,
        dir_id: String,
        rel_path: String,
    ) -> Result<String>;

    /// 拉取文件（files_json 契约归宿主实现解析）
    async fn peer_pull_files(
        &self,
        app: &tauri::AppHandle,
        node_id: String,
        dir_id: String,
        files_json: String,
    ) -> Result<u32>;

    /// 活跃传输批清单（send + receive pending + pull 三表聚合投影，JSON）
    async fn peer_active_transfers(&self, app: &tauri::AppHandle) -> Result<String>;

    /// 发送源收集（`[{ path, size }]` JSON；无 app 依赖——纯元数据面）
    async fn peer_collect_outgoing(&self, paths: Vec<String>) -> Result<String>;

    // ==================== platform / fs / config / db / notify ====================

    /// 系统多文件选择器（返回路径列表；用户取消为空）
    async fn platform_pick_files(&self, app: &tauri::AppHandle) -> Result<Vec<String>>;

    /// SAF 共享目录树选择器（用户取消 None；`(uri, doc_id, display_name)`）
    async fn platform_pick_shared_directory(&self) -> Result<Option<(String, String, String)>>;

    /// 解析 app 下载目录（Kotlin 桥外部私有目录 → app_data 回退，惰性创建）
    async fn resolve_downloads_dir(&self, app: &tauri::AppHandle) -> Result<String>;

    /// Android 经 Kotlin FileDeletePlugin 删文件（仅 Android 分支调用）
    async fn delete_file_android(&self, path: String) -> Result<()>;

    /// 写入落点越界校验（src 必须在宿主解析的 app 下载目录内）
    async fn is_within_app_downloads_dir(&self, app: &tauri::AppHandle, path: &str) -> Result<bool>;

    /// SAF 桥句柄（None = 未装配）
    fn saf_io(&self, app: &tauri::AppHandle) -> Option<Arc<dyn SafIoPort>>;

    /// 应用数据目录（插件私有库落点解析）
    async fn app_data_dir(&self, app: &tauri::AppHandle) -> Result<PathBuf>;

    /// Android 系统通知（TaskNotificationPlugin；非 Android 平台实现方返回 Err）
    async fn notify_show(&self, plugin_id: &str, handle: &tokio::runtime::Handle, title: &str, body: &str) -> std::result::Result<(), String>;
}

// ==================== 无头默认实现 ====================

/// 未装配端口的占位实现（无头 / 测试上下文；所有调用 fail-visible）
///
/// 全部方法返回显性错误或空值——**禁 panic**（wasmtime host fn 内 panic 会
/// 污染 Store 导致插件整体失效，见 `support::guarded_host_call` 注释）。
pub struct UnimplementedPorts;

#[async_trait]
impl HostEnginePorts for UnimplementedPorts {
    async fn egress_check(&self, _app: &tauri::AppHandle, _url: &str, _source: &str) -> std::result::Result<(), String> {
        Err(PORT_NOT_WIRED.to_string())
    }

    fn egress_redirect_policy(&self) -> reqwest::redirect::Policy {
        reqwest::redirect::Policy::none()
    }

    fn auth_engine(&self) -> Option<Arc<dyn AuthEnginePort>> {
        None
    }

    fn global_token(&self) -> String {
        String::new()
    }

    fn connection_engine(&self) -> Arc<dyn ConnectionEnginePort> {
        Arc::new(UnimplementedConnectionEngine)
    }

    fn reconnect_policy(&self, _max_retries: u32, _base_ms: u64, _max_ms: u64) -> Box<dyn WsReconnectPolicyPort> {
        Box::new(UnimplementedReconnectPolicy)
    }

    fn reconnect_bounds(&self) -> (u64, u64) {
        (1000, 60_000)
    }

    fn current_node_id(&self, _app: &tauri::AppHandle) -> Option<String> {
        None
    }

    async fn peer_dial_endpoint(
        &self,
        _app: &tauri::AppHandle,
        _node_id: String,
        _addr: String,
        _port: u16,
    ) -> Result<String> {
        Err(port_err("peer_dial_endpoint"))
    }

    async fn peer_disconnect(&self, _app: &tauri::AppHandle, _node_id: String) -> Result<bool> {
        Err(port_err("peer_disconnect"))
    }

    async fn peer_respond_consent(&self, _app: &tauri::AppHandle, _request_id: String, _accepted: bool) -> Result<bool> {
        Err(port_err("peer_respond_consent"))
    }

    async fn peer_list_trusted(&self, _app: &tauri::AppHandle) -> Result<String> {
        Err(port_err("peer_list_trusted"))
    }

    async fn peer_revoke_trusted(&self, _app: &tauri::AppHandle, _node_id: String) -> Result<bool> {
        Err(port_err("peer_revoke_trusted"))
    }

    async fn peer_set_shared_roots(&self, _app: &tauri::AppHandle, _entries_json: String) -> Result<()> {
        Err(port_err("peer_set_shared_roots"))
    }

    async fn peer_start_node(&self, _app: &tauri::AppHandle, _caller: &str) -> Result<bool> {
        Err(port_err("peer_start_node"))
    }

    async fn peer_stop_node(&self, _app: &tauri::AppHandle, _caller: &str) -> Result<bool> {
        Err(port_err("peer_stop_node"))
    }

    async fn peer_cancel_transfer(&self, _app: &tauri::AppHandle, _batch_id: String) -> Result<bool> {
        Err(port_err("peer_cancel_transfer"))
    }

    async fn peer_cancel_receiving(&self, _app: &tauri::AppHandle, _batch_id: String) -> Result<bool> {
        Err(port_err("peer_cancel_receiving"))
    }

    async fn peer_send_files_with_policy(
        &self,
        _app: &tauri::AppHandle,
        _node_id: String,
        _paths: Vec<String>,
        _force_encrypt: Option<bool>,
    ) -> Result<String> {
        Err(port_err("peer_send_files_with_policy"))
    }

    async fn peer_respond_transfer(&self, _app: &tauri::AppHandle, _batch_id: String, _accept: bool) -> Result<()> {
        Err(port_err("peer_respond_transfer"))
    }

    async fn peer_set_receive_policy(&self, _app: &tauri::AppHandle, _mode: String, _timeout_secs: u64) -> Result<()> {
        Err(port_err("peer_set_receive_policy"))
    }

    async fn peer_pause_transfer(&self, _app: &tauri::AppHandle, _batch_id: String) -> Result<bool> {
        Err(port_err("peer_pause_transfer"))
    }

    async fn peer_resume_transfer(&self, _app: &tauri::AppHandle, _batch_id: String) -> Result<bool> {
        Err(port_err("peer_resume_transfer"))
    }

    async fn peer_set_download_dir(&self, _app: &tauri::AppHandle, _path: Option<String>) -> Result<()> {
        Err(port_err("peer_set_download_dir"))
    }

    async fn peer_list_shared_roots(&self, _app: &tauri::AppHandle, _node_id: String) -> Result<String> {
        Err(port_err("peer_list_shared_roots"))
    }

    async fn peer_browse_directory(
        &self,
        _app: &tauri::AppHandle,
        _node_id: String,
        _dir_id: String,
        _rel_path: String,
    ) -> Result<String> {
        Err(port_err("peer_browse_directory"))
    }

    async fn peer_pull_files(
        &self,
        _app: &tauri::AppHandle,
        _node_id: String,
        _dir_id: String,
        _files_json: String,
    ) -> Result<u32> {
        Err(port_err("peer_pull_files"))
    }

    async fn peer_active_transfers(&self, _app: &tauri::AppHandle) -> Result<String> {
        Err(port_err("peer_active_transfers"))
    }

    async fn peer_collect_outgoing(&self, _paths: Vec<String>) -> Result<String> {
        Err(port_err("peer_collect_outgoing"))
    }

    async fn platform_pick_files(&self, _app: &tauri::AppHandle) -> Result<Vec<String>> {
        Err(port_err("platform_pick_files"))
    }

    async fn platform_pick_shared_directory(&self) -> Result<Option<(String, String, String)>> {
        Err(port_err("platform_pick_shared_directory"))
    }

    async fn resolve_downloads_dir(&self, _app: &tauri::AppHandle) -> Result<String> {
        Err(port_err("resolve_downloads_dir"))
    }

    async fn delete_file_android(&self, _path: String) -> Result<()> {
        Err(port_err("delete_file_android"))
    }

    async fn is_within_app_downloads_dir(&self, _app: &tauri::AppHandle, _path: &str) -> Result<bool> {
        Err(port_err("is_within_app_downloads_dir"))
    }

    fn saf_io(&self, _app: &tauri::AppHandle) -> Option<Arc<dyn SafIoPort>> {
        None
    }

    async fn app_data_dir(&self, _app: &tauri::AppHandle) -> Result<PathBuf> {
        Err(port_err("app_data_dir"))
    }

    async fn notify_show(
        &self,
        _plugin_id: &str,
        _handle: &tokio::runtime::Handle,
        _title: &str,
        _body: &str,
    ) -> std::result::Result<(), String> {
        Err(PORT_NOT_WIRED.to_string())
    }
}

/// 端口未装配统一错误文案（fail-visible：无头上下文命中引擎调用即此文本）
pub const PORT_NOT_WIRED: &str = "host engine port not wired (headless/unimplemented)";

fn port_err(method: &str) -> crate::error::AppError {
    crate::error::AppError::Internal(format!("{PORT_NOT_WIRED}: {method}"))
}

/// [`UnimplementedPorts`] 伴随的连接引擎占位
struct UnimplementedConnectionEngine;

#[async_trait]
impl ConnectionEnginePort for UnimplementedConnectionEngine {
    async fn primary_target(&self) -> Result<Option<(PrimaryTarget, bool)>> {
        Err(port_err("connection_primary_target"))
    }
}

/// [`UnimplementedPorts`] 伴随的重连策略占位（单轮后放弃，零延迟）
struct UnimplementedReconnectPolicy;

#[async_trait]
impl WsReconnectPolicyPort for UnimplementedReconnectPolicy {
    async fn start(&self) -> Option<()> {
        Some(())
    }

    async fn get_delay(&self) -> Duration {
        Duration::from_millis(1000)
    }

    async fn on_success(&self) {}
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 无头端口占位面 fail-visible：无 app 参数的方法逐一断言（显性错误 / 空值）。
    ///
    /// 带 `&tauri::AppHandle` 参数的方法**不在本断言面**：无头上下文里域函数
    /// 在进端口前已被 `require_app` 拦截（"headless context"），占位实现运行时
    /// 不可达；真 app 路径由宿主集成测试覆盖。新增「无 app 参数」端口方法必须
    /// 在此同步补断言（防「占位实现静默成功」漂移）。
    #[tokio::test]
    async fn unimplemented_ports_fail_visibly() {
        let ports = UnimplementedPorts;
        assert!(ports.auth_engine().is_none());
        assert!(ports.global_token().is_empty());
        let (min_ms, max_ms) = ports.reconnect_bounds();
        assert!(min_ms > 0 && max_ms >= min_ms);
        assert!(
            ports
                .peer_collect_outgoing(vec!["/tmp/x".into()])
                .await
                .is_err()
        );
        assert!(
            ports
                .platform_pick_shared_directory()
                .await
                .is_err()
        );
        assert!(
            ports
                .notify_show("t", &tokio::runtime::Handle::current(), "a", "b")
                .await
                .is_err()
        );
    }
}
