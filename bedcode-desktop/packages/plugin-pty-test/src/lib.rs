//! host-pty（ABI v16）fixture 插件
//!
//! 演示 spec `.scratch/2026-09-19-pty-base-service/spec.md` 的插件侧用法，并作为
//! 宿主测试套件的端到端载体（最高 seam：WIT → 宿主实现 → 组件接线 → 权限 → SDK → 真 PTY）：
//!
//! - **activate 期订阅** 属主私有退出事件 `<owner>::pty:exit`：宿主不缓冲、不重放，
//!   晚订阅期间的丢失靠 `is-running` 快照自愈（spec D4 硬约束），故订阅必须在任何
//!   spawn 之前完成；
//! - 命令驱动创建与交互回路：`pty-spawn`（裸命令 + 参数数组 + env/cwd/尺寸）/
//!   `pty-ring-fetch`（游标续拉）/ `pty-write`（写字节进进程）/ `pty-resize`
//!   （改尺寸）/ `pty-is-running`（存活快照）/ `pty-kill`（终止销毁）/ `pty-state`
//!   （读已收事件，供宿主断言）；
//! - 事件与拉取结果都存实例级静态（同 `plugin-ws-test`：wasm32-wasip3 的
//!   thread_local 是真 TLS，跨调用线程读空，故用静态 Mutex）。

use bedcode_plugin_api::host::{pty_event_topic, HostBus, HostLog, HostPty, PtySpawnConfig, PTY_EXIT};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::BusMessage;

/// 收到的 `<owner>::pty:exit` 事件 payload（宿主按属主投递；跨属主订阅被总线门禁拒绝）
static EVENTS: std::sync::Mutex<Vec<serde_json::Value>> = std::sync::Mutex::new(Vec::new());

/// host-pty fixture 插件
pub struct PtyTestPlugin;

impl WasmPlugin for PtyTestPlugin {
    const ID: &'static str = "com.bedcode.pty-test";

    fn manifest() -> PluginManifest {
        // ADR-0005 单一真源：plugin.json
        serde_json::from_str(include_str!("../plugin.json")).expect("plugin.json must be valid PluginManifest")
    }

    /// 订阅属主私有退出事件（**必须在任何 spawn 之前**：宿主不重放）
    ///
    /// 订阅失败降级为日志：隔离用例把同一产物以第二个属主 id 实例化
    /// （`com.bedcode.pty-test.peer`），而 guest 只能按编译期 `Self::ID` 拼自己的
    /// 命名空间 —— 票 05 的命名空间门禁会拒这种跨属主订阅（正是要它拒的行为）。
    /// 属主本体的订阅是否真生效，由 e2e 的「A 必须收到 pty:exit 投递」行为性兜住，
    /// 不靠这里的 Err。
    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        let topic = pty_event_topic(PTY_EXIT, Self::ID);
        match host.bus_subscribe(&topic) {
            Ok(()) => host.log_info(&format!("pty-test fixture activated: subscribed {topic}")),
            Err(e) => host.log_info(&format!("pty-test fixture activated: subscribe {topic} skipped: {e}")),
        }
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let host = WasmHost;
        match name {
            // 创建插件私有裸 PTY → `{ ptyId }`（config-json 由 SDK 助手组装，camelCase）
            "pty-spawn" => {
                let command = require_str(&args, "command")?;
                let mut config = PtySpawnConfig::new(&command);
                if let Some(arg_list) = args.get("args").and_then(|v| v.as_array()) {
                    let parsed: Vec<String> = arg_list
                        .iter()
                        .map(|v| v.as_str().unwrap_or_default().to_string())
                        .collect();
                    config = config.args(parsed);
                }
                if let Some(env) = args.get("env").and_then(|v| v.as_object()) {
                    let pairs: Vec<(String, String)> = env
                        .iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                        .collect();
                    config = config.env(pairs);
                }
                if let Some(dir) = args.get("workingDir").and_then(|v| v.as_str()) {
                    config = config.working_dir(dir);
                }
                if let Some(cols) = args.get("cols").and_then(|v| v.as_u64()) {
                    config = config.cols(cols as u16);
                }
                if let Some(rows) = args.get("rows").and_then(|v| v.as_u64()) {
                    config = config.rows(rows as u16);
                }
                // 输出环容量由插件声明（宿主仲裁上限，超限时 spawn 直接 Err）
                if let Some(ring_bytes) = args.get("ringBytes").and_then(|v| v.as_u64()) {
                    config = config.ring_bytes(ring_bytes);
                }
                let pty_id = host
                    .pty_spawn(&config.to_json())
                    .map_err(|e| anyhow::anyhow!("pty_spawn: {e}"))?;
                Ok(serde_json::json!({ "ptyId": pty_id }))
            }
            // 按游标拉取输出：追平时 `{ none: true }`，有字节时回 `{ data, nextOffset, truncated }`
            "pty-ring-fetch" => {
                let pty_id = require_str(&args, "ptyId")?;
                let from_offset = args.get("fromOffset").and_then(|v| v.as_u64()).unwrap_or(0);
                let max_bytes = args.get("maxBytes").and_then(|v| v.as_u64()).unwrap_or(4096) as u32;
                match host
                    .pty_ring_fetch(&pty_id, from_offset, max_bytes)
                    .map_err(|e| anyhow::anyhow!("pty_ring_fetch: {e}"))?
                {
                    None => Ok(serde_json::json!({ "none": true, "fromOffset": from_offset })),
                    Some(fetched) => Ok(serde_json::json!({
                        "data": fetched.data,
                        "nextOffset": fetched.next_offset,
                        "truncated": fetched.truncated,
                    })),
                }
            }
            // 写入输入字节（`bytes` 为 u8 数组；宿主内建分块，超单次上限直接报错）
            "pty-write" => {
                let pty_id = require_str(&args, "ptyId")?;
                let bytes = require_bytes(&args)?;
                host.pty_write(&pty_id, &bytes).map_err(|e| anyhow::anyhow!("pty_write: {e}"))?;
                Ok(serde_json::json!({ "ok": true, "len": bytes.len() }))
            }
            // 调整终端尺寸（全屏程序重绘依赖；不承诺同步生效时序）
            "pty-resize" => {
                let pty_id = require_str(&args, "ptyId")?;
                let cols = require_u64(&args, "cols")? as u16;
                let rows = require_u64(&args, "rows")? as u16;
                host.pty_resize(&pty_id, cols, rows).map_err(|e| anyhow::anyhow!("pty_resize: {e}"))?;
                Ok(serde_json::json!({ "ok": true, "cols": cols, "rows": rows }))
            }
            // 存活快照（丢失 pty:exit 后的自愈入口）
            "pty-is-running" => {
                let pty_id = require_str(&args, "ptyId")?;
                let running = host.pty_is_running(&pty_id).map_err(|e| anyhow::anyhow!("pty_is_running: {e}"))?;
                Ok(serde_json::json!({ "running": running }))
            }
            // 终止并销毁（`pty:spawn` 域）；`<owner>::pty:exit`（reason=killed）随后投递
            "pty-kill" => {
                let pty_id = require_str(&args, "ptyId")?;
                host.pty_kill(&pty_id).map_err(|e| anyhow::anyhow!("pty_kill: {e}"))?;
                Ok(serde_json::json!({ "ok": true, "ptyId": pty_id }))
            }
            // 未声明 api 的互调路径（ADR 0017）：fixture 的 manifest `api: []`，
            // 宿主互调门禁必须拒绝对它的 `bedcode.api.*` 调用（票 06 矩阵分格）
            "pty-call-undeclared-api" => {
                let target = args.get("api").and_then(|v| v.as_str()).unwrap_or("pty-spawn");
                host.bus_publish(
                    &format!("bedcode.api.{}.{target}", Self::ID),
                    &serde_json::json!({}),
                )
                .map_err(|e| anyhow::anyhow!("bus_publish: {e}"))?;
                Ok(serde_json::json!({ "ok": true }))
            }
            // 已收事件快照（宿主断言 `pty:exit` 投递与「spawn 不发事件」的负向契约）
            "pty-state" => Ok(serde_json::json!({
                "events": EVENTS.lock().unwrap().clone(),
            })),
            other => Err(anyhow::anyhow!("Unknown command: {other}")),
        }
    }

    /// 总线消息入口：记录属主私有事件（本票只有 `pty:exit` 一条）
    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        EVENTS.lock().unwrap().push(serde_json::json!({
            "topic": msg.topic,
            "sender": msg.sender,
            "payload": msg.payload,
        }));
        Ok(())
    }
}

/// 必填字符串参数（缺失即报错，避免静默用默认值掩盖用例拼装错误）
fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("{key} is required"))
}

/// 必填字节数组参数（`bytes` 为 u8 数组；缺失即报错）
fn require_bytes(args: &serde_json::Value) -> anyhow::Result<Vec<u8>> {
    args.get("bytes")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
        .ok_or_else(|| anyhow::anyhow!("bytes is required"))
}

/// 必填无符号整数参数
fn require_u64(args: &serde_json::Value, key: &str) -> anyhow::Result<u64> {
    args.get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("{key} is required"))
}

bedcode_plugin_api::wasm_entry!(PtyTestPlugin);
