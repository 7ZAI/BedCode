//! host-peer 宿主侧适配器（能力域端口 `PeerPorts` 的宿主实现 + 装配自报）
//!
//! 域机制（19 条 `host-peer` 原语 + 引擎收发/远端/传输三面）在
//! `bedcode-server-peer-net`；本文件是**宿主侧的另一半**——wasm-core 纯净性收口票 02
//! 批次 03 整文件迁自 `bedcode-wasm-core/src/host_api/peer.rs`（内核自此不点名本域、
//! 端口装配只有本文件一条来源）。
//!
//! **与 pty / mdns 的差异（Cargo 边保留）**：内核仍消费该 crate 的**引擎面**——
//! `manager/host/activation.rs::release_node_for`（停用回收释放节点）与
//! `host_api::context::PeerCtxProvider` 的 `PeerCtx` 类型。那些不是能力域端口，
//! 故 `bedcode-wasm-core → bedcode-server-peer-net` 这条边**本批次不摘**（摘它要先
//! 把引擎面消费点搬出内核，属另一件事）。
//!
//! 四件同处（与 pty / mdns 样板一致）：端口实现 / `install` 装配自报 / 白名单声明 +
//! 强制引用行 / 装配器静态。
//!
//! ## 哪些机制**刻意留在宿主**（AGENTS §5.1.3 允许的薄壳）
//!
//! - **权限门**：复用 wasm-core 的 `host_api::check_permission`，同一份 PermissionManager、
//!   同一条拒绝 warn 路径（AGENTS §8 结构化字段）；
//! - **引擎上下文的装配动作**：`peer_ctx` 由 `PeerCtxProvider` 端口给出（lib 注入
//!   `peer_net_cmds::peer_ctx`），未注入（无头 / 测试）与无 `app_handle` 同报
//!   `HEADLESS_UNAVAILABLE`——本 crate 的 `peer_net_cmds` 仍是唯一认识 `AppHandle`
//!   的引擎装配点，但那是**本文件**的依赖，不是能力域的；
//! - **同步↔异步桥**：复用宿主那份唯一的 `runtime_util::block_on_async`（ambient
//!   runtime + actix current_thread 自锁规避都是实测产物，能力域不得复制第二份）。
//!
//! ## 判定顺序是被断言的行为（勿调换）
//!
//! 属主判定刻意排在取 `peer_ctx` 之前（在能力域内）：非属主的一次 `close` 试探在
//! 无头 / 引擎未就绪的环境下也必须得到同一个答案（属主拒绝），否则「先报 headless」
//! 会把越权探测的结果随机化。迁移前后逐字保留。
//!
//! ## 端口句柄的形态
//!
//! 字段存擦除后的 `Arc<dyn HostPorts>`（host-kit 只能命名它），每次调用
//! [`downcast_host`] 还原 `WasmHostContext`；类型不符即 panic（装配期编程错误，
//! fail-visible）。与 pty / mdns 同形。

use std::any::Any;
use std::sync::Arc;

use bedcode_host_kit::ports::{downcast_host, HostPorts};
use bedcode_server_peer_net::plugin_binding::ports::{BoxedBlocked, PeerPorts, HEADLESS_UNAVAILABLE};
use bedcode_server_peer_net::PeerCtx;
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::WasmHostContext;

// 能力模块白名单条目与强制引用行**必须同处**（本文件），两者不漂移——漏引用 ⇒
// 该 crate 的 inventory 自报静态不进最终二进制（装载期 missing 方向点名）；漏白名单
// ⇒ unlisted 方向点名。判据与纪律见 `bedcode_host_kit::assembly` 模块文档。
bedcode_host_kit::expect_host_module!(bedcode_server_peer_net::plugin_binding::MODULE_NAME);
use bedcode_server_peer_net as _;

/// 端口的宿主实现
///
/// 生产路径在**开机期**捕获宿主上下文（`PluginHost::new` 已建好），之后每条原语两次
/// 取端口：一次权限门、一次引擎上下文。
pub struct HostPeerPorts {
    /// 宿主上下文（擦除形态；`ctx()` 向下转型取具体类型）
    ctx: Arc<dyn HostPorts>,
}

impl HostPeerPorts {
    /// 端口（生产 / 测试）：从宿主上下文构造
    pub fn from_ctx(ctx: Arc<WasmHostContext>) -> Self {
        Self { ctx }
    }

    /// 向下转型回具体宿主上下文（类型不符即 panic：装配期编程错误，fail-visible）
    fn ctx(&self) -> &WasmHostContext {
        downcast_host::<WasmHostContext>(self.ctx.as_ref())
    }
}

impl PeerPorts for HostPeerPorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3「安全闸门」——闸门不应可插拔）。复用 wasm-core 的
        // `host_api::check_permission`：同一份 PermissionManager、同一条拒绝 warn 路径，
        // 拿不到权限管理器时它返回 false（fail-safe），能力域据此回权限错误。
        check_permission(self.ctx(), plugin_id, permission, api)
    }

    fn peer_ctx(&self) -> Result<Arc<PeerCtx>, String> {
        // 引擎上下文的**装配动作**留宿主（见模块文档第 2 条），无头口径与文案由能力域定义
        let ctx = self.ctx();
        let app = ctx.app_handle().ok_or_else(|| HEADLESS_UNAVAILABLE.to_string())?;
        let provider = ctx
            .peer_ctx_provider()
            .ok_or_else(|| HEADLESS_UNAVAILABLE.to_string())?;
        Ok(provider(app))
    }

    fn block_on_any(&self, fut: BoxedBlocked) -> Box<dyn Any + Send> {
        // 复用宿主那份唯一的同步↔异步桥（见模块文档第 3 条）
        bedcode_wasm_core::runtime_util::block_on_async(fut)
    }
}

/// 装配回调（host-kit 自报）：开机期装本域端口
///
/// **两处登记**（各登一份等价对象，行为一致）：
/// - **实例级** `WasmHostContext::set_domain_ports`：供插件实例经
///   `bedcode_host_kit::ports::HostPorts::domain_ports` 取回，使权限判定落在**本实例的**
///   PermissionManager 上（多上下文场景下不会读到别人的授权）；
/// - **进程级** `bedcode_server_peer_net::plugin_binding::install_ports`：供插件实例
///   之外的非实例路径使用。
///
/// **必须早于任何插件激活**（激活发生在 `PluginHost::new` 内部）——内核装配链
/// （`install_capability_domain_ports`）遍历自报表时调用；guest 一调 `host-peer`
/// 原语就取端口，取不到直接 panic（fail-visible）。
fn install(host: Arc<dyn HostPorts>) {
    let ctx = downcast_host::<WasmHostContext>(host.as_ref());
    // 端口对象持一份引用（`ctx` 的向下转型借用仍在用 host，故不移动它）
    let ports: Arc<dyn PeerPorts> = Arc::new(HostPeerPorts { ctx: Arc::clone(&host) });
    ctx.set_domain_ports(
        bedcode_server_peer_net::plugin_binding::DOMAIN,
        Arc::new(Arc::clone(&ports)) as Arc<dyn Any + Send + Sync>,
    );
    bedcode_server_peer_net::plugin_binding::install_ports(ports);
}

/// 端口装配器静态（与白名单条目 / 强制引用行同处；内核装配链遍历本自报表）
pub static PEER_PORTS_INSTALLER: bedcode_host_kit::DomainPortsInstaller =
    bedcode_host_kit::DomainPortsInstaller {
        name: bedcode_server_peer_net::plugin_binding::MODULE_NAME,
        install,
    };

bedcode_host_kit::submit_domain_ports_installer!(PEER_PORTS_INSTALLER);

#[cfg(test)]
mod tests {
    // 用例全部经**跨 crate 的公开面**断言（能力域描述符 / host-kit 注册表 / 内核种子），
    // 故不引入 `super::*`。
    /// 白名单声明、接口路径、权限位三件与能力域描述符逐字一致
    #[test]
    fn host_module_declaration_matches_capability_domain_desc() {
        let module_name = bedcode_server_peer_net::plugin_binding::MODULE_NAME;
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&module_name),
            "能力模块白名单缺 {module_name}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(module_name, "peer-net", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            bedcode_server_peer_net::plugin_binding::MODULE_INTERFACES,
            &["bedcode:plugin/host-peer"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            bedcode_server_peer_net::plugin_binding::MODULE_PERMISSIONS,
            &["peer"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致"
        );
    }

    /// 装配器静态被内核装配链遍历到（自报 → 装 → 取回，三段闭环）
    #[test]
    fn submitted_installer_is_wired_into_the_boot_chain() {
        let (_, _ctx) = bedcode_wasm_core::test_support::setup_wasm_runtime();
        // 取回端口即证明安装过（内部 panic 文案会点名「host must call install_ports」）
        let ports = bedcode_server_peer_net::plugin_binding::ports::ports();
        // 点位：无头环境取不到引擎上下文 ⇒ 必须是能力域定义的 headless 口径，而不是别的错
        let err = ports
            .peer_ctx()
            .err()
            .expect("无头夹具下 peer_ctx 必取不到（未注入 PeerCtxProvider）");
        assert_eq!(
            err,
            bedcode_server_peer_net::plugin_binding::ports::HEADLESS_UNAVAILABLE,
            "无头口径文案由能力域定义，宿主侧只汇报「取不到」"
        );
    }
}
