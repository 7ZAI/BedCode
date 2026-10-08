//! `plugin_binding` 的跨分组测试脚手架（用例文件经 `use super::scaffold::*` 引用）
//!
//! **为什么自带假端口**：本域的端口实现属宿主（host_api 域的 peer 适配器），
//! crate 内不能引用宿主 bin crate ⇒ 单测必须在 crate 内造一份假实现。
//! 假端口同时是「机制自持」的可测性证据：权限门、属主判定、载荷校验、判定顺序
//! 的断言全在本域内闭环，不经宿主上下文。

use std::any::Any;
use std::collections::HashSet;
use std::sync::Arc;

use crate::PeerCtx;

use super::ports::{BoxedBlocked, PeerPorts};

/// 假端口（不触宿主、不起引擎）：只答权限门，引擎上下文一律「无头」
pub(super) struct FakePorts {
    /// 已授权的 `(plugin_id, permission)` 对
    granted: HashSet<(String, String)>,
}

impl FakePorts {
    /// 按「(插件, 权限)」清单造端口（空清单 = 一律拒绝）
    ///
    /// 返回 `Arc<dyn PeerPorts>`：域函数一律收端口 trait 对象，本组用例不为
    /// 「夹具类型」留后门。
    pub(super) fn with(grants: &[(&str, &str)]) -> Arc<dyn PeerPorts> {
        Arc::new(FakePorts {
            granted: grants
                .iter()
                .map(|(plugin, permission)| (plugin.to_string(), permission.to_string()))
                .collect(),
        })
    }
}

impl PeerPorts for FakePorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, _api: &str) -> bool {
        self.granted
            .iter()
            .any(|(p, perm)| p == plugin_id && perm == permission)
    }

    /// 无头口径：迁移前是 `require_app` 拿不到 `AppHandle` 的那条报错，逐字保留。
    /// 本组用例不构造真实 [`PeerCtx`]（它要四个引擎状态 + 端口聚合），故一律走
    /// 「不可得」分支——需要触达引擎的用例请另立组装型夹具（不要在本夹具里造
    /// 半真引擎状态）。
    fn peer_ctx(&self) -> Result<Arc<PeerCtx>, String> {
        Err(super::ports::HEADLESS_UNAVAILABLE.to_string())
    }

    /// 同步↔异步桥（测试替身）：新线程 + 独立 multi-thread runtime 驱动。
    ///
    /// 与宿主 `runtime_util::block_on_async` 的 current_thread 分支同策略
    /// （本域的用例可能跑在 current_thread 运行时里，同线程 `block_on` 必死锁）。
    fn block_on_any(&self, fut: BoxedBlocked) -> Box<dyn Any + Send> {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(2)
                        .enable_all()
                        .build()
                        .expect("test bridge runtime")
                        .block_on(fut)
                })
                .join()
                .expect("test bridge driver thread must not panic")
        })
    }
}

/// 统一拒绝文案（权限门未过时的唯一答案）
pub(super) fn denied() -> String {
    super::denied()
}

/// 句柄表条目夹具（属主固定为 `com.bedcode.owner-a`）
pub(super) fn entry(node: &str) -> super::SessionEntry {
    super::SessionEntry {
        node_id: node.to_string(),
        addr: "192.168.1.5".to_string(),
        port: 47821,
        owner: "com.bedcode.owner-a".to_string(),
    }
}

/// 经进程级句柄表铸造一条会话（域内唯一入口，返回 `sess-<uuid>` 句柄）
pub(super) fn mint(node: &str) -> String {
    super::with_handles(|t| t.mint_session(entry(node)))
}

/// 摘除一条会话（用例结束清理，避免跨用例污染进程级表）
pub(super) fn drop_handle(handle: &str) {
    super::with_handles(|t| {
        t.take_session(handle);
    });
}
