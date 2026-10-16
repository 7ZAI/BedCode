//! 宽松语义化版本比较（纯函数）
//!
//! numeric core 逐段比较，pre-release < 正式版；pre-release 之间不细比
//! （本插件仅用于「是否落后」的粗判断）。

use std::cmp::Ordering;

// ==================== 版本比较 ====================

/// 宽松语义化版本比较（纯函数）：numeric core 逐段比较，pre-release < 正式版；
/// pre-release 之间不细比（本插件仅用于「是否落后」的粗判断）
pub(crate) fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a_core, a_pre) = split_pre(a);
    let (b_core, b_pre) = split_pre(b);
    let mut sa = a_core.split('.').map(|s| s.parse::<u64>().unwrap_or(0));
    let mut sb = b_core.split('.').map(|s| s.parse::<u64>().unwrap_or(0));
    loop {
        match (sa.next(), sb.next()) {
            (None, None) => break,
            (x, y) => {
                let xv = x.unwrap_or(0);
                let yv = y.unwrap_or(0);
                if xv != yv {
                    return xv.cmp(&yv);
                }
            }
        }
    }
    match (a_pre.is_empty(), b_pre.is_empty()) {
        (true, true) | (false, false) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
    }
}

fn split_pre(v: &str) -> (&str, &str) {
    match v.split_once('-') {
        Some((core, pre)) => (core, pre),
        None => (v, ""),
    }
}

// ==================== Tests（纯函数单测，双平台形态覆盖） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 版本比较：逐段数值比较（1.18.30 > 1.18.9）、pre-release < 正式版、相等
    #[test]
    fn compare_versions_orders() {
        use Ordering::*;
        assert_eq!(compare_versions("1.18.30", "1.18.9"), Greater);
        assert_eq!(compare_versions("2.1.263", "2.2.0"), Less);
        assert_eq!(compare_versions("0.85.1", "0.85.1"), Equal);
        assert_eq!(compare_versions("1.0.0-beta", "1.0.0"), Less);
        assert_eq!(compare_versions("1.2", "1.2.0"), Equal);
        assert_eq!(compare_versions("0.153.4", "0.154.0"), Less);
    }
}
