//! 共享辅助：WASM 内存字符串读写 / 权限校验 / 结果写出 / panic 守卫

use super::super::WasmPluginState;

/// 从 WASM 线性内存读取字符串
pub(crate) fn read_wasm_string(caller: &mut wasmtime::Caller<'_, WasmPluginState>, ptr: u32, len: u32) -> Option<String> {
    if len == 0 {
        return Some(String::new());
    }
    let memory = caller.get_export("memory")?.into_memory()?;
    let data = memory.data(&caller);
    let start = ptr as usize;
    let end = start + len as usize;
    if end > data.len() {
        return None;
    }
    String::from_utf8(data[start..end].to_vec()).ok()
}


/// 将字符串写入 WASM 线性内存
pub(crate) fn write_wasm_string(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    s: &str,
) -> Option<(u32, u32)> {
    if s.is_empty() {
        return Some((0, 0));
    }

    let bytes = s.as_bytes();
    let len = bytes.len();

    let alloc_func = caller.get_export("__bedcode_allocate")?.into_func()?;
    let mut results = [wasmtime::Val::I32(0)];
    alloc_func.call(&mut *caller, &[wasmtime::Val::I32(len as i32)], &mut results).ok()?;
    let ptr = results[0].unwrap_i32() as u32;
    if ptr == 0 {
        return None;
    }

    let memory = caller.get_export("memory")?.into_memory()?;
    let data = memory.data_mut(caller);
    let start = ptr as usize;
    let end = start + len;
    if end > data.len() {
        return None;
    }
    data[start..end].copy_from_slice(bytes);

    Some((ptr, len as u32))
}


/// 检查插件是否拥有指定权限（host function 调用前校验）
///
/// 权限来自 manifest.permissions（实例化时注入 WasmPluginState）。
/// 校验失败返回 false，调用方记录日志并拒绝执行。
pub(crate) fn has_permission(caller: &wasmtime::Caller<'_, WasmPluginState>, permission: &str) -> bool {
    caller.data().granted_permissions.contains(permission)
}


/// 将 (ptr, len) 结果写入 WASM 线性内存的 out_ptr 位置（8 字节: ptr + len，小端序）
///
/// ABI v3：返回 (ptr, len) 的结果通过 out_ptr 输出参数传递，而非元组返回值
pub(crate) fn write_result_to_out_ptr(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    out_ptr: u32,
    ptr: u32,
    len: u32,
) -> bool {
    let memory = match caller.get_export("memory") {
        Some(e) => match e.into_memory() {
            Some(m) => m,
            None => return false,
        },
        None => return false,
    };
    let data = memory.data_mut(caller);
    let start = out_ptr as usize;
    let end = start + bedcode_plugin_api_mobile::abi::RESULT_PAIR_SIZE;
    if end > data.len() {
        return false;
    }
    data[start..start + 4].copy_from_slice(&ptr.to_le_bytes());
    data[start + 4..end].copy_from_slice(&len.to_le_bytes());
    true
}

// ==================== Host Function Implementations ====================
//
// 移动端 Host Function 约定：
// - 敏感 host function 调用前按 manifest.permissions 校验（has_permission），
//   通用/插件自身状态类（emit_event / notify / log_* / mark_plugin_error）不校验
// - host_terminal_send 通过 WebSocket 转发到桌面端
// - host_notify 转发 Kotlin TaskNotificationPlugin（Android）
// - host_session_list/get 为空操作

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
