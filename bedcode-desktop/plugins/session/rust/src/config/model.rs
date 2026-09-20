//! 会话配置模型与值对象辅助（票 08）
//!
//! **wire 形状是兼容红线**：字段名与宿主 `db::models::SessionConfig` 逐字一致
//! （camelCase）——宿主命令面与移动端 DTO 都消费这个形状（spec D5「线协议形状
//! 保持不变」），插件侧不引入第二套字段名。DB 列名是 snake_case，映射只存在于
//! [`super::store`] 的 wasm 实现里。
//!
//! 时间与 ID 由插件自产（真源在插件侧）：
//! - 时间：`wasi:clocks` → unix 秒（[`now_rfc3339`]），**必须产出宿主 `DateTime<Utc>`
//!   可反序列化的 RFC3339 字符串**——宿主把写入结果投影回主库时按 DTO 解析，
//!   格式错即投影失败；
//! - ID：`wasi:random` → UUID v4 字符串（[`new_config_id`]），与宿主既有配置
//!   id 形态一致（前端把它当不透明字符串，但形态统一便于排查）。

use serde::{Deserialize, Serialize};

/// 合法环境取值（与主库 `session_configs.environment` 的 CHECK 约束同集合；
/// 宿主 `validate_config` 还兼容 `powershell` / `cmd` 历史值并只 warn，但 DB
/// CHECK 会拒——插件侧取严格集合，写入前即报错）
pub const VALID_ENVIRONMENTS: &[&str] = &["windows", "wsl2", "linux"];

/// 空命令兜底（posix 分支：linux / wsl2）
pub const DEFAULT_COMMAND_POSIX: &str = "bash";
/// 空命令兜底（windows 分支）
pub const DEFAULT_COMMAND_WINDOWS: &str = "powershell";

/// 会话配置（插件私有库真源行；wire 形状 camelCase）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    pub id: String,
    pub name: String,
    pub environment: String,
    /// 仅 `environment = wsl2` 时有值
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wsl_distro: Option<String>,
    pub working_dir: String,
    pub command: String,
    pub auto_start: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 写入请求（宿主命令面入参 / 插件 api 入参）
///
/// 缺省字段回落既有值（`id` 命中时）；`id` 缺省/空表示新建。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wsl_distro: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start: Option<bool>,
}

impl ConfigDraft {
    /// 空命令兜底：按环境分支给默认 shell（业务规则，票 08 自宿主下沉）
    pub fn default_command_for(environment: &str) -> &'static str {
        if environment.eq_ignore_ascii_case("windows") {
            DEFAULT_COMMAND_WINDOWS
        } else {
            DEFAULT_COMMAND_POSIX
        }
    }
}

// ==================== 值对象辅助（时钟 / ID） ====================

/// 当前时间的 RFC3339（UTC，秒级）
pub fn now_rfc3339() -> String {
    rfc3339_from_unix(crate::pairing::jwt::now_secs())
}

/// unix 秒 → RFC3339（UTC，秒级，`Z` 结尾）
///
/// 手写 civil-from-days（Howard Hinnant 算法）：插件侧不引入 chrono（依赖面与
/// wasm 体积），但输出必须能被宿主 `DateTime<Utc>` 反序列化。
pub fn rfc3339_from_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        mo,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// 天数（1970-01-01 起）→ (年, 月, 日)
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ==================== HTTP wire 映射（纯函数，native 可测） ====================

/// 配置 → HTTP wire 条目（与宿主 `ConfigItem` 同形：只留 6 个 wire 字段，
/// `wslDistro` 为 **显式 null**——宿主 DTO 无 skip_serializing_if，移动端 `?? 空串`
/// 依赖这一格，插件模型自身的 skip_serializing_if 形状不得泄漏到 HTTP 面）
pub fn to_http_item(config: &SessionConfig) -> serde_json::Value {
    serde_json::json!({
        "id": config.id,
        "name": config.name,
        "environment": config.environment,
        "wslDistro": config.wsl_distro,
        "workingDir": config.working_dir,
        "command": config.command,
    })
}

/// 生成新配置 id（UUID v4 形态字符串）
pub fn new_config_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| format!("entropy unavailable: {}", e))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version = 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant = RFC 4122
    Ok(format!(
        "{}-{}-{}-{}-{}",
        hex::encode(&bytes[0..4]),
        hex::encode(&bytes[4..6]),
        hex::encode(&bytes[6..8]),
        hex::encode(&bytes[8..10]),
        hex::encode(&bytes[10..16])
    ))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC3339 输出必须是宿主 `chrono::DateTime<Utc>` 能解析的形态（秒级 + Z）
    #[test]
    fn rfc3339_from_unix_matches_known_instants() {
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        // 2000-01-01T00:00:00Z（Unix 时间戳 946684800，公认值）
        assert_eq!(rfc3339_from_unix(946_684_800), "2000-01-01T00:00:00Z");
        // 闰日边界：2024-02-29T12:34:56Z（2024-02-29 00:00 = 1709164800）
        assert_eq!(rfc3339_from_unix(1_709_210_096), "2024-02-29T12:34:56Z");
    }

    /// UUID v4 形态与版本/变体位（前端当不透明串，但形态一致性是可排查性的一部分）
    #[test]
    fn new_config_id_is_uuid_v4_shaped() {
        let id = new_config_id().expect("entropy");
        assert_eq!(id.len(), 36, "got: {id}");
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(parts[2].starts_with('4'), "version 位必须为 4, got: {id}");
        assert!(
            matches!(parts[3].chars().next(), Some('8' | '9' | 'a' | 'b')),
            "variant 位必须为 RFC4122 变体, got: {id}"
        );
        assert!(id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
    }

    /// 两个 id 必须不同（熵活；恒定输出即 guest 随机源坏）
    #[test]
    fn new_config_id_is_not_constant() {
        assert_ne!(new_config_id().unwrap(), new_config_id().unwrap());
    }

    /// 空命令兜底按环境分支（业务规则从宿主下沉后的权威实现）
    #[test]
    fn default_command_follows_environment() {
        assert_eq!(
            ConfigDraft::default_command_for("windows"),
            DEFAULT_COMMAND_WINDOWS
        );
        assert_eq!(
            ConfigDraft::default_command_for("linux"),
            DEFAULT_COMMAND_POSIX
        );
        assert_eq!(
            ConfigDraft::default_command_for("wsl2"),
            DEFAULT_COMMAND_POSIX
        );
    }
}
