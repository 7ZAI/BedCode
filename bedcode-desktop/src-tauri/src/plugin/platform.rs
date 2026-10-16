//! host-platform-desktop 宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数同处）
//!
//! v36 交集接口切片（票 02 批次 06）：`host-platform` 的桌面扩展函数拆入
//! `host-platform-desktop`（桌面独有）。2026-10-13 功能级统一（ADR 0052 D3）后，
//! `pick-folders` 已并入 core `host-platform`（内核实现 `host_api/platform.rs`），
//! 本域只剩 `wsl-distros` / `local-ipv4-addresses` / `reveal-in-dir` 3 条
//! （无权限门——平台事实 / 平台交互动作，ADR 0022 裁剪线）。
//!
//! 域函数随 WIT impl 走宿主（迁移前内核 `host_api/platform.rs` 的对应函数整段
//! 迁入，文案与判据逐字保留），**机制留内核**：平台分发
//! （`bedcode_wasm_core::system::opener`，宿主 `system.rs` 垫片引用）与
//! `local_ip_address` / `system::wsl` 依赖都在宿主侧。

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_wasm_core::runtime_util::block_on_async;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报）。路径 B 域
// 的自报静态住在本 crate（宿主 lib 即最终二进制）⇒ 无需强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "platform-desktop";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin@0.2.0/host-platform-desktop"];

/// 本域的权限位：空——`wsl-distros` / `local-ipv4-addresses` / `reveal-in-dir`
/// 全是平台事实 / 平台交互动作，不叠加权限门（ADR 0022 裁剪线，真源在
/// wasm-core `permission.rs` 的判据注释）。原 `pick-folders`（用 `fs:pick`）已随
/// 2026-10-13 功能级统一1 归 core `host-platform`（门随函数留在内核实现）。
pub const MODULE_PERMISSIONS: &[&str] = &[];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 36`：`host-platform-desktop` 在 ABI v36（交集接口切片）引入。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 36,
};

/// host-platform-desktop 能力模块
pub struct PlatformDesktopModule;

impl HostModule for PlatformDesktopModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_platform_desktop::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: PlatformDesktopModule = PlatformDesktopModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 域函数转发） ====================

impl bedcode::plugin::host_platform_desktop::Host for WasmPluginState {
    fn wsl_distros(&mut self) -> Result<String, String> {
        platform_wsl_distros()
    }

    fn local_ipv4_addresses(&mut self) -> Result<String, String> {
        platform_local_ipv4_addresses()
    }

    fn reveal_in_dir(&mut self, path: String) -> Result<(), String> {
        platform_reveal_in_dir(&path)
    }
}

// ==================== 域函数（v36 自 wasm-core host_api/platform.rs 迁入） ====================
//
// 2026-10-13 功能级统一（ADR 0052 D3）：原 `platform_pick_folders` +
// `authorize_picked` + `pick_folders` 对话框 + `require_pick_permission` /
// `require_app` / `sync_result` / `path_to_string` 辅助链已整体退役——`pick-folders`
// 统一为 core `host-platform.pick-folders`，实现与 `fs:pick` 权限门 / 选择后
// 授权校验留在内核 `host_api/platform.rs::platform_pick_folders`（桌面与移动同源）。
// 本域只剩不调对话框、不碰路径授权的三条平台事实 / 交互函数。

/// WSL 发行版名枚举（v19 函数级追加，票 13）：`string[]` JSON，顺序保持
/// `wsl --list --verbose` 输出顺序。
///
/// 宿主无 WSL（非 Windows / 未安装 / 命令不可用）时**显性报错**而非空数组——
/// 空数组会被消费方读成「装了 0 个发行版」，与「本机没有 WSL」不可区分。
/// 枚举是阻塞进程调用，搬 `spawn_blocking` 以免占住 async store 所在 worker。
fn platform_wsl_distros() -> Result<String, String> {
    let distros = block_on_async(async {
        tokio::task::spawn_blocking(crate::system::wsl::list_distributions)
            .await
            .map_err(|e| format!("wsl distro list task join failed: {e}"))?
            .map_err(|e| format!("wsl distro list failed: {e}"))
    })?;
    wsl_distro_names(distros)
}

/// 发行版列表 → JSON 名字数组（纯函数：只取 name、保持输入顺序）
fn wsl_distro_names(distros: Vec<crate::system::wsl::WslDistro>) -> Result<String, String> {
    let names: Vec<String> = distros.into_iter().map(|d| d.name).collect();
    serde_json::to_string(&names).map_err(|e| format!("serialize wsl distros failed: {e}"))
}

/// 本机可访问 IPv4 地址列表（v19 函数级追加，票 14）→ `string[]` JSON
///
/// 与宿主命令面 `get_local_ip_addresses` 同口径（只要 IPv4、排除回环与链路本地）；
/// **无可用地址时返回空数组**（与 `wsl-distros` 的显性报错口径不同）。
/// 网卡枚举是阻塞调用，搬 `spawn_blocking` 以免占住 async store 所在 worker。
fn platform_local_ipv4_addresses() -> Result<String, String> {
    let addresses = block_on_async(async {
        tokio::task::spawn_blocking(collect_local_ipv4)
            .await
            .map_err(|e| format!("local ipv4 list task join failed: {e}"))
    })?;
    serde_json::to_string(&addresses).map_err(|e| format!("serialize local ipv4 addresses failed: {e}"))
}

/// 真实网卡枚举（含过滤）
fn collect_local_ipv4() -> Vec<String> {
    let interfaces = local_ip_address::list_afinet_netifas().unwrap_or_default();
    filter_ipv4(interfaces)
}

/// 网卡条目过滤（纯函数，单测覆盖）：只保留 IPv4 且非回环 / 非链路本地，
/// 输出顺序即输入顺序（与宿主命令面同口径）
fn filter_ipv4(interfaces: Vec<(String, std::net::IpAddr)>) -> Vec<String> {
    interfaces
        .into_iter()
        .filter(|(_, ip)| match ip {
            std::net::IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_link_local(),
            std::net::IpAddr::V6(_) => false,
        })
        .map(|(_, ip)| ip.to_string())
        .collect()
}

/// 在系统文件管理器中定位并选中文件/目录（v22 函数级追加）：`()`
///
/// 与 `pick-*` **不同口径**：定位不调起选择器、也不交付任何路径（路径本就由调用方
/// 提供），因此**不叠加权限门**（ADR 0022 裁剪线）。实现本体在
/// [`bedcode_wasm_core::system::opener`]（同一份平台分发也被宿主 `open_log_dir`
/// 使用）；本函数只做「PathBuf 语义校验 + 错误转 String」的适配。
fn platform_reveal_in_dir(path: &str) -> Result<(), String> {
    crate::system::opener::reveal_existing_in_dir(path).map_err(|e| e.to_string())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约：只回发行版名（无 state / version / is_default 等派生信息），顺序保持
    #[test]
    fn wsl_distro_names_returns_names_in_order() {
        let json = wsl_distro_names(vec![distro("Ubuntu", true), distro("Debian", false)]).expect("serialize");
        assert_eq!(json, r#"["Ubuntu","Debian"]"#);
    }

    /// 空列表可用（安装了 WSL 但没有任何发行版 → 空数组；与「无 WSL」的区分
    /// 由错误通道承担，见下一用例）
    #[test]
    fn wsl_distro_names_empty_list() {
        assert_eq!(wsl_distro_names(Vec::new()).expect("serialize"), "[]");
    }

    /// 无 WSL 的宿主（非 Windows）显性报错，不静默回空数组
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn wsl_distros_fail_loudly_without_windows() {
        let err = platform_wsl_distros().expect_err("非 Windows 宿主必须显性报错");
        assert!(err.contains("wsl distro list"), "got: {err}");
    }

    /// IPv4 过滤契约（票 14）：只留 IPv4、排除回环与链路本地、IPv6 一律丢弃、
    /// 输出顺序与输入一致——与宿主命令面 `get_local_ip_addresses` 同口径
    #[test]
    fn filter_ipv4_excludes_loopback_link_local_and_ipv6() {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
        let interfaces = vec![
            ("lo".to_string(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
            ("eth0".to_string(), IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))),
            ("eth0".to_string(), IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))),
            ("eth1".to_string(), IpAddr::V6(Ipv6Addr::LOCALHOST)),
            ("wlan0".to_string(), IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))),
        ];
        assert_eq!(filter_ipv4(interfaces), vec!["192.168.1.10", "10.0.0.5"]);
    }

    /// 空输入 → 空数组（无可用网卡是合法状态，不是错误）
    #[test]
    fn filter_ipv4_empty_input_yields_empty_list() {
        assert!(filter_ipv4(Vec::new()).is_empty());
    }

    /// 真实网卡枚举不得出口回环地址（把过滤逻辑与真实调用串起来的最小断言；
    /// 无网卡环境下为空数组同样成立）
    #[test]
    fn collect_local_ipv4_never_returns_loopback() {
        for ip in collect_local_ipv4() {
            assert!(!ip.starts_with("127."), "回环地址不得出口: {ip}");
        }
    }

    /// 白名单三件一致：期望模块名 / 描述符与 WIT 接口、权限清单逐字一致
    /// （与 crypto / auth 样板同款）
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(
            MODULE_NAME, "platform-desktop",
            "白名单键即装载期日志与错误文案里的模块名"
        );
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin@0.2.0/host-platform-desktop"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            MODULE_PERMISSIONS,
            &[] as &[&str],
            "三函数（wsl-distros / local-ipv4-addresses / reveal-in-dir）均无权限门；\
             pick-folders 已归 core（门随函数留内核）"
        );
        assert_eq!(DESC.name, MODULE_NAME);
        assert_eq!(DESC.interfaces, MODULE_INTERFACES);
        assert_eq!(DESC.permissions, MODULE_PERMISSIONS);
    }

    fn distro(name: &str, is_default: bool) -> crate::system::wsl::WslDistro {
        crate::system::wsl::WslDistro {
            name: name.to_string(),
            is_default,
            state: "Running".to_string(),
            version: 2,
        }
    }
}
