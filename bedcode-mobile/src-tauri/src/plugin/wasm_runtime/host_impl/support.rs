//! 共享辅助：panic 守卫
//!
//! 组件路径（wasm_runtime/component.rs 的 Host trait impl → 逻辑层）专用。
//! core 形态的 WASM 线性内存读写辅助已随 09 清理删除。

// ==================== Host Call Panic Guard ====================
/// 在 wasmtime host function 内执行阻塞宿主调用并捕获 panic
///
/// wasmtime host function 经 extern "C" ABI 进入，panic 越过该边界是 UB
/// （release 下 panic=unwind 时 catch_unwind 生效，但 C ABI 边界自身不展开）。
/// host fn 内的 block_in_place / Handle::current() / 锁 unwrap 等异常会 panic，
/// 统一在此截获：记录 error 日志（含插件 ID 与调用名），返回 fallback 让调用方
/// 按失败语义继续 —— WASM 插件侧已有结构化错误处理（任务置 Failed 推送到前端），
/// 插件业务 panic 不再拖垮整个应用。
pub(crate) fn guarded_host_call<T>(
    plugin_id: &str,
    host_fn: &'static str,
    fallback: T,
    f: impl FnOnce() -> T,
) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(panic_err) => {
            let msg = panic_err
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| panic_err.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic payload".to_string());
            tracing::error!(
                plugin_id = %plugin_id,
                host_fn = host_fn,
                error = %msg,
                "host function panicked; swallowed and returning fallback (plugin survives)"
            );
            fallback
        }
    }
}
