//! 配置域宿主实现（白名单配置读取）
//!
//! `config_get`（权限/白名单校验 + 读取）供 Component Model 绑定
//! （`wasm_runtime::component`）调用。

use crate::wasm_core::host_api::context::WasmHostContext;
use crate::wasm_core::runtime_util::block_on_async;
use crate::system::config::AppConfig;
use bedcode_plugin_api::host::ConfigKey;

/// 配对码有效期缺省值（与宿主命令面 `get_pairing_code_ttl` 同源常量）
const DEFAULT_PAIRING_CODE_TTL_SECS: u64 = crate::system::constants::PAIRING_CODE_TTL_SECS;
/// QR token 有效期缺省值（宿主命令面 `get_qr_token_ttl` 的 300，两处同值）
const DEFAULT_QR_TOKEN_TTL_SECS: u64 = 300;

/// 读取宿主配置项（白名单 = SDK `ConfigKey` 枚举本身）
///
/// `from_str` 过滤非法 key，value match 穷尽所有变体 —— 新增配置项时
/// 编译器强制补实现，结构性杜绝"白名单声明了但实现缺失"的漂移
pub(crate) fn config_get(host_ctx: &WasmHostContext, plugin_id: &str, key: &str) -> Result<Option<String>, String> {
    // 白名单校验：仅接受 ConfigKey 枚举覆盖的 key
    let Some(config_key) = ConfigKey::from_str(key) else {
        tracing::warn!(plugin_id = %plugin_id, key = %key, "host_config_get: key not in whitelist");
        return Err(format!("key not in whitelist: {}", key));
    };

    // 穷尽 match：新增 ConfigKey 变体必须在此补实现（编译错误兜底）
    let value = match config_key {
        ConfigKey::NetworkPort => {
            // 优先获取服务器实际运行端口（端口冲突时会被重新分配）
            let supervisor = crate::server::core::supervisor::ServerSupervisor::global();
            let actual_port = block_on_async(supervisor.get_status_info()).port;
            // 实际端口为 0 表示服务器未启动，回退到配置值
            if actual_port > 0 {
                actual_port.to_string()
            } else {
                let config = AppConfig::global();
                config.network.port.to_string()
            }
        }
        ConfigKey::HomeDir => match dirs::home_dir() {
            Some(dir) => dir.to_string_lossy().to_string(),
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_config_get: home_dir not available");
                return Err("home_dir not available".to_string());
            }
        },
        ConfigKey::CurrentTimeMs => {
            // wasm32-unknown-unknown 无系统时钟（SystemTime/Instant 均 panic），
            // 插件经此获取真实时间（Unix 毫秒）
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().to_string())
                .map_err(|e| format!("system time unavailable: {}", e))?
        }
        ConfigKey::OsPlatform => {
            // 插件侧 wasm32-unknown-unknown 无法感知宿主 OS，经此获取平台名
            // （std::env::consts::OS：windows / linux / macos / …）
            std::env::consts::OS.to_string()
        }
        // 票 14：认证域两项有效期的**读侧**（写侧走 host-auth `auth-setting-set`，
        // 键白名单同这两键）。真源是宿主 `settings` 表；缺失 / 非法 / 不可读时
        // 回退默认值而非报错——与宿主命令面（`get_pairing_code_ttl` /
        // `get_qr_token_ttl`）同口径，保证设置分组回显与宿主行为一致。
        ConfigKey::PairingCodeTtl => {
            return auth_setting_seconds(host_ctx, "pairing_code_ttl", DEFAULT_PAIRING_CODE_TTL_SECS)
        }
        ConfigKey::QrTokenTtl => return auth_setting_seconds(host_ctx, "qr_token_ttl", DEFAULT_QR_TOKEN_TTL_SECS),
    };

    Ok(Some(value))
}

/// 读认证域设置项（`settings` 表）→ 十进制秒数字符串；缺失 / 非法回退默认值
fn auth_setting_seconds(host_ctx: &WasmHostContext, key: &str, default: u64) -> Result<Option<String>, String> {
    let db = host_ctx.db.clone();
    let setting_key = key.to_string();
    let stored = block_on_async(async move {
        let db = db.lock().await;
        db.get_setting(&setting_key)
            .map_err(|e| format!("database error: {}", e))
    })?;
    let seconds = stored.and_then(|v| v.parse::<u64>().ok()).unwrap_or(default);
    Ok(Some(seconds.to_string()))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 测试宿主上下文（内嵌内存库 + 真实 schema；与 host_impl 其余模块同一构造）
    fn host_ctx() -> Arc<WasmHostContext> {
        super::super::tests::build_host_ctx()
    }

    /// 白名单外 key：在触达任何全局单例前被拒绝（纯校验路径）
    #[test]
    fn config_get_key_not_in_whitelist_rejected() {
        let err = config_get(&host_ctx(), "test-plugin", "network.password").unwrap_err();
        assert!(err.contains("not in whitelist"), "got: {}", err);
        assert!(err.contains("network.password"));
    }

    /// 空 key 同样拒绝
    #[test]
    fn config_get_empty_key_rejected() {
        let err = config_get(&host_ctx(), "test-plugin", "").unwrap_err();
        assert!(err.contains("not in whitelist"), "got: {}", err);
    }

    /// system.time_ms：返回接近当前的 Unix 毫秒（纯 std 时间，无全局状态）
    ///
    /// 该 key 是 wasm 插件唯一的时钟来源（wasm32 无 SystemTime），值必须可解析
    #[test]
    fn config_get_current_time_ms_ok() {
        let before_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let value = config_get(&host_ctx(), "test-plugin", "system.time_ms")
            .expect("time key ok")
            .expect("some value");
        let parsed: u128 = value.parse().expect("parse ms");
        assert!(parsed >= before_ms, "got {} before now {}", parsed, before_ms);
    }

    /// home_dir：返回非空主目录路径
    #[test]
    fn config_get_home_dir_ok() {
        let value = config_get(&host_ctx(), "test-plugin", "home_dir")
            .expect("home dir ok")
            .expect("some value");
        assert!(!value.is_empty());
    }

    /// os.platform：返回宿主平台名（std::env::consts::OS，无全局状态）
    ///
    /// 插件侧 wasm32-unknown-unknown 无法感知宿主 OS，命令包装等
    /// 平台相关逻辑依赖此值（scheduler 插件 inline 命令 sh -c vs cmd /C）
    #[test]
    fn config_get_os_platform_ok() {
        let value = config_get(&host_ctx(), "test-plugin", "os.platform")
            .expect("platform key ok")
            .expect("some value");
        // 必须与 std 编译目标一致（当前进程的平台），插件据此分支
        assert_eq!(value, std::env::consts::OS);
        assert!(!value.is_empty());
    }

    /// network.port：服务器未启动时回退默认端口，仍应返回合法端口号
    ///
    /// ServerSupervisor 为 LazyLock 全局单例（测试进程内默认端口 8765），
    /// 不断言具体值（其它测试可能已改变端口），只验证语义：端口 > 0
    #[tokio::test]
    async fn config_get_network_port_ok() {
        let value = config_get(&host_ctx(), "test-plugin", "network.port")
            .expect("port key ok")
            .expect("some value");
        let port: u16 = value.parse().expect("port is u16");
        assert!(port > 0, "port must be > 0, got {}", port);
    }

    /// 票 14：认证域两项有效期的读侧契约——
    /// ① 键缺失 → 与宿主命令面同默认值；② 落库后读回新值（写侧走 host-auth
    /// `auth-setting-set`，两处共用 `settings` 表同一键）；③ 非法值回默认不报错
    #[test]
    fn config_get_auth_ttls_default_persisted_and_fallback() {
        let ctx = host_ctx();

        // ① 缺省（表内无键）
        assert_eq!(
            config_get(&ctx, "test-plugin", "pairing_code_ttl").expect("read"),
            Some("60".to_string())
        );
        assert_eq!(
            config_get(&ctx, "test-plugin", "qr_token_ttl").expect("read"),
            Some("300".to_string())
        );

        // ② 落库（模拟 host-auth auth-setting-set 的写入结果）后读回新值
        {
            let db = ctx.db.blocking_lock();
            db.set_setting("pairing_code_ttl", "600").expect("write");
            db.set_setting("qr_token_ttl", "1200").expect("write");
        }
        assert_eq!(
            config_get(&ctx, "test-plugin", "pairing_code_ttl").expect("read"),
            Some("600".to_string())
        );
        assert_eq!(
            config_get(&ctx, "test-plugin", "qr_token_ttl").expect("read"),
            Some("1200".to_string())
        );

        // ③ 非法值（非十进制）→ 回默认值，不报错（宿主命令面同口径）
        {
            let db = ctx.db.blocking_lock();
            db.set_setting("qr_token_ttl", "abc").expect("write");
        }
        assert_eq!(
            config_get(&ctx, "test-plugin", "qr_token_ttl").expect("read"),
            Some("300".to_string())
        );
    }
}
