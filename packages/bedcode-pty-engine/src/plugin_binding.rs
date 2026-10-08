//! host-pty 能力域 —— 插件私有伪终端（6 条原语 + 能力模块自报）
//!
//! spec：`.scratch/2026-10-06-pty-capability-domain/spec.md`（D1/D2：引擎 crate
//! 升格为能力域 crate，WIT 接线随域机制整体迁出内核，边界走窄端口 trait）。
//!
//! **零业务代码红线（ADR 0022）**：本文件与 [`primitives`] / [`registry`] /
//! [`output`] 只做引擎原语与机制闸门；PTY 之外的任何解读（会话、终端产品线、AI…）
//! 全在插件侧。
//!
//! ## 分层
//!
//! ```text
//!   本文件        接线（6 条原语的宿主实现 + 能力模块自报 + 端口取用）
//!   primitives.rs 域机制（创建 / 数据面 / 拉取 / 终止 / 退出监听）
//!   registry.rs  句柄表 + 配额表 + 回收面 + 退出事件组装
//!   output.rs    限频唤醒装饰器
//!   ports.rs     边界：权限门 / 总线投递 / 配置快照 / 异步桥 / 任务派生
//! ```
//!
//! 宿主侧只剩一个 adapter（`wasm_core::host_api::pty::HostPtyPorts`）与一次开机装配
//! 调用——与 http / ws / peer-net / mdns 四域同形。

use std::sync::Arc;

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use wasmtime::component::{bindgen, Linker};

use crate::plugin_binding::ports::PtyPorts;

/// 宿主能力端口（边界层；见 [`ports`] 模块文档）
pub mod ports;

/// 域机制（6 条原语 + 参数仲裁 + 退出监听）
pub mod primitives;

/// 句柄注册表 / 配额表 / 回收面
pub mod registry;

/// 输出可用通知（限频唤醒装饰器）
pub mod output;

/// [`ports::install_ports`] 的再导出（宿主 adapter 需要直接装**已构造好的**端口对象：
/// 同一份要同时登记进程级与实例级，不能经 [`install`] 新建）
pub use ports::install_ports;

/// 宿主生命周期面（停用回收 / 关停全量回收 / 在册计数 / 加载期配额登记）的再导出
///
/// **为什么经本层再导出**：这四件事的调用方是宿主（`manager::host::activation` /
/// `manager::loader` / `system::lifecycle`），它们问的是「这个能力域」，不是「这个
/// 域的注册表模块」——路径止于 `plugin_binding`，域内分层不外泄。
pub use registry::{
    kill_all_registered, kill_all_registered_with_ports, live_count, purge_for_plugin,
    purge_for_plugin_with_ports, register_quota,
};

// ==================== 能力模块自报 ====================

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 16`：`host-pty` 的 6 条原语自 ABI v16 追加，低于该版本的插件不导入
/// 本 interface。
pub const DESC: HostModuleDesc = HostModuleDesc {
    name: "pty",
    interfaces: &["bedcode:plugin/host-pty"],
    permissions: &["pty:spawn", "pty:io"],
    abi_min: 16,
};

/// PTY 能力域模块（`host-pty`，6 条原语）
pub struct PtyModule;

impl HostModule for PtyModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_pty::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与宿主既有接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: PtyModule = PtyModule;

// 能力模块自报（linker-section 静态）
//
// **依赖前提**：宿主必须有一行强制引用本 crate（见 `wasm_core::manager::runtime::component`
// 的 `use bedcode_pty_engine as _;`），否则本 rlib 不进最终二进制、静态不执行 ⇒
// 注册丢失，且 guest 会在实例化期报「无该 import」。
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

bindgen!({
    // provider 侧绑定：宿主自己的 `bedcode` 模块是 **guest 视角**（import 是调用
    // 函数，不是 `Host` trait + `add_to_linker`），能力 crate 要自己装配就必须生成
    // provider 侧。
    //
    // ⚠️ 由此产生的**硬约束**：本 crate 与宿主各自生成的
    // `bedcode::plugin::host_pty::Host` 是**同名但不同类型**的 trait。宿主必须
    // 同时删掉自己的 pty `Host` impl 与 `add_to_linker` 行，否则同一个 interface
    // 被注册两次 → 装配期 `defined twice`（本次迁移同批删除，见 spec §3.2）。
    path: "../../bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin",
    // 与宿主同款：全部导出绑定生成 async 变体（wasmtime async store 要求）
    exports: { default: async },
});

// ==================== 端口取用 ====================

/// 能力域名（宿主上下文里的键；[`bedcode_host_kit::ports::HostPorts::domain_ports`]）
pub const DOMAIN: &str = "pty";

/// 装配端口的便捷入口（宿主开机期调用）
pub fn install<P: PtyPorts + 'static>(ports: P) {
    ports::install_ports(Arc::new(ports));
}

/// 取本插件实例该用的端口：**实例级优先**，未装配则回落到进程级装配
///
/// 为什么要两级（见 `bedcode_host_kit::ports` 模块文档）：进程级只有一格，而一个进程
/// 可以有多份宿主上下文（无头测试每个用例一份）。实例级让端口与**本实例的**权限管理器
/// / 消息总线绑定——guest 调 `host-pty` 原语时的权限判定必须落在**本实例**的
/// PermissionManager 上，否则多上下文场景会读到别人的授权。
///
/// 宿主注入的是 `Arc<dyn Any>` 包着的 `Arc<dyn PtyPorts>`（能力域的端口类型只有
/// 能力域自己认识，kit 与宿主都不能把它裸存进表），故这里向下转型后**克隆内层 Arc**。
pub fn ports_for(state: &WasmPluginState) -> Arc<dyn PtyPorts> {
    match state
        .host
        .domain_ports(DOMAIN)
        .and_then(bedcode_host_kit::ports::downcast_domain_ports::<Arc<dyn PtyPorts>>)
    {
        Some(ports) => Arc::clone(&ports),
        None => ports::ports(),
    }
}

// ==================== 宿主绑定层（Host trait 实现） ====================
//
// 每个接口方法 = 一条 WIT 原语。权限门在 [`primitives`] 内的域函数里（随实现同迁，
// 经端口问宿主结果），此层只做「取端口 → 转调 → 按 WIT `result` 形状返回」。

impl bedcode::plugin::host_pty::Host for WasmPluginState {
    fn spawn(&mut self, config_json: String) -> Result<String, String> {
        primitives::pty_spawn(&ports_for(self), &self.plugin_id, &config_json)
    }

    fn write(&mut self, pty_id: String, data: Vec<u8>) -> Result<(), String> {
        primitives::pty_write(&ports_for(self), &self.plugin_id, &pty_id, &data)
    }

    fn resize(&mut self, pty_id: String, cols: u16, rows: u16) -> Result<(), String> {
        primitives::pty_resize(&ports_for(self), &self.plugin_id, &pty_id, cols, rows)
    }

    fn kill(&mut self, pty_id: String) -> Result<(), String> {
        primitives::pty_kill(&ports_for(self), &self.plugin_id, &pty_id)
    }

    fn ring_fetch(
        &mut self,
        pty_id: String,
        from_offset: u64,
        max_bytes: u32,
    ) -> Result<Option<bedcode::plugin::host_pty::RingFetchResult>, String> {
        primitives::pty_ring_fetch(
            &ports_for(self),
            &self.plugin_id,
            &pty_id,
            from_offset,
            max_bytes,
        )
        .map(|fetched| {
            fetched.map(|ring| bedcode::plugin::host_pty::RingFetchResult {
                data: ring.data,
                next_offset: ring.next_offset,
                truncated: ring.truncated,
            })
        })
    }

    fn is_running(&mut self, pty_id: String) -> Result<bool, String> {
        primitives::pty_is_running(&ports_for(self), &self.plugin_id, &pty_id)
    }
}
#[cfg(test)]
mod tests;
