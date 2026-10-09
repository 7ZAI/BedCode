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

// ws-client：`plugin-binary` world 的独立绑定（events-binary 是可选导出，不在
// plugin world 内——与 SDK `wasm_binary` 模块同款两次 generate 模式，分模块
// 避免两个 `export!` 宏重名）
#[cfg(feature = "ws-client")]
mod binary_bindings {
    wit_bindgen::generate!({
        path: "../plugin-sdk-mobile/rust/wit/bedcode.wit",
        world: "plugin-binary",
        pub_export_macro: true,
        default_bindings_module: "binary_bindings",
    });
}

struct ComponentTestPlugin;

// ==================== ws-client 观测记录 ====================

/// guest 侧观测记录（连接事件 + 下行帧），宿主经 `ws-collect` op 读取
///
/// wit-bindgen guest 是单线程环境，std Mutex 仅满足 API 形状
#[cfg(feature = "ws-client")]
mod ws_probe {
    use std::sync::Mutex;

    pub static RECORDS: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());

    pub fn record(value: serde_json::Value) {
        if let Ok(mut guard) = RECORDS.lock() {
            guard.push(value);
        }
    }

    pub fn drain() -> Vec<serde_json::Value> {
        RECORDS
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default()
    }
}

// ==================== abi ====================

impl AbiGuest for ComponentTestPlugin {
    fn version() -> u32 {
        #[cfg(feature = "high-abi")]
        {
            return 999;
        }
        // 与 SDK bedcode_plugin_api_mobile::abi::ABI_VERSION 同步
        // （=19，v19 = host-database（主库）整面退役——主库由 wasm-core 管理、
        //  不给插件直接调用，2026-10-09 双端机制决策；叠加 v18 host-notify 域收编、
        //  v17 host-terminal / terminal-hooks 整面退役（票 15 阶段 B）、
        //  v16 认证/配对编排下沉、v15 终端订阅协议客户端迁插件、
        //  v14 host-websocket 客户端域 5 函数）
        19
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

        // ws-client：ws 原语驱动面（宿主集成测试经 command.invoke 编排真实闭环）
        #[cfg(feature = "ws-client")]
        if name == "ws-op" {
            let args: serde_json::Value = serde_json::from_str(&args_json).unwrap_or_default();
            match args["op"].as_str() {
                Some("ws-connect") => {
                    let config = serde_json::json!({ "url": args["url"] }).to_string();
                    match bedcode::plugin::host_websocket::connect(&config) {
                        Ok(handle) => {
                            return serde_json::json!({ "ok": true, "handle": handle }).to_string()
                        }
                        Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                Some("ws-send-text") => {
                    let handle = args["handle"].as_str().unwrap_or_default();
                    let text = args["text"].as_str().unwrap_or_default();
                    match bedcode::plugin::host_websocket::send_text(handle, text) {
                        Ok(()) => return serde_json::json!({ "ok": true }).to_string(),
                        Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                Some("ws-close") => {
                    let handle = args["handle"].as_str().unwrap_or_default();
                    match bedcode::plugin::host_websocket::close(handle, "{}") {
                        Ok(hit) => {
                            return serde_json::json!({ "ok": true, "hit": hit }).to_string()
                        }
                        Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                Some("ws-is-connected") => {
                    let handle = args["handle"].as_str().unwrap_or_default();
                    match bedcode::plugin::host_websocket::is_connected(handle) {
                        Ok(open) => {
                            return serde_json::json!({ "ok": true, "open": open }).to_string()
                        }
                        Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                Some("ws-collect") => {
                    return serde_json::json!({ "ok": true, "records": ws_probe::drain() }).to_string();
                }
                _ => return serde_json::json!({ "ok": false, "error": "unknown ws op" }).to_string(),
            }
        }

        // terminal-session：票 12 ABI v15 面驱动（真实组件闭环用）
        #[cfg(feature = "terminal-session")]
        if name == "terminal-op" {
            let args: serde_json::Value = serde_json::from_str(&args_json).unwrap_or_default();
            match args["op"].as_str() {
                // connect：jwt-auth（宿主代发首消息认证帧——token 不落插件，C4）
                // + auto-reconnect（R1）；url 由宿主测试注入
                Some("ts-connect") => {
                    let config = serde_json::json!({
                        "url": args["url"],
                        "jwtAuth": true,
                        "autoReconnect": { "baseMs": 1000, "maxMs": 30000 },
                    })
                    .to_string();
                    match bedcode::plugin::host_websocket::connect(&config) {
                        Ok(handle) => {
                            // 订阅帧（协议编排在 guest——终端协议事实源 = 桌面
                            // ws_terminal.rs；本夹具模拟真插件的调用序）
                            let sid = args["sessionId"].as_str().unwrap_or_default();
                            let frame = serde_json::json!({
                                "type": "subscribe", "sessionId": sid, "mode": "live"
                            })
                            .to_string();
                            let send = bedcode::plugin::host_websocket::send_text(&handle, &frame);
                            return serde_json::json!({
                                "ok": send.is_ok(),
                                "handle": handle,
                                "error": send.err(),
                            })
                            .to_string();
                        }
                        Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                // ack 帧（64KB 阈值节流由真插件持有，夹具直发一帧供对端断言）
                Some("ts-ack") => {
                    let handle = args["handle"].as_str().unwrap_or_default();
                    let frame = serde_json::json!({ "type": "ack", "offset": args["offset"] })
                        .to_string();
                    match bedcode::plugin::host_websocket::send_text(handle, &frame) {
                        Ok(()) => return serde_json::json!({ "ok": true }).to_string(),
                        Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                // close（取消 auto-reconnect 的显式关闭路径）
                Some("ts-close") => {
                    let handle = args["handle"].as_str().unwrap_or_default();
                    match bedcode::plugin::host_websocket::close(handle, "{}") {
                        Ok(hit) => return serde_json::json!({ "ok": true, "hit": hit }).to_string(),
                        Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                // 输出窄转发（C3 二进制出口）：无头测试无登记 Channel → 宿主
                // 显性 Err（fail-visible）→ 记录错误文本供宿主断言转发被驱动
                Some("ts-forward") => {
                    let sid = args["sessionId"].as_str().unwrap_or_default();
                    let data: Vec<u8> = args["data"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
                        .unwrap_or_default();
                    match bedcode::plugin::host_terminal_stream::forward_output(sid, &data) {
                        Ok(()) => ws_probe::record(serde_json::json!({ "kind": "forward", "ok": true })),
                        Err(e) => {
                            ws_probe::record(serde_json::json!({ "kind": "forward", "ok": false, "error": e }))
                        }
                    }
                    return serde_json::json!({ "ok": true }).to_string();
                }
                // 主连接事实读取（票 13 复用同一原语；无头测试断言其可调用性）
                Some("ts-primary-target") => {
                    match bedcode::plugin::host_connection::primary_target() {
                        Ok(json) => {
                            return serde_json::json!({ "ok": true, "target": json }).to_string()
                        }
                        Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
                    }
                }
                Some("ws-collect") => {
                    return serde_json::json!({ "ok": true, "records": ws_probe::drain() }).to_string();
                }
                _ => return serde_json::json!({ "ok": false, "error": "unknown terminal op" }).to_string(),
            }
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
        // ws-client：activate 期完成三通道订阅（WIT 契约：宿主不缓冲不重放，
        // 晚订阅事件永久丢失——本夹具因此把订阅放在 activate）
        #[cfg(feature = "ws-client")]
        {
            const PID: &str = "com.bedcode.test";
            let _ = bedcode::plugin::host_bus::subscribe(&format!("{PID}:ws:open"));
            let _ = bedcode::plugin::host_bus::subscribe(&format!("{PID}:ws:close"));
            // 票 12（R1）：auto-reconnect 退避排期事件（宿主每轮退避前发布）
            let _ = bedcode::plugin::host_bus::subscribe(&format!(
                "{PID}:ws:reconnect-scheduled"
            ));
            let _ = bedcode::plugin::host_bus::subscribe_binary(&format!("{PID}:ws:message"));
        }
        Ok(())
    }

    fn deactivate() -> Result<(), String> {
        Ok(())
    }

    fn on_startup() -> Result<(), String> {
        // on-startup-fail feature：宿主 Degraded 状态机测试用（激活成功但启动初始化失败）
        if cfg!(feature = "on-startup-fail") {
            return Err("startup init failed (test)".to_string());
        }
        Ok(())
    }

    fn on_shutdown() -> Result<(), String> {
        Ok(())
    }
}

// ==================== events ====================

impl EventsGuest for ComponentTestPlugin {
    fn on_bus_message(topic: String, payload_json: String) -> Result<(), String> {
        // ws-client：属主状态事件（ws:open / ws:close）原样入观测记录
        #[cfg(feature = "ws-client")]
        ws_probe::record(serde_json::json!({ "kind": "event", "topic": topic, "payload": payload_json }));
        #[cfg(not(feature = "ws-client"))]
        let _ = (topic, payload_json);
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

// ==================== terminal-hooks ====================
//
// terminal-hooks 已随票 15 阶段 B 退役（ABI v17）：WIT export 删除，
// 本 Guest impl 同批移除（组件 world 声明即契约）。

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

// ws-client：导出 events-binary（宿主实例化后动态探测；探测命中 ⇒ 二进制帧
// 可投递回 guest——下行帧闭环的前提）
#[cfg(feature = "ws-client")]
impl binary_bindings::exports::bedcode::plugin::events_binary::Guest for ComponentTestPlugin {
    fn on_message_binary(topic: String, _sender: String, payload: Vec<u8>) {
        // 帧信封（宿主 host_impl::ws::frame_envelope）：kind(1) + handle 长度
        // u16 BE(2) + handle + 原始字节。载荷只记头 16 字节，断言 echo 内容够用
        let record = if payload.len() >= 3 {
            let frame_kind = payload[0];
            let handle_len = u16::from_be_bytes([payload[1], payload[2]]) as usize;
            if 3 + handle_len <= payload.len() {
                let handle = String::from_utf8_lossy(&payload[3..3 + handle_len]).into_owned();
                let body = &payload[3 + handle_len..];
                serde_json::json!({
                    "kind": "frame",
                    "topic": topic,
                    "frame_kind": frame_kind,
                    "handle": handle,
                    "payload_len": body.len(),
                    "payload_head": body.iter().take(16).copied().collect::<Vec<u8>>(),
                })
            } else {
                serde_json::json!({ "kind": "frame", "topic": topic, "error": "handle length overflow" })
            }
        } else {
            serde_json::json!({ "kind": "frame", "topic": topic, "error": "envelope too short" })
        };
        ws_probe::record(record);
    }
}

#[cfg(feature = "ws-client")]
binary_bindings::export!(ComponentTestPlugin);
