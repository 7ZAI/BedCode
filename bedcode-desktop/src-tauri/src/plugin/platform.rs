//! host-platform-desktop 宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数同处）
//!
//! v36 交集接口切片（票 02 批次 06）：`host-platform.pick-folders / wsl-distros /
//! local-ipv4-addresses / reveal-in-dir` 拆入 `host-platform-desktop`（桌面独有 4
//! 函数；移动端 `host-platform` 保持交集 `pick-files` / `pick-folder`，不跟演）。
//!
//! 域函数随 WIT impl 走宿主（迁移前内核 `host_api/platform.rs` 的对应函数整段
//! 迁入，文案与判据逐字保留），**机制留内核**：`fs:pick` 权限判据（
//! `bedcode_wasm_core::host_api::check_permission`）、选择结果授权校验（
//! `FsAuthChecker::authorize_picked`，经 `FsAuthScope` 公开面）、平台分发
//! （`bedcode_wasm_core::system::opener`，宿主 `system.rs` 垫片引用）。
//! 内核不再持有桌面平台依赖：`local_ip_address` crate / `system::wsl` 已随本域迁出。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::{AppHandleScope, FsAuthScope, PermissionScope, WasmHostContext};
use bedcode_wasm_core::permission::PERMISSION_FS_PICK;
use bedcode_wasm_core::runtime_util::block_on_async;
use std::sync::Arc;
use tauri_plugin_dialog::DialogExt;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报）。路径 B 域
// 的自报静态住在本 crate（宿主 lib 即最终二进制）⇒ 无需强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "platform-desktop";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-platform-desktop"];

/// 本域的权限位（仅 `pick-folders` 用机制位 `fs:pick`；wsl-distros /
/// local-ipv4-addresses / reveal-in-dir 无权限门——真源在 wasm-core `permission.rs`）
pub const MODULE_PERMISSIONS: &[&str] = &["fs:pick"];

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
    fn pick_folders(&mut self) -> Result<String, String> {
        platform_pick_folders(ctx_of(self), ctx_of(self), ctx_of(self), &self.plugin_id)
    }

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

/// guest 侧 import 取到 store state 里的宿主上下文（孤儿规则要求装配方自带
/// bindgen，`context` 由 host-kit 的 HostPorts 擦除面持有，此处向下转型还原）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

// ==================== 域函数（v36 自 wasm-core host_api/platform.rs 迁入） ====================

/// 权限门（`fs:pick`）：未声明即拒绝，**且不弹对话框**
///
/// 文案点名缺失的权限位与补法（fail-visible）：插件拿到的错误必须能直接回答
/// 「我在 manifest 里加什么」——「permission denied」这种无信息文案在这里是
/// 把排障成本推给插件作者。判据复用内核 `check_permission`（同一份
/// PermissionManager、同一条拒绝 warn 路径），无第二份判定。
fn require_pick_permission(perm: &dyn PermissionScope, plugin_id: &str, api: &str) -> Result<(), String> {
    if check_permission(perm, plugin_id, PERMISSION_FS_PICK, api) {
        return Ok(());
    }
    Err(format!(
        "permission denied: manifest must declare '{PERMISSION_FS_PICK}' to use the system file picker ({api})"
    ))
}

/// 系统多目录选择器 → string[] JSON（用户取消为空数组）。
/// 权限 `fs:pick`（准入门）+ 选择后授权校验（结果门），口径与交集 `pick-files` 一致。
fn platform_pick_folders(
    perm: &dyn PermissionScope,
    app: &dyn AppHandleScope,
    fs: &dyn FsAuthScope,
    plugin_id: &str,
) -> Result<String, String> {
    require_pick_permission(perm, plugin_id, "host_platform_pick_folders")?;
    let app = require_app(app)?;
    let picked = sync_result(block_on_async(pick_folders(app)))?;
    let picked = authorize_picked(fs, plugin_id, "host_platform_pick_folders", picked)?;
    serde_json::to_string(&picked).map_err(|e| format!("serialize picked folders failed: {e}"))
}

/// 选择结果的授权校验（`fs:pick` 契约第二段）——编排本体在核心机制
/// `FsAuthChecker::authorize_picked`；本包装随域迁宿主，语义逐字保留：
///
/// - **空选择**（用户取消）→ 直接放行：取消不是「拒绝授权」，不该多弹一个框；
/// - **命中已授权目录** → 静默放行；
/// - **未授权** → 弹一次框；拒绝 / 超时 / 无弹窗通道 → `Err`，**一个路径都不回传**。
fn authorize_picked(
    fs: &dyn FsAuthScope,
    plugin_id: &str,
    api: &str,
    picked: Vec<String>,
) -> Result<Vec<String>, String> {
    if picked.is_empty() {
        return Ok(picked);
    }
    let checker: Arc<bedcode_wasm_core::security::fs_auth::FsAuthChecker> = fs.fs_auth().clone();
    if block_on_async(checker.authorize_picked(plugin_id, &picked)) {
        return Ok(picked);
    }
    tracing::warn!(
        plugin_id = %plugin_id,
        api = %api,
        picked_count = picked.len(),
        "platform pick: selection not authorized (denied or timed out)"
    );
    Err(format!(
        "{api}: picked paths were not authorized (user denied or the request timed out); no path returned"
    ))
}

/// 多目录选择对话框（阻塞至用户选择；取消 → 空）
async fn pick_folders(app_handle: tauri::AppHandle) -> crate::Result<Vec<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_folders(move |selection| {
        if tx.send(selection).is_err() {
            tracing::debug!("platform_pick_folders: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(paths)) => paths.into_iter().map(path_to_string).collect(),
        // 用户取消选择
        Ok(None) => Ok(Vec::new()),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "platform_pick_folders: dialog channel closed: {e}"
        ))),
    }
}

/// Dialog FilePath → UTF-8 绝对路径串（非 UTF-8 路径显式报错而非静默丢弃）
fn path_to_string(file_path: tauri_plugin_dialog::FilePath) -> crate::Result<String> {
    let path = file_path
        .into_path()
        .map_err(|e| crate::AppError::InvalidInput(format!("platform pick: failed to convert selected path: {e}")))?;
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| crate::AppError::InvalidInput("platform pick: selected path is not valid UTF-8".to_string()))
}

fn require_app(app: &dyn AppHandleScope) -> Result<tauri::AppHandle, String> {
    app.app_handle()
        .map(|a| a.clone())
        .ok_or_else(|| "platform unavailable in headless context (no app_handle)".to_string())
}

fn sync_result<T>(r: crate::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

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
    use bedcode_wasm_core::host_api::grant_permissions;
    use bedcode_wasm_core::test_support::build_host_ctx_at;

    const PLUGIN: &str = "com.test.pick";

    /// C-101 未声明 `fs:pick` → 显性拒绝，且**在弹对话框之前**（无头上下文拿不到
    /// AppHandle，若顺序反了错误会是 headless 那条）
    #[test]
    fn pick_folders_requires_fs_pick_permission() {
        let ctx = build_host_ctx_at(None);
        for perm in ["fs:read", "fs:write", "storage", "peer"] {
            grant_permissions(&ctx, PLUGIN, &[perm]);
            let err = platform_pick_folders(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN)
                .expect_err("未声明 fs:pick 必须拒绝");
            assert!(
                err.contains(PERMISSION_FS_PICK),
                "错误必须点名缺失的权限位（插件作者据此知道 manifest 补什么），got: {err}"
            );
            assert!(
                !err.contains("headless"),
                "权限门必须早于对话框：拿到了 headless 报错说明顺序反了, got: {err}"
            );
        }
    }

    /// 声明 `fs:pick` → 过得了权限门（下一步才会去要 AppHandle）——无头上下文
    /// 拿不到系统对话框，验证权限门与对话框的先后
    #[test]
    fn pick_folders_passes_gate_then_headless_rejected() {
        let ctx = build_host_ctx_at(None);
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_PICK]);
        let err = platform_pick_folders(ctx.as_ref(), ctx.as_ref(), ctx.as_ref(), PLUGIN)
            .expect_err("无头上下文拿不到系统对话框");
        assert!(err.contains("headless"), "声明 fs:pick 后应走到对话框环节, got: {err}");
    }

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
            &["bedcode:plugin/host-platform-desktop"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(MODULE_PERMISSIONS, &["fs:pick"], "仅 pick-folders 用机制位 fs:pick");
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
