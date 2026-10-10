//! 宿主引擎端口装配（票 17 批次 2b）——[`HostEnginePorts`] 的宿主侧唯一实现
//!
//! 插件机制面（单一 wasm-core `bedcode-wasm-core` 的 `mobile-host` 面；
//! `bedcode-wasm-core-mobile` 为 package rename 别名，fork crate 已退役）的所有「离宿主无法实现」
//! 引擎调用经此注入：auth 引擎（C4 凭据零过境）/ egress 安全闸门（D5，判定 +
//! 弹窗编排整体在宿主）/ 主连接事实 / WS 重连策略 / peer 四模块 / mDNS 共享
//! 守护 / android 平台桥（android_plugins + SAF）。DTO 以原始值 / JSON 过界，
//! 宿主类型（DialEndpoint / RemotePullFileDto / ConsentRequest…）不出宿主。
//!
//! 装配点：`PluginManager::init_wasm_runtime_with` 构造 `WasmHostContext` 时
//! 注入 `Arc<HostPorts>`；fs_auth 闸门经 `FsAuthGate` 注入（真源 = 宿主
//! `plugin/fs_auth.rs`，白名单 / 弹窗 / 持久授权不迁移）。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bedcode_wasm_core_mobile::host_api::ports::{
    AuthEnginePort, ConnectionEnginePort, FsAuthGate, FsAuthOp, HostEnginePorts, PrimaryTarget,
    SafIoPort, WsReconnectPolicyPort,
};
use tauri::Manager;

use crate::plugin::fs_auth::FsOp;

// ==================== 子 trait 实现（宿主引擎类型直接 impl 外部端口 trait） ====================

/// 认证引擎端口投影（C4：密码学与凭据在 [`crate::auth::manager::AuthManager`]，
/// 端口只透传调用结果；`get_credentials` 只投影存在性）
#[async_trait]
impl AuthEnginePort for crate::auth::manager::AuthManager {
    async fn request_pairing(&self) -> bedcode_wasm_core_mobile::Result<()> {
        crate::auth::manager::AuthManager::request_pairing(self)
            .await
            .map_err(Into::into)
    }

    async fn verify_pairing_code(&self, code: &str) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::auth::manager::AuthManager::verify_pairing_code(self, code)
            .await
            .map_err(Into::into)
    }

    async fn authenticate_with_qr(&self, token: &str) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::auth::manager::AuthManager::authenticate_with_qr(self, token)
            .await
            .map_err(Into::into)
    }

    async fn authenticate_with_biometric(&self) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::auth::manager::AuthManager::authenticate_with_biometric(self)
            .await
            .map_err(Into::into)
    }

    async fn has_credentials(&self) -> bool {
        crate::auth::manager::AuthManager::get_credentials(self).await.is_some()
    }
}

/// 主连接事实端口投影（票 12 `host-connection.primary-target` 引擎读数）
#[async_trait]
impl ConnectionEnginePort for crate::connection::manager::ConnectionManager {
    async fn primary_target(&self) -> bedcode_wasm_core_mobile::Result<Option<(PrimaryTarget, bool)>> {
        let target = crate::connection::manager::ConnectionManager::get_target(self).await;
        let connected = crate::connection::manager::ConnectionManager::is_connected(self).await;
        Ok(target.map(|t| (PrimaryTarget { address: t.address, port: t.port }, connected)))
    }
}

/// 文件授权闸门端口投影（宿主 FsAuthChecker 的窄面：白名单 / 弹窗 /
/// 持久授权真源全部在宿主，crate 侧只问「能不能读/写」）
#[async_trait]
impl FsAuthGate for crate::plugin::fs_auth::FsAuthChecker {
    async fn check(&self, plugin_id: &str, path: &str, op: FsAuthOp) -> bool {
        let mapped = match op {
            FsAuthOp::Read => FsOp::Read,
            FsAuthOp::Write => FsOp::Write,
        };
        crate::plugin::fs_auth::FsAuthChecker::check(self, plugin_id, path, mapped).await
    }

    async fn check_batch(&self, plugin_id: &str, paths: &[String], op: FsAuthOp) -> bool {
        let mapped = match op {
            FsAuthOp::Read => FsOp::Read,
            FsAuthOp::Write => FsOp::Write,
        };
        crate::plugin::fs_auth::FsAuthChecker::check_batch(self, plugin_id, paths, mapped).await
    }
}

/// SAF 桥端口投影（宿主 `SafIo` trait 对象的窄转发）
struct SafIoBridge(Arc<dyn crate::plugin::saf_io::SafIo>);

impl SafIoPort for SafIoBridge {
    fn write_media_downloads(&self, src: &str, display_name: &str, mime_type: &str) -> std::result::Result<(), String> {
        self.0
            .write_media_downloads(src, display_name, mime_type)
            .map_err(|e| e.to_string())
    }

    fn save_to_document(&self, src: &str, suggested_name: &str, mime_type: &str) -> std::result::Result<(), String> {
        self.0
            .save_to_document(src, suggested_name, mime_type)
            .map_err(|e| e.to_string())
    }
}

/// WS 重连策略端口投影（真源 = 宿主 `connection::reconnect` 全局退避单一
/// 事实源：指数退避 + 抖动 + 1s 下限钳制）
struct ReconnectPolicyBridge(Arc<crate::connection::reconnect::ReconnectManager>);

#[async_trait]
impl WsReconnectPolicyPort for ReconnectPolicyBridge {
    async fn start(&self) -> Option<()> {
        // ReconnectManager.start() 返回 Some(delay)/None(放弃)——端口只关心是否继续
        crate::connection::reconnect::ReconnectManager::start(&self.0)
            .await
            .map(|_| ())
    }

    async fn get_delay(&self) -> Duration {
        crate::connection::reconnect::ReconnectManager::get_delay(&self.0).await
    }

    async fn on_success(&self) {
        crate::connection::reconnect::ReconnectManager::on_success(&self.0).await;
    }
}

// ==================== 主端口实现 ====================

/// 宿主引擎端口（装配于 `PluginManager::init_wasm_runtime_with`）
pub struct HostPorts;

#[async_trait]
impl HostEnginePorts for HostPorts {
    // ==================== egress（D5：闸门整体在宿主） ====================

    /// 三层判定 + NeedConsent 授权弹窗一次完成（原 host_impl/http.rs
    /// `check_egress` 的宿主侧完整形态）
    async fn egress_check(&self, app: &tauri::AppHandle, url: &str, source: &str) -> std::result::Result<(), String> {
        match crate::egress::policy().decide(url, source) {
            crate::egress::EgressDecision::Allow(_) => Ok(()),
            crate::egress::EgressDecision::Deny(e) => {
                tracing::warn!(url = %url, code = %e.code, "egress: plugin url denied");
                Err(format!("{}: {}", e.code, e.url))
            }
            crate::egress::EgressDecision::NeedConsent(mut req) => {
                // 弹窗事务 id：插件路径无前端 request_id，宿主生成 UUID
                req.id = uuid::Uuid::new_v4().to_string();
                let allowed = crate::egress::policy()
                    .request_consent(app, req)
                    .await
                    .map_err(|e| format!("egress consent flow failed: {e}"))?;
                if allowed {
                    Ok(())
                } else {
                    Err(format!("{}: {}", crate::egress::ERROR_URL_DENIED, url))
                }
            }
        }
    }

    fn egress_redirect_policy(&self) -> reqwest::redirect::Policy {
        crate::egress::redirect_policy()
    }

    // ==================== auth / token（C4） ====================

    fn auth_engine(&self) -> Option<Arc<dyn AuthEnginePort>> {
        Some(crate::state::get_auth_manager())
    }

    fn global_token(&self) -> String {
        crate::state::get_global_token()
    }

    // ==================== connection ====================

    fn connection_engine(&self) -> Arc<dyn ConnectionEnginePort> {
        crate::state::get_connection_manager()
    }

    // ==================== ws 重连 ====================

    fn reconnect_policy(&self, max_retries: u32, base_ms: u64, max_ms: u64) -> Box<dyn WsReconnectPolicyPort> {
        let policy = crate::connection::reconnect::ReconnectManager::new(
            crate::connection::reconnect::ReconnectConfig::new(max_retries, base_ms, max_ms),
        );
        Box::new(ReconnectPolicyBridge(policy))
    }

    fn reconnect_bounds(&self) -> (u64, u64) {
        (
            crate::system::constants::reconnect::MIN_RECONNECT_DELAY_MS,
            crate::system::constants::reconnect::DEFAULT_MAX_DELAY_MS,
        )
    }

    fn current_node_id(&self, app: &tauri::AppHandle) -> Option<String> {
        crate::peer_net::current_node_id(app)
    }

    // ==================== peer：peer_net 引擎 ====================

    async fn peer_dial_endpoint(
        &self,
        app: &tauri::AppHandle,
        node_id: String,
        addr: String,
        port: u16,
    ) -> bedcode_wasm_core_mobile::Result<String> {
        let dto = crate::peer_net::dial_peer_endpoint(
            app.clone(),
            crate::peer_net::DialEndpoint { node_id, addr, port },
        )
        .await?;
        Ok(dto.status)
    }

    async fn peer_disconnect(&self, app: &tauri::AppHandle, node_id: String) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::peer_net::disconnect_peer(app.clone(), node_id).await
    }

    async fn peer_respond_consent(
        &self,
        app: &tauri::AppHandle,
        request_id: String,
        accepted: bool,
    ) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::peer_net::respond_peer_consent(app.clone(), request_id, accepted).await
    }

    async fn peer_list_trusted(&self, app: &tauri::AppHandle) -> bedcode_wasm_core_mobile::Result<String> {
        let dtos = crate::peer_net::list_trusted_peers(app.clone()).await?;
        serde_json::to_string(&dtos).map_err(|e| crate::AppError::Serialization(e))
    }

    async fn peer_revoke_trusted(&self, app: &tauri::AppHandle, node_id: String) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::peer_net::revoke_trusted_peer(app.clone(), node_id).await
    }

    async fn peer_set_shared_roots(&self, app: &tauri::AppHandle, entries_json: String) -> bedcode_wasm_core_mobile::Result<()> {
        let seeds: Vec<SharedRootSeed> = serde_json::from_str(&entries_json)
            .map_err(|e| crate::AppError::Serialization(e))?;
        let entries = seeds
            .into_iter()
            .map(|s| bedcode_peer_net::SharedDirEntry {
                id: s.id,
                name: s.name,
                root: bedcode_peer_net::SharedDirRoot::Saf { tree_uri: s.saf_tree_uri },
            })
            .collect();
        crate::peer_net::set_shared_roots(app.clone(), entries).await
    }

    async fn peer_start_node(&self, app: &tauri::AppHandle, caller: &str) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::peer_net::start_node_owned(app, caller).await
    }

    async fn peer_stop_node(&self, app: &tauri::AppHandle, caller: &str) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::peer_net::stop_node_owned(app, caller).await
    }

    // ==================== peer：transfer / receive / remote ====================

    async fn peer_cancel_transfer(&self, app: &tauri::AppHandle, batch_id: String) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::peer_transfer::cancel_peer_transfer(app.clone(), batch_id).await
    }

    async fn peer_cancel_receiving(&self, app: &tauri::AppHandle, batch_id: String) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::peer_receive::cancel_peer_receiving(app.clone(), batch_id).await
    }

    async fn peer_send_files_with_policy(
        &self,
        app: &tauri::AppHandle,
        node_id: String,
        paths: Vec<String>,
        force_encrypt: Option<bool>,
    ) -> bedcode_wasm_core_mobile::Result<String> {
        crate::peer_transfer::send_files_to_peer_with_policy(app.clone(), node_id, paths, force_encrypt).await
    }

    async fn peer_respond_transfer(&self, app: &tauri::AppHandle, batch_id: String, accept: bool) -> bedcode_wasm_core_mobile::Result<()> {
        crate::peer_receive::respond_peer_transfer(app.clone(), batch_id, accept).await?;
        Ok(())
    }

    async fn peer_set_receive_policy(&self, app: &tauri::AppHandle, mode: String, timeout_secs: u64) -> bedcode_wasm_core_mobile::Result<()> {
        crate::peer_receive::set_peer_receive_policy(app.clone(), mode, timeout_secs).await
    }

    async fn peer_pause_transfer(&self, app: &tauri::AppHandle, batch_id: String) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::peer_transfer::pause_peer_transfer(app.clone(), batch_id).await
    }

    async fn peer_resume_transfer(&self, app: &tauri::AppHandle, batch_id: String) -> bedcode_wasm_core_mobile::Result<bool> {
        crate::peer_transfer::resume_peer_transfer(app.clone(), batch_id).await
    }

    async fn peer_set_download_dir(&self, app: &tauri::AppHandle, path: Option<String>) -> bedcode_wasm_core_mobile::Result<()> {
        crate::peer_receive::set_peer_download_dir(app, path).await
    }

    async fn peer_list_shared_roots(&self, app: &tauri::AppHandle, node_id: String) -> bedcode_wasm_core_mobile::Result<String> {
        let roots = crate::peer_remote::list_peer_shared_roots(app.clone(), node_id).await?;
        serde_json::to_string(&roots).map_err(|e| crate::AppError::Serialization(e))
    }

    async fn peer_browse_directory(
        &self,
        app: &tauri::AppHandle,
        node_id: String,
        dir_id: String,
        rel_path: String,
    ) -> bedcode_wasm_core_mobile::Result<String> {
        let dto = crate::peer_remote::browse_peer_directory(app.clone(), node_id, dir_id, rel_path).await?;
        serde_json::to_string(&dto).map_err(|e| crate::AppError::Serialization(e))
    }

    async fn peer_pull_files(
        &self,
        app: &tauri::AppHandle,
        node_id: String,
        dir_id: String,
        files_json: String,
    ) -> bedcode_wasm_core_mobile::Result<u32> {
        let files: Vec<crate::peer_remote::RemotePullFileDto> =
            serde_json::from_str(&files_json).map_err(|e| crate::AppError::Serialization(e))?;
        crate::peer_remote::pull_peer_files(app.clone(), node_id, dir_id, files)
            .await
            .map(|n| n as u32)
    }

    async fn peer_active_transfers(&self, app: &tauri::AppHandle) -> bedcode_wasm_core_mobile::Result<String> {
        // 三表聚合投影（send 句柄表 + receive pending 询问表 + pull 会话表）
        let mut rows = crate::peer_transfer::active_send_transfer_rows(app);
        rows.extend(crate::peer_receive::active_receive_rows(app));
        rows.extend(crate::peer_remote::active_pull_rows(app));
        serde_json::to_string(&rows).map_err(|e| crate::AppError::Serialization(e))
    }

    async fn peer_collect_outgoing(&self, paths: Vec<String>) -> bedcode_wasm_core_mobile::Result<String> {
        crate::peer_transfer::collect_outgoing_for_plugin(paths).await
    }

    // ==================== platform / fs / config / db / notify ====================

    async fn platform_pick_files(&self, app: &tauri::AppHandle) -> bedcode_wasm_core_mobile::Result<Vec<String>> {
        crate::peer_transfer::peer_pick_files(app.clone()).await
    }

    async fn platform_pick_shared_directory(&self) -> bedcode_wasm_core_mobile::Result<Option<(String, String, String)>> {
        crate::plugin::android_plugins::pick_shared_directory_android().await
    }

    async fn resolve_downloads_dir(&self, app: &tauri::AppHandle) -> bedcode_wasm_core_mobile::Result<String> {
        crate::plugin::android_plugins::resolve_app_downloads_dir(app)
            .await
            .ok_or_else(|| crate::AppError::Internal("downloads dir not available".to_string()))
    }

    async fn delete_file_android(&self, path: String) -> bedcode_wasm_core_mobile::Result<()> {
        crate::plugin::android_plugins::delete_file(&path).await
    }

    async fn is_within_app_downloads_dir(&self, app: &tauri::AppHandle, path: &str) -> bedcode_wasm_core_mobile::Result<bool> {
        Ok(crate::plugin::android_plugins::is_within_app_downloads_dir(app, path).await)
    }

    fn saf_io(&self, app: &tauri::AppHandle) -> Option<Arc<dyn SafIoPort>> {
        let state = app.state::<crate::plugin::saf_io::SafIoState>();
        Some(Arc::new(SafIoBridge(state.inner().0.clone())))
    }

    async fn app_data_dir(&self, app: &tauri::AppHandle) -> bedcode_wasm_core_mobile::Result<PathBuf> {
        crate::peer_net::app_data_dir(app)
    }

    // ==================== notify（系统通知与提醒反馈，ABI v18） ====================
    //
    // 全部经 Kotlin TaskNotificationPlugin（`run_mobile_plugin_async` 直接
    // await 驱动——端口方法是 async fn，被 host_impl 的
    // `block_in_place + block_on` 驱动，内部不再嵌套 block_on）。

    async fn notify_show(
        &self,
        plugin_id: &str,
        title: &str,
        body: &str,
        vibrate: bool,
        sound: bool,
    ) -> std::result::Result<(), String> {
        #[cfg(target_os = "android")]
        {
            use crate::plugin::android_plugins::notification_plugin_handle;

            let Some(h) = notification_plugin_handle() else {
                return Err("TaskNotificationPlugin not registered".to_string());
            };
            let payload = serde_json::json!({
                "title": title,
                "body": body,
                "vibrate": vibrate,
                "sound": sound,
            });
            let _response: serde_json::Value = h
                .run_mobile_plugin_async("showPluginNotification", payload)
                .await
                .map_err(|e| format!("notification failed: {e}"))?;
            Ok(())
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (plugin_id, title, body, vibrate, sound);
            Err("only supported on Android".to_string())
        }
    }

    async fn notify_check_permission(&self) -> std::result::Result<bool, String> {
        #[cfg(target_os = "android")]
        {
            use crate::plugin::android_plugins::notification_plugin_handle;

            let Some(h) = notification_plugin_handle() else {
                return Err("TaskNotificationPlugin not registered".to_string());
            };
            let response: serde_json::Value = h
                .run_mobile_plugin_async("checkNotificationPermission", serde_json::json!({}))
                .await
                .map_err(|e| format!("notification permission check failed: {e}"))?;
            Ok(response
                .get("granted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false))
        }
        #[cfg(not(target_os = "android"))]
        {
            Err("only supported on Android".to_string())
        }
    }

    async fn notify_request_permission(&self) -> std::result::Result<bool, String> {
        #[cfg(target_os = "android")]
        {
            use crate::plugin::android_plugins::notification_plugin_handle;

            let Some(h) = notification_plugin_handle() else {
                return Err("TaskNotificationPlugin not registered".to_string());
            };
            let response: serde_json::Value = h
                .run_mobile_plugin_async("requestNotificationPermission", serde_json::json!({}))
                .await
                .map_err(|e| format!("notification permission request failed: {e}"))?;
            Ok(response
                .get("granted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false))
        }
        #[cfg(not(target_os = "android"))]
        {
            Err("only supported on Android".to_string())
        }
    }

    async fn notify_vibrate(&self, duration_ms: u32) -> std::result::Result<(), String> {
        #[cfg(target_os = "android")]
        {
            use crate::plugin::android_plugins::notification_plugin_handle;

            let Some(h) = notification_plugin_handle() else {
                return Err("TaskNotificationPlugin not registered".to_string());
            };
            let payload = serde_json::json!({ "durationMs": duration_ms });
            let _response: serde_json::Value = h
                .run_mobile_plugin_async("pluginVibrate", payload)
                .await
                .map_err(|e| format!("vibrate failed: {e}"))?;
            Ok(())
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = duration_ms;
            Err("only supported on Android".to_string())
        }
    }

    async fn notify_play_sound(&self) -> std::result::Result<(), String> {
        #[cfg(target_os = "android")]
        {
            use crate::plugin::android_plugins::notification_plugin_handle;

            let Some(h) = notification_plugin_handle() else {
                return Err("TaskNotificationPlugin not registered".to_string());
            };
            let _response: serde_json::Value = h
                .run_mobile_plugin_async("pluginPlaySound", serde_json::json!({}))
                .await
                .map_err(|e| format!("play sound failed: {e}"))?;
            Ok(())
        }
        #[cfg(not(target_os = "android"))]
        {
            Err("only supported on Android".to_string())
        }
    }
}

/// `peer_set_shared_roots` 的 JSON 契约（camelCase；域内不做形状解析——
/// 解析责任在端口实现方，与迁移前域内 SharedRootSeed 同形状）
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedRootSeed {
    id: String,
    name: String,
    saf_tree_uri: String,
}
