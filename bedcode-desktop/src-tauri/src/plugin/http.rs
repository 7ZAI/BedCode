//! host-http 宿主侧适配器（能力域端口 `HttpPorts` 的宿主实现 + 装配自报）
//!
//! 域机制（入站端点注册表 / 出站 fetch 的执行体 / 跳转与响应体裁决）在
//! `bedcode-server-http`；本文件是**宿主侧的另一半**——wasm-core 纯净性收口票 02
//! 批次 03 整段迁自 `bedcode-wasm-core/src/host_api/http.rs`。
//!
//! 四件同处（与 pty / mdns / peer / ws 样板一致）：端口实现 / `install` 装配自报 /
//! 白名单声明 + 强制引用行 / 装配器静态。
//!
//! 内核侧 `host_api/http.rs` **只剩** `HttpUnitExecutor`（host-task 单元执行器：
//! 注册点必须与留 core 的任务引擎同侧，随票 02 批次 05 统一处置）；它执行
//! `http.fetch` 时经**实例级端口下发通道**（`HostPorts::domain_ports`）取本文件装入
//! 的端口，不再自行构造 adapter。
//!
//! ## 哪些机制**刻意留在宿主**（AGENTS §5.1.3 允许的薄壳）
//!
//! - **权限门**（`network:http`）：复用 wasm-core 的 `host_api::check_permission`（同一
//!   份 PermissionManager、同一条拒绝 warn 路径）；
//! - **出站授权闸门整条链**（授权记录库 / 档位策略 / 弹窗询问面）：闸门不应可插拔；
//! - **前端事件面**：`AppHandle::emit` 包装（无 AppHandle ⇒ `event_sink` 返回 `None`，
//!   由能力域对无头流式请求显性报错，不静默丢 chunk）；
//! - **同步↔异步桥**：`runtime_util::block_on_async`（ambient runtime + actix
//!   current_thread 自锁规避都是实测产物，能力域不得复制第二份）。
//!
//! ## 端口句柄的形态
//!
//! 与 pty / mdns / peer / ws 同形：字段存擦除后的 `Arc<dyn HostPorts>`（host-kit 只能
//! 命名它），每次调用 [`downcast_host`] 还原 `WasmHostContext`；类型不符即 panic
//! （装配期编程错误，fail-visible，不静默降级）。

use std::any::Any;
use std::sync::Arc;

use bedcode_host_kit::ports::{downcast_host, HostPorts};
use bedcode_server_http::plugin_binding::ports::{BoxedBlocked, HttpEventSink, HttpPorts, OutboundAuth};

use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::WasmHostContext;

// 能力模块白名单条目与强制引用行**必须同处**（本文件），两者不漂移——漏引用 ⇒
// 该 crate 的 inventory 自报静态不进最终二进制（装载期 missing 方向点名）；漏白名单
// ⇒ unlisted 方向点名。判据与纪律见 `bedcode_host_kit::assembly` 模块文档。
bedcode_host_kit::expect_host_module!(bedcode_server_http::plugin_binding::MODULE_NAME);
use bedcode_server_http as _;

/// 端口的宿主实现
///
/// 生产路径在**开机期**捕获宿主上下文（`PluginHost::new` 已建好），之后每条原语
/// 零查表地取权限门 / 出站授权 / 事件通道（转型见模块文档）。
pub struct HostHttpPorts {
    /// 宿主上下文（擦除形态；`ctx()` 向下转型取具体类型）
    ctx: Arc<dyn HostPorts>,
}

impl HostHttpPorts {
    /// 端口（生产 / 测试）：从宿主上下文构造
    pub fn from_ctx(ctx: Arc<WasmHostContext>) -> Self {
        Self { ctx }
    }

    /// 向下转型回具体宿主上下文（类型不符即 panic：装配期编程错误，见模块文档）
    fn ctx(&self) -> &WasmHostContext {
        downcast_host::<WasmHostContext>(self.ctx.as_ref())
    }
}

impl HttpPorts for HostHttpPorts {
    fn check_permission(&self, plugin_id: &str, permission: &str, api: &str) -> bool {
        // 权限门留宿主（AGENTS §5.1.3「安全闸门」——闸门不应可插拔）。复用既有
        // `host_api::check_permission`：同一份 PermissionManager、同一条拒绝 warn
        // 路径（AGENTS §8 结构化字段），不另起一套判定。
        check_permission(self.ctx(), plugin_id, permission, api)
    }

    fn authorize_outbound(&self, plugin_id: &str, url: &str, may_prompt: bool) -> OutboundAuth {
        // 出站授权闸门整条链留宿主（票 06）。`may_prompt = false` 走「只认记录、
        // 绝不弹窗」那一支——弹窗会占住任务池槽位最短 30s，且用户在错误的时机
        // 看到问题（与 fs 任务单元同款约束）。
        let checker = self.ctx().net_auth();
        let verdict = bedcode_wasm_core::runtime_util::block_on_async(async {
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
        let handle = self.ctx().app_handle()?;
        Some(Arc::new(AppHandleEventSink {
            handle: Arc::new(handle.clone()),
        }))
    }

    fn block_on_any(&self, fut: BoxedBlocked) -> Box<dyn Any + Send> {
        // 复用宿主那份唯一的同步↔异步桥（ambient runtime + actix current_thread
        // 自锁规避都是实测产物，能力域不得复制第二份，见 ports 模块文档）
        bedcode_wasm_core::runtime_util::block_on_async(fut)
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

/// 装配回调（host-kit 自报）：开机期装本域端口
///
/// **两处登记**（各登一份等价对象，行为一致）：
/// - **实例级** `WasmHostContext::set_domain_ports`：供插件实例经
///   `bedcode_host_kit::ports::HostPorts::domain_ports` 取回（能力域 `ports_for` 的第一
///   来源；内核 `HttpUnitExecutor` 取端口也走这条），使权限判定与出站授权落在**本实例
///   的**权限管理器 / 授权记录库上；
/// - **进程级** `bedcode_server_http::plugin_binding::install_ports`：供插件实例之外的
///   非实例路径（停用回收转发）使用。
///
/// **必须早于任何插件激活**——内核装配链（`install_capability_domain_ports`）遍历
/// 自报表时调用；guest 一调 `host-http` 原语就取端口，取不到直接 panic（fail-visible）。
fn install(host: Arc<dyn HostPorts>) {
    let ctx = downcast_host::<WasmHostContext>(host.as_ref());
    // 端口对象持一份引用（`ctx` 的向下转型借用仍在用 host，故不移动它）
    let ports: Arc<dyn HttpPorts> = Arc::new(HostHttpPorts { ctx: Arc::clone(&host) });
    ctx.set_domain_ports(
        bedcode_server_http::plugin_binding::DOMAIN,
        Arc::new(Arc::clone(&ports)) as Arc<dyn Any + Send + Sync>,
    );
    bedcode_server_http::plugin_binding::install_ports(ports);
}

/// 端口装配器静态（与白名单条目 / 强制引用行同处；内核装配链遍历本自报表）
pub static HTTP_PORTS_INSTALLER: bedcode_host_kit::DomainPortsInstaller =
    bedcode_host_kit::DomainPortsInstaller {
        name: bedcode_server_http::plugin_binding::MODULE_NAME,
        install,
    };

bedcode_host_kit::submit_domain_ports_installer!(HTTP_PORTS_INSTALLER);

#[cfg(test)]
mod tests {
    use super::*;

    /// 白名单声明、接口路径、权限位三件与能力域描述符逐字一致
    ///
    /// `expected_host_modules()` 含本模块名即证明 `expect_host_module!` 行仍在且被收集
    /// （漏了 ⇒ 装载期 unlisted 点名；本用例让它在单测就红）。
    #[test]
    fn host_module_declaration_matches_capability_domain_desc() {
        let module_name = bedcode_server_http::plugin_binding::MODULE_NAME;
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&module_name),
            "能力模块白名单缺 {module_name}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(module_name, "http", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            bedcode_server_http::plugin_binding::MODULE_INTERFACES,
            &["bedcode:plugin/host-http", "bedcode:plugin/host-http-endpoint"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配；票 04 拆出服务端域）"
        );
        assert_eq!(
            bedcode_server_http::plugin_binding::MODULE_PERMISSIONS,
            &["network:http"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致（装载期一致性核对用）"
        );
    }

    /// 装配器静态被内核装配链遍历到（自报 → 装 → 取回，三段闭环）
    ///
    /// 两条通道都断言：进程级 `ports()` 可取回；实例级 `domain_ports` 也装了同一份
    /// ——后者正是内核 `HttpUnitExecutor` 执行 `http.fetch` 时的取端口路径。
    #[test]
    fn submitted_installer_is_wired_into_the_boot_chain() {
        let (_, ctx) = bedcode_wasm_core::test_support::setup_wasm_runtime();
        // 进程级：取回端口即证明安装过（内部 panic 文案会点名「host must call install_ports」）
        let ports = bedcode_server_http::plugin_binding::ports::ports();
        // 实例级：容器在场且能还原成域端口（缺了 ⇒ 单元执行器执行期显性报错）
        let raw = ctx
            .domain_ports(bedcode_server_http::plugin_binding::DOMAIN)
            .expect("宿主 adapter 必须在实例上下文上登记端口（HttpUnitExecutor 靠它取端口）");
        assert!(
            bedcode_host_kit::ports::downcast_domain_ports::<Arc<dyn HttpPorts>>(raw).is_some(),
            "实例级通道里装的必须是 `Arc<dyn HttpPorts>` 容器"
        );
        // 点位：权限门对未授权插件 fail-closed（复用宿主 check_permission 的 fail-safe 分支）
        assert!(
            !ports.check_permission("com.bedcode.never-granted", "network:http", "host_http.test"),
            "未授权插件必须被权限门拒绝"
        );
    }
}
