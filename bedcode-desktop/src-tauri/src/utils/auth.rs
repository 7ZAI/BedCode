//! Authentication Module
//!
//! 认证模块（v33 / ADR 0033 + v34 / B-downsink 起**不含任何设备凭证材料**）：
//! - [`identity`]：认证中心裁决后交给宿主的连接身份（字段集被 L2 锁钉死）
//! - [`auth_center`]：L2 桥接门（问中心一次 + `deny_kind` 三态分类 + 组合式零解析转发）
//!
//! `jwt`（宿主自持 HS256 签发/验签）与 `host_secrets`（宿主属主密钥托管）已随
//! ADR 0033 整模块退役——入场密钥的真源在认证中心（`com.bedcode.terminal-session`）；
//! `biometric`（生物凭证验签/挑战管理）已随 B-downsink（2026-09-30）退役——生物
//! 公钥托管与验签执行下沉认证中心私有库（`auth_records::biometric_key_*` + WASM 内
//! p256）。宿主认证面只剩「问中心」这一个方向。

pub mod auth_center;
pub mod identity;
/// 测试夹具：经认证中心签发设备入场 token（v33 起宿主无签发面，测试也不能自己造）
#[cfg(test)]
pub(crate) mod test_tokens;
