//! host_config_get — 配置读取

use super::super::WasmPluginState;
use super::support::{guarded_host_call, read_wasm_string, write_result_to_out_ptr, write_wasm_string};

/// 配置：读取宿主配置项
///
/// 参数：(key_ptr, key_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
///
/// 白名单 = SDK `ConfigKey` 枚举本身：`from_str` 过滤非法 key，
/// value match 穷尽所有变体 —— 新增配置项时编译器强制补实现，
/// 结构性杜绝"白名单声明了但实现缺失"的漂移
pub(crate) fn host_config_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();

    let key = match read_wasm_string(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_config_get: failed to read key");
            return -1;
        }
    };

    // 白名单校验：仅接受 ConfigKey 枚举覆盖的 key
    let Some(config_key) = bedcode_plugin_api_mobile::ConfigKey::from_str(&key) else {
        tracing::warn!(plugin_id = %plugin_id, key = %key, "host_config_get: key not in whitelist");
        return -1;
    };

    // 穷尽 match：新增 ConfigKey 变体必须在此补实现（编译错误兜底）
    let value = match config_key {
        bedcode_plugin_api_mobile::ConfigKey::AppDownloadsDir => {
            match resolve_downloads_dir(&caller) {
                Some(path) => path,
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_config_get: downloads dir not available");
                    return -1;
                }
            }
        }
        bedcode_plugin_api_mobile::ConfigKey::CurrentTimeMs => {
            // wasm32-unknown-unknown 无系统时钟（SystemTime/Instant 均 panic），
            // 插件经此获取真实时间（Unix 毫秒）
            match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => d.as_millis().to_string(),
                Err(e) => {
                    tracing::error!(plugin_id = %plugin_id, error = %e, "host_config_get: system time unavailable");
                    return -1;
                }
            }
        }
    };

    match write_wasm_string(&mut caller, &value) {
        Some((ptr, len)) => {
            // 值可能含敏感配置（API key 等），仅记录长度不落盘原文
            tracing::debug!(plugin_id = %plugin_id, key = %key, value_len = value.len(), "host_config_get: ok");
            if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                0
            } else {
                -1
            }
        }
        None => {
            tracing::error!(plugin_id = %plugin_id, key = %key, "host_config_get: failed to write result to WASM memory");
            -1
        }
    }
}


/// 解析下载目录路径
///
/// 解析链与命令层 plugin_saf_list_dir 共用（android_plugins.rs
/// resolve_app_downloads_dir）：Kotlin 桥外部私有目录 → app_data 回退，
/// 目录不存在时惰性创建。两处共用保证外部存储不可用时特殊条目的
/// 派生路径与浏览白名单一致。
pub(crate) fn resolve_downloads_dir(caller: &wasmtime::Caller<'_, WasmPluginState>) -> Option<String> {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();
    let handle = caller.data().runtime_handle.clone();
    let app_handle = host_ctx.app_handle.clone();

    guarded_host_call(&plugin_id, "resolve_downloads_dir", None, || {
        tokio::task::block_in_place(|| {
            handle.block_on(crate::plugin::android_plugins::resolve_app_downloads_dir(
                &app_handle,
            ))
        })
    })
}
