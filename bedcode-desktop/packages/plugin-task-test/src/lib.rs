//! host-task（ABI v20）fixture 插件 —— 宿主并发任务域闭环载体
//!
//! 演示 spec `.scratch/2026-09-21-host-task-concurrency/spec.md` 的插件侧用法，并作为
//! 宿主测试套件的端到端验证（最高 seam：WIT → 宿主实现 → 组件接线 → 权限 → SDK →
//! 宿主专用 OS 线程池真并行）：
//!
//! - `execute-batch`（同步档）：扇出 → join → 一次性拿全量结果（HTTP 命令面同步
//!   拿并行 stat 的场景）；
//! - `submit`（异步档）：登记即返 `task-<hex>`，`events-task#on-task-event` 回调
//!   进度/终态（SDK 无条件导出默认空实现，fixture 覆盖为累积进 storage）；
//! - `status` / `cancel` / `list-jobs`：自愈快照 / 协作式取消 / 在册清单；
//! - 重入纪律（spec §8）：回调内不等待自己任务的事件，等待一律走 execute-batch。
//!
//! 事件累积用实例级静态（同 `plugin-ws-test` / `plugin-pty-test`：wasm32-wasip3
//! 的 thread_local 是真 TLS，跨调用线程读空，故用静态 Mutex）+ storage 双写
//! （storage 供宿主断言非空；静态供宿主读序列）。回调投递发生在宿主消费派发
//! 任务（tokio），命令查询发生在宿主调用线程——静态 Mutex 是唯一可靠载体。

use bedcode_plugin_api::host::{HostLog, HostStorage, HostTask};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;

/// 收到的 `on-task-event` 原始事件 JSON 序列（供宿主断言，按到达序）
static TASK_EVENTS: std::sync::Mutex<Vec<serde_json::Value>> = std::sync::Mutex::new(Vec::new());

/// host-task fixture 插件
pub struct TaskTestPlugin;

impl WasmPlugin for TaskTestPlugin {
    const ID: &'static str = "com.bedcode.task-test";

    fn manifest() -> PluginManifest {
        // ADR-0005 单一真源：plugin.json
        serde_json::from_str(include_str!("../plugin.json")).expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("task-test fixture activated: host-task（v20）ready");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let host = WasmHost;
        match name {
            // execute-batch：args = { plan: "<plan-json>" } → 返回全量结果 JSON
            "execute-batch" => {
                let plan = require_str(&args, "plan")?;
                let out = host
                    .execute_batch(plan)
                    .map_err(|e| anyhow::anyhow!("execute_batch failed: {}", e.message))?;
                Ok(serde_json::json!({ "result": out }))
            }
            // submit：args = { plan: "<plan-json>" } → { jobId }
            "submit" => {
                let plan = require_str(&args, "plan")?;
                let job_id = host
                    .submit(plan)
                    .map_err(|e| anyhow::anyhow!("submit failed: {}", e.message))?;
                Ok(serde_json::json!({ "jobId": job_id }))
            }
            // status：args = { jobId } → { statusJson | null }
            "status" => {
                let job_id = require_str(&args, "jobId")?;
                let out = host
                    .task_status(job_id)
                    .map_err(|e| anyhow::anyhow!("task_status failed: {}", e.message))?;
                Ok(serde_json::json!({ "status": out }))
            }
            // cancel：args = { jobId } → { hit }
            "cancel" => {
                let job_id = require_str(&args, "jobId")?;
                let hit = host
                    .cancel(job_id)
                    .map_err(|e| anyhow::anyhow!("cancel failed: {}", e.message))?;
                Ok(serde_json::json!({ "hit": hit }))
            }
            // list-jobs → { jobs: "[...]" }
            "list-jobs" => {
                let out = host
                    .list_jobs()
                    .map_err(|e| anyhow::anyhow!("list_jobs failed: {}", e.message))?;
                Ok(serde_json::json!({ "jobs": out }))
            }
            // 读已收任务事件序列（供宿主断言）
            "task-events" => {
                let events = TASK_EVENTS.lock().unwrap_or_else(|e| e.into_inner()).clone();
                Ok(serde_json::json!({ "events": events }))
            }
            _ => Err(anyhow::anyhow!("unknown command: {}", name)),
        }
    }

    /// 宿主并发任务进度/终态回调（v20，events-task 可选导出）：累积进
    /// 实例级静态 + storage（观察型回调，无返回值；失败经 host-log 记录）
    fn on_task_event(event_json: &str) -> anyhow::Result<()> {
        let parsed: serde_json::Value = serde_json::from_str(event_json)
            .map_err(|e| anyhow::anyhow!("on_task_event: bad event json: {}", e))?;
        {
            let mut events = TASK_EVENTS.lock().unwrap_or_else(|e| e.into_inner());
            events.push(parsed.clone());
        }
        let host = WasmHost;
        let mut stored = host
            .storage_get(TASK_EVENTS_KEY)
            .ok()
            .flatten()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        stored.push(parsed);
        host.storage_set(TASK_EVENTS_KEY, &serde_json::json!(stored))?;
        Ok(())
    }
}

/// storage 累积键（宿主断言用）
pub const TASK_EVENTS_KEY: &str = "task-events.v1";

fn require_str<'a>(args: &'a serde_json::Value, key: &str) -> anyhow::Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing string field: {}", key))
}

bedcode_plugin_api::wasm_entry!(TaskTestPlugin);