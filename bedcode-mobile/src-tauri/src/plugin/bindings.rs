//! 移动宿主 WIT 绑定 + 域 adapter 公共 helper（wasm-core-mobile-purity 票 04）
//!
//! **为什么宿主要自带一份绑定**（孤儿规则）：`impl Host for WasmPluginState`
//! 要求 trait 或类型之一本地——`WasmPluginState` 属 host-kit（对任何消费方都是
//! 外部类型），故**装配方必须自己生成 trait**（桌面先例
//! `bedcode-desktop/src-tauri/src/plugin/bindings.rs`）。**票 05 起本文件是移动
//! WIT 绑定的唯一落点**：服务移动独有域 adapter（notify / fs_mobile /
//! terminal_stream / connection / auth / 移动 events）与随域迁入的 12 个 core 域
//! （`plugin/host_domains/`）——各自的 `Host` impl 经 `mobile-host-core` 模块
//! 自报，装配期由内核 `ModuleRegistry` 收集（核心里已零 WIT 绑定，见
//! `packages/bedcode-wasm-core/src/manager/runtime/core_binding.rs` 只绑 core 交集）。
//!
//! **纪律**：宿主 adapter 的 `Host` impl 一律引用**本文件**的绑定；不得再实现
//! wasm-core 内核的同名 trait——同一 interface 被两份 `add_to_linker` 注册
//! ⇒ 装配期 `defined twice`。绑定配置与内核 core 交集侧逐字一致
//! （`exports: { default: async }`——测试组件与真实插件的调用路径同形）。

use wasmtime::component::bindgen;

bindgen!({
    // 移动端 `wit/` 是生成物目录（core.wit + cap-mobile.wit + bedcode.wit 同
    // package 拼装，compose-wit.mjs 产物）；相对本 crate 根定位
    path: "../packages/plugin-sdk-mobile/rust/wit",
    world: "plugin",
    // 与 wasm-core bare 面同款：全部导出绑定生成 async 变体
    exports: { default: async },
});

// ==================== 公共 helper ====================

/// 从统一 state 取移动宿主上下文（类型不符即 panic——装配期编程错误，fail-visible）
pub fn ctx_of(state: &bedcode_host_kit::WasmPluginState) -> &crate::plugin::wasm_runtime::WasmHostContext {
    bedcode_host_kit::ports::downcast_host::<crate::plugin::wasm_runtime::WasmHostContext>(state.host.as_ref())
}

/// 插件权限门（宿主 adapter 统一入口）
///
/// 权限集 = `WasmHostContext::granted_permissions`（实例化时按插件注入）；
/// 拒绝时落 `warn`（结构化字段，AGENTS §8），域函数不重复落日志。
pub fn check_permission(state: &bedcode_host_kit::WasmPluginState, permission: &str, api: &str) -> bool {
    let allowed = ctx_of(state).granted_permissions.contains(permission);
    if !allowed {
        tracing::warn!(
            plugin_id = %state.plugin_id,
            permission = %permission,
            api = %api,
            "plugin permission denied"
        );
    }
    allowed
}

/// WASM host function 的 panic 守卫（与 wasm-core `host_impl/support.rs`
/// `guarded_host_call` 同语义的宿主版：wasmtime 经 extern "C" 进入，panic
/// 越过该边界是 UB——统一截获、记 error 日志、返回 fallback）
pub fn guarded<T>(plugin_id: &str, host_fn: &'static str, fallback: T, f: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(panic_err) => {
            let msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = panic_err.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!(
                plugin_id = %plugin_id,
                host_fn = %host_fn,
                error = %msg,
                "host function panicked and was caught by boundary guard"
            );
            fallback
        }
    }
}
