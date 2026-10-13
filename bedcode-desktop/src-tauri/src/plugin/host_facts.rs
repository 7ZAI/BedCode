//! 宿主端事实（平台标识 / 入站服务器端口）——端差异的实现层落点
//!
//! 来源：wasm-core 单形态收口（2026-10-13 用户裁决「将差异放到实现层，也就是宿主」）。
//! 单形态之前这两项端事实写在 wasm-core 里按 `desktop-assembly` 编译期二选一
//! （`host_api::platform::platform_id` 的 `"desktop"` / `"mobile"` 字面量、
//! `host_api::config` 的 `NetworkPort` 臂直读 ServerSupervisor）——按 ADR 0049 D1
//! 的判定口径（「桌面这样写、移动那样写」就不是机制）应变现宿主的端事实。
//! 现在内核只消费 `bedcode_wasm_core::host_api::ports::HostFactsPort`，
//! 本文件是桌面实现（移动对应实现：
//! `bedcode-mobile/src-tauri/src/plugin/host_ports.rs`）。
//!
//! 装配点：宿主启动装配时作为 `PluginHost::new` 末参注入，写入宿主上下文
//! （`WasmHostContext::set_host_facts`）——必须在任何插件 `activate()` **之前**：
//! 跨端共享产物 file-transfer 在激活期经 `host-platform.platform()` 探测端值。
//! 运行期消费者还有 `host-config` 的 `network.port`（同一端口取入站服务器运行态）。

use bedcode_wasm_core::host_api::ports::HostFactsPort;

/// 桌面宿主端事实
pub struct DesktopHostFacts;

impl HostFactsPort for DesktopHostFacts {
    fn platform_id(&self) -> String {
        // 双端契约取值之一（guest 侧 file-transfer 单产物据此分支；取值不再是
        // 内核按 feature 二选一，见文件头）
        "desktop".to_string()
    }

    fn inbound_server_port(&self) -> Result<String, String> {
        // 入站服务器（HTTP / WS 组合根）的**运行态**端口（端口冲突时可能已重分配）。
        // 服务器未启动时显性报错、不回退配置默认端口：回退会让插件拿到一个看似
        // 合法实则连不上的端口，排查成本远高于一次显式失败。
        let supervisor = bedcode_server_core::supervisor::ServerSupervisor::global();
        let actual_port =
            bedcode_wasm_core::runtime_util::block_on_async(supervisor.get_status_info()).port;
        if actual_port > 0 {
            Ok(actual_port.to_string())
        } else {
            Err("network.port unavailable: host server not started".to_string())
        }
    }
}
