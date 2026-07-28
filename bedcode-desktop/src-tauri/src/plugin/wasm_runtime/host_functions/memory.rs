//! WASM 线性内存读写辅助
//!
//! Host Functions 与插件线性内存之间的字符串传输协议实现（宿主侧）：
//! - 读：从 (ptr, len) 指向的区域拷贝出字符串
//! - 写：调用插件的 `__bedcode_allocate` 导出分配空间后写入
//! - 结果回传：将 (ptr, len) 对写入 out_ptr 指向的 8 字节（小端）

use crate::plugin::wasm_runtime::WasmPluginState;
use bedcode_plugin_api::abi;

/// 将 (ptr, len) 结果写入 WASM 线性内存中的 out_ptr 位置（8 字节: ptr:u32 + len:u32）
///
/// 返回 0 表示成功，-1 表示写入失败
pub(super) fn write_result_to_out_ptr(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    out_ptr: u32,
    ptr: u32,
    len: u32,
) -> i32 {
    let memory = match caller.get_export(abi::MEMORY).and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -1,
    };
    let data = memory.data_mut(caller);
    let start = out_ptr as usize;
    let end = start + abi::RESULT_PAIR_SIZE;
    if end > data.len() {
        return -1;
    }
    data[start..start + 4].copy_from_slice(&ptr.to_le_bytes());
    data[start + 4..end].copy_from_slice(&len.to_le_bytes());
    0
}

/// 从 WASM 线性内存读取字符串
pub(super) fn read_wasm_string(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    ptr: u32,
    len: u32,
) -> Option<String> {
    if len == 0 {
        return Some(String::new());
    }
    let memory = caller.get_export(abi::MEMORY)?.into_memory()?;
    let data = memory.data(&caller);
    let start = ptr as usize;
    let end = start + len as usize;
    if end > data.len() {
        return None;
    }
    String::from_utf8(data[start..end].to_vec()).ok()
}

/// 从 WASM 线性内存读取字符串并回收其 guest 内存（插件 → 宿主参数方向）
///
/// 插件通过 `wasm_alloc_string` 传入的字符串参数读完即可立即回收，
/// 消除长驻插件每次 host call 的参数内存泄漏。
/// 插件未导出 `__bedcode_deallocate`（v2 之前的旧插件）时自动退化为 v1 不回收行为。
pub(super) fn read_wasm_string_consume(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    ptr: u32,
    len: u32,
) -> Option<String> {
    let s = read_wasm_string(caller, ptr, len);
    dealloc_wasm_string(caller, ptr, len);
    s
}

/// 调用插件的 `__bedcode_deallocate` 导出回收线性内存
fn dealloc_wasm_string(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    ptr: u32,
    len: u32,
) {
    if ptr == 0 || len == 0 {
        return;
    }
    // 旧版插件（v2 之前）无 deallocate 导出 — 跳过回收，退化 v1 行为
    let Some(func) = caller.get_export(abi::export::DEALLOCATE).and_then(|e| e.into_func()) else {
        return;
    };
    // 嵌套调用回实例（host function → 插件 dealloc）；
    // 回收函数不会再回调 host，无递归风险
    let _ = func.call(
        &mut *caller,
        &[wasmtime::Val::I32(ptr as i32), wasmtime::Val::I32(len as i32)],
        &mut [],
    );
}

/// 将字符串写入 WASM 线性内存，返回 (ptr, len)
///
/// 通过插件的 `__bedcode_allocate` 导出函数分配内存
pub(super) fn write_wasm_string(
    caller: &mut wasmtime::Caller<'_, WasmPluginState>,
    s: &str,
) -> Option<(u32, u32)> {
    if s.is_empty() {
        return Some((0, 0));
    }

    let bytes = s.as_bytes();
    let len = bytes.len();

    // 调用插件的内存分配器
    let alloc_func = caller.get_export(abi::export::ALLOCATE)?.into_func()?;
    let mut results = [wasmtime::Val::I32(0)];
    alloc_func.call(&mut *caller, &[wasmtime::Val::I32(len as i32)], &mut results).ok()?;
    let ptr = results[0].unwrap_i32() as u32;
    if ptr == 0 {
        return None;
    }

    // 重新获取 memory 引用（alloc_func.call 消费了 caller 的借用）
    // 使用 caller 的 AsContextMut 实现直接访问内存
    let memory = caller.get_export(abi::MEMORY)?.into_memory()?;
    let data = memory.data_mut(caller);
    let start = ptr as usize;
    let end = start + len;
    if end > data.len() {
        return None;
    }
    data[start..end].copy_from_slice(bytes);

    Some((ptr, len as u32))
}
