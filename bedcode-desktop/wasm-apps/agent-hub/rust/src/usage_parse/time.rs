//! 时间戳解析（ISO8601 / RFC3339 → epoch 毫秒）
//!
//! 儒略日换算用 Hinnant 算法（civil_from_days 逆运算），手工实现以免引入
//! chrono/time 依赖（插件 crate 依赖保持最小）。裸时间戳（无时区）按
//! 本地时未知语义拒绝——避免误当 UTC 造成统计偏移。

// ==================== 时间戳（ISO8601 → epoch ms） ====================

/// 解析 ISO8601 / RFC3339 时间戳为 epoch ms
///
/// 支持形态：`2026-09-04T01:17:52Z`、`2026-09-04T01:17:52.455Z`、
/// 带时区偏移 `2026-09-04T01:17:52+08:00`；日期时间以 `T`/空格分隔。
/// 儒略日换算用 Hinnant 算法（civil_from_days 逆运算），手工实现以
/// 免引入 chrono/time 依赖（插件 crate 依赖保持最小）。
pub(crate) fn parse_iso8601_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    let bytes = s.as_bytes();
    // 形如 2026-09-04T01:17:52(.fff)?(Z|±HH:MM)?，最短 16 字符（无秒时区）
    if bytes.len() < 16 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let sep = bytes[10];
    if sep != b'T' && sep != b't' && sep != b' ' {
        return None;
    }
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    if bytes[13] != b':' {
        return None;
    }
    let minute: i64 = s.get(14..16)?.parse().ok()?;
    let (second, rest) = if bytes.len() > 16 && bytes[16] == b':' {
        (s.get(17..19)?.parse::<i64>().ok()?, &s[19..])
    } else {
        (0, &s[16..])
    };

    // 时区：必须显式声明（Z 或 ±HH:MM）——裸时间戳按本地时未知语义拒绝，
    // 避免误当 UTC 造成统计偏移
    let mut offset_minutes: i64 = 0;
    let mut frac_ms: i64 = 0;
    let mut chars = rest.chars();
    match chars.next() {
        Some('Z') | Some('z') => {}
        None => return None,
        Some('.') => {
            let frac: String = chars.by_ref().take_while(|c| c.is_ascii_digit()).collect();
            let digits = frac.len();
            if digits == 0 {
                return None;
            }
            // 毫秒截断（更多小数位四舍五入到 ms 粒度内截断即可）
            let mut ms: i64 = frac[..digits.min(3)].parse().ok()?;
            for _ in 0..3usize.saturating_sub(digits) {
                ms *= 10;
            }
            frac_ms = ms;
            let tail = chars.as_str();
            offset_minutes = match tail {
                "" | "Z" | "z" => 0,
                _ => parse_offset_minutes(tail)?,
            };
        }
        Some(_) => {
            offset_minutes = parse_offset_minutes(rest)?;
        }
    }

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3600 + minute * 60 + second - offset_minutes * 60;
    Some(secs * 1000 + frac_ms)
}

/// 解析 `Z` 之外的时区偏移（`+08:00` / `-0530`）；空串（无偏移）拒绝
fn parse_offset_minutes(tail: &str) -> Option<i64> {
    let t = tail.trim();
    if t.is_empty() {
        return None;
    }
    let sign = match t.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = t[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 2 || digits.len() > 4 {
        return None;
    }
    let hours: i64 = digits[..2].parse().ok()?;
    let minutes: i64 = if digits.len() >= 4 {
        digits[2..4].parse().ok()?
    } else {
        0
    };
    Some(sign * (hours * 60 + minutes))
}

/// Hinnant days_from_civil：civil 日期 → 自 1970-01-01 的天数
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]：3 月 = 0
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_z_with_fraction() {
        // 常数与权威实现互证（Python datetime）：2026-09-04T00:00:00Z =
        // 1_788_480_000_000；实机 cost-state startTime 1788484654355 ≈
        // 01:17:34.355Z 同日锚定
        assert_eq!(
            parse_iso8601_ms("2026-09-04T01:17:52.455Z"),
            Some(1_788_484_672_455)
        );
    }

    #[test]
    fn iso8601_z_without_fraction() {
        assert_eq!(
            parse_iso8601_ms("2026-09-04T01:17:52Z"),
            Some(1_788_484_672_000)
        );
    }

    #[test]
    fn iso8601_offset_timezone() {
        // +08:00 → UTC 减 8 小时
        assert_eq!(
            parse_iso8601_ms("2026-09-04T09:17:52+08:00"),
            parse_iso8601_ms("2026-09-04T01:17:52Z")
        );
        assert_eq!(
            parse_iso8601_ms("2026-09-04T01:17:52-02:00"),
            Some(1_788_484_672_000 + 2 * 3600 * 1000)
        );
    }

    #[test]
    fn iso8601_space_separator() {
        assert_eq!(
            parse_iso8601_ms("2026-09-04 01:17:52Z"),
            Some(1_788_484_672_000)
        );
    }

    #[test]
    fn iso8601_invalid_inputs() {
        assert_eq!(parse_iso8601_ms(""), None);
        assert_eq!(parse_iso8601_ms("not-a-date"), None);
        assert_eq!(parse_iso8601_ms("2026-13-04T01:17:52Z"), None);
        assert_eq!(parse_iso8601_ms("2026-09-32T01:17:52Z"), None);
        assert_eq!(parse_iso8601_ms("2026-09-04T01:17:52"), None); // 无时区 → 拒绝
        assert_eq!(parse_iso8601_ms("2026-09-04X01:17:52Z"), None);
    }

    #[test]
    fn iso8601_epoch_known_value() {
        // 2026-01-01T00:00:00Z = 1767225600
        assert_eq!(
            parse_iso8601_ms("2026-01-01T00:00:00Z"),
            Some(1_767_225_600_000)
        );
    }
}
