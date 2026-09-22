//! 宿主能力：宿主并发任务域（v20，host-task，**desktop 独有，双端偏离**）
//!
//! WASM 插件（wasm32-wasip3 组件）无法创建 OS 线程；其全部宿主调用在当前 ABI
//! 下同步阻塞——要并发做 50 个 `fs.stat` / 并行跑 3 条 `git diff`，只能串行排队。
//! host-task 让插件把「单元操作计划」提交给宿主，由宿主专用 OS 线程池**真并行**
//! 执行既有宿主原语（零业务语义，ADR 0022：宿主不做任何编排解释；fail-collect
//! 不 fail-fast，单元独立成败，结果按 units 原顺序返回，`id` 由插件指定用于关联，
//! 编排判断归插件）。
//!
//! 两档 API 共享同一单元模型与执行器：
//! - [`HostTask::execute_batch`]（同步档）：扇出 → join → 一次性返回全部单元结果。
//!   阻塞 Store 至全部单元终态（或超时）——同 `run-sync` 的阻塞语义，仅限快操作
//!   （数十～百毫秒级）；长任务一律用 [`HostTask::submit`]。
//! - [`HostTask::submit`]（异步档）：登记后立即返回句柄 `task-<uuid>`，宿主线
//!   线程池执行；进度/终态经可选导出 `events-task#on-task-event`（[`WasmPlugin::on_task_event`](crate::wasm::WasmPlugin::on_task_event)）
//!   回调；[`HostTask::cancel`] 协作式取消。
//!
//! 权限：**双门结构**——`task:run` 管「占用宿主线程资源」这件事本身（manifest
//! `permissions` 声明）；每个单元**另过**其 kind 对应的既有域权限门
//! （`fs:read` / `fs:write` / `process:run` / `network:http`）。仅授 `task:run` 不授
//! 域权限的插件，所有单元都会失败——并发能力与数据访问能力解耦授权、解耦审计。
//!
//! 单元 `params` = 对应宿主原语既有请求 JSON **原样内嵌**（如 `process.run-sync`
//! 的 params 就是 run-sync 的 request-json），宿主零新 DTO 映射，语义单点在既有实现。
//!
//! **重入纪律（红线，宿主与 SDK 文档双写）**：
//! 1. 插件**禁止**在 guest 调用栈内同步等待自己任务的事件（回调投递需要实例锁，
//!    而 guest 正持有它 → 自死锁）。等待语义一律走 `execute_batch`（宿主侧 join，
//!    不经 Store）；异步任务的结果消费只能在事件回调 / 后续空闲调用里做。
//! 2. 回调内再 `submit` 允许（新调用、新拿锁，非嵌套），但受宿主每插件在册任务
//!    配额约束，避免「回调风暴」模式。
//! 3. `execute_batch` 阻塞上限：由单批单元数、各单元自身耗时与宿主任务墙钟
//!    `PLUGIN_TASK_JOB_TIMEOUT_MS` 共同决定最坏阻塞时长。**宿主没有单元级超时**
//!    （单元执行体是同步阻塞直调，池线程内无法中断）⇒ 沿用 run-sync 的「百毫秒～
//!    秒级适用」口径，长任务一律 `submit`，阻塞型单元（`process.run-sync`）的超时
//!    走被调用方自带参数。

use serde::Serialize;
use super::HostError;

/// 单元操作 kind 初始集（与宿主 `host_impl/task.rs` 白名单逐条对应，不得漂移）。
/// 追加新 kind = 宿主既有 interface 函数级追加（批次内不 bump），SDK 侧同步新增
/// 该 kind 的 `TaskUnit::new` 组装入口时须同批核对权限映射。
pub mod kinds {
    /// 读文本文件（权限 `fs:read` + fs_auth 三层校验；params = `{ path }`）
    pub const FS_READ: &str = "fs.read";
    /// 列目录（权限 `fs:read` + fs_auth；params = `{ path }`）
    pub const FS_READ_DIR: &str = "fs.read-dir";
    /// 文件元数据（权限 `fs:read` + fs_auth；params = `{ path }`）
    pub const FS_STAT: &str = "fs.stat";
    /// 存在性检查（权限 `fs:read` + fs_auth；params = `{ path }`）
    pub const FS_EXISTS: &str = "fs.exists";
    /// 写文本文件（权限 `fs:write` + fs_auth；params = `{ path, data }`）
    pub const FS_WRITE: &str = "fs.write";
    /// 同步执行外部进程并捕获输出（权限 `process:run`；params = run-sync 的
    /// request-json：`{ command, args?, cwd?, env?, timeoutMs? }`，无 output_path）
    pub const PROCESS_RUN_SYNC: &str = "process.run-sync";
    /// HTTP 请求（权限 `network:http`；params = http-fetch 的 request-json）
    pub const HTTP_FETCH: &str = "http.fetch";
}

/// 一个单元操作（plan 的最小执行单位）
///
/// `id` 由插件指定用于结果关联（不要求唯一，但同批内重复 id 会让结果对应模糊）；
/// `params` 是**该 kind 对应宿主原语的既有请求 JSON 原样内嵌**，宿主零新 DTO。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUnit {
    pub id: String,
    pub kind: String,
    pub params: serde_json::Value,
}

impl TaskUnit {
    /// `fs.read`：读文本文件（`fs:read` + fs_auth）
    pub fn fs_read(id: &str, path: &str) -> Self {
        Self::new(id, kinds::FS_READ, serde_json::json!({ "path": path }))
    }

    /// `fs.read-dir`：列目录条目（`fs:read` + fs_auth）
    pub fn fs_read_dir(id: &str, path: &str) -> Self {
        Self::new(id, kinds::FS_READ_DIR, serde_json::json!({ "path": path }))
    }

    /// `fs.stat`：文件/目录元数据（`fs:read` + fs_auth）
    pub fn fs_stat(id: &str, path: &str) -> Self {
        Self::new(id, kinds::FS_STAT, serde_json::json!({ "path": path }))
    }

    /// `fs.exists`：存在性检查（`fs:read` + fs_auth）
    pub fn fs_exists(id: &str, path: &str) -> Self {
        Self::new(id, kinds::FS_EXISTS, serde_json::json!({ "path": path }))
    }

    /// `fs.write`：写文本文件（`fs:write` + fs_auth）
    pub fn fs_write(id: &str, path: &str, data: &str) -> Self {
        Self::new(id, kinds::FS_WRITE, serde_json::json!({ "path": path, "data": data }))
    }

    /// `process.run-sync`：同步执行进程并捕获输出（`process:run`）。
    /// `params_json` = run-sync 的 request-json（`{ command, args?, cwd?, env?, timeoutMs? }`）
    pub fn process_run_sync(id: &str, params_json: serde_json::Value) -> Self {
        Self::new(id, kinds::PROCESS_RUN_SYNC, params_json)
    }

    /// `http.fetch`：HTTP 请求（`network:http`）。
    /// `params_json` = http-fetch 的 request-json（url / method / headers / body / …）
    pub fn http_fetch(id: &str, params_json: serde_json::Value) -> Self {
        Self::new(id, kinds::HTTP_FETCH, params_json)
    }

    fn new(id: &str, kind: &str, params: serde_json::Value) -> Self {
        Self {
            id: id.to_string(),
            kind: kind.to_string(),
            params,
        }
    }
}

/// progress 事件节流（可选；缺省宿主按常量节奏推进度）
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    /// 每完成 N 个单元推一次进度事件
    pub every_units: Option<u32>,
    /// 距上次进度事件的最短间隔（毫秒）
    pub every_ms: Option<u64>,
}

impl Default for TaskProgress {
    fn default() -> Self {
        Self { every_units: None, every_ms: None }
    }
}

/// 单元操作计划（`execute_batch` / `submit` 共用）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPlan {
    pub units: Vec<TaskUnit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,
}

impl TaskPlan {
    pub fn new(units: Vec<TaskUnit>) -> Self {
        Self {
            units,
            max_concurrency: None,
            job_timeout_ms: None,
            progress: None,
        }
    }

    /// 并发度上限（≤ 宿主全局池线程数；缺省 = 池满即排队）
    pub fn max_concurrency(mut self, n: usize) -> Self {
        self.max_concurrency = Some(n);
        self
    }

    /// 任务墙钟超时（毫秒；缺省取宿主常量；超时 → cancelled + 已完成结果保留）
    pub fn job_timeout_ms(mut self, ms: u64) -> Self {
        self.job_timeout_ms = Some(ms);
        self
    }

    /// progress 事件节流
    pub fn progress(mut self, p: TaskProgress) -> Self {
        self.progress = Some(p);
        self
    }

    /// 序列化为 plan-json（camelCase）
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"units\":[]}".to_string())
    }
}

/// 宿主并发任务域（v20，host-task）—— 签名与 WIT `host-task` 一一对应
pub trait HostTask {
    /// 同步批：并发执行 plan 全部单元并 join，一次性返回全部结果（JSON 字符串：
    /// `{ jobId, results: [{ id, ok, value?|error?, durationMs }], cancelled }`）。
    /// 阻塞 Store 至全部单元终态（或超时）——同 `run-sync` 的阻塞语义，仅限快操作。
    fn execute_batch(&self, plan_json: &str) -> Result<String, HostError>;

    /// 异步任务：登记后立即返回句柄 `task-<uuid>`；进度/终态经
    /// [`WasmPlugin::on_task_event`](crate::wasm::WasmPlugin::on_task_event) 回调
    /// （event-json：`{ jobId, phase, doneUnits?, failedUnits?, result? }`）。
    fn submit(&self, plan_json: &str) -> Result<String, HostError>;

    /// 任务状态自愈快照（事件丢失后查询）：state / 计数器 / 终态 results（有界保留）。
    /// `Ok(None)` = 任务不存在或非属主。
    fn task_status(&self, job_id: &str) -> Result<Option<String>, HostError>;

    /// 取消（协作式：正在执行的单元跑完或超时，未开始单元 skipped）；幂等。
    fn cancel(&self, job_id: &str) -> Result<bool, HostError>;

    /// 本插件在册任务清单（自愈快照）：`[ { jobId, state, doneUnits?, failedUnits?, createdAt } ]`
    fn list_jobs(&self) -> Result<String, HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_plan_serializes_camel_case() {
        let plan = TaskPlan::new(vec![TaskUnit::fs_stat("u1", "/tmp/a")])
            .max_concurrency(2)
            .job_timeout_ms(5000);
        let json = plan.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        // camelCase 键名（沿 WIT 惯例），宿主侧 serde(rename_all = "camelCase") 解析
        assert_eq!(v["units"][0]["kind"], "fs.stat");
        assert_eq!(v["units"][0]["params"]["path"], "/tmp/a");
        assert_eq!(v["maxConcurrency"], 2);
        assert_eq!(v["jobTimeoutMs"], 5000);
        // 未设置字段省略（Option skip_serializing_if）
        assert!(v.get("progress").is_none());
    }

    #[test]
    fn task_unit_kind_constants_match() {
        // 漂移锁：SDK 常量必须与宿主白名单 kind 逐字节一致（五同步点之一）
        assert_eq!(kinds::FS_READ, "fs.read");
        assert_eq!(kinds::FS_READ_DIR, "fs.read-dir");
        assert_eq!(kinds::FS_STAT, "fs.stat");
        assert_eq!(kinds::FS_EXISTS, "fs.exists");
        assert_eq!(kinds::FS_WRITE, "fs.write");
        assert_eq!(kinds::PROCESS_RUN_SYNC, "process.run-sync");
        assert_eq!(kinds::HTTP_FETCH, "http.fetch");
    }
}