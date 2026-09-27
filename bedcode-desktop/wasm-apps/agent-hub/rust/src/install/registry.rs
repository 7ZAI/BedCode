//! 测速与最新版本查询（host-http，免 shell）
//!
//! 测速：对候选源（内置 + 自定义）各做一次小 GET 计时（计时用
//! `ConfigKey::CurrentTimeMs`——wasm32-unknown-unknown 无系统时钟，
//! `Instant::now()` 会 panic），给出换源推荐；最新版本：registry HTTP API
//! `GET /<pkg>/latest`（官方源失败回落 npmmirror），与本地版本比较得 outdated。
//! 候选源白名单常量亦在此（测速列表 + apply-mirror 白名单共用）。

use super::recipe::npm_package;
use super::state::{all_sources, emit, emit_and_return, now_ms, read_state, write_state};
use super::version::compare_versions;
use crate::detect;
use bedcode_plugin_api::host::{HostHttp, HostLog, HostStorage};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::cmp::Ordering;

/// npm 官方源与 npmmirror（spec §4.2 固定两源）
pub(crate) const NPMJS: &str = "https://registry.npmjs.org";
pub(crate) const NPMMIRROR: &str = "https://registry.npmmirror.com";
/// 可切换 npm 源白名单（id, url）：测速列表 + apply-mirror 白名单共用，
/// 不拼接用户输入。列表已实测连通（2026-09-14，GET /semver/latest 200）；
/// id 供前端 i18n 映射展示名（hub.speed.source.<id>）。用户自定义源
/// （mirror.customSources）在测速/换源时追加到本表之后。
pub(crate) const MIRROR_SOURCES: [(&str, &str); 5] = [
    ("npmmirror", "https://registry.npmmirror.com"),
    ("npmjs", "https://registry.npmjs.org"),
    ("huawei", "https://mirrors.huaweicloud.com/repository/npm/"),
    ("tencent", "https://mirrors.cloud.tencent.com/npm/"),
    ("yarn", "https://registry.yarnpkg.com"),
];
/// 测速样本包：元数据小、两端长期存在，`/<pkg>/latest` 形态稳定
const PROBE_PKG: &str = "semver";
// ==================== 测速 ====================
// ==================== 测速 ====================

/// 两源小 GET 计时（一次探测一个源，host-http 非流式）
fn probe_registry(h: &WasmHost, url: &str) -> Result<u64, String> {
    let t0 = now_ms(h)?;
    let request = json!({ "method": "GET", "url": url });
    let resp = h
        .http_fetch(&request)
        .map_err(|e| format!("http failed: {e}"))?
        .ok_or_else(|| "empty response".to_string())?;
    let t1 = now_ms(h)?;
    let status = resp.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
    if status != 200 {
        return Err(format!("status {status}"));
    }
    Ok(t1.saturating_sub(t0).max(1))
}

/// 测速全部候选源并给出推荐：sources 数组（可达按耗时升序、不可达置后）+ recommend
/// （最快可达源的 id）。只测速不切换——切换由用户在列表中选择（apply-mirror 白名单）。
pub(crate) fn speed_test(h: &WasmHost) -> anyhow::Result<Value> {
    let mut state = read_state(h);
    state["mirror"]["speed"] = json!({
        "status": "testing",
        "sources": [],
        "recommend": null,
        "error": null,
        "testedAt": null,
    });
    write_state(h, &state);
    emit(h, &state);
    h.log_info("registry speed test started");

    // wasm 单线程阻塞式 http_fetch：顺序测速（候选源少，耗时叠加可接受）
    let candidates = all_sources(&state);
    let mut results: Vec<Value> = Vec::new();
    for (id, url) in &candidates {
        let ms = probe_registry(h, &format!("{url}/{PROBE_PKG}/latest")).ok();
        results.push(json!({
            "id": id,
            "url": url,
            "ms": ms,
            "reachable": ms.is_some(),
        }));
    }
    results.sort_by(compare_source);
    // 推荐：全部候选里最快可达者（排序后首位可达）
    let recommend = results
        .iter()
        .find(|r| r["reachable"] == json!(true))
        .map(|r| r["id"].clone());
    let error = if recommend.is_none() {
        Some("all registries unreachable".to_string())
    } else {
        None
    };
    let tested_at = now_ms(h).unwrap_or(0);

    state["mirror"]["speed"] = json!({
        "status": if recommend.is_some() { "ok" } else { "error" },
        // 供选择的候选：按速度排序，最多前 10（用户诉求：列出前 5/前 10 供选择）
        "sources": results.into_iter().take(10).collect::<Vec<_>>(),
        "recommend": recommend,
        "error": error,
        "testedAt": tested_at,
    });
    write_state(h, &state);
    h.log_info(&format!(
        "registry speed test done ({} candidates, recommend={recommend:?})",
        candidates.len()
    ));
    emit_and_return(h, &state)
}

/// sources 排序：可达按耗时升序在前，不可达置后
fn compare_source(a: &Value, b: &Value) -> Ordering {
    match (a["ms"].as_u64(), b["ms"].as_u64()) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
// ==================== 最新版本查询 ====================
// ==================== 最新版本查询 ====================

/// registry HTTP API 查最新版本（免 shell）：官方源失败回落 npmmirror；
/// scoped 包名按 registry 约定转义 `/` → `%2f`
fn fetch_latest_version(h: &WasmHost, pkg: &str) -> Result<String, String> {
    let scoped = pkg.replace('/', "%2f");
    for base in [NPMJS, NPMMIRROR] {
        let request = json!({ "method": "GET", "url": format!("{base}/{scoped}/latest") });
        match h.http_fetch(&request) {
            Ok(Some(resp)) => {
                let status = resp.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
                let body = resp.get("body").and_then(|v| v.as_str()).unwrap_or("");
                if status == 200 {
                    if let Ok(meta) = serde_json::from_str::<Value>(body) {
                        if let Some(v) = meta.get("version").and_then(|v| v.as_str()) {
                            return Ok(v.to_string());
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => h.log_debug(&format!("install: latest query failed on {base}: {e}")),
        }
    }
    Err(format!("latest version unavailable for {pkg}"))
}

/// 批量检查更新：四家 CLI 各查一次 latest，与探测到的本地版本比较得 outdated
pub(crate) fn check_updates(h: &WasmHost) -> anyhow::Result<Value> {
    let mut state = read_state(h);
    let detection = h.storage_get(crate::STATE_KEY).ok().flatten();
    let checked_at = now_ms(h).unwrap_or(0);

    for cli in detect::CLI_KINDS {
        let local = detection
            .as_ref()
            .and_then(|d| d["clis"][cli]["version"].as_str())
            .map(|s| s.to_string());
        let entry = match npm_package(cli) {
            Some(pkg) => match fetch_latest_version(h, pkg) {
                Ok(latest) => {
                    let outdated = local
                        .as_deref()
                        .map(|l| compare_versions(l, &latest) == Ordering::Less);
                    json!({ "latest": latest, "outdated": outdated, "checkedAt": checked_at, "error": null })
                }
                Err(e) => {
                    json!({ "latest": null, "outdated": null, "checkedAt": checked_at, "error": e })
                }
            },
            None => json!({
                "latest": null, "outdated": null, "checkedAt": checked_at,
                "error": format!("no package for {cli}"),
            }),
        };
        state["updates"][cli] = entry;
    }
    write_state(h, &state);
    h.log_info("update check finished");
    emit_and_return(h, &state)
}

// ==================== Tests（纯函数单测，双平台形态覆盖） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 候选源白名单：id/url 唯一、npmmirror 在列、URL 以 https:// 开头
    #[test]
    fn mirror_sources_whitelist_wellformed() {
        assert_eq!(MIRROR_SOURCES.len(), 5);
        let ids: Vec<&str> = MIRROR_SOURCES.iter().map(|(id, _)| *id).collect();
        let urls: Vec<&str> = MIRROR_SOURCES.iter().map(|(_, url)| *url).collect();
        let mut uniq_ids = ids.clone();
        let mut uniq_urls = urls.clone();
        uniq_ids.sort_unstable();
        uniq_urls.sort_unstable();
        uniq_ids.dedup();
        uniq_urls.dedup();
        assert_eq!(uniq_ids.len(), ids.len(), "duplicate source id");
        assert_eq!(uniq_urls.len(), urls.len(), "duplicate source url");
        assert!(ids.contains(&"npmmirror"));
        assert!(urls.contains(&"https://registry.npmmirror.com"));
        assert!(urls.iter().all(|u| u.starts_with("https://")));
    }

    /// sources 排序：可达按耗时升序在前，不可达置后
    #[test]
    fn compare_source_sorts_reachable_first() {
        let slow = json!({ "id": "a", "ms": 500, "reachable": true });
        let fast = json!({ "id": "b", "ms": 120, "reachable": true });
        let down = json!({ "id": "c", "ms": null, "reachable": false });
        let mut v = vec![slow.clone(), down.clone(), fast.clone()];
        v.sort_by(compare_source);
        assert_eq!(v[0]["id"], json!("b"));
        assert_eq!(v[1]["id"], json!("a"));
        assert_eq!(v[2]["id"], json!("c"));
    }
}
