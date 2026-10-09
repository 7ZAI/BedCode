//! 设备发现桥接的宿主侧状态：endpoint memo + session 句柄表 + 设备快照持久化。
//!
//! 职责切分（与退役前逐字一致）：设备缓存状态机（去重/TTL/展示名/能力位）在**前端**
//! `deviceState.ts`（wasm32 无时钟，TTL 判定需 `Date.now`）；核内只做三件事：
//!
//! 1. `mdns:found` / `mdns:lost` 原样透传给前端（wire 形状不过翻译）；
//! 2. 设备快照持久化（前端 debounce 写入，activate 时供前端载入首屏）；
//! 3. endpoint memo + session 句柄映射，支撑重试回放与 close 寻址。
//!
//! **状态是实例而非全局静态**：`SessionTable` 由各端适配器持有（`OnceLock<Mutex<_>>`）。
//! 全局静态放在核内会让跨用例状态互清（退役前双端只能靠一把 `statics_lock` 串行化），
//! 而这里每个用例自建一份即可。

use std::collections::HashMap;

use crate::domain::{DeviceSnapshotEntry, DialEndpoint};
use crate::ports::{KvStore, PortError, PortResult};

/// 设备快照存储键（双端一致）
pub const DEVICE_SNAPSHOT_KEY: &str = "device_snapshot";

/// 失效目标解析失败时的语义：`None` = 显式与 memo 双双缺失
#[derive(Debug, Default)]
pub struct SessionTable {
    /// nodeId → session 句柄
    sessions: HashMap<String, String>,
    /// nodeId → endpoint memo（数据面命令记忆；重试回放寻址源）
    endpoints: HashMap<String, DialEndpoint>,
}

impl SessionTable {
    pub fn new() -> Self {
        SessionTable::default()
    }

    /// 登记已连接节点的 session 句柄与 endpoint memo
    pub fn remember_session(&mut self, endpoint: &DialEndpoint, handle: String) {
        self.endpoints
            .insert(endpoint.node_id.clone(), endpoint.clone());
        self.sessions.insert(endpoint.node_id.clone(), handle);
    }

    /// 连接断开摘除句柄（endpoint memo 保留供自动重连）
    pub fn forget_session(&mut self, node_id: &str) {
        self.sessions.remove(node_id);
    }

    /// 仅登记 endpoint memo（不铸 session 句柄）。
    ///
    /// 入站（被连侧）连接的对端寻址来源：被连侧没有拨号句柄，数据面命令
    /// （浏览/拉取/发送）经 memo 重拨铸真实句柄。endpoint 取自前端自建设备缓存。
    pub fn remember_endpoint(&mut self, endpoint: &DialEndpoint) {
        self.endpoints
            .insert(endpoint.node_id.clone(), endpoint.clone());
    }

    /// 空 `node_id` 拒绝：写入会污染 memo（后续任意查询均不命中但长期驻留）且掩盖前端缺字段 bug
    pub fn remember_endpoint_checked(&mut self, endpoint: &DialEndpoint) -> PortResult<()> {
        if endpoint.node_id.is_empty() {
            return Err(PortError::new(
                "remember-peer-endpoint: empty node_id rejected",
            ));
        }
        self.remember_endpoint(endpoint);
        Ok(())
    }

    /// 解析目标：显式 endpoint 参数优先（同时刷新 memo）；否则查 memo。
    pub fn resolve_endpoint(
        &mut self,
        explicit: Option<DialEndpoint>,
        node_id: &str,
    ) -> Option<DialEndpoint> {
        if let Some(ep) = explicit {
            self.endpoints.insert(node_id.to_string(), ep.clone());
            return Some(ep);
        }
        self.endpoints.get(node_id).cloned()
    }

    /// 取节点的活跃 session 句柄（未连接返回 None）
    pub fn session_of(&self, node_id: &str) -> Option<String> {
        self.sessions.get(node_id).cloned()
    }

    /// 摘除并返回全部活跃 session 句柄（插件停用时调用：先断开连接再清空表，
    /// 让对端即时感知断线——否则连接残留、对端仍显示在线）
    pub fn drain_sessions(&mut self) -> Vec<String> {
        let handles: Vec<String> = self.sessions.values().cloned().collect();
        self.sessions.clear();
        handles
    }

    /// 清空 endpoint memo（停用收尾；sessions 已由 `drain_sessions` 清空）
    ///
    /// 不清理时跨 activate/deactivate 残留旧地址，对端换 IP/端口后数据面重拨会命中过期 memo。
    pub fn clear_peer_state(&mut self) {
        self.endpoints.clear();
    }
}

// ==================== 快照持久化 ====================

/// 读取快照（无快照返回空数组；损坏值按空表——快照是「锦上添花」的首屏数据，
/// 与设置面不同，不值得让损坏文件阻断激活）
pub fn load_snapshot<H: KvStore>(h: &H) -> PortResult<Vec<DeviceSnapshotEntry>> {
    let Some(v) = h.storage_get(DEVICE_SNAPSHOT_KEY)? else {
        return Ok(vec![]);
    };
    Ok(serde_json::from_value(v).unwrap_or_default())
}

/// 保存快照（前端 debounce 调用；条数上限由前端裁剪）
pub fn save_snapshot<H: KvStore>(h: &H, entries: &[DeviceSnapshotEntry]) -> PortResult<()> {
    let value = serde_json::to_value(entries)
        .map_err(|e| PortError::new(format!("device snapshot serialize failed: {e}")))?;
    h.storage_set(DEVICE_SNAPSHOT_KEY, &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{KvStore, PortResult};
    use serde_json::Value;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MockKv(RefCell<HashMap<String, Value>>);

    impl KvStore for MockKv {
        fn storage_get(&self, key: &str) -> PortResult<Option<Value>> {
            Ok(self.0.borrow().get(key).cloned())
        }
        fn storage_set(&self, key: &str, value: &Value) -> PortResult<()> {
            self.0.borrow_mut().insert(key.to_string(), value.clone());
            Ok(())
        }
        fn storage_delete(&self, key: &str) -> PortResult<()> {
            self.0.borrow_mut().remove(key);
            Ok(())
        }
    }

    fn ep(node: &str, port: u16) -> DialEndpoint {
        DialEndpoint {
            node_id: node.into(),
            addr: format!("10.0.0.{port}"),
            port,
        }
    }

    #[test]
    fn explicit_endpoint_wins_and_refreshes_memo() {
        let mut t = SessionTable::new();
        t.remember_session(&ep("n1", 1), "sess-x".into());
        assert_eq!(t.session_of("n1").as_deref(), Some("sess-x"));

        let fresh = ep("n1", 2);
        assert_eq!(t.resolve_endpoint(Some(fresh.clone()), "n1"), Some(fresh));
        // 显式入参已刷新 memo：下次无显式也拿到新地址
        assert_eq!(t.resolve_endpoint(None, "n1"), Some(ep("n1", 2)));
        assert_eq!(t.resolve_endpoint(None, "never-seen"), None);
    }

    #[test]
    fn disconnect_removes_handle_but_keeps_memo() {
        let mut t = SessionTable::new();
        t.remember_session(&ep("n1", 1), "sess-x".into());
        t.forget_session("n1");
        assert_eq!(t.session_of("n1"), None, "断开必须摘除句柄");
        assert!(
            t.resolve_endpoint(None, "n1").is_some(),
            "memo 保留供自动重连"
        );
    }

    #[test]
    fn remember_endpoint_writes_memo_without_session() {
        let mut t = SessionTable::new();
        t.remember_endpoint_checked(&ep("inbound", 9)).unwrap();
        assert_eq!(t.resolve_endpoint(None, "inbound"), Some(ep("inbound", 9)));
        assert_eq!(t.session_of("inbound"), None, "memo-only：不得铸出句柄");
    }

    #[test]
    fn empty_node_id_is_rejected_before_polluting_memo() {
        let mut t = SessionTable::new();
        let err = t.remember_endpoint_checked(&ep("", 1)).unwrap_err();
        assert!(err.message().contains("empty node_id"), "实得 {err}");
        // 拒绝路径不得留下任何痕迹（校验必须在写入之前，否则「污染 memo」的告警就是空话）
        assert_eq!(t.resolve_endpoint(None, ""), None);
        // 合法 id 照常写入（证明拒绝的是空 id，而不是整个调用被短路）
        t.remember_endpoint_checked(&ep("n1", 1)).unwrap();
        assert_eq!(t.resolve_endpoint(None, "n1"), Some(ep("n1", 1)));
    }

    #[test]
    fn drain_returns_all_and_clears_then_is_idempotent() {
        let mut t = SessionTable::new();
        t.remember_session(&ep("a", 1), "sess-a".into());
        t.remember_session(&ep("b", 2), "sess-b".into());
        let mut handles = t.drain_sessions();
        handles.sort();
        assert_eq!(handles, vec!["sess-a", "sess-b"]);
        assert_eq!(t.session_of("a"), None);
        assert_eq!(t.session_of("b"), None);
        // memo 仍在（数据面重连语义）；clear_peer_state 收尾清空
        assert!(t.resolve_endpoint(None, "a").is_some());
        t.clear_peer_state();
        assert_eq!(t.resolve_endpoint(None, "a"), None);
        assert!(t.drain_sessions().is_empty(), "重复 drain 幂等");
    }

    #[test]
    fn tables_are_per_instance_not_process_global() {
        // 核内不持全局静态：两个实例互不影响（退役前双端需靠 statics_lock 串行化用例）
        let mut a = SessionTable::new();
        let mut b = SessionTable::new();
        a.remember_session(&ep("shared", 1), "sess-a".into());
        assert_eq!(a.session_of("shared").as_deref(), Some("sess-a"));
        assert_eq!(b.session_of("shared"), None);
        assert!(b.drain_sessions().is_empty());
    }

    #[test]
    fn snapshot_roundtrips_and_absent_key_reads_empty() {
        let h = MockKv::default();
        assert!(load_snapshot(&h).unwrap().is_empty());
        let entries = vec![DeviceSnapshotEntry {
            node_id: "abc".into(),
            device_name: "Desktop".into(),
            addr: "10.0.0.1".into(),
            port: 47821,
            capabilities_hex: "1".into(),
            instance_name: "inst".into(),
            last_seen_ms: 1234,
        }];
        save_snapshot(&h, &entries).unwrap();
        assert_eq!(load_snapshot(&h).unwrap(), entries);
    }

    #[test]
    fn corrupt_snapshot_degrades_to_empty_instead_of_blocking_activation() {
        let h = MockKv::default();
        h.storage_set(DEVICE_SNAPSHOT_KEY, &serde_json::json!("broken"))
            .unwrap();
        assert!(load_snapshot(&h).unwrap().is_empty());
    }
}
