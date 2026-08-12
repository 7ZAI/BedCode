//! host_session_*_noop — 会话列表/查询（移动端空操作）

use super::super::WasmPluginState;
use super::support::{guarded_host_call, write_result_to_out_ptr, write_wasm_string};

/// 会话列表：移动端空操作，保持 ABI 兼容
pub(crate) fn host_session_list_noop(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    out_ptr: u32,
) -> i32 {
    match write_wasm_string(&mut caller, "[]") {
        Some((ptr, len)) => {
            if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                0
            } else {
                -1
            }
        }
        None => -1,
    }
}


/// 会话获取：移动端空操作，保持 ABI 兼容
pub(crate) fn host_session_get_noop(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    _sid_ptr: u32,
    _sid_len: u32,
    out_ptr: u32,
) -> i32 {
    let _ = write_result_to_out_ptr(&mut caller, out_ptr, 0, 0);
    0
}
