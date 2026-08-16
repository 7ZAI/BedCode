//! host_config_get — 配置读取（逻辑层）

use super::super::WasmPluginState;
use super::support::guarded_host_call;

/// 逻辑层：读取宿主配置项
///
/// 白名单 = SDK `ConfigKey` 枚举本身：`from_str` 过滤非法 key，
/// value match 穷尽所有变体 —— 新增配置项时编译器强制补实现，
/// 结构性杜绝"白名单声明了但实现缺失"的漂移
pub(crate) fn config_get(state: &WasmPluginState, key: &str) -> Result<Option<String>, String> {
    // 白名单校验：仅接受 ConfigKey 枚举覆盖的 key
    let Some(config_key) = bedcode_plugin_api_mobile::ConfigKey::from_str(key) else {
        return Err(format!("key not in whitelist: {}", key));
    };

    // 穷尽 match：新增 ConfigKey 变体必须在此补实现（编译错误兜底）
    let value = match config_key {
        bedcode_plugin_api_mobile::ConfigKey::AppDownloadsDir => {
            resolve_downloads_dir_state(state)?
        }
        bedcode_plugin_api_mobile::ConfigKey::CurrentTimeMs => {
            // wasm32-unknown-unknown 无系统时钟（SystemTime/Instant 均 panic），
            // 插件经此获取真实时间（Unix 毫秒）
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| format!("system time unavailable: {}", e))?
                .as_millis()
                .to_string()
        }
    };
    // 值可能含敏感配置（API key 等），仅记录长度不落盘原文
    tracing::debug!(plugin_id = %state.plugin_id, key = %key, value_len = value.len(), "host_config_get: ok");
    Ok(Some(value))
}

/// 解析下载目录路径（状态版；供组件 trait impl 调用）
///
/// 解析链与命令层 plugin_saf_list_dir 共用（android_plugins.rs
/// resolve_app_downloads_dir）：Kotlin 桥外部私有目录 → app_data 回退，
/// 目录不存在时惰性创建。无 app_handle（无头/测试）时不可用。
pub(crate) fn resolve_downloads_dir_state(state: &WasmPluginState) -> Result<String, String> {
    let Some(app_handle) = state.host_ctx.app_handle.clone() else {
        return Err("app_handle unavailable".to_string());
    };
    let path = guarded_host_call(&state.plugin_id, "resolve_downloads_dir", None, || {
        tokio::task::block_in_place(|| {
            state
                .runtime_handle
                .block_on(crate::plugin::android_plugins::resolve_app_downloads_dir(&app_handle))
        })
    })
    .ok_or_else(|| "downloads dir not available".to_string())?;
    Ok(path)
}
