//! Authentication Module
//!
//! 认证模块（v33 / ADR 0033 起**不含任何设备 JWT 密码学**）：
//! - [`identity`]：认证中心裁决后交给宿主的连接身份（字段集被 L2 锁钉死）
//! - [`auth_center`]：L2 桥接门（问中心一次 + `deny_kind` 三态分类 + 组合式零解析转发）
//! - [`biometric`]：生物凭证验签（宿主托管 P-256 公钥，与设备入场 JWT 是两条线）
//!
//! `jwt`（宿主自持 HS256 签发/验签）与 `host_secrets`（宿主属主密钥托管）已随
//! ADR 0033 整模块退役——入场密钥的真源在认证中心（`com.bedcode.terminal-session`）。

pub mod auth_center;
pub mod biometric;
pub mod identity;
/// 测试夹具：经认证中心签发设备入场 token（v33 起宿主无签发面，测试也不能自己造）
#[cfg(test)]
pub(crate) mod test_tokens;
