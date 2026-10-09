//! 设备发现桥接层（宿主侧状态）——**实现已迁双端共享核**
//! （`bedcode-file-transfer-core::sessions`，ADR 0044），本文件保留移动端宿主 I/O 包装。
//!
//! 职责切分（与退役前逐字一致）：设备缓存状态机（去重/TTL/展示名/能力位）在前端
//! `deviceState.ts`（wasm32 无时钟，TTL 判定需 `Date.now`）；本层只做三件事：
//!
//! 1. `mdns:found` / `mdns:lost` 原样透传给前端（wire 形状不过翻译）；
//! 2. 设备快照持久化（前端 debounce 写入 storage 键 `device_snapshot`）；
//! 3. endpoint memo + session 句柄映射，支撑重试回放与 close 寻址。
//!
//! 差异：核内状态是 `SessionTable` **实例**（可并行测试），端内以一把 `OnceLock<Mutex<_>>`
//! 持有——退役前那份模块级全局静态表需靠 `statics_lock` 串行化跨用例，现在只在端内需要。

use crate::adapters::MobilePorts;
use bedcode_file_transfer_core::sessions::SessionTable;
use bedcode_plugin_api_mobile::host::HostStorage;
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
/// 不额外暴露「无校验版本」——那等于给回归留一条短路（退役前前端命令面就是这样分叉的）。
pub(crate) fn remember_endpoint(endpoint: &DialEndpoint) -> anyhow::Result<()> {
    with_table(|t| t.remember_endpoint_checked(endpoint)).map_err(anyhow::Error::new)
}

/// 摘除并返回全部活跃 session 句柄（插件停用时调用：先断开连接再清空表，
/// 让对端即时感知断线——否则连接残留、对端仍显示在线）
pub(crate) fn drain_sessions() -> Vec<String> {
    with_table(|t| t.drain_sessions())
}

/// 清空 endpoint memo（停用收尾；sessions 已由 `drain_sessions` 清空）
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
    core_sessions::load_snapshot(&MobilePorts(h)).map_err(anyhow::Error::new)
}

/// 保存快照（前端 debounce 调用；条数上限由前端裁剪）
pub(crate) fn save_snapshot<H: HostStorage + ?Sized>(
    h: &H,
    entries: &[DeviceSnapshotEntry],
) -> anyhow::Result<()> {
    core_sessions::save_snapshot(&MobilePorts(h), entries).map_err(anyhow::Error::new)
}

// ==================== Tests ====================

/// 测试串行锁：端内进程级静态表跨并行用例互清，跨模块共用同一把
#[cfg(test)]
pub(crate) fn statics_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

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
        let _guard = statics_lock().lock().expect("statics lock");
        let node = "memo-test-node";
        forget_session(node);
        assert_eq!(session_of(node), None);
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
        assert_eq!(resolve_endpoint(None, node), Some(memo_ep_after_refresh()));
        // 未知节点无解
        assert_eq!(resolve_endpoint(None, "never-seen"), None);
        // 断开摘除句柄但保留 memo（激活期内自动重连语义，与停用全清不同）
        forget_session(node);
        assert_eq!(session_of(node), None);
        assert!(resolve_endpoint(None, node).is_some());
        clear_peer_state();
    }

    fn memo_ep_after_refresh() -> DialEndpoint {
        DialEndpoint {
            node_id: "memo-test-node".into(),
            addr: "10.0.0.2".into(),
            port: 2,
        }
    }

    #[test]
    fn drain_sessions_returns_all_and_clears_table() {
        let _guard = statics_lock().lock().expect("statics lock");
        let node_a = "drain-a";
        let node_b = "drain-b";
        forget_session(node_a);
        forget_session(node_b);
        clear_peer_state();

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

        // 全部摘除并返回句柄（deactivate 据此逐个 peer_close）
        let mut handles = drain_sessions();
        handles.sort();
        assert_eq!(handles, vec!["sess-a", "sess-b"]);
        assert_eq!(session_of(node_a), None, "sessions 必须清空");
        assert_eq!(session_of(node_b), None);

        // endpoint memo 仍在（数据面重连语义）；clear_peer_state 收尾清空
        assert!(resolve_endpoint(None, node_a).is_some());
        clear_peer_state();
        assert_eq!(resolve_endpoint(None, node_a), None);

        // 幂等：重复 drain 无句柄可摘
        assert!(drain_sessions().is_empty());
    }

    /// 入站连接的寻址登记：仅写 memo 不造句柄（session_of 保持 None，
    /// ensure_target 走 memo 重拨铸真实句柄）
    #[test]
    fn remember_endpoint_writes_memo_without_session() {
        let _guard = statics_lock().lock().expect("statics lock");
        let node = "inbound-node";
        forget_session(node);
        clear_peer_state();

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
        clear_peer_state();
    }

    /// 空 node_id 拒绝：适配器接线后该判据在核内，此处验证「拒绝路径不留痕」
    #[test]
    fn empty_node_id_rejected_before_polluting_memo() {
        let _guard = statics_lock().lock().expect("statics lock");
        clear_peer_state();
        let err = remember_endpoint(&DialEndpoint {
            node_id: String::new(),
            addr: "x".into(),
            port: 1,
        })
        .unwrap_err();
        assert!(err.to_string().contains("empty node_id"), "实得 {err}");
        assert_eq!(resolve_endpoint(None, ""), None, "拒绝路径不得写入 memo");
        clear_peer_state();
    }
}
