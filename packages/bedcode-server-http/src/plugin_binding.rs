//! host-http 能力域 —— HTTP 协议能力（入站端点注册 2 条 + 出站 fetch 1 条）
//!
//! spec：`.scratch/2026-10-04-wasm-core-lib-split/spec.md` §3.1 / §7 D10；票 06
//! （自宿主 host_api 域的 http 实现整体迁入本 crate，入站 + 出站同 crate 两个模块）。
//!
//! **零业务代码红线（D1）**：本模块只做引擎原语——端点登记与属主仲裁、句柄铸造、
//! 权限门位置、载荷形状校验、出站请求执行；HTTP 语义之外的任何解读（会话、供应商
//! 协议、AI 格式…）全在插件侧。本域一律以 JSON 文本过界，不新增任何业务字段。
//!
//! ## 分层
//!
//! ```text
//!   本文件        入站机制 + WIT 接线（3 条原语的宿主实现 + 能力模块自报）
//!   egress.rs     出站机制（客户端池 / 跳转裁决 / 请求执行 / SSE 切分）
//!   ports.rs      边界：权限门 / 出站授权裁决 / 前端事件通道 / 异步桥
//! ```
//!
//! 宿主侧只剩一个 adapter（宿主 host_api 域的 http 适配器）与一次开机装配调用。
//!
//! ## 入站（服务端域，ABI v29）
//!
//! 插件在宿主 HTTP 服务器挂载端点：内部路径 `/api/plugin/<owner>/<path>`（命名空间
//! 段由宿主注入）+ 可选对外 URL 别名。端点表与冲突仲裁是**本 crate 既有**的
//! [`crate::registry`]（本域只做「权限门 + config 契约解析 + 属主仲裁」三件事，
//! 登记本身一次转发），fail-visible 语义不变：config 解析失败 / 形状非法 /
//! 冲突 → `Err`，**不覆盖在位者**。

use std::sync::Arc;

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api::permission::PERMISSION_NETWORK_HTTP;
use bedcode_plugin_api::EndpointAuth;
use serde::Deserialize;
use wasmtime::component::{bindgen, Linker};

use crate::plugin_binding::ports::HttpPorts;

/// 宿主能力端口（边界层；见 [`ports`] 模块文档）
pub mod ports;

/// 出站段（客户端池 / 跳转裁决 / 请求执行 / SSE 切分）
pub mod egress;

// ==================== 服务端域（ABI v29：插件动态路由注册） ====================

/// `register-endpoint` 的 config-json 契约（服务端域，camelCase）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EndpointRegistrationConfig {
    /// 插件内相对端点段（宿主拼出 `/api/plugin/<plugin-id>/<path>`）
    path: String,
    /// 可选对外 URL 别名（支持 `{id}` 模板段）
    #[serde(default)]
    host: Option<String>,
    /// host 别名的允许方法（缺省 `["GET"]`）
    #[serde(default)]
    methods: Vec<String>,
    /// 认证档位："jwt"（缺省，最严）| "none"（免凭证）
    #[serde(default)]
    auth: Option<String>,
}

/// 注册插件 HTTP 端点（WIT `host-http.register-endpoint`，ABI v29 服务端域）
///
/// 权限门 `network:http`（与前端面 `http.registerEndpoint` 同权限位）；config 解析
/// 失败 / 形状非法 / 冲突仲裁 → `Err`（fail-visible，不覆盖在位者）。
/// 成功 → 返回端点句柄 `http-<uuid>`。
pub fn http_register_endpoint(
    ports: &Arc<dyn HttpPorts>,
    plugin_id: &str,
    config_json: &str,
) -> Result<String, String> {
    if !ports.check_permission(plugin_id, PERMISSION_NETWORK_HTTP, "host_http_register_endpoint") {
        return Err("permission denied: network:http".to_string());
    }
    let config: EndpointRegistrationConfig =
        serde_json::from_str(config_json).map_err(|e| format!("http register-endpoint: invalid config: {e}"))?;
    // HTTP 面缺省档 = jwt（未声明即最严，与 manifest 声明面同一裁决）；
    // 未定义取值报错，绝不静默降级为较宽档位
    let auth = EndpointAuth::parse_with(config.auth.as_deref(), EndpointAuth::Jwt)
        .map_err(|e| format!("http register-endpoint: {e}"))?;
    let entry = crate::registry::register(plugin_id, &config.path, config.host.as_deref(), &config.methods, auth)?;
    Ok(entry.endpoint_id)
}

/// 注销插件 HTTP 端点（WIT `host-http.unregister-endpoint`，ABI v29 服务端域）
///
/// 属主仲裁：未知句柄 → `Ok(false)`（幂等）；他人句柄 → `Err`。
/// 插件停用时的自动回收另见 [`purge_for_plugin`]（该回收不设权限门——它是宿主
/// 生命周期动作，不是 guest 调用）。
pub fn http_unregister_endpoint(
    ports: &Arc<dyn HttpPorts>,
    plugin_id: &str,
    endpoint_id: &str,
) -> Result<bool, String> {
    if !ports.check_permission(plugin_id, PERMISSION_NETWORK_HTTP, "host_http_unregister_endpoint") {
        return Err("permission denied: network:http".to_string());
    }
    crate::registry::remove_if_owner(endpoint_id, plugin_id)
}

/// 回收指定插件的全部 HTTP 端点（插件停用/卸载时由宿主调用；只碰本人）
///
/// 不经权限门、不取端口：这是**宿主生命周期动作**（停用回收），与 guest 调用面
/// 不是一回事——停用时插件已不可调用任何原语。
pub fn purge_for_plugin(plugin_id: &str) -> usize {
    let entries = crate::registry::purge_for_plugin(plugin_id);
    entries.len()
}

// ==================== 能力模块自报 ====================

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 29`：入站服务端域两条原语在 ABI v29 追加，低于该版本的插件不导入
/// 本 interface 的服务端段。
const DESC: HostModuleDesc = HostModuleDesc {
    name: "http",
    interfaces: &["bedcode:plugin/host-http"],
    permissions: &["network:http"],
    abi_min: 29,
};

/// HTTP 能力域模块（`host-http`，3 条原语）
pub struct HttpModule;

impl HostModule for HttpModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_http::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与宿主既有接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: HttpModule = HttpModule;

// 能力模块自报（linker-section 静态）
//
// **依赖前提**：宿主必须有一行强制引用本 crate（见宿主的组件运行时接线处），
// 否则本 rlib 不进最终二进制、静态不执行 ⇒ 注册丢失，且 guest 会在实例化期报
// 「无该 import」。
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

/// 能力域名（宿主上下文里的键；[`bedcode_host_kit::ports::HostPorts::domain_ports`]）
pub const DOMAIN: &str = "http";

/// 装配端口的便捷入口（宿主开机期调用）
pub fn install<P: HttpPorts + 'static>(ports: P) {
    ports::install_ports(Arc::new(ports));
}

/// [`ports::install_ports`] 的再导出（宿主 adapter 需要直接装**已构造好的**端口对象：
/// 同一份要同时登记进程级与实例级，不能经 `install` 新建）
pub use ports::install_ports;

/// 取本插件实例该用的端口：**实例级优先**，未装配则回落到进程级装配
///
/// 为什么要两级（见 `bedcode_host_kit::ports` 模块文档）：进程级只有一格，而一个
/// 进程可以有多份宿主上下文（无头测试每个用例一份）；实例级让端口与**本实例的**
/// 权限管理器 / 出站授权闸门绑定，能力域代码不感知上下文数量。
///
/// 宿主注入的是 `Arc<dyn Any>` 包着的 `Arc<dyn HttpPorts>`（能力域的端口类型只有
/// 能力域自己认识，kit 与宿主都不能把它裸存进表），故这里向下转型后**克隆内层
/// Arc**（同形对象，多个实例共享一份 adapter，无副作用）。
pub fn ports_for(state: &WasmPluginState) -> Arc<dyn HttpPorts> {
    match state
        .host
        .domain_ports(DOMAIN)
        .and_then(bedcode_host_kit::ports::downcast_domain_ports::<Arc<dyn HttpPorts>>)
    {
        Some(ports) => Arc::clone(&ports),
        None => ports::ports(),
    }
}

bindgen!({
    // provider 侧绑定：宿主自己的 `bedcode` 模块是 **guest 视角**（import 是调用
    // 函数，不是 `Host` trait + `add_to_linker`），能力 crate 要自己装配就必须生成
    // provider 侧。
    //
    // ⚠️ 由此产生的**硬约束**：本 crate 与宿主各自生成的
    // `bedcode::plugin::host_http::Host` 是**同名但不同类型**的 trait。宿主必须
    // 同时删掉自己的 http `Host` impl 与 `add_to_linker` 行，否则同一个 interface
    // 被注册两次 → 装配期 `defined twice`。
    path: "../../bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin",
    // 与宿主同款：全部导出绑定生成 async 变体（wasmtime async store 要求）
    exports: { default: async },
});

// ==================== 宿主绑定层（Host trait 实现） ====================
//
// 每个接口方法 = 一条 WIT 原语。权限门在本 crate 内的域函数里（随实现同迁，
// 经端口问宿主结果），此层只做「取端口 → 转调 → 按 WIT `result` 形状返回」。

impl bedcode::plugin::host_http::Host for WasmPluginState {
    fn fetch(&mut self, request_json: String) -> Result<Option<String>, String> {
        // may_prompt = true：插件主流程（guest 调用栈）可弹窗询问出站授权
        egress::http_fetch(&ports_for(self), &self.plugin_id, &request_json, true)
    }

    // v29 服务端域：插件动态 HTTP 路由注册（权限门 + 注册表仲裁在同 crate 内）
    fn register_endpoint(&mut self, config_json: String) -> Result<String, String> {
        http_register_endpoint(&ports_for(self), &self.plugin_id, &config_json)
    }

    fn unregister_endpoint(&mut self, endpoint_id: String) -> Result<bool, String> {
        http_unregister_endpoint(&ports_for(self), &self.plugin_id, &endpoint_id)
    }
}

#[cfg(test)]
mod tests;
