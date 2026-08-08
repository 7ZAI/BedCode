//! 配置域 Host Functions（白名单配置读取）
//!
//! 逻辑层 `config_get`（权限/白名单校验 + 读取）供 core module 胶水
//! 与 Component Model 绑定（`wasm_runtime::component`）共用。

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};
use crate::system::config::AppConfig;
use bedcode_plugin_api::host::ConfigKey;

/// 逻辑层：读取宿主配置项（白名单 = SDK `ConfigKey` 枚举本身）
///
/// `from_str` 过滤非法 key，value match 穷尽所有变体 —— 新增配置项时
/// 编译器强制补实现，结构性杜绝"白名单声明了但实现缺失"的漂移
pub(crate) fn config_get(plugin_id: &str, key: &str) -> Result<Option<String>, String> {
    // 白名单校验：仅接受 ConfigKey 枚举覆盖的 key
    let Some(config_key) = ConfigKey::from_str(key) else {
        tracing::warn!(plugin_id = %plugin_id, key = %key, "host_config_get: key not in whitelist");
        return Err(format!("key not in whitelist: {}", key));
    };

    // 穷尽 match：新增 ConfigKey 变体必须在此补实现（编译错误兜底）
    let value = match config_key {
        ConfigKey::NetworkPort => {
            // 优先获取服务器实际运行端口（端口冲突时会被重新分配）
            let supervisor = crate::server::supervisor::ServerSupervisor::global();
            let actual_port = block_on_async(supervisor.get_status_info()).port;
            // 实际端口为 0 表示服务器未启动，回退到配置值
            if actual_port > 0 {
                actual_port.to_string()
            } else {
                let config = AppConfig::global();
                config.network.port.to_string()
            }
        }
        ConfigKey::HomeDir => {
            match dirs::home_dir() {
                Some(dir) => dir.to_string_lossy().to_string(),
                None => {
                    tracing::error!(plugin_id = %plugin_id, "host_config_get: home_dir not available");
                    return Err("home_dir not available".to_string());
                }
            }
        }
    };

    Ok(Some(value))
}

/// 配置：读取宿主配置项
///
/// 参数：(key_ptr, key_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
pub(super) fn host_config_get(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    key_ptr: u32,
    key_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();

    let key = match read_wasm_string_consume(&mut caller, key_ptr, key_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_config_get: failed to read key");
            return -1;
        }
    };

    match config_get(&plugin_id, &key) {
        Ok(Some(value)) => match write_wasm_string(&mut caller, &value) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!(plugin_id = %plugin_id, key = %key, "host_config_get: failed to write result to WASM memory");
                -1
            }
        },
        Ok(None) => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
        Err(e) => {
            tracing::error!(plugin_id = %plugin_id, key = %key, "host_config_get: {}", e);
            -1
        }
    }
}
