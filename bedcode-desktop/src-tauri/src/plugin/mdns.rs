//! host-mdns 宿主侧适配器（能力域端口 `DiscoveryPorts` 的宿主实现 + 装配自报）
//!
//! 域机制（共享守护 + 浏览 / 广播句柄表 + 事件 + 停用回收）在
//! `bedcode-discovery-engine`；本文件是**宿主侧的另一半**——wasm-core 纯净性收口票 02
//! 批次 03 整文件迁自 `bedcode-wasm-core/src/host_api/mdns.rs`（内核自此不依赖
//! `bedcode-discovery-engine`；`host_context_registry` 一并退役——它唯一消费者就是
//! 本 adapter 的旧零大小形态）。
//!
//! 四件同处（与 pty 样板一致）：端口实现 / `install` 装配自报 / 白名单声明 +
//! 强制引用行 / 装配器静态。
//!
//! ## 与旧形态的两处结构差异
//!
//! 1. **端口持上下文**（旧版是零大小类型 + 进程级注册表按调用取 `WasmHostContext`）：
//!    现在与 pty 同形——字段 `Arc<dyn HostPorts>`，调用时 [`downcast_host`] 还原；
//!    注册表退役后不再有「全局单例取上下文」这条路径。
//! 2. **停用回收经自报钩子**（`bedcode-discovery-engine::HOOKS` 的 `on_plugin_purge`）：
//!    旧版由内核 `manager/host/activation.rs` 直调 `host_api::mdns::purge_for_plugin`
//!    （内核点名该域）；现在与 pty 同款走 `DomainHooksRegistry` 遍历。
//!
//! ## 五条原语的能力路由
//!
//! 路由判据（系统组件提供者优先，无提供者回落本域引擎）**仍只有内核一份**：
//! 经 `bedcode_wasm_core::host_api::forward_mdns_*` 再导出调用（实现在内核
//! `manager/capability.rs` 的闭表），宿主不得复制判据。未装配路由表（无头）与
//! 「有表无提供者」同形（`None` → 能力域走本域引擎）。

use std::sync::Arc;

use bedcode_discovery_engine::ports::BoxedTask;
use bedcode_discovery_engine::{DiscoveryPorts, DiscoveryTask};
use bedcode_host_kit::ports::{downcast_host, HostPorts};
use bedcode_plugin_api::permission::PERMISSION_MDNS;
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::WasmHostContext;

// 能力模块白名单条目与强制引用行**必须同处**（本文件），两者不漂移——漏引用 ⇒
// 该 crate 的 inventory 自报静态不进最终二进制（装载期 missing 方向点名）；漏白名单
// ⇒ unlisted 方向点名。判据与纪律见 `bedcode_host_kit::assembly` 模块文档。
bedcode_host_kit::expect_host_module!(bedcode_discovery_engine::MODULE_NAME);
use bedcode_discovery_engine as _;

/// 端口的宿主实现
///
/// 迁移前是零大小类型（浏览器事件循环要长期持有 `'static` 端口，故经进程级注册表
/// 按调用取上下文）；现在直接持擦除后的宿主上下文——同样是 `'static`，且不再有
/// 「全局只有一格上下文」的限制。
pub struct HostDiscoveryPorts {
    /// 宿主上下文（擦除形态；`ctx()` 向下转型取具体类型）
    ctx: Arc<dyn HostPorts>,
}

impl HostDiscoveryPorts {
    /// 端口（生产 / 测试）：从宿主上下文构造
    pub fn from_ctx(ctx: Arc<WasmHostContext>) -> Self {
        Self { ctx }
    }

    /// 向下转型回具体宿主上下文（类型不符即 panic：装配期编程错误，fail-visible）
    fn ctx(&self) -> &WasmHostContext {
        downcast_host::<WasmHostContext>(self.ctx.as_ref())
    }
}

impl DiscoveryPorts for HostDiscoveryPorts {
    fn check_permission(&self, plugin_id: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3「安全闸门」——闸门不应可插拔）。复用
        // wasm-core 的 `host_api::check_permission`：同一份 PermissionManager、同一条
        // 拒绝 warn 路径（AGENTS §8 结构化字段），不另起一套判定。
        check_permission(self.ctx(), plugin_id, PERMISSION_MDNS, api)
    }

    fn local_node_id(&self) -> Option<String> {
        // `app_handle()` 在无头环境为 None ⇒ 返回 None ⇒ 能力域按「无法比对即不拦
        // 自播」口径处理；对等网络上下文经端口取（无头 / 测试未注入 → None，同口径）
        let ctx = self.ctx();
        let app = ctx.app_handle()?;
        let provider = ctx.peer_ctx_provider()?;
        let peer_ctx = provider(app);
        bedcode_server_peer_net::current_node_id(&peer_ctx)
    }

    fn publish(&self, topic: &str, payload: serde_json::Value) {
        // sender 恒为 `host`（机制常量）：总线负责命名空间门禁与投递
        self.ctx().message_bus.publish(topic, "host", payload);
    }

    fn spawn(&self, task: BoxedTask) -> Arc<dyn DiscoveryTask> {
        Arc::new(HostDiscoveryTask {
            handle: tauri::async_runtime::spawn(task),
        })
    }

    // ==================== 能力路由（票 09 扩表） ====================
    //
    // 五条原语各问一次路由层：命中系统组件提供者就转发（组件侧再经 `host-mdns`
    // 同形导出），没命中返回 `None` 让能力域走本域引擎。
    //
    // 能力名取自**词汇真源**（`bedcode_discovery_engine::routing::CAPABILITY`，票 02
    // 批次 03）：内核的 `forward_mdns_*` 不再是字面量的出处，只吃调用方给的能力名。

    fn forward_mdns_browse(&self, plugin_id: &str, service_type: &str) -> Option<Result<String, String>> {
        bedcode_wasm_core::host_api::forward_mdns_browse(
            self.ctx(),
            bedcode_discovery_engine::routing::CAPABILITY,
            plugin_id,
            service_type,
        )
    }

    fn forward_mdns_stop_browse(&self, plugin_id: &str, browser_id: &str) -> Option<Result<bool, String>> {
        bedcode_wasm_core::host_api::forward_mdns_stop_browse(
            self.ctx(),
            bedcode_discovery_engine::routing::CAPABILITY,
            plugin_id,
            browser_id,
        )
    }

    fn forward_mdns_advertise(&self, plugin_id: &str, config_json: &str) -> Option<Result<String, String>> {
        bedcode_wasm_core::host_api::forward_mdns_advertise(
            self.ctx(),
            bedcode_discovery_engine::routing::CAPABILITY,
            plugin_id,
            config_json,
        )
    }

    fn forward_mdns_stop_advertise(&self, plugin_id: &str, advertise_id: &str) -> Option<Result<bool, String>> {
        bedcode_wasm_core::host_api::forward_mdns_stop_advertise(
            self.ctx(),
            bedcode_discovery_engine::routing::CAPABILITY,
            plugin_id,
            advertise_id,
        )
    }

    fn forward_mdns_is_advertising(&self, plugin_id: &str, advertise_id: &str) -> Option<Result<bool, String>> {
        bedcode_wasm_core::host_api::forward_mdns_is_advertising(
            self.ctx(),
            bedcode_discovery_engine::routing::CAPABILITY,
            plugin_id,
            advertise_id,
        )
    }
}

/// 后台任务句柄的宿主实现（`cancel` ⇒ `abort`，与迁移前逐字一致）
struct HostDiscoveryTask {
    handle: tauri::async_runtime::JoinHandle<()>,
}

impl DiscoveryTask for HostDiscoveryTask {
    fn cancel(&self) {
        self.handle.abort();
    }
}

/// 装配回调（host-kit 自报）：开机期装本域端口
///
/// **只登记进程级一格**（`bedcode_discovery_engine::install_ports` 的 `OnceLock`）：
/// 本域没有 `DOMAIN` / 实例级 `domain_ports` 通道（与迁移前逐字一致——旧版也是
/// 进程级唯一装配）。**必须早于任何插件激活**——浏览事件循环一开就取端口，
/// 取不到直接 panic（fail-visible）。
fn install(host: Arc<dyn HostPorts>) {
    bedcode_discovery_engine::install(HostDiscoveryPorts { ctx: host });
}

/// 端口装配器静态（与白名单条目 / 强制引用行同处；内核装配链遍历本自报表）
pub static MDNS_PORTS_INSTALLER: bedcode_host_kit::DomainPortsInstaller =
    bedcode_host_kit::DomainPortsInstaller {
        name: bedcode_discovery_engine::MODULE_NAME,
        install,
    };

bedcode_host_kit::submit_domain_ports_installer!(MDNS_PORTS_INSTALLER);

#[cfg(test)]
mod tests {
    // 本模块的用例全部经**跨 crate 的公开面**断言（能力域描述符、host-kit 注册表、
    // 内核种子），故不引入 `super::*`——那会让「用例只验公开面」这条纪律失去证据。
    /// 白名单声明、接口路径、权限位三件与能力域描述符逐字一致
    ///
    /// `expected_host_modules()` 含本模块名即证明 `expect_host_module!` 行仍在且被收集
    /// （漏了 ⇒ 装载期 unlisted 点名；本用例让它在单测就红）。
    #[test]
    fn host_module_declaration_matches_capability_domain_desc() {
        let module_name = bedcode_discovery_engine::MODULE_NAME;
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&module_name),
            "能力模块白名单缺 {module_name}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(module_name, "discovery", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            bedcode_discovery_engine::MODULE_INTERFACES,
            &["bedcode:plugin/host-mdns"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            bedcode_discovery_engine::MODULE_PERMISSIONS,
            &["network:mdns"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致（装载期一致性核对用）"
        );
    }

    /// 装配器静态被内核装配链遍历到（自报 → 装 → 取回，三段闭环）
    ///
    /// `setup_wasm_runtime` 走生产同一装配入口（`install_capability_domain_ports`）；
    /// 之后 `ports::ports()` 必然可取（未装配即 panic——fail-visible）。漏
    /// `submit_domain_ports_installer!` 行则本用例在取端口时 panic 报装配链缺该域。
    #[test]
    fn submitted_installer_is_wired_into_the_boot_chain() {
        let (_, _ctx) = bedcode_wasm_core::test_support::setup_wasm_runtime();
        // 取回端口即证明安装过（内部 panic 文案会点名「host must call install_ports」）
        let ports = bedcode_discovery_engine::ports::ports();
        // 点位：端口的权限门是同一条宿主判定路径（未授权插件必拒——fail-closed）
        assert!(
            !ports.check_permission("com.bedcode.never-granted", "host_mdns.test"),
            "未授权插件必须被权限门拒绝（复用宿主 check_permission 的 fail-safe 分支）"
        );
    }

    /// 路由词汇必须真的自报进宿主二进制（票 02 批次 03）
    ///
    /// 漏 `submit_routable_capability!` 行 / 漏本文件顶部的强制引用行 ⇒ 内核路由表里
    /// 没有本能力 ⇒ 提供者探测永远不命中，**系统组件静默接管不了 `host-mdns`**。
    /// 内核侧测试做不到这一向（内核测试二进制不链能力域 ⇒ 自报集为空），只能在宿主
    /// 二进制里断言。
    #[test]
    fn route_is_self_reported_into_the_host_binary() {
        let routes = bedcode_host_kit::collected_routable_capabilities();
        let route = routes
            .iter()
            .find(|route| route.capability == bedcode_discovery_engine::routing::CAPABILITY)
            .expect("宿主二进制里应有本域自报的路由词汇（漏 submit! / 漏强制引用行）");
        assert_eq!(
            route.forward_prefix,
            bedcode_discovery_engine::routing::FORWARD_PREFIX
        );
        assert_eq!(route.exports, bedcode_discovery_engine::routing::EXPORTS);
    }

    /// 反向闭表锁：内核提供者窄端口的方法族 == 域自报的导出表（逐项）
    ///
    /// 内核侧那条锁只能做「行 → 方法」方向（见 `manager::capability` 的闭表锁文档）；
    /// 「方法 → 行」的孤儿判定需要自报行全在场 ⇒ 在宿主二进制里做（本用例）。
    #[test]
    fn mdns_target_method_family_matches_the_self_reported_exports() {
        let ctx_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/bedcode-wasm-core/src/host_api/context.rs");
        let ctx = std::fs::read_to_string(&ctx_path)
            .unwrap_or_else(|e| panic!("读内核 CapabilityTarget 声明 {}: {e}", ctx_path.display()));
        let start = ctx.find("pub trait CapabilityTarget").expect("trait 必须存在");
        let block = &ctx[start..];
        let end = block.find("\n}\n").expect("trait 必须以列零 `}` 收尾");
        // marker 对的是**剥掉 `fn ` 之后的**方法名前缀（`mdns_`）——首版写成
        // `fn mdns_` 与 `strip_prefix("fn ")` 叠加后恒不匹配（计数恒 0、锁恒红，
        // 由本用例首次实跑暴露；宿主测试此前受磁盘限制未跑，参见票面记录）。
        let marker = format!("{}_", bedcode_discovery_engine::routing::FORWARD_PREFIX);
        let decls = block[..end]
            .lines()
            .filter_map(|line| line.trim().strip_prefix("fn "))
            .filter(|rest| rest.starts_with(&marker))
            .count();
        assert_eq!(
            decls,
            bedcode_discovery_engine::routing::EXPORTS.len(),
            "内核 `{marker}*` 方法族与域自报导出表逐项对应（一条导出一个方法，不多不少）"
        );
    }
}
