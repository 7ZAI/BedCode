//! cron 6 段表达式解析器（纯函数，零依赖，可单测）
//!
//! 语法：`秒 分 时 日 月 周`（空白分隔），支持：
//! - `*` 任意值；`?` 与 `*` 同义（标准 cron 仅日/周字段允许，本实现全字段接受）
//! - 数字精确匹配；`a-b` 闭区间；`*/n`、`a-b/n`、`n/m` 步进；`,` 列表
//! - 周字段：0 与 7 均为周日（7 归一为 0），1-6 为周一至周六
//! - 日（DOM）与周（DOW）同字段受限时 OR 匹配（标准 cron 语义）
//!
//! 时间基准：本地时间字符串 `YYYY-MM-DD HH:MM:SS`（字典序即时间序）。
//! WASM 无系统时钟、无时区数据：本模块做纯日历字符串运算，**不感知 DST**。
//! DST 跳变日不存在的时刻（如 02:30）由引擎自然跳过——宿主注入的 now_local
//! 序列不含该时刻，`next_after` 只沿字符串推进不回头（见 spec §5.1：不做特殊补偿）。

use std::fmt;

/// 逐日迭代上限（天）：覆盖任意合法 schedule 的最长间隔
/// （如闰日 2/29 自 2097 年需等 7 年 ≈ 2557 天），超限视为不可能匹配返回 None
const MAX_DAY_ITERATIONS: u32 = 3660;

/// cron 规格（6 字段展开后的匹配集合）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronSpec {
    sec: Field,
    min: Field,
    hour: Field,
    dom: Field,
    month: Field,
    dow: Field,
}

/// 单字段匹配集合：any = 通配（`*`/`?`）；否则 values 为升序去重的精确值
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    any: bool,
    /// 字段值域上限（any 展开与步进校验用；dow 上限为 7，7 归一为 0）
    max: u32,
    values: Vec<u32>,
}

impl Field {
    fn matches(&self, v: u32) -> bool {
        self.any || self.values.binary_search(&v).is_ok()
    }

    /// 升序遍历 >= from 的匹配值（any 展开为 from..=max）
    fn iter_from(&self, from: u32) -> impl Iterator<Item = u32> + '_ {
        let from = from.min(self.max);
        if self.any {
            (from..=self.max).collect::<Vec<_>>().into_iter()
        } else {
            self.values
                .iter()
                .copied()
                .filter(move |v| *v >= from)
                .collect::<Vec<_>>()
                .into_iter()
        }
    }

    /// 解析字段表达式；range 为值域 (min, max)，name 用于错误消息
    fn parse(s: &str, min: u32, max: u32, name: &str) -> Result<Self, String> {
        if s == "*" || s == "?" {
            return Ok(Field { any: true, max, values: Vec::new() });
        }
        let mut values = Vec::new();
        for part in s.split(',') {
            let (range_part, step) = match part.split_once('/') {
                Some((r, st)) => {
                    let step: u32 = st
                        .parse()
                        .map_err(|_| format!("invalid step '{}' in {} field", st, name))?;
                    if step == 0 {
                        return Err(format!("step must be >= 1 in {} field: '{}'", name, part));
                    }
                    (r, step)
                }
                None => (part, 1),
            };
            let (lo, hi) = match range_part {
                "*" | "?" => (min, max),
                _ => {
                    let (lo_s, hi_s) = match range_part.split_once('-') {
                        Some((a, b)) => (a, b),
                        None => (range_part, range_part),
                    };
                    let lo: u32 = lo_s
                        .trim()
                        .parse()
                        .map_err(|_| format!("invalid value '{}' in {} field", lo_s, name))?;
                    let hi: u32 = hi_s
                        .trim()
                        .parse()
                        .map_err(|_| format!("invalid value '{}' in {} field", hi_s, name))?;
                    if lo < min || hi > max || lo > hi {
                        return Err(format!(
                            "range {}-{} out of bounds [{}-{}] in {} field",
                            lo, hi, min, max, name
                        ));
                    }
                    (lo, hi)
                }
            };
            let mut v = lo;
            while v <= hi {
                values.push(v);
                v += step;
            }
        }
        values.sort_unstable();
        values.dedup();
        Ok(Field { any: false, max, values })
    }

    /// 周字段解析：7 归一为 0（周日），与 0 去重
    fn parse_dow(s: &str) -> Result<Self, String> {
        let mut f = Self::parse(s, 0, 7, "dow")?;
        for v in f.values.iter_mut() {
            if *v == 7 {
                *v = 0;
            }
        }
        f.values.sort_unstable();
        f.values.dedup();
        Ok(f)
    }
}

/// 解析 cron 6 段表达式
pub fn parse(schedule: &str) -> Result<CronSpec, String> {
    let parts: Vec<&str> = schedule.split_whitespace().collect();
    if parts.len() != 6 {
        return Err(format!(
            "expected 6 fields (sec min hour dom month dow), got {}: '{}'",
            parts.len(),
            schedule
        ));
    }
    Ok(CronSpec {
        sec: Field::parse(parts[0], 0, 59, "sec")?,
        min: Field::parse(parts[1], 0, 59, "min")?,
        hour: Field::parse(parts[2], 0, 23, "hour")?,
        dom: Field::parse(parts[3], 1, 31, "dom")?,
        month: Field::parse(parts[4], 1, 12, "month")?,
        dow: Field::parse_dow(parts[5])?,
    })
}

impl fmt::Display for CronSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn fmt_field(f: &mut fmt::Formatter<'_>, field: &Field) -> fmt::Result {
            if field.any {
                write!(f, "*")
            } else {
                write!(
                    f,
                    "{}",
                    field
                        .values
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
        }
        fmt_field(f, &self.sec)?;
        write!(f, " ")?;
        fmt_field(f, &self.min)?;
        write!(f, " ")?;
        fmt_field(f, &self.hour)?;
        write!(f, " ")?;
        fmt_field(f, &self.dom)?;
        write!(f, " ")?;
        fmt_field(f, &self.month)?;
        write!(f, " ")?;
        fmt_field(f, &self.dow)
    }
}

// ==================== next_after ====================

/// 解析本地时间字符串 `YYYY-MM-DD HH:MM:SS`（校验真实日历日）
fn parse_datetime(s: &str) -> Option<DateTime> {
    let (date, time) = s.split_once(' ')?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let m: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    if d.next().is_some() {
        return None;
    }
    let mut t = time.split(':');
    let hh: u32 = t.next()?.parse().ok()?;
    let mm: u32 = t.next()?.parse().ok()?;
    let ss: u32 = t.next()?.parse().ok()?;
    if t.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&m) || day < 1 || day > days_in_month(y, m) {
        return None;
    }
    if hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    Some(DateTime { y, m, d: day, hh, mm, ss })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DateTime {
    y: i64,
    m: u32,
    d: u32,
    hh: u32,
    mm: u32,
    ss: u32,
}

impl DateTime {
    /// 格式化回本地时间字符串
    fn to_string(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.y, self.m, self.d, self.hh, self.mm, self.ss
        )
    }

    /// 前进 1 秒（含跨分钟/小时/日回卷）
    fn add_second(&mut self) {
        self.ss += 1;
        if self.ss < 60 {
            return;
        }
        self.ss = 0;
        self.mm += 1;
        if self.mm < 60 {
            return;
        }
        self.mm = 0;
        self.hh += 1;
        if self.hh < 24 {
            return;
        }
        self.hh = 0;
        self.advance_day();
    }

    /// 前进到次日 00:00:00
    fn advance_day(&mut self) {
        self.d += 1;
        if self.d > days_in_month(self.y, self.m) {
            self.d = 1;
            self.m += 1;
            if self.m > 12 {
                self.m = 1;
                self.y += 1;
            }
        }
        self.hh = 0;
        self.mm = 0;
        self.ss = 0;
    }

    /// 当日时刻（时分秒编码，字典序即时间序）
    fn tod(&self) -> (u32, u32, u32) {
        (self.hh, self.mm, self.ss)
    }

    /// 星期（0=周日 … 6=周六），以 1970-01-01（周四）为锚
    fn weekday(&self) -> u32 {
        ((days_since_epoch(self.y, self.m, self.d) + 4).rem_euclid(7)) as u32
    }
}

fn leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap_year(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// 自 1970-01-01 起的天数（标准 civil 算法，Howard Hinnant）
fn days_since_epoch(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = y - era * 400; // [0, 399]
    let mp = (m as i64 + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// DOM 与 DOW 的 OR 语义：两字段均受限时任一匹配即触发（标准 cron）
fn day_matches(spec: &CronSpec, dt: &DateTime) -> bool {
    let dom_any = spec.dom.any;
    let dow_any = spec.dow.any;
    match (dom_any, dow_any) {
        (true, true) => true,
        (true, false) => spec.dow.matches(dt.weekday()),
        (false, true) => spec.dom.matches(dt.d),
        (false, false) => spec.dom.matches(dt.d) || spec.dow.matches(dt.weekday()),
    }
}

/// 当日内查找第一个满足字段的时刻（>= 给定时刻，见调用处语义）
fn first_match_in_day(spec: &CronSpec, dt: &DateTime) -> Option<(u32, u32, u32)> {
    let (hh, mm, ss) = dt.tod();
    for h in spec.hour.iter_from(hh) {
        let m_from = if h == hh { mm } else { 0 };
        for m in spec.min.iter_from(m_from) {
            let s_from = if h == hh && m == mm { ss } else { 0 };
            for s in spec.sec.iter_from(s_from) {
                return Some((h, m, s));
            }
        }
    }
    None
}

/// 计算 now_local 之后（严格大于）的下一次触发时刻
///
/// - now_local：本地时间字符串 `YYYY-MM-DD HH:MM:SS`
/// - 返回同格式字符串；schedule 永不匹配（如 2/30）或 now_local 非法返回 None
/// - 纯日历字符串运算：DST 跳变日不存在的时刻不出现在结果序列中
///   （从跳变后时刻计算时自然跳过；跳变前的 next_at 由引擎按宽限/补触发语义处理）
pub fn next_after(spec: &CronSpec, now_local: &str) -> Option<String> {
    let mut dt = parse_datetime(now_local)?;
    // 起点为 now 的下一秒：语义是"之后的下一次"（严格大于 now）
    dt.add_second();
    for _ in 0..MAX_DAY_ITERATIONS {
        if spec.month.matches(dt.m) && day_matches(spec, &dt) {
            if let Some((h, m, s)) = first_match_in_day(spec, &dt) {
                dt.hh = h;
                dt.mm = m;
                dt.ss = s;
                return Some(dt.to_string());
            }
        }
        dt.advance_day();
    }
    None
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn next(schedule: &str, now: &str) -> Option<String> {
        next_after(&parse(schedule).expect("parse ok"), now)
    }

    // ==================== parse ====================

    #[test]
    fn parse_accepts_six_fields_and_wildcards() {
        let spec = parse("* * * * * *").expect("all wildcards");
        assert!(spec.sec.any && spec.min.any && spec.hour.any);
        assert!(spec.dom.any && spec.month.any && spec.dow.any);
        // ? 与 * 同义（全字段接受）
        let q = parse("? ? ? ? ? ?").expect("all question marks");
        assert_eq!(spec, q);
    }

    #[test]
    fn parse_rejects_wrong_field_count() {
        assert!(parse("* * * * *").is_err());
        assert!(parse("* * * * * * *").is_err());
        assert!(parse("").is_err());
        assert!(parse("* * * * * * *").unwrap_err().contains("6 fields"));
    }

    #[test]
    fn parse_expands_numbers_ranges_lists_steps() {
        let spec = parse("0 5,10-12,*/15 * * * *").expect("parse ok");
        // sec: {0}
        assert_eq!(spec.sec.values, vec![0]);
        // min: 5 ∪ {10,11,12} ∪ {0,15,30,45}
        assert_eq!(spec.min.values, vec![0, 5, 10, 11, 12, 15, 30, 45]);
        assert!(spec.hour.any);
    }

    #[test]
    fn parse_rejects_out_of_range_and_bad_syntax() {
        assert!(parse("60 * * * * *").is_err()); // sec 越界
        assert!(parse("* 60 * * * *").is_err()); // min 越界
        assert!(parse("* * 24 * * *").is_err()); // hour 越界
        assert!(parse("* * * 0 * *").is_err()); // dom 越界（<1）
        assert!(parse("* * * 32 * *").is_err()); // dom 越界（>31）
        assert!(parse("* * * * 0 *").is_err()); // month 越界（<1）
        assert!(parse("* * * * 13 *").is_err()); // month 越界（>12）
        assert!(parse("* * * * * 8").is_err()); // dow 越界（>7）
        assert!(parse("1-0 * * * * *").is_err()); // 区间倒置
        assert!(parse("*/0 * * * * *").is_err()); // 步进为 0
        assert!(parse("1/0 * * * * *").is_err());
        assert!(parse("a * * * * *").is_err()); // 非数字
        assert!(parse("1,,2 * * * * *").is_err()); // 空列表元素
    }

    #[test]
    fn parse_dow_normalizes_seven_to_sunday() {
        let sun0 = parse("0 0 0 * * 0").unwrap();
        let sun7 = parse("0 0 0 * * 7").unwrap();
        assert_eq!(sun0.dow.values, vec![0]);
        assert_eq!(sun7.dow.values, vec![0]);
        // 0-7 覆盖整周（去重后 0..=6）
        let week = parse("0 0 0 * * 0-7").unwrap();
        assert_eq!(week.dow.values, (0..=6).collect::<Vec<_>>());
        // 5-7 → {5,6,0}
        let fri_sun = parse("0 0 0 * * 5-7").unwrap();
        assert_eq!(fri_sun.dow.values, vec![0, 5, 6]);
    }

    // ==================== 日历/星期锚点 ====================

    #[test]
    fn weekday_anchors_are_locked() {
        // 1970-01-01 周四；2024-01-01 周一；2024-02-29 周四；2024-12-31 周二
        assert_eq!(parse_datetime("1970-01-01 00:00:00").unwrap().weekday(), 4);
        assert_eq!(parse_datetime("2024-01-01 00:00:00").unwrap().weekday(), 1);
        assert_eq!(parse_datetime("2024-02-29 00:00:00").unwrap().weekday(), 4);
        assert_eq!(parse_datetime("2024-12-31 00:00:00").unwrap().weekday(), 2);
        assert_eq!(parse_datetime("2000-01-01 00:00:00").unwrap().weekday(), 6);
    }

    #[test]
    fn leap_year_rules() {
        assert!(leap_year(2024)); // %4
        assert!(!leap_year(2023));
        assert!(!leap_year(2100)); // %100 非闰
        assert!(leap_year(2000)); // %400 闰
    }

    #[test]
    fn parse_datetime_validates_real_dates() {
        assert!(parse_datetime("2024-02-29 00:00:00").is_some()); // 闰日合法
        assert!(parse_datetime("2023-02-29 00:00:00").is_none()); // 非闰年 2/29
        assert!(parse_datetime("2024-04-31 00:00:00").is_none()); // 4 月无 31 日
        assert!(parse_datetime("2024-13-01 00:00:00").is_none()); // 13 月
        assert!(parse_datetime("2024-01-01 24:00:00").is_none()); // 24 时
        assert!(parse_datetime("not a time").is_none());
        assert!(parse_datetime("2024-01-01").is_none()); // 缺时间
    }

    // ==================== next_after 基本语义 ====================

    #[test]
    fn next_after_is_strictly_after_now() {
        // 每秒触发：now 的下一个整秒
        assert_eq!(next("* * * * * *", "2024-01-01 00:00:00").unwrap(), "2024-01-01 00:00:01");
        // 整分触发（:00 秒）：now 恰为 :00 时指向下一分钟
        assert_eq!(next("0 * * * * *", "2024-01-01 09:30:00").unwrap(), "2024-01-01 09:31:00");
        assert_eq!(next("0 * * * * *", "2024-01-01 09:30:15").unwrap(), "2024-01-01 09:31:00");
        // 定点时分秒：now 过后指向次日
        assert_eq!(
            next("30 9 8 * * *", "2024-01-01 08:09:30").unwrap(),
            "2024-01-02 08:09:30"
        );
        // 秒字段步进
        assert_eq!(next("*/15 * * * * *", "2024-01-01 00:00:10").unwrap(), "2024-01-01 00:00:15");
        assert_eq!(next("*/15 * * * * *", "2024-01-01 00:00:15").unwrap(), "2024-01-01 00:00:30");
    }

    #[test]
    fn next_after_hourly_and_daily() {
        // 每小时 :00
        assert_eq!(next("0 0 * * * *", "2024-01-01 09:30:00").unwrap(), "2024-01-01 10:00:00");
        // 每天 09:00
        assert_eq!(next("0 0 9 * * *", "2024-01-01 09:00:00").unwrap(), "2024-01-02 09:00:00");
        assert_eq!(next("0 0 9 * * *", "2024-01-01 08:59:59").unwrap(), "2024-01-01 09:00:00");
        // 跨天
        assert_eq!(next("0 0 23 * * *", "2024-01-01 23:30:00").unwrap(), "2024-01-02 23:00:00");
    }

    #[test]
    fn next_after_monthly_31st_skips_short_months() {
        // 每月 31 日 00:00：1/31 触发后跳过 2 月（无 31 日）到 3/31
        assert_eq!(
            next("0 0 0 31 * *", "2024-01-31 00:00:00").unwrap(),
            "2024-03-31 00:00:00"
        );
        // 12/31 后跨年到次年 1/31
        assert_eq!(
            next("0 0 0 31 * *", "2024-12-31 00:00:00").unwrap(),
            "2025-01-31 00:00:00"
        );
    }

    #[test]
    fn next_after_leap_day() {
        // 2024-02-29 之后：下一个 2/29 是 2028
        assert_eq!(
            next("0 0 0 29 2 *", "2024-02-29 00:00:00").unwrap(),
            "2028-02-29 00:00:00"
        );
        // 非闰年 2 月无 29 日：从 2024-03-01 起，下一个 2/29 是 2028
        assert_eq!(
            next("0 0 0 29 2 *", "2024-03-01 00:00:00").unwrap(),
            "2028-02-29 00:00:00"
        );
        // 世纪非闰年：2096-02-29 之后的下一个是 2104（2100 非闰）
        assert_eq!(
            next("0 0 0 29 2 *", "2096-02-29 00:00:00").unwrap(),
            "2104-02-29 00:00:00"
        );
    }

    #[test]
    fn next_after_year_boundary() {
        assert_eq!(
            next("0 0 0 1 1 *", "2024-12-31 23:59:59").unwrap(),
            "2025-01-01 00:00:00"
        );
        assert_eq!(
            next("0 30 9 * 6 *", "2025-12-31 10:00:00").unwrap(),
            "2026-06-01 09:30:00"
        );
    }

    #[test]
    fn next_after_dom_dow_or_semantics() {
        // dom=13 OR dow=周五：2024-09-01 之后第一个匹配是 9/6（周五，OR 命中 dow）
        assert_eq!(
            next("0 0 0 13 * 5", "2024-09-01 00:00:00").unwrap(),
            "2024-09-06 00:00:00"
        );
        // 9/6 之后：下一个匹配 9/13（13 日恰为周五，两字段同时命中）
        assert_eq!(
            next("0 0 0 13 * 5", "2024-09-07 00:00:00").unwrap(),
            "2024-09-13 00:00:00"
        );
        // 仅周字段（dom=*）：2024-09 的第一个周五是 9/6
        assert_eq!(
            next("0 0 0 * * 5", "2024-09-01 00:00:00").unwrap(),
            "2024-09-06 00:00:00"
        );
        // 仅日字段（dow=*）：每月 13 日
        assert_eq!(
            next("0 0 0 13 * *", "2024-09-01 00:00:00").unwrap(),
            "2024-09-13 00:00:00"
        );
    }

    #[test]
    fn next_after_weekday_specific() {
        // 每周一 09:00：2024-01-01 是周一
        assert_eq!(next("0 0 9 * * 1", "2024-01-01 00:00:00").unwrap(), "2024-01-01 09:00:00");
        // 周一 09:00 之后：下周一
        assert_eq!(next("0 0 9 * * 1", "2024-01-01 09:00:00").unwrap(), "2024-01-08 09:00:00");
        // 周日用 7 表达与 0 等价
        assert_eq!(next("0 0 9 * * 7", "2024-01-05 00:00:00").unwrap(), "2024-01-07 09:00:00");
        assert_eq!(next("0 0 9 * * 0", "2024-01-05 00:00:00").unwrap(), "2024-01-07 09:00:00");
    }

    #[test]
    fn next_after_month_restricted() {
        // 每年 6 月每天 09:00：2024-01 之后 → 2024-06-01
        assert_eq!(next("0 0 9 * 6 *", "2024-01-15 00:00:00").unwrap(), "2024-06-01 09:00:00");
        // 6 月内：逐日触发
        assert_eq!(next("0 0 9 * 6 *", "2024-06-15 00:00:00").unwrap(), "2024-06-15 09:00:00");
        // 7 月之后 → 2025-06-01
        assert_eq!(next("0 0 9 * 6 *", "2024-07-01 00:00:00").unwrap(), "2025-06-01 09:00:00");
    }

    #[test]
    fn next_after_impossible_schedule_returns_none() {
        // 2 月无 30 日：永不匹配
        assert_eq!(next("0 0 0 30 2 *", "2024-01-01 00:00:00"), None);
        // 无效 now_local
        assert_eq!(next("* * * * * *", "not-a-time"), None);
        assert_eq!(next("* * * * * *", "2023-02-29 00:00:00"), None);
    }

    #[test]
    fn next_after_dst_gap_is_skipped_by_string_sequence() {
        // DST 跳变（2024-03-31 02:00 → 03:00，美制）后的时刻起算：
        // 每天 02:30 的下一次是次日 02:30 —— 当日 gap 内的 02:30 不回头
        assert_eq!(
            next("0 30 2 * * *", "2024-03-31 03:00:00").unwrap(),
            "2024-04-01 02:30:00"
        );
        // 跳变前起算（01:59:59）：纯字符串语义得到 02:30（该时刻在真实时钟上
        // 不存在，引擎层由宿主 now_local 跳变自然跳过——见模块文档 DST 说明）
        assert_eq!(
            next("0 30 2 * * *", "2024-03-31 01:59:59").unwrap(),
            "2024-03-31 02:30:00"
        );
        // 1s 粒度的秒级调度跨 gap：不产生 gap 内时刻
        assert_eq!(
            next("* * * * * *", "2024-03-31 03:00:00").unwrap(),
            "2024-03-31 03:00:01"
        );
    }

    #[test]
    fn next_after_format_is_lexicographically_comparable() {
        // 输出格式统一（补零），字典序即时间序 —— 引擎直接字符串比较
        let a = next("0 5 9 * * *", "2024-01-01 00:00:00").unwrap();
        let b = next("0 5 9 * * *", "2024-01-02 00:00:00").unwrap();
        assert_eq!(a, "2024-01-01 09:05:00");
        assert!(a < b);
        assert_eq!(a.len(), 19);
    }
}
