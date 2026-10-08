//! 设备入场认证引擎面（WIT `host-auth`，ABI v16，票 14 阶段 B）
//!
//! **编排在消费插件，密码学与凭据在宿主**（C4 红线）：本 trait 只投影
//! 「对桌面端 `/api/auth/*` 六端点的引擎调用」，流程顺序 / 事件发射 / 状态
//! 派生由插件编排。凭据零过境——认证成功后 JWT 由宿主落地，本域不向插件
//! 返回任何凭据材料（对齐票 12 `jwt-auth`「token 不落插件」先例）。
//!
//! 权限位 `auth`（fail-closed，未声明即拒）。
//!
//! 消费者：`com.bedcode.terminal-session`（配对 / 认证编排域）。

use crate::host::HostError;

/// 设备入场认证引擎面 trait —— 函数签名与 WIT `host-auth` 一一对应
pub trait HostAuth {
    /// 请求配对（桌面端生成一次性配对码并展示）。
    /// `Ok(())` = 桌面端受理（进入等码态）；`Err` = 网络故障 / 桌面端拒绝
    fn auth_request_pairing(&self) -> Result<(), HostError>;

    /// 验证配对码。`Ok(true)` = 受理且凭据已落地宿主；`Ok(false)` = 桌面端
    /// 业务拒绝（码无效 / 过期等）；`Err` = 网络故障
    fn auth_verify_pairing_code(&self, code: &str) -> Result<bool, HostError>;

    /// QR token 认证。结果语义同 [`HostAuth::auth_verify_pairing_code`]
    fn auth_qr_connect(&self, token: &str) -> Result<bool, HostError>;

    /// 生物认证登录（HTTP 挑战-应答 + Keystore 签名，全在宿主执行）。
    /// `Ok(false)` = 本地生物验证未通过 / 用户取消
    fn auth_biometric_authenticate(&self) -> Result<bool, HostError>;

    /// 引擎事实：宿主当前是否持有认证凭据（JWT）。供插件激活期对账
    /// （事件不重放，状态以引擎事实为准）
    fn auth_has_credentials(&self) -> Result<bool, HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// trait 存在性锚：类型检查通过即签名与 WIT 对齐
    #[test]
    fn trait_is_object_safe_shape() {
        fn assert_trait<T: HostAuth>() {}
        fn probe<T: HostAuth>() {}
        let _ = assert_trait::<crate::wasm_host::WasmHost>;
        let _ = probe::<crate::wasm_host::WasmHost>;
    }
}
