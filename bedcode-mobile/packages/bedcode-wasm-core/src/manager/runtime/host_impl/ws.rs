//! host-websocket 客户端域 —— 移动端薄适配器（ADR 0043 引擎抽根）
//!
//! 引擎机制（句柄表 / 属主仲裁 / reader-writer 双任务 / 心跳 / 退避重连 / 帧信封 /
//! 停用回收）全部在通用能力 crate [`bedcode_ws_client_engine`]（仓库根
//! `packages/`，零 WIT / 零 SDK / 零平台形态）。本模块只保留两件事：
//!
//! 1. **5 条原语薄转发**：签名与历史一致（component.rs 接线 / host_impl 聚合入口
//!    不变），body 改为调引擎域函数；
//! 2. **移动端 [`MobileWsClientPorts`] 适配器**：权限门（manifest
//!    `granted_permissions`）/ 总线事件与帧投递 / 宿主运行时任务派生 / jwt 代发
//!    token / 全局退避钳制与策略——五个平台差异面的移动端实现。
//!
//! **同步↔异步桥留在宿主侧**：WIT host fn 是同步上下文，而引擎 `connect` 是 async
//! （握手 await）；`guarded_host_call`（panic 边界）+ `block_on_async`（重入安全桥）
//! 两件事都是宿主运行时形态，故在适配层，本模块不碰 engine 以外的任何机制。
//!
//! **只做客户端域**：移动端不跑 WS 服务器（ADR 0018），服务端域 9 函数与
//! `connection-context` 不跟演（ADR 0019 双端偏离）；不引入 `ws:server` 权限位。
//!
//! 与桌面端的能力域实现（`bedcode-server-websocket` 的 `plugin_binding` 客户端段）
//! **同构但分叉**：桌面那份与 actix 服务器栈 + 过滤器链同 crate 耦合；本 crate 用的
//! 是零平台依赖的通用引擎，两份之间的显式重复（ADR 0041 D1 桌面侧条件未满足）留待
//! 后续票据。
//!
//! 行为契约（权限门 fail-closed / 属主隔离 / 句柄生命周期 / close code 语义 /
//! 队列满 fail-fast / purge 回收 / 入参校验 / 帧信封形状 / 重连钳制）由引擎自身
//! 单测覆盖（`bedcode-ws-client-engine` crate 测试，20 例），本模块只保薄转发
//! 语义断言：端口装配正确（权限位读 manifest、token 读宿主认证状态、任务挂宿主
//! 运行时）与「回收聚合入口只走一条路」。

use std::sync::Arc;

use bedcode_ws_client_engine::ports::{BoxedTask, ReconnectPolicy, WsClientPorts, WsTask};

use super::super::{block_on_async, WasmPluginState};
use super::support::guarded_host_call;

use crate::host_api::ports::HostEnginePorts;
use crate::runtime_util::spawn_with_error_boundary_on;

// ==================== 移动端 WsClientPorts 适配器 ====================

/// 移动端端口适配：实现引擎 `WsClientPorts` 的五个平台差异面。
///
/// 按插件实例构建（[`Self::from_state`],权限门读该插件 manifest
/// `granted_permissions`）；reader / writer / 重连任务会克隆本端口并长期持有
/// （`'static`），故字段只放 `Arc` / 可克隆宿主句柄。
struct MobileWsClientPorts {
    /// 插件消息总线（事件与帧的投递出口）
    bus: Arc<crate::bus::MessageBus>,
    /// 宿主引擎端口（jwt token / 全局退避边界与策略的投影）
    host_ports: Arc<dyn HostEnginePorts>,
    /// 宿主运行时句柄（后台任务派生面：host fn 可能在无 runtime 上下文的线程上执行）
    runtime: tokio::runtime::Handle,
    /// 该插件实例是否已授权 `ws:client`（构造时结算，权限门零延迟）
    permitted: bool,
}

impl MobileWsClientPorts {
    /// 从插件实例状态构建（宿主原语路径）；返回 trait 对象（引擎 API 要求）
    fn from_state(state: &WasmPluginState) -> Arc<dyn WsClientPorts> {
        Arc::new(Self {
            bus: Arc::clone(&state.host_ctx.message_bus),
            host_ports: Arc::clone(&state.host_ctx.ports),
            runtime: state.runtime_handle.clone(),
            permitted: state
                .granted_permissions
                .contains(bedcode_plugin_api_mobile::permission::PERMISSION_WS_CLIENT),
        })
    }
}

/// 宿主运行时任务包装（tokio JoinHandle → 引擎 `WsTask`）
///
/// `abort` 是幂等的（重复调用不 panic），符合引擎 `WsTask::cancel` 的契约。
struct RuntimeTask(tokio::task::JoinHandle<()>);

impl WsTask for RuntimeTask {
    fn cancel(&self) {
        self.0.abort();
    }
}

/// 退避策略适配：把宿主的 `WsReconnectPolicyPort` 转成引擎的 `ReconnectPolicy`
///
/// 两个 trait 的三方法逐字同形（ADR 0040 共享核的机制同源），本包装只做类型转换，
/// **不复制任何退避逻辑**——指数退避 / 抖动 / 下限钳制的真源始终在宿主
/// `connection::reconnect`。
struct MobileReconnectPolicy(Box<dyn crate::host_api::ports::WsReconnectPolicyPort>);

#[async_trait::async_trait]
impl ReconnectPolicy for MobileReconnectPolicy {
    async fn start(&self) -> Option<()> {
        self.0.start().await
    }

    async fn get_delay(&self) -> std::time::Duration {
        self.0.get_delay().await
    }

    async fn on_success(&self) {
        self.0.on_success().await
    }
}

impl WsClientPorts for MobileWsClientPorts {
    /// 权限门（fail-closed）：出站连接是 SSRF 面，未在 manifest 声明即拒
    fn check_permission(&self, _plugin_id: &str) -> bool {
        // 权限仲裁走 manifest granted_permissions（与 mdns 域同语义）；引擎在每次
        // 原语入口先问本门，denied 时原语直接拒绝并给出统一文案
        self.permitted
    }

    /// 宿主 jwt token（代发首帧用；未认证时空串，引擎据此显性失败）
    ///
    /// **凭据不落插件**（C4）：只交出「代发时那一刻的 token 值」，本模块与引擎都
    /// 不打印其值（日志只记长度）。
    fn global_token(&self) -> String {
        self.host_ports.global_token()
    }

    /// 全局退避钳制边界（宿主全局常量的投影）
    fn reconnect_bounds(&self) -> (u64, u64) {
        self.host_ports.reconnect_bounds()
    }

    fn reconnect_policy(
        &self,
        max_retries: u32,
        base_ms: u64,
        max_ms: u64,
    ) -> Box<dyn ReconnectPolicy> {
        Box::new(MobileReconnectPolicy(self.host_ports.reconnect_policy(
            max_retries,
            base_ms,
            max_ms,
        )))
    }

    /// 状态事件：引擎已拼好**完整 topic**（属主私有命名空间），本方法只负责
    /// 「往这个 topic 上发」并由总线做订阅方隔离
    ///
    /// sender 恒 `"host"`：总线会过滤「发布者 == 订阅者」，若用插件 id 当 sender
    /// 会把自己的事件一起过滤掉。
    fn publish(&self, topic: &str, payload: serde_json::Value) {
        self.bus.publish(topic, "host", payload);
    }

    /// 入站帧：属主私有二进制 topic（零 JSON 编解码）
    fn publish_binary(&self, topic: &str, payload: Vec<u8>) {
        self.bus.publish_binary(topic, "host", payload);
    }

    /// 宿主运行时任务派生 + panic 错误边界（任务 panic 不得静默终止，也不得污染
    /// 宿主状态）
    fn spawn(&self, task_name: &'static str, task: BoxedTask) -> Arc<dyn WsTask> {
        Arc::new(RuntimeTask(spawn_with_error_boundary_on(
            &self.runtime,
            task_name,
            task,
        )))
    }
}

// ==================== 5 条原语薄转发（签名与历史一致） ====================

/// 建立出站 WS 连接（同步阻塞至握手完成；panic 边界 + 异步桥在宿主侧）
pub(crate) fn ws_connect(state: &WasmPluginState, config_json: &str) -> Result<String, String> {
    let ports = MobileWsClientPorts::from_state(state);
    let plugin_id = state.plugin_id.clone();
    // 引擎 `connect` 是 async（握手 await），WIT host fn 是同步上下文：`block_on_async`
    // 在宿主运行时上驱动；`guarded_host_call` 截获 panic（越界 UB 面）并返回与超时
    // 同形的失败文案
    guarded_host_call(
        &state.plugin_id,
        "host_websocket_connect",
        Err("ws connect: handshake timed out after 5s".to_string()),
        || {
            block_on_async(&state.runtime_handle, async move {
                bedcode_ws_client_engine::engine::connect(&ports, &plugin_id, config_json).await
            })
        },
    )
}

/// 发送文本帧（UTF-8）
pub(crate) fn ws_send_text(
    state: &WasmPluginState,
    handle: &str,
    text: &str,
) -> Result<(), String> {
    let ports = MobileWsClientPorts::from_state(state);
    bedcode_ws_client_engine::engine::send_text(&ports, &state.plugin_id, handle, text)
}

/// 发送二进制帧
pub(crate) fn ws_send_binary(
    state: &WasmPluginState,
    handle: &str,
    payload: Vec<u8>,
) -> Result<(), String> {
    let ports = MobileWsClientPorts::from_state(state);
    bedcode_ws_client_engine::engine::send_binary(&ports, &state.plugin_id, handle, payload)
}

/// 主动关闭连接：返回是否命中（幂等：未知句柄 false；命中退避中的旧句柄即取消重连）
pub(crate) fn ws_close(
    state: &WasmPluginState,
    handle: &str,
    close_json: &str,
) -> Result<bool, String> {
    let ports = MobileWsClientPorts::from_state(state);
    bedcode_ws_client_engine::engine::close(&ports, &state.plugin_id, handle, close_json)
}

/// 查询连接是否 open；仅属主可查（句柄不存在 ⇒ false）
pub(crate) fn ws_is_connected(state: &WasmPluginState, handle: &str) -> Result<bool, String> {
    let ports = MobileWsClientPorts::from_state(state);
    bedcode_ws_client_engine::engine::is_connected(&ports, &state.plugin_id, handle)
}

// ==================== 停用回收 ====================

/// 停用回收：下线该插件全部出站连接 + 取消全部重连会话（只碰本人句柄）
///
/// 无需装配端口：回收是纯表操作（任务中止 + 条目摘除），不发布任何事件。
pub(crate) fn purge_for_plugin(plugin_id: &str) -> usize {
    bedcode_ws_client_engine::engine::purge_for_plugin(plugin_id)
}

// ==================== Tests（薄转发语义，零真实握手） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::MessageBus;
    use crate::manager::runtime::WasmHostContext;
    use crate::security::fs_auth::FsAuthChecker;
    use crate::storage::PluginStorage;
    use std::collections::HashSet;

    /// 无头插件状态：权限集按 `ws:client` 授权开关构建
    fn state(
        plugin_id: &str,
        ws_granted: bool,
        runtime: tokio::runtime::Handle,
    ) -> WasmPluginState {
        let db = Arc::new(std::sync::Mutex::new(crate::db::Database::from_connection(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        )));
        let storage = PluginStorage::test_storage();
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None, Vec::new()));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        let mut granted_permissions = HashSet::new();
        if ws_granted {
            granted_permissions
                .insert(bedcode_plugin_api_mobile::permission::PERMISSION_WS_CLIENT.to_string());
        }
        WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx: Arc::new(WasmHostContext::new_headless(
                db,
                storage,
                None,
                fs_auth,
                Arc::new(MessageBus::new()),
                status_reporter,
            )),
            runtime_handle: runtime,
            granted_permissions,
            on_message_binary: None,
        }
    }

    /// 权限门 fail-closed：未授权时全部 5 条原语在引擎权限门处直接拒绝，
    /// 且拒绝发生在**任何宿主能力被触碰之前**（零握手 / 零 spawn / 零发布）
    #[tokio::test]
    async fn ws_denied_without_permission_short_circuits_in_engine_gate() {
        let s = state("deny-plugin", false, tokio::runtime::Handle::current());
        for err in [
            ws_connect(&s, r#"{"url":"ws://127.0.0.1:1/"}"#).expect_err("connect must be denied"),
            ws_send_text(&s, "wsc-x", "hi").expect_err("send-text must be denied"),
            ws_send_binary(&s, "wsc-x", vec![1]).expect_err("send-binary must be denied"),
            ws_close(&s, "wsc-x", "{}").expect_err("close must be denied"),
            ws_is_connected(&s, "wsc-x").expect_err("is-connected must be denied"),
        ] {
            assert_eq!(
                err,
                format!(
                    "permission denied: {}",
                    bedcode_plugin_api_mobile::permission::PERMISSION_WS_CLIENT
                ),
                "拒绝文案必须与移动 SDK 权限字面量一致"
            );
        }
    }

    /// 端口装配面：jwt token 与退避钳制边界**从宿主引擎端口取**（不在本模块留常量
    /// 副本——双真源漂移），且未认证时 token 为空串（引擎据此显性失败而非伪发）
    #[tokio::test]
    async fn ports_read_token_and_backoff_bounds_from_host_engine_ports() {
        let host_ports: Arc<dyn HostEnginePorts> = Arc::new(crate::test_support::MockPorts::new());
        let ports = MobileWsClientPorts {
            bus: Arc::new(MessageBus::new()),
            host_ports,
            runtime: tokio::runtime::Handle::current(),
            permitted: false,
        };

        // 退避边界：端口直接透传宿主全局常量（引擎侧再对 config 做钳制）
        let (min_ms, max_ms) = ports.reconnect_bounds();
        assert!(
            min_ms >= 1_000,
            "下限钳制是自愈风暴的教训沉淀（2026-09-29）"
        );
        assert!(max_ms >= min_ms);

        // 未认证 ⇒ token 空串（凭据不落插件面；引擎据此显性失败而非伪发 auth 帧）
        assert!(
            ports.global_token().is_empty(),
            "无头宿主未认证 ⇒ token 空串"
        );

        // 未授权插件 ⇒ 权限门假（fail-closed 的装配侧读数）
        assert!(!ports.check_permission("probe-plugin"), "未授权即拒");
    }

    /// 停用回收空表零副作用（无端口装配：纯表操作，不发布事件）
    #[tokio::test]
    async fn purge_unknown_plugin_is_noop() {
        assert_eq!(purge_for_plugin("ghost-plugin"), 0);
    }
}
