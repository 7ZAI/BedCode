//! 配置域宿主实现（白名单配置读取）
//!
//! `config_get`（权限/白名单校验 + 读取）供 Component Model 绑定
//! （`wasm_runtime::component`）调用。

use crate::plugin::wasm_runtime::block_on_async;
use crate::system::config::AppConfig;
use bedcode_plugin_api::host::ConfigKey;

/// 读取宿主配置项（白名单 = SDK `ConfigKey` 枚举本身）
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
        ConfigKey::CurrentTimeMs => {
            // wasm32-unknown-unknown 无系统时钟（SystemTime/Instant 均 panic），
            // 插件经此获取真实时间（Unix 毫秒）
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().to_string())
                .map_err(|e| format!("system time unavailable: {}", e))?
        }
    };

    Ok(Some(value))
}
