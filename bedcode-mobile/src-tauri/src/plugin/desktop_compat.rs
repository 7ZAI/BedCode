//! 桌面兼容面桩（file-transfer 双端单产物，2026-10-13；规格
//! `.scratch/2026-10-13-file-transfer-single-wasm/spec.md`）
//!
//! ## 为什么移动宿主要实现两个「桌面专属」interface
//!
//! file-transfer 双端共享一份 wasm 产物（按桌面 `world plugin` 构建）。该产物的
//! import 面 = core 子集 + **`host-api-call`**（auth_center 互调认证中心）+
//! **`host-platform-desktop`**（reveal-in-dir 等桌面平台事实）——后两者只存在于桌面
//! cap 分片，移动宿主的 `world plugin`（core + cap-mobile）不提供。组件模型**没有
//! 「缺省 import」**：import 不闭合即实例化失败。
//!
//! 故本模块为这两个 interface 提供 **fail-visible 桩**：全部函数显性
//! `Err("unsupported on mobile: …")`。桌面分支在移动端**不可达**——共享产物激活时
//! 经 `host-platform.platform()` 探测到 `mobile`，走移动侧端口实现（直答宿主原语、
//! 无目录多选 / 无 reveal），永不调用这两个 interface。桩的存在只是为了 import
//! 闭合，**不是「移动端实现桌面能力」**；若哪天真被调用（= 平台探测失效），错误
//! 文案会当场点名，不会静默走空实现。
//!
//! ## 绑定来源（跨端读桌面 wit 生成物）
//!
//! 本模块 `bindgen!` 的是**桌面 SDK 的 wit 目录**（`compose-wit.mjs` 生成物）的
//! `world desktop-compat`——该 world 定义在桌面 wit-src `cap-desktop.wit`
//! （binding-only，同 `plugin-binary` 先例）。桌面 wit 演进时本模块的 `Host` impl
//! 若漏跟，**编译期**即红（生成 trait 多方法而 impl 缺失）——契约同步是编译错误
//! 而非运行期断链。移动 SDK / 移动 wit / 移动插件契约面**零变更**。

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use wasmtime::component::bindgen;

bindgen!({
    // 桌面 wit 生成物目录（相对本 crate 根 `bedcode-mobile/src-tauri`）；
    // `desktop-compat` world = import host-api-call + import host-platform-desktop
    // （定义见桌面 wit-src cap-desktop.wit，compose 后进生成物）。
    path: "../../bedcode-desktop/packages/plugin-sdk-desktop/rust/wit",
    world: "desktop-compat",
});

// ==================== 桩实现（fail-visible unsupported） ====================

/// 桌面专属错误文案（fail-visible：点名「移动不提供」与「为何不应被调用」）
fn unsupported(api: &str) -> String {
    format!(
        "unsupported on mobile: {api} is a desktop-assembly primitive; \
         the shared file-transfer artifact takes its mobile branch after \
         host-platform.platform() probes `mobile` — reaching here means the \
         platform probe failed"
    )
}

impl bedcode::plugin::host_api_call::Host for WasmPluginState {
    /// 插件互调（桌面认证中心路径）：移动不提供——共享产物的 `ConsentGate`
    /// 移动实现走宿主原语直答，不经互调
    fn call(
        &mut self,
        _request_topic: String,
        _payload_json: String,
        _timeout_ms: u64,
    ) -> Result<String, String> {
        Err(unsupported("host-api-call.call"))
    }
}

impl bedcode::plugin::host_platform_desktop::Host for WasmPluginState {
    /// WSL 发行版枚举：桌面专属平台事实，移动不提供
    fn wsl_distros(&mut self) -> Result<String, String> {
        Err(unsupported("host-platform-desktop.wsl-distros"))
    }

    /// 本机 IPv4 枚举：桌面专属平台事实，移动不提供
    fn local_ipv4_addresses(&mut self) -> Result<String, String> {
        Err(unsupported("host-platform-desktop.local-ipv4-addresses"))
    }

    /// 在系统文件管理器中定位：移动不提供（无对应平台交互）
    fn reveal_in_dir(&mut self, _path: String) -> Result<(), String> {
        Err(unsupported("host-platform-desktop.reveal-in-dir"))
    }
}

// ==================== 能力模块自报 ====================

/// 能力模块名（白名单键 = 装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "mobile-desktop-compat";

/// 本模块提供的 WIT 接口（桌面兼容面；移动宿主 world 本身不 import 它们）
pub const MODULE_INTERFACES: &[&str] = &[
    "bedcode:plugin@0.2.0/host-api-call",
    "bedcode:plugin@0.2.0/host-platform-desktop",
];

/// 本模块读写的权限位：**空**——桩不放行任何能力，调用即拒
pub const MODULE_PERMISSIONS: &[&str] = &[];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；禁带产品名词）
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    // 移动 ABI 21（shared file-transfer 产物自报版本；本模块随该产物形态引入）
    abi_min: 21,
};

/// 桌面兼容面桩能力模块
pub struct MobileDesktopCompatModule;

impl HostModule for MobileDesktopCompatModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(
        &self,
        linker: &mut wasmtime::component::Linker<WasmPluginState>,
    ) -> wasmtime::Result<()> {
        type D = wasmtime::component::HasSelf<WasmPluginState>;
        bedcode::plugin::host_api_call::add_to_linker::<WasmPluginState, D>(linker, |s| s)?;
        bedcode::plugin::host_platform_desktop::add_to_linker::<WasmPluginState, D>(linker, |s| s)?;
        Ok(())
    }
}

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: MobileDesktopCompatModule = MobileDesktopCompatModule;

bedcode_host_kit::expect_host_module!(MODULE_NAME);

// 能力模块自报（自报静态住在宿主 lib = 最终二进制 ⇒ 无需强制引用行）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}
