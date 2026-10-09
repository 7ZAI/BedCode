//! host-mdns 能力路由词汇（域自持；票 02 批次 03）
//!
//! ## 为什么在域侧
//!
//! `host-mdns` 的能力名、三层方法族前缀与 5 条导出函数名都是 **WIT 接口的词汇**——
//! 只有本域知道自己导出什么。此前它们写在内核 `manager/capability.rs` 的 `const`
//! 闭表里（`CAP_HOST_MDNS` / `EXPORT_MDNS_*` / `FORWARD_MDNS`）：接口改名要改内核，
//! 而本 crate 早已从内核依赖图摘出（批次 03），内核连引用都做不到。
//!
//! 现在：本域经 `bedcode_host_kit::submit_routable_capability!` 自报（与能力模块
//! 自报 / 生命周期钩子自报同范式），内核收集后并入路由表——提供者探测（「组件是否
//! 提供该能力」按 [`EXPORTS`] 全命中判定）与 `is_routable` 判据都用它。
//!
//! ## 调用面仍在内核（刻意，不是遗漏）
//!
//! 提供者实例是 wasmtime 组件，调它的导出必须**编译期具名**：内核
//! `CapabilityTarget::mdns_*`（5 方法）+ `forward_mdns_*`（5 转发）就是这层调用面，
//! 属机制（通用注册表 + 寻址，AGENTS §5.1.3 薄壳③）。宿主 adapter 调用时把本模块的
//! [`CAPABILITY`] 传进内核函数，于是**内核源码里不再出现本域的任何字面量**。
//!
//! ## 漂移锁（本文件单测）
//!
//! 1. [`EXPORTS`] 条数 == 端口 `DiscoveryPorts` 的 `forward_mdns_*` 方法数
//!    （一条导出对应一条转发方法，少一条即那条原语永远走本域引擎）；
//! 2. 每条导出都属于 [`crate::MODULE_INTERFACES`] 里那个以 [`CAPABILITY`] 结尾的接口；
//! 3. [`FORWARD_PREFIX`] == [`CAPABILITY`] 去掉 `host-` 前缀（三层命名一致的机械判据）。

/// 能力名（宿主原语能力清单里逐字同名的一项）
pub const CAPABILITY: &str = "host-mdns";

/// 三层路由方法族的共同前缀（端口 `forward_mdns_*` / 内核转发 / 提供者窄端口 `mdns_*`）
pub const FORWARD_PREFIX: &str = "mdns";

/// 提供者组件必须**全部**导出才算提供该能力的函数（`ItemName` 路径语法）
///
/// 顺序与内核 `CapabilityTarget` 的方法族一致（browse / stop-browse / advertise /
/// stop-advertise / is-advertising），宿主 adapter 逐条按名取用。
pub const EXPORTS: &[&str] = &[
    "bedcode:plugin/host-mdns.browse",
    "bedcode:plugin/host-mdns.stop-browse",
    "bedcode:plugin/host-mdns.advertise",
    "bedcode:plugin/host-mdns.stop-advertise",
    "bedcode:plugin/host-mdns.is-advertising",
];

/// 自报项（`inventory` 静态）
///
/// 漏 `submit!` 行 / 漏宿主的强制引用行 ⇒ 内核路由表里没有本能力 ⇒ **系统组件再也
/// 接管不了 `host-mdns`**（探测永远不命中，静默回落本域引擎）。宿主侧由
/// `tests/mdns_wiring.rs` 的对偶锁点名。
#[cfg(feature = "desktop-host")]
static ROUTE: bedcode_host_kit::RoutableCapability = bedcode_host_kit::RoutableCapability {
    capability: CAPABILITY,
    forward_prefix: FORWARD_PREFIX,
    exports: EXPORTS,
};

#[cfg(feature = "desktop-host")]
bedcode_host_kit::submit_routable_capability!(ROUTE);

#[cfg(test)]
mod tests {
    use super::*;

    /// 导出表与端口转发方法族一一对应（少一条 ⇒ 那条原语永远走本域引擎）
    #[test]
    fn exports_match_the_port_forward_method_family() {
        let ports_src = include_str!("ports.rs");
        let marker = format!("forward_{FORWARD_PREFIX}_");
        let decls = ports_src
            .lines()
            .filter(|line| {
                line.trim()
                    .strip_prefix("fn ")
                    .map(|rest| rest.starts_with(&marker))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            decls,
            EXPORTS.len(),
            "端口声明了 {decls} 条 `{marker}*` 而导出表列了 {} 条——一条导出对应一条转发方法",
            EXPORTS.len()
        );
    }

    /// 每条导出都属于 `MODULE_INTERFACES` 里那个以能力名结尾的接口
    #[test]
    fn exports_belong_to_the_wit_interface() {
        let iface = crate::MODULE_INTERFACES
            .iter()
            .find(|iface| iface.ends_with(CAPABILITY))
            .unwrap_or_else(|| {
                panic!(
                    "MODULE_INTERFACES {:?} 里应有接口以 {CAPABILITY} 结尾",
                    crate::MODULE_INTERFACES
                )
            });
        for export in EXPORTS {
            assert!(
                export.starts_with(&format!("{iface}.")),
                "导出 '{export}' 不属于接口 {iface}"
            );
        }
    }

    /// 三层命名一致的机械判据：前缀 = 能力名去掉 `host-`
    #[test]
    fn forward_prefix_derives_from_capability_name() {
        assert_eq!(
            CAPABILITY.strip_prefix("host-"),
            Some(FORWARD_PREFIX),
            "前缀必须与能力名同源（`host-<prefix>`），否则三层方法族锁与端口前缀对不上"
        );
    }

    /// 自报项真的进了全局注册表（漏 `submit!` ⇒ 内核路由表缺本能力）
    #[cfg(feature = "desktop-host")]
    #[test]
    fn route_is_self_reported_into_the_registry() {
        let routes = bedcode_host_kit::collected_routable_capabilities();
        let route = routes
            .iter()
            .find(|route| route.capability == CAPABILITY)
            .unwrap_or_else(|| panic!("本域路由词汇未被自报（应为 {CAPABILITY}）"));
        assert_eq!(route.forward_prefix, FORWARD_PREFIX);
        assert_eq!(route.exports, EXPORTS);
    }
}
