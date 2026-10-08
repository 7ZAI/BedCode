//! 认证模块（wasm-core 纯净性收口票 05 收口后）
//!
//! 桥接门（`session_active`）、裁决面（`enforce_connection_policy`）已回迁宿主 lib
//! （`src-tauri/src/utils/auth/`）；`invoke_auth_method`（host-auth WIT 原语实现链）
//! 并入 `host_api/auth_center.rs`。本模块保留连接身份形状（`identity`，真源
//! bedcode-server-base）与测试夹具（`test_tokens`，常编译 pub，依赖本 crate 内部
//! `test_seed_plugin_secret` 故留此——lib 集成测试经 `bedcode_wasm_core::utils::auth::
//! test_tokens` 消费）。

/// 连接身份（真源 bedcode-server-base，路径与 lib `utils/auth.rs` 一致）
pub mod identity;
/// 测试夹具（常编译 pub，lib 集成测试消费；依赖 `crate::host_api::test_seed_plugin_secret`）
pub mod test_tokens;