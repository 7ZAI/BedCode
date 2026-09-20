//! v17：可选导出 `auth-policy` 的独立绑定（独立 world，不进 plugin world）
//!
//! 与 `wasm_binary.rs` / `wasm_ws.rs` 同构：宿主对 `plugin` world 的导出全部
//! 按必选实例化，非认证中心插件不导出 `auth-policy`，因此该接口**不能**声明进
//! `plugin` world（否则旧插件无法加载）；宿主改为实例化后动态探测
//! `bedcode:plugin/auth-policy.verify-device-token`（core-plugin-manager 装配
//! 框架：`exported_capabilities` 探测 / `call_capability_export`，票 12 C3）。
//! 本模块以 WIT 的 `plugin-auth-policy` world 生成 guest 侧绑定
//! （`exports::...::Guest` + `export!`），使 `wasm_entry!` 能无条件导出默认
//! 实现（默认**拒绝**——非认证中心插件不提供策略；宿主动态探测命中但中间件
//! 只对认证中心 `com.bedcode.session` 实例调用本导出）——插件只需覆盖
//! [`crate::wasm::WasmPlugin::verify_device_token_policy`]。
//!
//! 独立文件承载：与 `wasm.rs` 的第一个 `generate!` 分处不同模块，避免两个
//! `pub_export_macro` 生成的 `export!` 宏在 `crate::wasm` 模块内重名。

wit_bindgen::generate!({
    path: "wit/bedcode.wit",
    world: "plugin-auth-policy",
    pub_export_macro: true,
    default_bindings_module: "$crate::wasm_auth_policy",
});
