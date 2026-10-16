//! general — crate 内单元测试（自 bedcode-desktop/wasm-apps/agent-hub/rust/src/usage/mod.rs 迁出）

use super::*;

use crate::usage::sources::normalize_sources_paths;

/// 状态默认形状：四内置适配器槽位 + 四内置来源齐备（自定义来源待添加）
#[test]
fn default_state_shape() {
    let s = default_state();
    assert_eq!(s["status"], "idle");
    for name in ["claude", "codex", "opencode", "pi"] {
        assert!(s["adapters"][name].is_object(), "缺少适配器槽位 {name}");
    }
    let sources = s["sources"].as_array().expect("sources array");
    assert_eq!(sources.len(), 4, "三家 JSONL 目录 + opencode SQLite 库");
    assert!(sources.iter().all(|x| x["builtin"] == json!(true)));
    // opencode 是 SQLite 源：条目带 kind=sqlite，paths 是展开后的库文件
    let oc = sources
        .iter()
        .find(|x| x["name"] == json!("opencode"))
        .expect("opencode source");
    assert_eq!(oc["kind"], json!("sqlite"));
    assert!(oc["paths"][0]
        .as_str()
        .unwrap_or("")
        .ends_with("opencode.db"));
    // 三家 JSONL 源显式标 kind（前端据此区分「可添加/移除的目录」）
    let jsonl: Vec<&Value> = sources
        .iter()
        .filter(|x| x["kind"] == json!("jsonl"))
        .collect();
    assert_eq!(jsonl.len(), 3);
    // 内置源每条都带 paths 数组（首个元素为默认路径）
    assert!(sources
        .iter()
        .all(|x| x["paths"].as_array().map(|a| a.len() == 1).unwrap_or(false)));
    // 正在使用的项目会话：默认空映射（扫描收尾重算）
    assert!(s["activeSessions"]
        .as_object()
        .map(|o| o.is_empty())
        .unwrap_or(false));
}
/// 旧状态升级（票 06 的库/存储）必须补齐新槽位与新来源，且**不丢**
/// 用户已添加的自定义来源
#[test]
fn legacy_state_backfills_without_losing_custom_sources() {
    // 票 06 形状：adapters 只有 claude/pi，sources 只有两内置条目
    let legacy = json!({
        "status": "ok",
        "adapters": {
            "claude": { "files": 2, "parsed": 2, "skipped": 0, "sessions": 4, "error": null },
            "pi": { "files": 1, "parsed": 1, "skipped": 0, "sessions": 1, "error": null },
        },
        "sources": [
            { "name": "claude", "path": "/home/u/.claude/projects", "kind": "jsonl", "builtin": true },
            { "name": "pi", "path": "/home/u/.pi/agent/sessions", "kind": "jsonl", "builtin": true },
            { "name": "my-logs", "path": "/data/logs", "kind": "jsonl", "builtin": false },
        ],
    });
    // 直接验证 read_state 的补齐规则（不依赖 host：形状变换写成本地 helper）
    let mut merged = legacy["sources"].as_array().cloned().expect("sources");
    for want in json!([
        { "name": "codex", "paths": ["/home/u/.codex/sessions"], "kind": "jsonl", "builtin": true },
        { "name": "opencode", "paths": ["/home/u/.local/share/opencode/opencode.db"], "kind": "sqlite", "builtin": true },
    ])
    .as_array()
    .cloned()
    .expect("builtins")
    {
        let name = want["name"].as_str().unwrap_or("");
        if !merged.iter().any(|s| s["name"].as_str() == Some(name)) {
            merged.push(want);
        }
    }
    // 旧单 path 条目迁移为 paths 数组（read_state 的幂等归一）
    let merged = normalize_sources_paths(merged);
    let names: Vec<&str> = merged
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    // 五条：四内置 + 用户自定义；自定义来源**未丢**
    assert_eq!(names.len(), 5);
    assert!(names.contains(&"my-logs"));
    assert!(names.contains(&"codex"));
    assert!(names.contains(&"opencode"));
    // 迁移后的每条都带非空 paths 数组且无遗留 path 字段
    for s in merged.as_array().unwrap() {
        assert!(s["paths"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false));
        assert!(s.get("path").is_none());
    }
}
/// 扫描失活判定（2026-09-28 实机卡死回归锁）
///
/// 契约：`syncing` + 心跳超窗 → 失活（落 error，按钮复原可重试）；
/// 心跳在窗内 → 存活（回灌慢 ≠ 卡死）；**无心跳的 syncing → 直接判失活**
/// （旧版本落库 / 崩溃现场的形态）；非 syncing 状态永不判失活。
#[test]
fn scan_is_stale_heals_only_abandoned_runs() {
    let now = 1_000_000_000u64;

    // 正例：syncing 且心跳过期 → 失活
    let stuck = json!({ "status": "syncing", "scanStartedAt": now - SCAN_STALE_MS - 1 });
    assert!(scan_is_stale(&stuck, now));

    // 反例守门：心跳在窗内 → 存活（回灌 852 文件耗时数分钟属正常）
    let live = json!({ "status": "syncing", "scanStartedAt": now - 5_000 });
    assert!(!scan_is_stale(&live, now));

    // 边界：恰好等于窗口不算失活（严格大于才判定）
    let edge = json!({ "status": "syncing", "scanStartedAt": now - SCAN_STALE_MS });
    assert!(!scan_is_stale(&edge, now));

    // 实机形态：syncing 却无心跳（旧版本落库）→ 立即判失活
    assert!(scan_is_stale(&json!({ "status": "syncing" }), now));
    assert!(scan_is_stale(
        &json!({ "status": "syncing", "scanStartedAt": null }),
        now
    ));

    // 非在途状态永不判失活（否则每次读状态都会误伤一次成功扫描）
    for status in ["idle", "ok", "error", "auth-required"] {
        let s = json!({ "status": status });
        assert!(!scan_is_stale(&s, now), "{status} 不该被判失活");
        let s_old = json!({ "status": status, "scanStartedAt": 0 });
        assert!(
            !scan_is_stale(&s_old, now),
            "{status}（旧心跳）不该被判失活"
        );
    }

    // 时钟回拨（now < started）不得下溢成「超窗」
    let future = json!({ "status": "syncing", "scanStartedAt": now + 10_000 });
    assert!(!scan_is_stale(&future, now));
}
