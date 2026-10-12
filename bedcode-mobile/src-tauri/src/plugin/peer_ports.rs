//! 移动宿主 host-peer 端口（`PeerPorts` 实现 + 开机装配）
//!
//! host-peer 域机制（19 条原语 + 句柄表 / 自动重拨 / 载荷校验）在共享能力域
//! `bedcode-server-peer-net`；本文件是**宿主侧的另一半**——wasm-core bare 面
//! 的 19 条薄转发经 crate `plugin_binding::ports::ports()` 取本文件装入的
//! 进程级端口（wasm-core-mobile-purity 票 03；桌面同构先例
//! `bedcode-desktop/src-tauri/src/plugin/peer.rs`）。
//!
//! 三个端口方法即「离宿主无法实现」的三件事：
//!
//! - [`PeerPorts::check_permission`]：权限门（安全闸门不外出）——复用移动唯一
//!   权限判定 `PluginManager::has_permission`（加载期 manifest 过滤后的
//!   `granted_permissions` 集合）；manager 未初始化一律拒绝（fail-safe）。
//!   拒绝时宿主侧落 `warn`（结构化字段，AGENTS §8），能力域不重复落日志。
//! - [`PeerPorts::peer_ctx`]：引擎上下文装配（`crate::peer_net::peer_ctx` 是
//!   唯一认识 `AppHandle` 的引擎装配点）；无头（未持 AppHandle）返回能力域
//!   定义的 `HEADLESS_UNAVAILABLE` 文案，不做静默降级。
//! - [`PeerPorts::block_on_any`]：同步↔异步桥——复用 wasm-core 唯一那份
//!   `runtime_util::block_on_async`（block_in_place / 重入检测 / ambient
//!   runtime 语义是实测产物，能力域与宿主都不复制第二份）。
//!
//! 装配点：`PluginManager::init_wasm_runtime_with`（与既有 `HostPorts` 同处
//! 开机期）。crate 侧是进程级 `OnceLock`、首装胜出；未装配时 wasm-core 侧
//! 取端口即 panic（fail-visible，见 crate `ports::ports` 文档）。

use std::any::Any;
use std::sync::Arc;

use bedcode_server_peer_net::plugin_binding::ports::{BoxedBlocked, PeerPorts, HEADLESS_UNAVAILABLE};
use bedcode_server_peer_net::PeerCtx;

/// 端口的宿主实现（持 AppHandle；无头 / 测试为 `None`）
pub struct HostPeerPorts {
    app: Option<Arc<tauri::AppHandle>>,
}

impl HostPeerPorts {
    pub fn new(app: Option<Arc<tauri::AppHandle>>) -> Self {
        Self { app }
    }
}

impl PeerPorts for HostPeerPorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, api: &str) -> bool {
        let Some(pm) = crate::state::try_get_plugin_manager() else {
            tracing::warn!(
                plugin_id = %plugin_id,
                permission = %permission,
                api = %api,
                "peer permission denied (plugin manager not initialized)"
            );
            return false;
        };
        // `has_permission` 是 async（plugins 为 tokio RwLock）——经 wasm-core 唯一
        // 那份同步桥驱动；host fn 线程可能是 worker / 已让出 / 纯 std 线程，
        // 桥内已按 runtime flavor 分流（见 runtime_util 模块文档）
        let allowed =
            bedcode_wasm_core::runtime_util::block_on_async(pm.has_permission(plugin_id, permission));
        if !allowed {
            tracing::warn!(
                plugin_id = %plugin_id,
                permission = %permission,
                api = %api,
                "peer permission denied"
            );
        }
        allowed
    }

    fn peer_ctx(&self) -> Result<Arc<PeerCtx>, String> {
        // 引擎装配动作留宿主；无头口径与文案由能力域定义（HEADLESS_UNAVAILABLE），
        // 宿主只汇报「取不到」
        let app = self
            .app
            .as_ref()
            .ok_or_else(|| HEADLESS_UNAVAILABLE.to_string())?;
        Ok(crate::peer_net::peer_ctx(app))
    }

    fn block_on_any(&self, fut: BoxedBlocked) -> Box<dyn Any + Send> {
        // 复用 wasm-core 唯一那份同步桥（见模块头注第 3 条）
        bedcode_wasm_core::runtime_util::block_on_async(fut)
    }
}

/// 装配进程级端口（开机期调用一次；crate 侧 OnceLock 幂等——首装胜出）
pub fn install(app: Option<Arc<tauri::AppHandle>>) {
    bedcode_server_peer_net::plugin_binding::install_ports(Arc::new(HostPeerPorts::new(app)));
}
