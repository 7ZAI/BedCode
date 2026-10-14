//! host-auth-client 宿主 adapter（移动独有认证引擎面；wasm-core-mobile-purity 票 04 自 wasm-core 迁入）
//!
//! **编排在消费插件，密码学与凭据在宿主**（C4 红线）：设备身份文件 / JWT 持有 /
//! AuthHttpClient / 生物凭证绑定与 Keystore 签名全部留在 auth 引擎
//! （`crate::auth::manager` 经 `HostEnginePorts::auth_engine` 投影），本 adapter
//! 只把引擎调用投影给插件；**凭据零过境**——任何函数都不向插件返回凭据材料。
//!
//! 权限位 `auth`（fail-closed，未声明即拒——与 ws:client 同语义）。

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api_mobile::permission::PERMISSION_AUTH;

use crate::plugin::bindings::{bedcode, check_permission, ctx_of};
use bedcode_wasm_core::host_api::ports::PORT_NOT_WIRED;
use bedcode_wasm_core::runtime_util::block_on_async;

bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名
pub const MODULE_NAME: &str = "mobile-auth";

/// 本域提供的 WIT 接口（必须与 `cap-mobile.wit` 逐字一致）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin@0.2.0/host-auth-client"];

/// 本域的权限位（与 WIT / SDK 权限表逐字一致）
pub const MODULE_PERMISSIONS: &[&str] = &["auth"];

/// 能力模块描述符
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 16,
};

/// host-auth-client 能力模块
pub struct AuthModule;

impl HostModule for AuthModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_auth_client::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

static MODULE: AuthModule = AuthModule;

inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== 域逻辑 ====================

fn denied() -> String {
    format!("permission denied: {PERMISSION_AUTH}")
}

/// 取认证引擎（宿主端口投影；未装配 = 无头/测试上下文，fail-visible 拒绝）
///
/// C4：密码学与凭据留宿主引擎（`auth_engine()` 投影五方法），本域只透传调用
/// 结果——凭据零过境。
fn require_auth_engine(
    state: &WasmPluginState,
) -> Result<std::sync::Arc<dyn bedcode_wasm_core::host_api::ports::AuthEnginePort>, String> {
    ctx_of(state)
        .ports
        .auth_engine()
        .ok_or_else(|| PORT_NOT_WIRED.to_string())
}

// ==================== WIT 层（import 接口 → 域逻辑） ====================

impl bedcode::plugin::host_auth_client::Host for WasmPluginState {
    /// 请求配对（HTTP `POST /api/auth/request-pairing`，桌面端出码）
    fn request_pairing(&mut self) -> Result<(), String> {
        if !check_permission(self, PERMISSION_AUTH, "host_auth_request_pairing") {
            return Err(denied());
        }
        let plugin_id = self.plugin_id.clone();
        let auth = require_auth_engine(self)?;
        block_on_async(async move { auth.request_pairing().await })
            .inspect(|_| {
                tracing::info!(plugin_id = %plugin_id, "host-auth: pairing requested (code issued by peer host)");
            })
            .map_err(|e| {
                tracing::warn!(plugin_id = %plugin_id, error = %e, "host-auth: request-pairing failed");
                e.to_string()
            })
    }

    /// 验证配对码（HTTP `POST /api/auth/verify-pairing-code`）
    ///
    /// Ok(true) = 受理且凭据已落地宿主；Ok(false) = 桌面端业务拒绝；Err = 网络故障。
    /// 凭据留在宿主，不向插件返回。
    fn verify_pairing_code(&mut self, code: String) -> Result<bool, String> {
        if !check_permission(self, PERMISSION_AUTH, "host_auth_verify_pairing_code") {
            return Err(denied());
        }
        let plugin_id = self.plugin_id.clone();
        let auth = require_auth_engine(self)?;
        block_on_async(async move { auth.verify_pairing_code(&code).await })
            .inspect(|accepted| {
                tracing::info!(plugin_id = %plugin_id, accepted = accepted, "host-auth: pairing code verified");
            })
            .map_err(|e| {
                tracing::warn!(plugin_id = %plugin_id, error = %e, "host-auth: verify-pairing-code failed");
                e.to_string()
            })
    }

    /// QR token 认证（HTTP `POST /api/auth/qr-connect`）
    fn qr_connect(&mut self, token: String) -> Result<bool, String> {
        if !check_permission(self, PERMISSION_AUTH, "host_auth_qr_connect") {
            return Err(denied());
        }
        let plugin_id = self.plugin_id.clone();
        let auth = require_auth_engine(self)?;
        block_on_async(async move { auth.authenticate_with_qr(&token).await })
            .inspect(|accepted| {
                tracing::info!(plugin_id = %plugin_id, accepted = accepted, "host-auth: qr connect done");
            })
            .map_err(|e| {
                tracing::warn!(plugin_id = %plugin_id, error = %e, "host-auth: qr-connect failed");
                e.to_string()
            })
    }

    /// 生物认证登录（HTTP 挑战-应答 + Keystore 签名，全在宿主执行）
    fn biometric_authenticate(&mut self) -> Result<bool, String> {
        if !check_permission(self, PERMISSION_AUTH, "host_auth_biometric_authenticate") {
            return Err(denied());
        }
        let plugin_id = self.plugin_id.clone();
        let auth = require_auth_engine(self)?;
        block_on_async(async move { auth.authenticate_with_biometric().await })
            .inspect(|accepted| {
                tracing::info!(plugin_id = %plugin_id, accepted = accepted, "host-auth: biometric authenticate done");
            })
            .map_err(|e| {
                tracing::warn!(plugin_id = %plugin_id, error = %e, "host-auth: biometric-authenticate failed");
                e.to_string()
            })
    }

    /// 引擎事实：宿主当前是否持有认证凭据（JWT）
    fn has_credentials(&mut self) -> Result<bool, String> {
        if !check_permission(self, PERMISSION_AUTH, "host_auth_has_credentials") {
            return Err(denied());
        }
        let auth = require_auth_engine(self)?;
        Ok(block_on_async(async move { auth.has_credentials().await }))
    }
}
