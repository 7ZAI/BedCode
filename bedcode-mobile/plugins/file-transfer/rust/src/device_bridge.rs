//! 设备发现桥接层（issue 13 Phase 3 步骤 1）
//!
//! 职责切分：设备缓存状态机（去重/TTL/展示名/能力位）在前端 deviceState.ts
//! 纯函数自持（wasm32-unknown-unknown 无时钟，TTL 判定需 Date.now）；本模块
//! 只做四件宿主侧的事：
//!
//! 1. mDNS browse 生命周期：activate 即 browse、deactivate 停止（宿主 purge 兜底）；
//! 2. `mdns:found` / `mdns:lost` 原样透传给前端（wire 形状不过翻译）；
//! 3. 设备快照持久化：前端 debounce 写入 storage 键 `device_snapshot`（含
//!    last-seen），activate 时供前端载入首屏（「最近可见」标注）；
//! 4. endpoint memo + session 句柄映射：数据面命令按 nodeId 记忆
//!    `{nodeId, addr, port}` 与 `sess-<uuid>`，支撑重试回放与 close 寻址。

use bedcode_plugin_api_mobile::host::{HostLog, HostMdns, HostStorage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

/// 对等网络服务类型（与引擎广播一致；SDK 内联常量——插件 crate 不依赖引擎包）
pub(crate) const SERVICE_TYPE: &str = "_bedcode-peer._tcp.local.";

/// 设备快照 storage 键（双端一致）
pub(crate) const DEVICE_SNAPSHOT_KEY: &str = "device_snapshot";

// ==================== 进程级状态 ====================

/// 活跃 browser 句柄（activate browse / deactivate stop）
static BROWSER_ID: OnceLock<Mutex<String>> = OnceLock::new();
/// nodeId → session 句柄（dial-peer-endpoint 铸造；connection 断开时摘除）
static SESSIONS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
/// nodeId → endpoint memo（数据面命令记忆；重试回放寻址源）
static ENDPOINTS: OnceLock<Mutex<HashMap<String, DialEndpoint>>> = OnceLock::new();

fn browser_id() -> &'static Mutex<String> {
    BROWSER_ID.get_or_init(|| Mutex::new(String::new()))
}

fn sessions() -> &'static Mutex<HashMap<String, String>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn endpoints() -> &'static Mutex<HashMap<String, DialEndpoint>> {
    ENDPOINTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 拨号 endpoint（camelCase wire 形状）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DialEndpoint {
    pub node_id: String,
    pub addr: String,
    pub port: u16,
}

// ==================== browse 生命周期 ====================

/// activate 即 browse：启动失败不阻断插件激活（节点未启动等场景如实记日志，
/// 设备列表留空；用户重启节点后 refresh 重试）
pub(crate) fn start_browse(h: &(impl HostMdns + HostLog)) {
    match h.mdns_browse(SERVICE_TYPE) {
        Ok(id) => {
            *browser_id().lock().expect("browser id lock") = id;
            h.log_info("mdns browse started (file-transfer)");
        }
        Err(e) => h.log_info(&format!("mdns browse failed (non-fatal): {e}")),
    }
}

/// deactivate 停止浏览并清空会话句柄映射（endpoint memo 保留——跨激活重连复用）
pub(crate) fn stop_browse(h: &impl HostMdns) {
    let id = browser_id().lock().expect("browser id lock").clone();
    if !id.is_empty() {
        let _ = h.mdns_stop_browse(&id);
        *browser_id().lock().expect("browser id lock") = String::new();
    }
    sessions().lock().expect("sessions lock").clear();
}

// ==================== session 句柄映射 ====================

/// 登记已连接节点的 session 句柄与 endpoint memo
pub(crate) fn remember_session(endpoint: &DialEndpoint, handle: String) {
    endpoints().lock().expect("endpoints lock").insert(endpoint.node_id.clone(), endpoint.clone());
    sessions().lock().expect("sessions lock").insert(endpoint.node_id.clone(), handle);
}

/// 连接断开摘除句柄（endpoint memo 保留供自动重连）
pub(crate) fn forget_session(node_id: &str) {
    sessions().lock().expect("sessions lock").remove(node_id);
}

/// 解析目标：显式 endpoint 参数优先；否则查 memo。返回 None = 双双缺失。
pub(crate) fn resolve_endpoint(explicit: Option<DialEndpoint>, node_id: &str) -> Option<DialEndpoint> {
    if let Some(ep) = explicit {
        // 显式入参同时刷新 memo（前端设备缓存是最新真源）
        endpoints().lock().expect("endpoints lock").insert(node_id.to_string(), ep.clone());
        return Some(ep);
    }
    endpoints().lock().expect("endpoints lock").get(node_id).cloned()
}

/// 取节点的活跃 session 句柄（未连接返回 None）
pub(crate) fn session_of(node_id: &str) -> Option<String> {
    sessions().lock().expect("sessions lock").get(node_id).cloned()
}

// ==================== 快照持久化 ====================

/// 单条设备快照（前端缓存条目的落盘形状；lastSeenMs 由前端 Date.now 盖章）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceSnapshotEntry {
    pub node_id: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub addr: String,
    #[serde(default)]
    pub port: u16,
    /// 能力位图小写 hex（bit0 = 文件传输；空 = 无能力位）
    #[serde(default)]
    pub capabilities_hex: String,
    #[serde(default)]
    pub instance_name: String,
    #[serde(default)]
    pub last_seen_ms: u64,
}

/// 读取快照（无快照返回空数组）
pub(crate) fn load_snapshot(h: &impl HostStorage) -> anyhow::Result<Vec<DeviceSnapshotEntry>> {
    let Some(v) = h.storage_get(DEVICE_SNAPSHOT_KEY)? else {
        return Ok(vec![]);
    };
    Ok(serde_json::from_value(v).unwrap_or_default())
}

/// 保存快照（前端 debounce 调用；≤50 条由前端裁剪）
pub(crate) fn save_snapshot(
    h: &impl HostStorage,
    entries: &[DeviceSnapshotEntry],
) -> anyhow::Result<()> {
    h.storage_set(
        DEVICE_SNAPSHOT_KEY,
        &serde_json::to_value(entries)
            .map_err(|e| anyhow::anyhow!("device snapshot serialize failed: {e}"))?,
    )?;
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_roundtrips_camel_case() {
        let ep = DialEndpoint { node_id: "aa".into(), addr: "192.168.1.5".into(), port: 47821 };
        let v = serde_json::to_value(&ep).unwrap();
        assert_eq!(v["nodeId"], "aa");
        assert_eq!(v["addr"], "192.168.1.5");
        assert_eq!(v["port"], 47821);
        let back: DialEndpoint = serde_json::from_value(v).unwrap();
        assert_eq!(back, ep);
    }

    #[test]
    fn snapshot_entry_defaults_tolerate_partial_payload() {
        let e: DeviceSnapshotEntry = serde_json::from_value(serde_json::json!({
            "nodeId": "abc", "lastSeenMs": 1234
        }))
        .unwrap();
        assert_eq!(e.node_id, "abc");
        assert_eq!(e.last_seen_ms, 1234);
        assert_eq!(e.port, 0);
        assert_eq!(e.capabilities_hex, "");
    }

    #[test]
    fn resolve_endpoint_prefers_explicit_and_updates_memo() {
        // 静态态隔离：本测试独占进程内静态（cargo test 单线程执行同模块用例序）
        let node = "memo-test-node";
        forget_session(node);
        let memo_ep = DialEndpoint { node_id: node.into(), addr: "10.0.0.1".into(), port: 1 };
        remember_session(&memo_ep, "sess-x".into());
        assert_eq!(session_of(node).as_deref(), Some("sess-x"));
        // 显式优先且刷新 memo
        let fresh = DialEndpoint { node_id: node.into(), addr: "10.0.0.2".into(), port: 2 };
        assert_eq!(resolve_endpoint(Some(fresh.clone()), node), Some(fresh));
        // 无显式走 memo
        assert_eq!(
            resolve_endpoint(None, node),
            Some(DialEndpoint { node_id: node.into(), addr: "10.0.0.2".into(), port: 2 })
        );
        // 未知节点无解
        assert_eq!(resolve_endpoint(None, "never-seen"), None);
        // 断开摘除句柄但保留 memo
        forget_session(node);
        assert_eq!(session_of(node), None);
        assert!(resolve_endpoint(None, node).is_some());
    }
}
