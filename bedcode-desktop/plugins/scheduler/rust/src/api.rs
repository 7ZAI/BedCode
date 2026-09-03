//! 计划任务插件互调 api（ADR-0017 / spec §9.5）
//!
//! `#[plugin_api]` 宏由 trait 生成三样东西：
//! - `ScheduleApiDispatcher`：实现方 JSON-RPC 分派（解析请求 → 调 trait 方法
//!   → 回响应），activate() 时 register() 订阅请求 topic，on_message() 里
//!   dispatch 接线
//! - `ScheduleApiClient`：调用方类型化客户端（`ApiClient::<ScheduleApi>::new`
//!   语义，供其他插件互调本插件）
//! - 构建期防漂移：trait 方法推导的 api 清单（`<manifest.id>.<method>`）与
//!   plugin.json 的 `api` 字段做精确集合比对，不一致构建失败（改 manifest
//!   不改 trait 或反之都会在构建期暴露）
//!
//! api 语义与 CLI/HTTP 端点对等（spec §9.5）：参数校验与错误消息直接复用
//! engine 层实现，不复制业务逻辑。

use bedcode_plugin_api::host::HostBus;
use bedcode_plugin_api::plugin_api;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

/// 对外互调 api（trait 方法名 ↔ plugin.json `api` 条目末段，构建期比对）
#[plugin_api(manifest = "../plugin.json")]
pub trait ScheduleApi {
    /// 创建任务，返回 job_id
    fn add(
        name: String,
        schedule: String,
        exec_type: String,
        exec_value: String,
        cwd: Option<String>,
        env: Option<Value>,
        timeout_sec: Option<i64>,
        once: bool,
    ) -> Result<String, String>;

    /// 删除任务及其执行记录
    fn remove(job_id: String) -> Result<bool, String>;

    /// 列出全部任务（按 next_at 升序，附最近一次执行摘要）
    fn list() -> Result<Vec<Value>, String>;

    /// 任务详情 + 最近 20 条执行记录
    fn show(job_id: String) -> Result<Value, String>;

    /// 手动立即执行一次（不改变 next_at），返回 exec_id
    fn run(job_id: String) -> Result<String, String>;

    /// 最近执行记录（含输出文件路径；job_id 不存在时为空列表）
    fn logs(job_id: String, limit: Option<i64>) -> Result<Value, String>;
}

impl ScheduleApi for crate::SchedulerPlugin {
    fn add(
        name: String,
        schedule: String,
        exec_type: String,
        exec_value: String,
        cwd: Option<String>,
        env: Option<Value>,
        timeout_sec: Option<i64>,
        once: bool,
    ) -> Result<String, String> {
        let host = WasmHost;
        crate::engine::add_job(
            &host,
            &name,
            &schedule,
            &exec_type,
            &exec_value,
            cwd.as_deref(),
            env.as_ref(),
            timeout_sec,
            once,
        )
    }

    fn remove(job_id: String) -> Result<bool, String> {
        let host = WasmHost;
        if crate::engine::remove_job(&host, &job_id) {
            Ok(true)
        } else {
            Err(format!("job not found: {}", job_id))
        }
    }

    fn list() -> Result<Vec<Value>, String> {
        let host = WasmHost;
        Ok(crate::engine::list_jobs(&host))
    }

    fn show(job_id: String) -> Result<Value, String> {
        let host = WasmHost;
        crate::engine::show_job(&host, &job_id).ok_or_else(|| format!("job not found: {}", job_id))
    }

    fn run(job_id: String) -> Result<String, String> {
        let host = WasmHost;
        crate::engine::run_job(&host, &job_id)
    }

    fn logs(job_id: String, limit: Option<i64>) -> Result<Value, String> {
        let host = WasmHost;
        // 与 HTTP logs 端点一致的 limit 语义（默认 20，封顶 100）
        let limit = limit.unwrap_or(20).clamp(1, 100);
        Ok(crate::engine::job_logs(&host, &job_id, limit))
    }
}
