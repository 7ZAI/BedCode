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
//!
//! **wasm-core 纯净性收口票 05（回迁 lib）**：`auth_center` 裁决面/桥接门从
//! `bedcode-wasm-core` 迁回本模块（用户裁定：auth 桥接是宿主薄壳，不应留在 wasm
//! core 中）。注册表真源仍在 wasm-core 的 `host_api::auth_center`（WIT 绑定面），
//! 本模块 lib 单向依赖取用。
//!
//! **wasm-core 纯净性收口票 02 批次 04**：host-auth 域（含 `test_seed_plugin_secret`
//! 种子函数）自 wasm-core 迁宿主 `src/plugin/auth.rs`（路径 B）后，`test_tokens`
//! 测试夹具随依赖**迁回本模块**——旧理由（依赖 wasm-core 内部函数）失效。消费方 =
//! lib 集成测试（session_e2e / system_component_test / auth_center_perf）。

pub mod auth_center;
pub mod identity;
pub mod test_tokens;