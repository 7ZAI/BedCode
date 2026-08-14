//! 组件端到端测试插件（迁移 ticket 02/03 宿主单测专用）
//!
//! 基于 WIT 契约（plugin-sdk-mobile/rust/wit/bedcode.wit，单一事实来源）生成
//! wit-bindgen 0.60.0 绑定；`features` 控制异常形态供宿主拒绝场景单测使用：
//! `high-abi` / `spin-loop` / `big-alloc`（见 Cargo.toml；import-extra 已随
//! 03 全量接线移除——所有 WIT 接口均已注册，未知接口由类型系统构造性排除）。
//!
//! 产出形态：core module 含 `component-type` 段，由宿主导入链路用 wit-component
//! 编码为组件（字节 `00 61 73 6d 0d 00 01 00`）。
//!
//! 与桌面端 packages/plugin-component-test 对称：宿主测试内嵌 cargo build +
//! encode（源码/产物新鲜度检测策略一致）。

wit_bindgen::generate!({
    path: "../plugin-sdk-mobile/rust/wit/bedcode.wit",
    world: "plugin",
});

// world 导出 → `exports::bedcode::plugin::*::Guest` trait；world 全部导出接口
// 必须实现（组件 world 声明即契约，宿主 Plugin::new 按全量导出校验）
use exports::bedcode::plugin::abi::Guest as AbiGuest;
use exports::bedcode::plugin::command::Guest as CommandGuest;
use exports::bedcode::plugin::events::Guest as EventsGuest;
use exports::bedcode::plugin::lifecycle::Guest as LifecycleGuest;
use exports::bedcode::plugin::manifest::Guest as ManifestGuest;
use exports::bedcode::plugin::terminal_hooks::Guest as TerminalHooksGuest;
use exports::bedcode::plugin::transfer_request_hook::Guest as TransferRequestHookGuest;
use exports::bedcode::plugin::upload_hook::Guest as UploadHookGuest;

struct ComponentTestPlugin;

// ==================== abi ====================

impl AbiGuest for ComponentTestPlugin {
    fn version() -> u32 {
        #[cfg(feature = "high-abi")]
        {
            return 999;
        }
        // 与 SDK bedcode_plugin_api_mobile::abi::ABI_VERSION 同步（=6）
        6
    }
}

// ==================== command ====================

impl CommandGuest for ComponentTestPlugin {
    fn invoke(name: String, args_json: String) -> String {
        #[cfg(feature = "spin-loop")]
        {
            // 纯 guest 死循环：烧完单次调用燃料预算被 trap（宿主断言 Err）
            loop {}
        }

        #[cfg(feature = "big-alloc")]
        {
            // 直接调用 wasm memory.grow 指令（绕过 dlmalloc 的分配策略不确定性）：
            // 一次性申请 300MB（4800 页 × 64KB）。若 Store limiter 对组件内存生效，
            // grow 返回 -1（usize::MAX）——宿主断言返回 "failed" 即 ResourceLimiter 拦截生效。
            // 若成功 grow（old != MAX），说明组件内存未受 limiter 约束（安全事件，须上报）。
            //
            // 历史坑：vec![0u8; N] / Vec::with_capacity 会被 LLVM 整体消除——
            // wasm 内存初始即零，"分配+zero-fill+仅读首字节" 是 no-op（观测不到
            // memory.grow，返回 len=300MB 是逻辑长度≠实际分配）。故必须用显式
            // memory.grow 观测，或分配后写入非零值保持
            let pages = 300 * 1024 * 1024 / (64 * 1024);
            let old = unsafe { core::arch::wasm32::memory_grow(0, pages) };
            return serde_json::json!({"grow": if old == usize::MAX { "failed" } else { "ok" }})
                .to_string();
        }


        // 正常形态：host-storage 往返 + host-log 埋点 + host-config 读取
        // （03 全量接线验证：storage/log/config 三组 import 同时活跃）
        let stored = bedcode::plugin::host_storage::get("test-key")
            .ok()
            .flatten()
            .unwrap_or_default();
        bedcode::plugin::host_log::info("component-test invoke");
        // host-config 接线验证：system.time_ms（wasm guest 无系统时钟）
        let now_ms = bedcode::plugin::host_config::get("system.time_ms")
            .ok()
            .flatten()
            .unwrap_or_default();

        serde_json::json!({
            "name": name,
            "args": args_json,
            "stored": stored,
            "now_ms": now_ms,
        })
        .to_string()
    }
}

// ==================== lifecycle ====================

impl LifecycleGuest for ComponentTestPlugin {
    fn activate() -> Result<(), String> {
        Ok(())
    }

    fn deactivate() -> Result<(), String> {
        Ok(())
    }

    fn on_startup() {}

    fn on_shutdown() {}
}

// ==================== events ====================

impl EventsGuest for ComponentTestPlugin {
    fn on_bus_message(_topic: String, _payload_json: String) -> Result<(), String> {
        Ok(())
    }

    fn on_auth_success() -> Result<(), String> {
        Ok(())
    }

    fn on_disconnect(_reason: String) -> Result<(), String> {
        Ok(())
    }

    fn on_session_created(_session_id: String) -> Result<(), String> {
        Ok(())
    }

    fn on_session_stopped(_session_id: String) -> Result<(), String> {
        Ok(())
    }
}

// ==================== terminal-hooks / upload / transfer ====================

impl TerminalHooksGuest for ComponentTestPlugin {
    fn on_terminal_input(_session_id: String, _text: String) -> Option<String> {
        None
    }

    fn on_terminal_output(_session_id: String, _data: String) -> Option<String> {
        None
    }
}

impl UploadHookGuest for ComponentTestPlugin {
    fn on_upload_request(_meta_json: String) -> String {
        // 固定拒绝决定 JSON：宿主 fail-closed 透传断言用（上层 manager 06 起解析）
        serde_json::json!({"approved": false, "reason": "component-test default deny"})
            .to_string()
    }
}

impl TransferRequestHookGuest for ComponentTestPlugin {
    fn on_transfer_request(_meta_json: String) -> String {
        serde_json::json!({"approved": false, "reason": "component-test default deny"})
            .to_string()
    }
}

// ==================== manifest ====================

impl ManifestGuest for ComponentTestPlugin {
    fn get() -> String {
        serde_json::json!({
            "id": "com.bedcode.component-test",
            "name": "Component Test",
        })
        .to_string()
    }
}

export!(ComponentTestPlugin);
