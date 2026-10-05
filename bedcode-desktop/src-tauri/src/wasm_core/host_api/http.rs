//! host-http 宿主侧适配器（能力域已迁出本模块）
//!
//! **本文件只剩四样东西**，原 1,161 行的 http 实现（入站 2 条 + 出站 1 条原语）
//! 已整体迁入 `bedcode_server_http::plugin_binding`（wasm-core-lib-split 票 06：
//! 入站与出站是同一 crate 里的两个模块——出站只有 1 条原语、独立 crate 过薄，
//! 见 spec §7 D10）：
//!
//! 1. [`HostHttpPorts`] —— 能力域端口的宿主实现；
//! 2. [`install`] —— 开机期装配端口（供 PluginHost 装配链调用）；
//! 3. [`HttpUnitExecutor`] —— 任务单元执行器（**注册点留宿主**：它注册进留 core 的
//!    任务引擎；执行体本身在能力域）；
//! 4. 停用回收转发 —— 插件停用时 [`purge_for_plugin`] 只碰本人。
//!
//! ## 哪些机制**刻意留在宿主**（票 06 验收项）
//!
//! - **出站授权闸门整条链**（授权记录库 / 档位策略 / 弹窗询问面）：安全闸门属宿主
//!   允许的四类薄壳之一，且闸门不应可插拔 ⇒ 能力域只经
//!   [`HttpPorts::authorize_outbound`] 消费三态结果；
//! - **声明门**（`network:http`）：复用既有 `host_api::check_permission`，同一份
//!   PermissionManager、同一条拒绝 warn 路径；
//! - **任务引擎与执行器注册表**：任务引擎留 core，执行器只提供「kind → 域函数」的
//!   转发（params 原样透传，见 `manager::task`）。
//!
//! ## 出站闸门为什么在本文件里驱动异步（而不是给能力域一个 future）
//!
//! 授权 future 由宿主造（它才认得授权检查器），能力域造不出来；而 guest 侧的
//! `host-http.fetch` 是**同步**函数。端口方法因此是同步的：宿主内部用那份唯一的
//! 同步↔异步桥驱动完再交结果（桥的实现在 `runtime_util`，ambient runtime 与
//! actix `current_thread` 自锁规避都是实测产物，能力域不得复制第二份）。

use std::any::Any;
use std::sync::Arc;

use bedcode_server_http::plugin_binding::egress;
use bedcode_server_http::plugin_binding::ports::{BoxedBlocked, HttpEventSink, HttpPorts, OutboundAuth};

use crate::wasm_core::host_api::context::WasmHostContext;
use crate::wasm_core::host_api::unit_executor::UnitExecutor;

/// 停用回收的再导出（插件停用时由 `PluginHost` 调用；只碰本人）
///
/// 实现与端点注册表都在能力域（服务端域注册表本就是本传输面 crate 的既有资产），
/// 宿主这一侧只保留调用路径（与 `ws::purge_for_plugin` 同形）。
pub use bedcode_server_http::plugin_binding::purge_for_plugin;

/// 能力模块名（必须与 `bedcode_server_http::plugin_binding::DESC.name` 逐字一致）
///
/// 它同时是 `component.rs` 里 `HOST_MODULES` 白名单的键——两者不同即红。
pub const HOST_MODULE_NAME: &str = "http";

/// 端口的宿主实现
///
/// 生产路径在**开机期**捕获 `Arc<WasmHostContext>`（`PluginHost::new` 已建好），
/// 之后每条原语零查表地取权限门 / 出站授权 / 事件通道。
pub struct HostHttpPorts {
    /// 宿主上下文（权限门、出站授权检查器、AppHandle 三者都在它身上）
    ctx: Arc<WasmHostContext>,
}

impl HostHttpPorts {
    /// 端口（生产）：从宿主上下文取权限管理器、出站授权检查器与 `AppHandle`
    pub fn from_ctx(ctx: Arc<WasmHostContext>) -> Self {
        Self { ctx }
    }
}

impl HttpPorts for HostHttpPorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3 四类薄壳之「安全闸门」——闸门不应可插拔）。
        // 复用既有 `host_api::check_permission`：同一份 PermissionManager、同一条
        // 拒绝 warn 路径（AGENTS §8 结构化字段），不另起一套判定。
        super::check_permission(self.ctx.as_ref(), plugin_id, permission, api)
    }

    fn authorize_outbound(&self, plugin_id: &str, url: &str, may_prompt: bool) -> OutboundAuth {
        // 出站授权闸门整条链留宿主（票 06）。`may_prompt = false` 走「只认记录、
        // 绝不弹窗」那一支——弹窗会占住任务池槽位最短 30s，且用户在错误的时机
        // 看到问题（与 fs 任务单元同款约束）。
        let checker = self.ctx.net_auth();
        let verdict = crate::wasm_core::runtime_util::block_on_async(async {
            if may_prompt {
                checker.authorize_outbound(plugin_id, url).await
            } else {
                checker.authorize_outbound_quiet(plugin_id, url).await
            }
        });
        match verdict {
            // origin 是归一化后的 origin（不含 path / query，AGENTS §8 凭据红线）
            Ok(verdict) if verdict.is_allowed() => OutboundAuth::Allowed {
                origin: verdict.origin().to_string(),
            },
            Ok(verdict) => OutboundAuth::Denied {
                reason: verdict.reason().to_string(),
                origin: verdict.origin().to_string(),
            },
            Err(e) => OutboundAuth::CheckFailed(e.to_string()),
        }
    }

    fn event_sink(&self) -> Option<Arc<dyn HttpEventSink>> {
        // 「缺席」与「投递失败」是两件事（能力域据此对无头流式请求显性报错），
        // 故这里返回 `None` 而不是给一个静默丢弃的 sink。
        let handle = self.ctx.app_handle()?;
        Some(Arc::new(AppHandleEventSink {
            handle: Arc::new(handle.clone()),
        }))
    }

    fn block_on_any(&self, fut: BoxedBlocked) -> Box<dyn Any + Send> {
        // 复用宿主那份唯一的同步↔异步桥（ambient runtime + actix current_thread
        // 自锁规避都是实测产物，能力域不得复制第二份，见 ports 模块文档）
        crate::wasm_core::runtime_util::block_on_async(fut)
    }
}

/// 前端事件投递（`AppHandle::emit` 包装）
///
/// 失败语义与迁移前一致：流式路径逐 chunk emit 不上抛（一次投递失败不应中断整个
/// 响应），故此处按迁移前的 `let _ =` 处置。
struct AppHandleEventSink {
    handle: Arc<tauri::AppHandle>,
}

impl HttpEventSink for AppHandleEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        let _ = self.handle.emit(event, payload);
    }
}

/// 开机期装配能力域端口（幂等：重复装配被忽略，见 `install_ports`）
///
/// **两处登记**（各登一份等价对象，行为一致）：
/// - **进程级** [`bedcode_server_http::plugin_binding::install_ports`]：供插件实例
///   之外的非实例路径（停用回收转发）使用；
/// - **实例级** [`WasmHostContext::set_domain_ports`]：供插件实例经
///   [`bedcode_host_kit::ports::HostPorts::domain_ports`] 取回，使权限判定与出站
///   授权落在**本实例的**权限管理器 / 授权记录库上（多上下文场景下不会读到别人的）。
///
/// **必须早于任何插件激活**——guest 一调 `host-http` 原语就取端口，取不到直接
/// panic（fail-visible）。与 `set_services` / `set_task_engine` / `ws::install`
/// 同属两阶段注入的装配链。
pub fn install(ctx: Arc<WasmHostContext>) {
    let ports: Arc<dyn HttpPorts> = Arc::new(HostHttpPorts::from_ctx(Arc::clone(&ctx)));
    ctx.set_domain_ports(
        bedcode_server_http::plugin_binding::DOMAIN,
        Arc::new(Arc::clone(&ports)) as Arc<dyn std::any::Any + Send + Sync>,
    );
    bedcode_server_http::plugin_binding::install_ports(Arc::clone(&ports));
}

// ==================== 任务单元执行器（注册点留宿主） ====================

/// http 单元执行器（kind `http.fetch`）
///
/// **为什么执行器留宿主**：它注册进留 core 的任务引擎（`manager::task` 的执行器
/// 注册表），注册点必须与引擎同侧——能力域不持有任务引擎，也不该知道 kind 路由表。
/// 本类型只做「kind 匹配 + 取端口 + 转调能力域域函数」，执行体在
/// [`bedcode_server_http::plugin_binding::egress`]。params 原样透传，返回体与既有
/// `execute_unit` 语义一致：`value` 按原生值 JSON 编码。
pub(crate) struct HttpUnitExecutor;

impl UnitExecutor for HttpUnitExecutor {
    fn matches(&self, kind: &str) -> bool {
        kind == "http.fetch"
    }

    fn execute(
        &self,
        host_ctx: &Arc<WasmHostContext>,
        owner: &str,
        _kind: &str,
        params: &serde_json::Value,
    ) -> Result<Option<String>, String> {
        // 端口按本次调用的宿主上下文现造（执行器是**进程级注册**的进程级对象，
        // 不持有任何上下文）——语义等价于迁移前每次调用从 host_ctx 取三件套。
        let ports: Arc<dyn HttpPorts> = Arc::new(HostHttpPorts::from_ctx(Arc::clone(host_ctx)));
        // may_prompt = false：池线程绝不弹窗（与 fs 任务单元同款约束，见 egress 侧文档）
        match egress::http_fetch(&ports, owner, &params.to_string(), false) {
            Ok(opt) => Ok(opt.map(|v| serde_json::json!(v).to_string())),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 执行器注册面不变：`http.fetch` 归本执行器，其余 kind 不归
    #[test]
    fn unit_executor_claims_only_http_fetch_kind() {
        let executor = HttpUnitExecutor;
        assert!(executor.matches("http.fetch"));
        assert!(!executor.matches("fs.read"));
        assert!(!executor.matches("http.register-endpoint"));
    }
}
