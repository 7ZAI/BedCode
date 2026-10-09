//! 设备发现桥接层（宿主侧状态）——**实现已迁双端共享核**
//! （`bedcode-file-transfer-core::sessions`，ADR 0044），本文件保留桌面端宿主 I/O 包装。
//!
//! 职责切分（与退役前逐字一致）：设备缓存状态机（去重/TTL/展示名/能力位）在前端
//! `deviceState.ts`；本层只做三件事：
//!
//! 1. `mdns:found` / `mdns:lost` 原样透传给前端（wire 形状不过翻译）；
//! 2. 设备快照持久化（前端 debounce 写入 storage 键 `device_snapshot`）；
//! 3. endpoint memo + session 句柄映射，支撑重试回放与 close 寻址。
//!
//! 差异：核内状态是 `SessionTable` **实例**（可并行测试），端内以一把 `OnceLock<Mutex<_>>` 持有。

use crate::adapters::DesktopPorts;
use bedcode_file_transfer_core::sessions::SessionTable;
use bedcode_plugin_api::host::HostStorage;
use std::sync::Mutex;
use std::sync::OnceLock;

pub(crate) use bedcode_file_transfer_core::domain::{DeviceSnapshotEntry, DialEndpoint};
pub(crate) use bedcode_file_transfer_core::sessions as core_sessions;

/// 进程级状态（endpoint memo + session 句柄表）
static SESSIONS: OnceLock<Mutex<SessionTable>> = OnceLock::new();

fn table() -> &'static Mutex<SessionTable> {
    SESSIONS.get_or_init(|| Mutex::new(SessionTable::new()))
}

fn with_table<R>(f: impl FnOnce(&mut SessionTable) -> R) -> R {
    f(&mut table().lock().expect("session table lock"))
}

// ==================== 会话句柄映射 / 停用清理 ====================

/// 登记已连接节点的 session 句柄与 endpoint memo
pub(crate) fn remember_session(endpoint: &DialEndpoint, handle: String) {
    with_table(|t| t.remember_session(endpoint, handle));
}

/// 连接断开摘除句柄（endpoint memo 保留供自动重连）
pub(crate) fn forget_session(node_id: &str) {
    with_table(|t| t.forget_session(node_id));
}

/// 仅登记 endpoint memo（不铸 session 句柄）
///
/// 入站（被连侧）连接的对端寻址来源：被连侧没有拨号句柄，数据面命令
/// （浏览/拉取/发送）经 memo 重拨铸真实句柄。endpoint 取自前端自建设备缓存。
///
/// **只此一个入口**：空 `node_id` 直接拒绝（写入会污染 memo 且掩盖前端缺字段 bug）。
pub(crate) fn remember_endpoint(endpoint: &DialEndpoint) -> anyhow::Result<()> {
    with_table(|t| t.remember_endpoint_checked(endpoint)).map_err(anyhow::Error::new)
}

/// 摘除并返回全部活跃 session 句柄（插件停用时调用：先断开连接再清空表，
/// 让对端即时感知断线——否则连接残留、对端仍显示在线）
pub(crate) fn drain_sessions() -> Vec<String> {
    with_table(|t| t.drain_sessions())
}

/// 清空 endpoint memo（停用收尾；sessions 已由 `drain_sessions` 清空）
///
/// 不清理时跨 activate/deactivate 残留旧地址，对端换 IP/端口后数据面重拨会命中过期 memo。
pub(crate) fn clear_peer_state() {
    with_table(|t| t.clear_peer_state());
}

/// 解析目标：显式 endpoint 参数优先；否则查 memo。返回 None = 双双缺失。
pub(crate) fn resolve_endpoint(
    explicit: Option<DialEndpoint>,
    node_id: &str,
) -> Option<DialEndpoint> {
    with_table(|t| t.resolve_endpoint(explicit, node_id))
}

/// 取节点的活跃 session 句柄（未连接返回 None）
pub(crate) fn session_of(node_id: &str) -> Option<String> {
    with_table(|t| t.session_of(node_id))
}

// ==================== 快照持久化 ====================

/// 读取快照（无快照回空数组；损坏值按空表——快照是首屏锦上添花，不值得阻断激活）
pub(crate) fn load_snapshot<H: HostStorage + ?Sized>(
    h: &H,
) -> anyhow::Result<Vec<DeviceSnapshotEntry>> {
    core_sessions::load_snapshot(&DesktopPorts(h)).map_err(anyhow::Error::new)
}

/// 保存快照（前端 debounce 调用；条数上限由前端裁剪）
pub(crate) fn save_snapshot<H: HostStorage + ?Sized>(
    h: &H,
    entries: &[DeviceSnapshotEntry],
) -> anyhow::Result<()> {
    core_sessions::save_snapshot(&DesktopPorts(h), entries).map_err(anyhow::Error::new)
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_roundtrips_camel_case() {
        let ep = DialEndpoint {
            node_id: "aa".into(),
            addr: "192.168.1.5".into(),
            port: 47821,
        };
        let v = serde_json::to_value(&ep).unwrap();
        assert_eq!(v["nodeId"], "aa");
        assert_eq!(v["addr"], "192.168.1.5");
        assert_eq!(v["port"], 47821);
        assert_eq!(serde_json::from_value::<DialEndpoint>(v).unwrap(), ep);
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
        // 静态态隔离：本测试独占 key（与下方各用例互不重叠）
        let node = "memo-test-node";
        forget_session(node);
        let memo_ep = DialEndpoint {
            node_id: node.into(),
            addr: "10.0.0.1".into(),
            port: 1,
        };
        remember_session(&memo_ep, "sess-x".into());
        assert_eq!(session_of(node).as_deref(), Some("sess-x"));
        // 显式优先且刷新 memo
        let fresh = DialEndpoint {
            node_id: node.into(),
            addr: "10.0.0.2".into(),
            port: 2,
        };
        assert_eq!(resolve_endpoint(Some(fresh.clone()), node), Some(fresh));
        // 无显式走 memo
        assert_eq!(
            resolve_endpoint(None, node),
            Some(DialEndpoint {
                node_id: node.into(),
                addr: "10.0.0.2".into(),
                port: 2
            })
        );
        // 未知节点无解
        assert_eq!(resolve_endpoint(None, "never-seen"), None);
        // 断开摘除句柄但保留 memo
        forget_session(node);
        assert_eq!(session_of(node), None);
        assert!(resolve_endpoint(None, node).is_some());
    }

    /// 入站连接的寻址登记：仅写 memo 不造句柄（session_of 保持 None，
    /// ensure_target 走 memo 重拨铸真实句柄）
    #[test]
    fn remember_endpoint_writes_memo_without_session() {
        let node = "inbound-endpoint-only-node";
        forget_session(node);

        let ep = DialEndpoint {
            node_id: node.into(),
            addr: "10.0.0.9".into(),
            port: 9,
        };
        remember_endpoint(&ep).unwrap();
        assert_eq!(resolve_endpoint(None, node), Some(ep));
        assert_eq!(
            session_of(node),
            None,
            "memo-only: no session handle fabricated"
        );
        forget_session(node);
    }

    /// 空 node_id 拒绝：判据单点在核内，此处验证「拒绝路径不留痕」
    #[test]
    fn empty_node_id_rejected_before_polluting_memo() {
        let err = remember_endpoint(&DialEndpoint {
            node_id: String::new(),
            addr: "x".into(),
            port: 1,
        })
        .unwrap_err();
        assert!(err.to_string().contains("empty node_id"), "实得 {err}");
        assert_eq!(resolve_endpoint(None, ""), None, "拒绝路径不得写入 memo");
    }

    /// 停用清理：全部摘除并返回句柄，重复 drain 幂等
    #[test]
    fn drain_sessions_returns_all_and_clears_table() {
        let node_a = "drain-a";
        let node_b = "drain-b";
        forget_session(node_a);
        forget_session(node_b);
        remember_session(
            &DialEndpoint {
                node_id: node_a.into(),
                addr: "10.0.0.1".into(),
                port: 1,
            },
            "sess-a".into(),
        );
        remember_session(
            &DialEndpoint {
                node_id: node_b.into(),
                addr: "10.0.0.2".into(),
                port: 2,
            },
            "sess-b".into(),
        );

        let mut handles = drain_sessions();
        handles.sort();
        assert_eq!(handles, vec!["sess-a", "sess-b"]);
        assert_eq!(session_of(node_a), None);
        assert_eq!(session_of(node_b), None);
        // endpoint memo 仍在（数据面重连语义）；clear_peer_state 收尾清空
        assert!(resolve_endpoint(None, node_a).is_some());
        clear_peer_state();
        assert_eq!(resolve_endpoint(None, node_a), None);
        assert!(drain_sessions().is_empty(), "重复 drain 幂等");
    }
}
