//! 设备与配对域命令面（票 14）
//!
//! 插件前端设备页（配对码 / QR / 设备列表 / 连接历史 / 设置分组）的命令入口。
//! 这些命令**不是**互调 api（不对外提供给其他插件），因此不进 manifest `api`；
//! 它们与互调 api 面共享同一批底层实现（配对码 / QR 走 crate 根的 `pair_*` /
//! `qr_*`，信任走 [`crate::trust`]），本模块只补「前端取数编排」这一层。
//!
//! 能力来源（spec D2 红线）：只经宿主 `host-*` 原语——`host-config`（端口 /
//! 两项有效期）、`host-platform`（本机 IPv4 枚举）、`host-auth`（配对记录 /
//! 连接历史 / 认证域设置写入）。**无新增宿主命令、无宿主特化**。
//!
//! 失败语义：native（cargo test，链接中无宿主符号）下显性报错而非返回空视图——
//! 空列表会被消费方读成「没有任何已配对设备 / 没有连接历史」，是最危险的默认值。

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{ConfigKey, HostAuth, HostConfig, HostPlatform};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 配对码有效期缺省（秒）：与宿主命令面 `get_pairing_code_ttl` 同源常量
const DEFAULT_PAIRING_CODE_TTL_SECS: u64 = 60;
/// QR token 有效期缺省（秒）：与宿主命令面 `get_qr_token_ttl` 的 300 同值
const DEFAULT_QR_TOKEN_TTL_SECS: u64 = 300;

// ==================== 网络信息（端口 + 本机 IPv4） ====================

/// 网络信息：`{port, addresses}`——设备页网络信息条的数据源
///
/// 端口取 host-config `network.port`（服务器实际运行端口，端口冲突时为重分配值）；
/// 地址取 host-platform 的本机 IPv4 枚举（已排除回环与链路本地）。
/// 地址为空是合法状态（前端渲染「未找到可用的 IPv4 地址」），不报错。
#[cfg(target_arch = "wasm32")]
pub fn network_info_via_host() -> Result<serde_json::Value, String> {
    let port = config_seconds(ConfigKey::NetworkPort, 0)?;
    let addresses = WasmHost
        .platform_local_ipv4_addresses()
        .map_err(|e| e.message)?;
    Ok(serde_json::json!({ "port": port, "addresses": addresses }))
}

/// 网络信息（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn network_info_via_host() -> Result<serde_json::Value, String> {
    Err("network info unavailable outside wasm runtime".to_string())
}

// ==================== 有效期设置（设置分组） ====================

/// 读两项有效期：`{pairingCodeTtl, qrTokenTtl}`（秒）
///
/// 读侧经 host-config 白名单两键（宿主 `settings` 表，缺失回宿主默认值）；
/// 写侧见 [`ttl_set_via_host`]（host-auth `auth-setting-set` 同两键）。
#[cfg(target_arch = "wasm32")]
pub fn ttl_get_via_host() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "pairingCodeTtl": config_seconds(ConfigKey::PairingCodeTtl, DEFAULT_PAIRING_CODE_TTL_SECS)?,
        "qrTokenTtl": config_seconds(ConfigKey::QrTokenTtl, DEFAULT_QR_TOKEN_TTL_SECS)?,
    }))
}

/// 读两项有效期（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn ttl_get_via_host() -> Result<serde_json::Value, String> {
    Err("auth settings unavailable outside wasm runtime".to_string())
}

/// 写一项有效期：`{key, value}`（key 只接受 `pairingCodeTtl` / `qrTokenTtl`）
///
/// 前端只做 UX 侧范围收敛，**最终仲裁在宿主**：`auth-setting-set` 的键白名单
/// （`pairing_code_ttl` / `qr_token_ttl`）+ 正整数校验；非法键在此显性报错，
/// 不静默降级（键名拼错必须可见）。返回写入后的两项当前值。
#[cfg(target_arch = "wasm32")]
pub fn ttl_set_via_host(key: &str, value: u64) -> Result<serde_json::Value, String> {
    let host_key = auth_setting_key(key)
        .ok_or_else(|| format!("unknown auth setting key: {}", key))?;
    WasmHost
        .auth_setting_set(host_key, &value.to_string())
        .map_err(|e| e.message)?;
    ttl_get_via_host()
}

/// 前端设置键 → 宿主 `settings` 键白名单映射（纯逻辑，native 单测覆盖）
///
/// 两侧字面量刻意不同形（前端 camelCase / 宿主 snake_case），映射是唯一转换点：
/// 未列入白名单的前端键返回 `None`，由调用方显性报错（拼错键必须可见，不静默降级）。
pub fn auth_setting_key(key: &str) -> Option<&'static str> {
    match key {
        "pairingCodeTtl" => Some("pairing_code_ttl"),
        "qrTokenTtl" => Some("qr_token_ttl"),
        _ => None,
    }
}

/// 写一项有效期（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn ttl_set_via_host(_key: &str, _value: u64) -> Result<serde_json::Value, String> {
    Err("auth settings unavailable outside wasm runtime".to_string())
}

// ==================== 配对码 / QR（复用 crate 根状态机，补 TTL 与连接信息组装） ====================

/// 生成配对码（TTL 取自认证域设置项，与宿主命令面同源）→ 宿主 `PairingCode` 形状
#[cfg(target_arch = "wasm32")]
pub fn pairing_generate_via_host() -> Result<serde_json::Value, String> {
    let ttl = config_seconds(ConfigKey::PairingCodeTtl, DEFAULT_PAIRING_CODE_TTL_SECS)?;
    crate::pair_code_generate(ttl)
}

/// 生成配对码（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn pairing_generate_via_host() -> Result<serde_json::Value, String> {
    Err("pairing code unavailable outside wasm runtime".to_string())
}

/// 当前配对码（过滤过期）→ `PairingCode | null`（与 crate 根 api 面同一状态机）
///
/// 命令面统一以 `Value` 收口（无码 = `null`）：插件命令通道回执是 JSON 值，
/// `Option` 与 `null` 对前端同义，但命令面签名必须单一形状（无需前端分支解包）。
pub fn pairing_status_via_host() -> Result<serde_json::Value, String> {
    Ok(crate::pair_code_status()?.unwrap_or(serde_json::Value::Null))
}

/// 清除当前配对码（幂等）
pub fn pairing_clear_via_host() -> Result<(), String> {
    *crate::CURRENT_CODE
        .lock()
        .map_err(|e| format!("pairing code lock: {e}"))? = None;
    Ok(())
}

/// 生成 QR 连接信息：`{host, port, token, remainingSecs}`
///
/// `host` 来自前端选择（用户选的移动端可访问 IP，落插件存储）；缺省时回退本机
/// 第一个可用 IPv4（与宿主原页「未指定 host 时自动选第一个」同口径）。
/// 端口取 host-config `network.port`——**线协议载荷形状不变**（host / port / token
/// 三要素，移动端扫码逻辑零改动）。
#[cfg(target_arch = "wasm32")]
pub fn qr_generate_via_host(host: Option<&str>) -> Result<serde_json::Value, String> {
    let ttl = config_seconds(ConfigKey::QrTokenTtl, DEFAULT_QR_TOKEN_TTL_SECS)?;
    let generated = crate::qr_generate(ttl)?;
    qr_connection_info(host, &generated)
}

/// 生成 QR 连接信息（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn qr_generate_via_host(_host: Option<&str>) -> Result<serde_json::Value, String> {
    Err("qr connection info unavailable outside wasm runtime".to_string())
}

/// 当前 QR 连接信息（无活跃 token → `null`）
#[cfg(target_arch = "wasm32")]
pub fn qr_info_via_host(host: Option<&str>) -> Result<serde_json::Value, String> {
    match crate::qr_status()? {
        None => Ok(serde_json::Value::Null),
        Some(status) => qr_connection_info(host, &status),
    }
}

/// 当前 QR 连接信息（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn qr_info_via_host(_host: Option<&str>) -> Result<serde_json::Value, String> {
    Err("qr connection info unavailable outside wasm runtime".to_string())
}

/// 清除当前 QR token（幂等）
pub fn qr_clear_via_host() -> Result<(), String> {
    crate::qr_manager().clear();
    Ok(())
}

/// 组装 QR 连接信息（host 缺省回退本机首个可用 IPv4；端口取宿主运行端口）
#[cfg(target_arch = "wasm32")]
fn qr_connection_info(
    host: Option<&str>,
    status: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let port = config_seconds(ConfigKey::NetworkPort, 0)?;
    let resolved_host = match host.map(str::trim).filter(|h| !h.is_empty()) {
        Some(h) => h.to_string(),
        None => WasmHost
            .platform_local_ipv4_addresses()
            .map_err(|e| e.message)?
            .into_iter()
            .next()
            .unwrap_or_default(),
    };
    let token = status
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "qr status missing token".to_string())?;
    let remaining = status.get("remaining").and_then(|v| v.as_u64()).unwrap_or(0);
    Ok(serde_json::json!({
        "host": resolved_host,
        "port": port,
        "token": token,
        "remainingSecs": remaining,
    }))
}

// ==================== 已配对设备（活跃记录） ====================

/// 已配对设备列表：`PairedDeviceInfo[]`（camelCase，与前端 DTO 同形）
///
/// 数据源 = host-auth `trusted-devices-list` **全表原始记录**（含软删行），
/// 本模块只保留 `isActive = true` 的行并按其 RFC3339 `pairedAt` 倒序——过滤与
/// 排序是插件的展示组织，内核只给原始事实（ADR 0022 裁剪线）。
/// 凭据列（session token / public key）不在记录面内（宿主 §8 红线）。
#[cfg(target_arch = "wasm32")]
pub fn paired_list_via_host() -> Result<serde_json::Value, String> {
    let raw = WasmHost
        .auth_trusted_devices_list()
        .map_err(|e| e.message)?;
    Ok(serde_json::Value::Array(active_pairings(&raw)))
}

/// 已配对设备列表（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn paired_list_via_host() -> Result<serde_json::Value, String> {
    Err("paired devices unavailable outside wasm runtime".to_string())
}

/// 原始配对记录 → 活跃设备列表（纯逻辑，native 单测覆盖）
///
/// 契约：`isActive` 缺省视为活跃（旧记录无该列时不误判为撤销）；缺字段的条目
/// 跳过（宽容解析，与 [`crate::devices::parse_connections`] 同范式）；
/// 输出按 `pairedAt` 倒序（最新配对在前），空列表合法。
pub fn active_pairings(raw: &serde_json::Value) -> Vec<serde_json::Value> {
    let Some(arr) = raw.as_array() else {
        return Vec::new();
    };
    let mut rows: Vec<serde_json::Value> = arr
        .iter()
        .filter_map(|v| {
            let id = v.get("id")?.as_str()?.to_string();
            let device_name = v.get("deviceName")?.as_str()?.to_string();
            let device_fingerprint = v.get("deviceFingerprint")?.as_str()?.to_string();
            let paired_at = v.get("pairedAt")?.as_str()?.to_string();
            let active = v.get("isActive").and_then(|x| x.as_bool()).unwrap_or(true);
            if !active {
                return None;
            }
            let address = v
                .get("address")
                .and_then(|x| x.as_str())
                .map(str::to_string);
            let last_seen = v
                .get("lastSeen")
                .and_then(|x| x.as_str())
                .map(str::to_string);
            let connect_count = v.get("connectCount").and_then(|x| x.as_u64()).unwrap_or(0);
            Some(serde_json::json!({
                "id": id,
                "deviceName": device_name,
                "deviceFingerprint": device_fingerprint,
                "address": address,
                "pairedAt": paired_at,
                "lastSeen": last_seen,
                "connectCount": connect_count,
            }))
        })
        .collect();
    rows.sort_by(|a, b| {
        let left = a.get("pairedAt").and_then(|v| v.as_str()).unwrap_or("");
        let right = b.get("pairedAt").and_then(|v| v.as_str()).unwrap_or("");
        right.cmp(left)
    });
    rows
}

// ==================== 连接历史 ====================

/// 设备连接历史：原始记录 JSON 数组（`device-id` = 配对记录 id，倒序由宿主查询给出）
#[cfg(target_arch = "wasm32")]
pub fn history_list_via_host(device_id: &str) -> Result<serde_json::Value, String> {
    WasmHost
        .auth_connection_history_list(device_id)
        .map_err(|e| e.message)
}

/// 设备连接历史（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn history_list_via_host(_device_id: &str) -> Result<serde_json::Value, String> {
    Err("connection history unavailable outside wasm runtime".to_string())
}

/// 清空设备连接历史 → `{cleared}`（是否命中了至少一条记录；空历史幂等 false）
///
/// 不影响配对状态（与撤销配对的连带删除区分开：那是隐式清理，本函数是显式动作）。
#[cfg(target_arch = "wasm32")]
pub fn history_clear_via_host(device_id: &str) -> Result<serde_json::Value, String> {
    let cleared = WasmHost
        .auth_connection_history_clear(device_id)
        .map_err(|e| e.message)?;
    Ok(serde_json::json!({ "cleared": cleared }))
}

/// 清空设备连接历史（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn history_clear_via_host(_device_id: &str) -> Result<serde_json::Value, String> {
    Err("connection history unavailable outside wasm runtime".to_string())
}

// ==================== 内部：配置项读取（共用错误包装） ====================

/// 读 host-config 白名单项并解析为秒数；缺失 / 非法回退 `default`
#[cfg(target_arch = "wasm32")]
fn config_seconds(key: ConfigKey, default: u64) -> Result<u64, String> {
    let raw = WasmHost.config_get(key).map_err(|e| e.message)?;
    Ok(raw
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn pairing(id: &str, name: &str, paired_at: &str, active: bool) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "deviceName": name,
            "deviceFingerprint": format!("fp-{id}"),
            "address": "192.168.1.50:9000",
            "pairedAt": paired_at,
            "lastSeen": "2026-09-20T02:00:00Z",
            "connectCount": 3,
            "isActive": active,
        })
    }

    /// 只回活跃记录：软删行（已撤销）不得出现在设备列表里
    #[test]
    fn active_pairings_filters_revoked_records() {
        let raw = serde_json::json!([
            pairing("p-1", "Pixel", "2026-09-18T00:00:00Z", true),
            pairing("p-2", "Reno", "2026-09-19T00:00:00Z", false),
        ]);
        let rows = active_pairings(&raw);
        assert_eq!(rows.len(), 1, "已撤销记录必须被过滤");
        assert_eq!(rows[0]["id"], "p-1");
        assert_eq!(rows[0]["deviceName"], "Pixel");
        assert_eq!(rows[0]["address"], "192.168.1.50:9000");
    }

    /// 排序契约：按 pairedAt 倒序（最新配对在前），与宿主列表观感一致
    #[test]
    fn active_pairings_sorted_by_paired_at_desc() {
        let raw = serde_json::json!([
            pairing("old", "Old", "2026-09-01T00:00:00Z", true),
            pairing("new", "New", "2026-09-19T00:00:00Z", true),
            pairing("mid", "Mid", "2026-09-10T00:00:00Z", true),
        ]);
        let ids: Vec<String> = active_pairings(&raw)
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, vec!["new", "mid", "old"]);
    }

    /// 宽容解析：`isActive` 缺省视为活跃（旧记录不误判撤销）、缺必需字段的条目跳过、
    /// 非数组输入回空（不 panic）
    #[test]
    fn active_pairings_is_lenient() {
        assert!(active_pairings(&serde_json::json!("boom")).is_empty());

        let raw = serde_json::json!([
            {
                "id": "p-1",
                "deviceName": "Legacy",
                "deviceFingerprint": "fp-1",
                "pairedAt": "2026-09-18T00:00:00Z"
            },
            { "id": "p-2", "deviceName": "Broken" },
        ]);
        let rows = active_pairings(&raw);
        assert_eq!(rows.len(), 1, "缺必需字段的条目跳过");
        assert_eq!(rows[0]["id"], "p-1");
        assert_eq!(rows[0]["connectCount"], 0, "缺省连接次数为 0");
        assert!(rows[0]["address"].is_null(), "缺省地址为 null");
    }

    /// native 面显性报错（不静默返回空视图——空列表会被读成「没有设备」）
    #[test]
    fn command_faces_fail_loudly_on_native() {
        for (name, result) in [
            ("network.info", network_info_via_host()),
            ("ttl.get", ttl_get_via_host()),
            ("ttl.set", ttl_set_via_host("qrTokenTtl", 600)),
            ("paired-list", paired_list_via_host()),
            ("history.list", history_list_via_host("p-1")),
            ("history.clear", history_clear_via_host("p-1")),
            ("pairing.generate", pairing_generate_via_host()),
            ("qr.generate", qr_generate_via_host(Some("192.168.1.10"))),
            ("qr.info", qr_info_via_host(None)),
        ] {
            let err = result.expect_err("native 必须显性失败");
            assert!(
                err.contains("unavailable outside wasm runtime"),
                "{name} 错误信息缺上下文: {err}"
            );
        }
    }

    /// 设置键映射：只接受白名单前端键，未列入返回 None（调用方据此显性报错，
    /// 不静默写错键）；宿主键与前端键字面量不同形，混用必须不通过
    #[test]
    fn auth_setting_key_maps_only_whitelisted_frontend_keys() {
        assert_eq!(auth_setting_key("pairingCodeTtl"), Some("pairing_code_ttl"));
        assert_eq!(auth_setting_key("qrTokenTtl"), Some("qr_token_ttl"));
        assert_eq!(auth_setting_key("nope"), None);
        assert_eq!(
            auth_setting_key("pairing_code_ttl"),
            None,
            "宿主键字面量不是前端键（映射是唯一转换点）"
        );
    }
}
