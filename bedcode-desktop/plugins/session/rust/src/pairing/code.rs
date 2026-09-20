//! 配对码策略（终端会话中心 pairing 域 · 票 04）
//!
//! 语义从宿主 `src-tauri/src/utils/auth/pairing.rs` 平移（行为等价约束）：
//!
//! - 6 位数字配对码，默认 TTL 60s（与宿主 `PAIRING_CODE_DIGITS=6` /
//!   `PAIRING_CODE_TTL_SECS=60` 对齐）
//! - 过期判定：`elapsed > ttl`（**严格大于**才过期，与 QR token 的 `>=` 语义不同）
//! - `verify(input) = !is_expired && code == input`（先判过期再比码）
//! - 序列化：`code` / `created_at`（RFC3339）/ `expires_in` = **剩余时间**
//!   （宿主 `Serialize` 行为：序列化时用剩余秒而非原始 TTL）
//! - 反序列化产物丢失内存态时钟，只能走 created_at 快照判定（宿主
//!   `created_instant: None` fallback 语义）
//!
//! 时间表示差异（留档）：宿主快路径用 `std::time::Instant`（亚秒精度），
//! fallback 用 `chrono::DateTime<Utc>` 秒差；插件统一 unix 秒快照 + 注入
//! 时间戳（`*_at(now)`），对照测试注入同一 now 断言同一决策。生成分布
//! （getrandom 拒绝采样无偏 digit）不在「同一输入同输出」对照范围（对照
//! 聚焦 verify 决策与剩余时间计算）。

use serde::{Deserialize, Serialize, Serializer};

/// 配对码位数 — 与宿主 `system::constants::auth::PAIRING_CODE_DIGITS` 对齐
pub const PAIRING_CODE_DIGITS: usize = 6;

/// 配对码有效期（秒）— 与宿主 `PAIRING_CODE_TTL_SECS` 对齐
pub const PAIRING_CODE_TTL_SECS: u64 = 60;

/// 配对码（6 位数字，单次有效）
#[derive(Debug, Clone)]
pub struct PairingCode {
    /// 配对码
    pub code: String,
    /// 创建时间（unix 秒）
    pub created_at_secs: u64,
    /// 原始 TTL（秒）
    pub expires_in: u64,
}

impl PairingCode {
    /// 生成新的 6 位数字配对码（默认 TTL）— 语义同宿主 `generate`。
    /// 命令面走 `generate_with_ttl_at`（注入时间戳）；本方法保留为语义完整
    /// API（对照测试 + 后续票命令面扩展），wasm 产物路径未调用
    #[allow(dead_code)]
    pub fn generate() -> Self {
        Self::generate_with_ttl_at(PAIRING_CODE_TTL_SECS, crate::pairing::jwt::now_secs())
    }

    /// 生成新的 6 位数字配对码，指定 TTL（注入时间戳，测试用）
    pub fn generate_with_ttl_at(ttl_secs: u64, now_secs: u64) -> Self {
        Self {
            code: random_digits(PAIRING_CODE_DIGITS),
            created_at_secs: now_secs,
            expires_in: ttl_secs,
        }
    }

    /// 检查是否过期：`elapsed > ttl`（严格大于；宿主 `is_expired` 快路径
    /// `instant.elapsed() > expires_in` 语义，`elapsed == ttl` 未过期）
    pub fn is_expired_at(&self, now_secs: u64) -> bool {
        now_secs.saturating_sub(self.created_at_secs) > self.expires_in
    }

    /// 获取剩余有效时间（秒）：过期钳制 0；`elapsed >= ttl` → 0
    /// （宿主快路径 `elapsed >= ttl → 0` 语义）
    pub fn remaining_secs_at(&self, now_secs: u64) -> u64 {
        let elapsed = now_secs.saturating_sub(self.created_at_secs);
        if elapsed >= self.expires_in {
            0
        } else {
            self.expires_in - elapsed
        }
    }

    /// 验证配对码：`!is_expired && code == input` — 语义同宿主 `verify`
    pub fn verify_at(&self, input: &str, now_secs: u64) -> bool {
        !self.is_expired_at(now_secs) && self.code == input
    }
}

/// 序列化 PairingCode：`expires_in` 使用剩余时间（宿主 `Serialize` 行为；
/// `created_at` 为 RFC3339 UTC 秒级字符串）
impl Serialize for PairingCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("PairingCode", 3)?;
        state.serialize_field("code", &self.code)?;
        state.serialize_field("created_at", &format_rfc3339_utc(self.created_at_secs))?;
        state.serialize_field(
            "expires_in",
            &self.remaining_secs_at(crate::pairing::jwt::now_secs()),
        )?;
        state.end()
    }
}

/// 反序列化 PairingCode（`created_at` 支持 RFC3339 字符串）— 产物只有
/// created_at 快照，无内存态时钟（宿主 `created_instant: None` 语义）
impl<'de> Deserialize<'de> for PairingCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct PairingCodeData {
            code: String,
            created_at: String,
            expires_in: u64,
        }
        let data = PairingCodeData::deserialize(deserializer)?;
        let created_at_secs = parse_rfc3339_utc(&data.created_at)
            .ok_or_else(|| serde::de::Error::custom("invalid RFC3339 created_at"))?;
        Ok(PairingCode {
            code: data.code,
            created_at_secs,
            expires_in: data.expires_in,
        })
    }
}

/// 生成 `digits` 位随机数字配对码（拒绝采样保证无偏 digit：字节 < 250 时
/// `byte % 10`，256 = 10×25 + 6 的余数偏差被拒绝；宿主 rand gen_range 无偏，
/// 生成分布非对照范围，此处对齐无偏性）
fn random_digits(digits: usize) -> String {
    let mut out = String::with_capacity(digits);
    while out.len() < digits {
        let mut byte = [0u8; 1];
        getrandom::fill(&mut byte).expect("getrandom fill for pairing code digit");
        if byte[0] < 250 {
            out.push(char::from(b'0' + (byte[0] % 10)));
        }
    }
    out
}

/// unix 秒 → RFC3339 UTC（秒级，无小数）— 与宿主 chrono `DateTime<Utc>` 的
/// serde 输出同为 RFC3339；差异：宿主含小数秒（纳秒精度），插件秒级
/// （序列化 shape 对齐，值精度差留档）
pub(crate) fn format_rfc3339_utc(unix_secs: u64) -> String {
    let days = unix_secs / 86_400;
    let secs_of_day = unix_secs % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    let (h, m, s) = (
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60,
    );
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// RFC3339 UTC（秒级）→ unix 秒；解析失败返回 None
pub(crate) fn parse_rfc3339_utc(s: &str) -> Option<u64> {
    // 期望形态 "YYYY-MM-DDTHH:MM:SSZ"（容忍尾部 Z/时区偏移为 UTC 零偏移）
    let (date_part, time_part) = s.split_once('T')?;
    let time_part = time_part.strip_suffix('Z').unwrap_or(time_part);
    let mut date_it = date_part.split('-');
    let year: i64 = date_it.next()?.parse().ok()?;
    let month: i64 = date_it.next()?.parse().ok()?;
    let day: i64 = date_it.next()?.parse().ok()?;
    let mut time_it = time_part.split(':');
    let hour: i64 = time_it.next()?.parse().ok()?;
    let min: i64 = time_it.next()?.parse().ok()?;
    let sec: i64 = time_it.next()?.parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    Some((days as u64) * 86_400 + (hour as u64) * 3600 + (min as u64) * 60 + sec as u64)
}

/// Howard Hinnant `days_from_civil` 算法（无符号运算安全版）— 公历日序号 → 日期
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Howard Hinnant `days_from_civil` 逆算法 — 日期 → 公历日序号
fn days_from_civil(y: i64, m: i64, d: i64) -> Option<i64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    Some(era * 146_097 + doe - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 注入时间构造（对齐宿主测试 `fallback_code` 辅助的注入方式）
    fn code_with_created_at(created_at_secs: u64, ttl: u64) -> PairingCode {
        PairingCode {
            code: "123456".to_string(),
            created_at_secs: created_at_secs,
            expires_in: ttl,
        }
    }

    #[test]
    fn generate_creates_six_digit_code() {
        let code = PairingCode::generate();
        assert_eq!(code.code.len(), PAIRING_CODE_DIGITS);
        assert!(code.code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn fresh_code_not_expired() {
        let now = crate::pairing::jwt::now_secs();
        let code = PairingCode::generate();
        assert!(!code.is_expired_at(now));
        assert!(code.remaining_secs_at(now) > 0);
        assert!(code.remaining_secs_at(now) <= PAIRING_CODE_TTL_SECS);
    }

    /// TTL 边界：`elapsed == ttl` 未过期（宿主 `>` 语义；与 QR token 的 `>=` 不同）
    #[test]
    fn ttl_boundary_elapsed_equals_ttl_not_expired() {
        let code = code_with_created_at(1000, 60);
        assert!(
            !code.is_expired_at(1060),
            "elapsed == ttl 未过期（严格大于才过期）"
        );
        assert_eq!(code.remaining_secs_at(1060), 0);
    }

    #[test]
    fn expired_when_elapsed_exceeds_ttl() {
        let code = code_with_created_at(1000, 60);
        assert!(code.is_expired_at(1061), "elapsed > ttl 过期");
        assert_eq!(code.remaining_secs_at(1061), 0);
    }

    #[test]
    fn verify_accepts_correct_code() {
        let now = crate::pairing::jwt::now_secs();
        let code = PairingCode::generate();
        assert!(code.verify_at(&code.code, now));
    }

    #[test]
    fn verify_rejects_wrong_code() {
        let now = crate::pairing::jwt::now_secs();
        let code = PairingCode::generate();
        let wrong = if code.code == "000000" {
            "111111"
        } else {
            "000000"
        };
        assert!(!code.verify_at(wrong, now));
    }

    #[test]
    fn verify_rejects_expired_code() {
        // 码正确但已过期 → 校验必须失败（先判过期）
        let code = code_with_created_at(1000, 60);
        assert!(!code.verify_at("123456", 1100));
    }

    /// remaining：注入 elapsed 10s → 60 - 10 = 50（宿主 `as_secs` 截断语义）
    #[test]
    fn remaining_secs_computes_residual() {
        let code = code_with_created_at(1000, 60);
        assert_eq!(code.remaining_secs_at(1010), 50);
        assert_eq!(code.remaining_secs_at(1059), 1);
        assert_eq!(code.remaining_secs_at(1060), 0);
        assert_eq!(
            code.remaining_secs_at(2000),
            0,
            "过期后剩余必须钳制 0，不能下溢"
        );
    }

    /// 双轨对照（票 04）：与宿主 `utils/auth/pairing.rs::
    /// test_fallback_ttl_decision_table_matches_plugin` 同一张决策表——宿主
    /// fallback 路径（chrono 秒差）与本模块的 unix 秒公式必须逐行同判
    #[test]
    fn fallback_ttl_decision_table_matches_host() {
        // (elapsed 秒, 是否过期, 剩余秒)
        let cases: [(u64, bool, u64); 4] = [
            (10, false, 50),
            (59, false, 1),
            (60, false, 0),
            (120, true, 0),
        ];
        for (elapsed, expired, remaining) in cases {
            let code = code_with_created_at(1000, PAIRING_CODE_TTL_SECS);
            let now = 1000 + elapsed;
            assert_eq!(
                code.is_expired_at(now),
                expired,
                "elapsed={elapsed}s 过期判定"
            );
            assert_eq!(
                code.remaining_secs_at(now),
                remaining,
                "elapsed={elapsed}s 剩余秒"
            );
            assert_eq!(
                code.verify_at("123456", now),
                !expired,
                "elapsed={elapsed}s 验证决策"
            );
        }
    }

    /// 序列化：expires_in = 剩余时间而非原始 TTL（宿主 Serialize 行为）
    #[test]
    fn serialize_uses_remaining_time_not_raw_ttl() {
        // created_at 回拨：now = created + 10 → 剩余 50
        let now = crate::pairing::jwt::now_secs();
        let code = PairingCode {
            code: "123456".to_string(),
            created_at_secs: now - 10,
            expires_in: 60,
        };
        let json = serde_json::to_value(&code).unwrap();
        assert_eq!(json["code"], "123456");
        assert_eq!(json["expires_in"], 50);
    }

    /// 反序列化往返：created_at 还原为快照秒，决策语义不变
    #[test]
    fn deserialize_roundtrip_keeps_created_at_snapshot() {
        let now = crate::pairing::jwt::now_secs();
        let code = PairingCode {
            code: "123456".to_string(),
            created_at_secs: now - 5,
            expires_in: 60,
        };
        let json = serde_json::to_string(&code).unwrap();
        let decoded: PairingCode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.code, "123456");
        assert_eq!(decoded.created_at_secs, now - 5);
        // 序列化写的是剩余时间（now-5 → 55），反序列化原样取回（宿主同语义）
        assert_eq!(decoded.expires_in, 55);
        // 反序列化产物只能走 created_at 快照（无内存态时钟）
        assert!(decoded.verify_at("123456", now));
        assert!(!decoded.verify_at("123456", now + 56), "过期后验证失败");
    }

    /// RFC3339 往返：1700000000 = 2023-11-14T22:13:20Z
    #[test]
    fn rfc3339_roundtrip() {
        assert_eq!(format_rfc3339_utc(1700000000), "2023-11-14T22:13:20Z");
        assert_eq!(parse_rfc3339_utc("2023-11-14T22:13:20Z"), Some(1700000000));
        // 闰年边界：2024-02-29（2024 闰年）→ 1709164800
        assert_eq!(format_rfc3339_utc(1709164800), "2024-02-29T00:00:00Z");
        assert_eq!(parse_rfc3339_utc("2024-02-29T00:00:00Z"), Some(1709164800));
        assert_eq!(parse_rfc3339_utc("garbage"), None);
        assert_eq!(
            parse_rfc3339_utc("2023-13-01T00:00:00Z"),
            None,
            "非法月份拒绝"
        );
    }
}
