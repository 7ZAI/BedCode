//! 配置域宿主实现（白名单配置读取）
//!
//! `config_get`（白名单校验 + 读取）供 core 绑定层
//! （`runtime/core_binding.rs` 的 `host-config` `Host` impl）调用。
//!
//! ## 白名单词汇为何是「内核并集」而非某端 SDK 枚举（票 03）
//!
//! `host-config` 是 core.wit 的双端全等 interface，其 `Host` impl 收进单份 core 绑定
//! 后，实现体不能再引某端 SDK 类型——两端 `ConfigKey` 枚举**实质不同**（桌面含
//! `network.port` / `home_dir` / `os.platform` + 两项 TTL；移动是
//! `app.downloads-dir` / `system.time_ms`），不是同一形状的两个副本。
//!
//! 故本模块持**并集词汇**：跨端项（时间 / 平台 / TTL）内核直算；端专属项
//! （`network.port` 需服务器 supervisor、`app.downloads-dir` 需移动 SAF 路径）
//! 经 [`HostEnginePorts`] 取宿主注入值，**未注入即 fail-visible 报错**而不是静默
//! 返回空值——断链必须可见。
//!
//! 注意：并集意味着「某端插件可读到另一端的键」，这是词汇可用性的放宽而非
//! 数据泄露面（各端键取的都是本端事实，缺失键按端口缺省 fail-visible）。

use crate::host_api::ports::HostEnginePorts;
use crate::runtime_util::block_on_async;
#[cfg(test)]
use crate::host_api::context::WasmHostContext;

/// 配置项白名单（双端并集词汇，内核自持）
///
/// 变体即 wire 字符串，新增项必须在 [`config_get`] 的 match 里补实现（编译器兜底）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKey {
    /// 服务器实际运行端口（端口冲突时返回重新分配后的值）——桌面专属
    NetworkPort,
    /// 用户主目录绝对路径
    HomeDir,
    /// 宿主当前 Unix 毫秒时间戳（wasm32-unknown-unknown 无系统时钟）
    CurrentTimeMs,
    /// 宿主操作系统平台（`std::env::consts::OS`）
    OsPlatform,
    /// 配对码有效期（秒，十进制字符串）
    PairingCodeTtl,
    /// QR token 有效期（秒，十进制字符串）
    QrTokenTtl,
    /// 应用下载目录绝对路径（移动专属）
    AppDownloadsDir,
}

impl ConfigKey {
    /// wire 字符串 → 枚举（白名单校验）
    pub fn from_str(key: &str) -> Option<Self> {
        Some(match key {
            "network.port" => Self::NetworkPort,
            "home_dir" => Self::HomeDir,
            "system.time_ms" => Self::CurrentTimeMs,
            "os.platform" => Self::OsPlatform,
            "pairing_code_ttl" => Self::PairingCodeTtl,
            "qr_token_ttl" => Self::QrTokenTtl,
            "app.downloads_dir" => Self::AppDownloadsDir,
            _ => return None,
        })
    }
}

/// 配对码有效期缺省值（与宿主命令面 `get_pairing_code_ttl` 同源常量）
const DEFAULT_PAIRING_CODE_TTL_SECS: u64 = crate::system::constants::PAIRING_CODE_TTL_SECS;
/// QR token 有效期缺省值（宿主命令面 `get_qr_token_ttl` 的 300，两处同值）
const DEFAULT_QR_TOKEN_TTL_SECS: u64 = 300;

/// 读取宿主配置项（白名单 = [`ConfigKey`] 并集词汇）
///
/// `from_str` 过滤非法 key，value match 穷尽所有变体 —— 新增配置项时
/// 编译器强制补实现，结构性杜绝"白名单声明了但实现缺失"的漂移。
pub(crate) fn config_get(
    db: &dyn crate::host_api::context::DbScope,
    #[cfg_attr(not(feature = "common-host"), allow(unused_variables))]
    ports: &dyn HostEnginePorts,
    plugin_id: &str,
    key: &str,
) -> Result<Option<String>, String> {
    // 白名单校验：仅接受 ConfigKey 枚举覆盖的 key
    let Some(config_key) = ConfigKey::from_str(key) else {
        tracing::warn!(plugin_id = %plugin_id, key = %key, "host_config_get: key not in whitelist");
        return Err(format!("key not in whitelist: {}", key));
    };

    // 穷尽 match：新增 ConfigKey 变体必须在此补实现（编译错误兜底）
    let value = match config_key {
        // `network.port` 是**桌面入站服务器**的运行态事实。取不到实际端口（服务器
        // 未启动）时报错而非回退配置默认值——回退会让插件拿到一个看似合法实则连不上
        // 的端口，排查成本远高于一次显式失败。
        //
        // 形态门控是**过渡缝**（票 03 / T-04 收口）：随桌面绑定层整体迁出后本臂与
        // `common-host` 一同消失，非桌面侧恒走下方 fail-visible 分支。
        #[cfg(feature = "common-host")]
        ConfigKey::NetworkPort => {
            let supervisor = bedcode_server_core::supervisor::ServerSupervisor::global();
            let actual_port = crate::runtime_util::block_on_async(supervisor.get_status_info()).port;
            if actual_port > 0 {
                actual_port.to_string()
            } else {
                return Err(format!(
                    "network.port unavailable: host server not started (plugin {})",
                    plugin_id
                ));
            }
        }
        #[cfg(not(feature = "common-host"))]
        ConfigKey::NetworkPort => {
            return Err(format!(
                "network.port unavailable: this host has no inbound server (plugin {})",
                plugin_id
            ));
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
            return auth_setting_seconds(db, "pairing_code_ttl", DEFAULT_PAIRING_CODE_TTL_SECS)
        }
        ConfigKey::QrTokenTtl => return auth_setting_seconds(db, "qr_token_ttl", DEFAULT_QR_TOKEN_TTL_SECS),
        // 下载目录是**移动宿主**的 SAF 路径事实（Android 外部私有目录），桌面无
        // 对应物；未注入即 fail-visible。
        ConfigKey::AppDownloadsDir => {
            let dir = ports.app_downloads_dir().ok_or_else(|| {
                format!(
                    "app.downloads_dir unavailable: host injected no downloads dir (plugin {})",
                    plugin_id
                )
            })?;
            dir
        }
    };

    Ok(Some(value))
}

/// 读认证域设置项（`settings` 表）→ 十进制秒数字符串；缺失 / 非法回退默认值
fn auth_setting_seconds(
    db: &dyn crate::host_api::context::DbScope,
    key: &str,
    default: u64,
) -> Result<Option<String>, String> {
    let db = db.database().clone();
    let setting_key = key.to_string();
    let stored = block_on_async(async move {
        let db = db.lock().expect("config db lock poisoned");
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
        let err = config_get(host_ctx().as_ref(), host_ctx().ports().as_ref(), "test-plugin", "network.password").unwrap_err();
        assert!(err.contains("not in whitelist"), "got: {}", err);
        assert!(err.contains("network.password"));
    }

    /// 空 key 同样拒绝
    #[test]
    fn config_get_empty_key_rejected() {
        let err = config_get(host_ctx().as_ref(), host_ctx().ports().as_ref(), "test-plugin", "").unwrap_err();
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
        let value = config_get(host_ctx().as_ref(), host_ctx().ports().as_ref(), "test-plugin", "system.time_ms")
            .expect("time key ok")
            .expect("some value");
        let parsed: u128 = value.parse().expect("parse ms");
        assert!(parsed >= before_ms, "got {} before now {}", parsed, before_ms);
    }

    /// home_dir：返回非空主目录路径
    #[test]
    fn config_get_home_dir_ok() {
        let value = config_get(host_ctx().as_ref(), host_ctx().ports().as_ref(), "test-plugin", "home_dir")
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
        let value = config_get(host_ctx().as_ref(), host_ctx().ports().as_ref(), "test-plugin", "os.platform")
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
        let value = config_get(host_ctx().as_ref(), host_ctx().ports().as_ref(), "test-plugin", "network.port")
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
            config_get(ctx.as_ref(), ctx.ports().as_ref(), "test-plugin", "pairing_code_ttl").expect("read"),
            Some("60".to_string())
        );
        assert_eq!(
            config_get(ctx.as_ref(), ctx.ports().as_ref(), "test-plugin", "qr_token_ttl").expect("read"),
            Some("300".to_string())
        );

        // ② 落库（模拟 host-auth auth-setting-set 的写入结果）后读回新值
        {
            let db = ctx.database().lock().expect("db lock");
            db.set_setting("pairing_code_ttl", "600").expect("write");
            db.set_setting("qr_token_ttl", "1200").expect("write");
        }
        assert_eq!(
            config_get(ctx.as_ref(), ctx.ports().as_ref(), "test-plugin", "pairing_code_ttl").expect("read"),
            Some("600".to_string())
        );
        assert_eq!(
            config_get(ctx.as_ref(), ctx.ports().as_ref(), "test-plugin", "qr_token_ttl").expect("read"),
            Some("1200".to_string())
        );

        // ③ 非法值（非十进制）→ 回默认值，不报错（宿主命令面同口径）
        {
            let db = ctx.database().lock().expect("db lock");
            db.set_setting("qr_token_ttl", "abc").expect("write");
        }
        assert_eq!(
            config_get(ctx.as_ref(), ctx.ports().as_ref(), "test-plugin", "qr_token_ttl").expect("read"),
            Some("300".to_string())
        );
    }
}
