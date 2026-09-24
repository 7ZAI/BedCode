//! 宿主能力：宿主并发任务域（WIT `host-task`，ABI v20，**desktop 独有双端偏离**）
//!
//! 入口职责（spec §6「双门结构」）：
//! - `task:run` 权限门：管「占用宿主线程池资源」这件事本身；
//! - plan 解析与配额仲裁（单元数上限 / 每插件在册任务上限，超限 fail-visible）；
//! - 单元 kind 白名单预检：**每种 kind 的域权限门在单元执行时由目标 host_impl
//!   函数内部再校验**（fs:read / fs:write / process:run / network:http + fs_auth）——
//!   仅授 `task:run` 不授域权限的插件所有单元都会失败（并发能力与数据访问能力
//!   解耦授权、解耦审计）。
//!
//! fs_auth 弹窗差异（行为契约）：单元操作只做 fs_auth **已授权校验**，未授权路径
//! 直接 `Err`，**绝不从池线程触发用户弹窗**（弹窗会长时间占用池槽位并困惑用户）；
//! 需要新授权的路径，插件须在普通调用栈里先 `host-fs.request-auth`。
//!
//! 执行与事件管道见 [`crate::wasm_core::manager::task`]（core-task）：本模块只做
//! 权限 / 配额 / 解析 / 凭据红线（全量审计日志记 plan 摘要——kind / 单元数 /
//! 属主，不记 params 全文）。

use crate::wasm_core::manager::task as core_task;
use crate::wasm_core::host_api::context::WasmHostContext;
use crate::wasm_core::permission::PERMISSION_TASK_RUN;
use std::sync::Arc;

/// `execute-batch`：同步批（扇出 → join → 一次性返回全部单元结果）。
/// 阻塞 Store 至全部单元终态（或超时）——同 `run-sync` 语义，仅限快操作。
pub(crate) fn execute_batch(
    host_ctx: &Arc<WasmHostContext>,
    plugin_id: &str,
    plan_json: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_execute_batch") {
        return Err("permission denied".to_string());
    }
    core_task::execute_batch(host_ctx.clone(), plugin_id, plan_json)
}

/// `submit`：异步任务，登记后立即返回 `task-<hex>` 句柄；进度/终态经
/// `events-task#on-task-event` 回调；`cancel` 协作式取消。
pub(crate) fn submit(host_ctx: &Arc<WasmHostContext>, plugin_id: &str, plan_json: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_submit") {
        return Err("permission denied".to_string());
    }
    core_task::submit(host_ctx.clone(), plugin_id, plan_json)
}

/// `status`：任务状态自愈快照（事件丢失后查询）。`Ok(None)` = 不存在 / 非属主。
pub(crate) fn status(host_ctx: &Arc<WasmHostContext>, plugin_id: &str, job_id: &str) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_status") {
        return Err("permission denied".to_string());
    }
    core_task::status(plugin_id, job_id)
}

/// `cancel`：协作式取消（正在执行的单元跑完或超时，未开始单元 skipped）；幂等。
pub(crate) fn cancel(host_ctx: &Arc<WasmHostContext>, plugin_id: &str, job_id: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_cancel") {
        return Err("permission denied".to_string());
    }
    core_task::cancel(plugin_id, job_id)
}

/// `list-jobs`：本插件在册任务清单（自愈快照）
pub(crate) fn list_jobs(host_ctx: &Arc<WasmHostContext>, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_list_jobs") {
        return Err("permission denied".to_string());
    }
    core_task::list_jobs(plugin_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};

    /// build_host_ctx() 返回 Arc<WasmHostContext>（host_impl 共享测试设施）
    fn ctx() -> Arc<WasmHostContext> {
        build_host_ctx()
    }

    #[test]
    fn task_requires_permission() {
        // 无 task:run：全部入口拒绝（五同步点之一：host_impl 权限门）
        let ctx = ctx();
        let p = "com.bedcode.task-test";
        assert_eq!(
            execute_batch(&ctx, p, r#"{"units":[]}"#),
            Err("permission denied".to_string())
        );
        assert_eq!(submit(&ctx, p, r#"{"units":[]}"#), Err("permission denied".to_string()));
        assert_eq!(status(&ctx, p, "task-x"), Err("permission denied".to_string()));
        assert_eq!(cancel(&ctx, p, "task-x"), Err("permission denied".to_string()));
        assert_eq!(list_jobs(&ctx, p), Err("permission denied".to_string()));
    }

    #[test]
    fn plan_validation_failures_are_visible() {
        let ctx = ctx();
        grant_permissions(&ctx, "com.bedcode.task-test", &[PERMISSION_TASK_RUN]);
        // 空 units：plan 校验失败（可见的错误，不静默）
        let err = execute_batch(&ctx, "com.bedcode.task-test", r#"{"units":[]}"#).unwrap_err();
        assert!(err.contains("no units"), "got: {}", err);
    }

    #[test]
    fn unknown_unit_kind_fails_in_that_unit_only() {
        let ctx = ctx();
        grant_permissions(&ctx, "com.bedcode.task-test", &[PERMISSION_TASK_RUN]);
        // fail-collect：未知 kind 只让该单元 ok=false，不拖垮同批其他单元
        let plan = r#"{"units":[{"id":"bad","kind":"no.such.kind","params":{}},{"id":"ok","kind":"fs.exists","params":{"path":"/nonexistent-xyz"}}]}"#;
        let json = execute_batch(&ctx, "com.bedcode.task-test", plan).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let results = v["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["id"], "bad");
        assert_eq!(results[0]["ok"], false);
        assert!(
            results[0]["error"].as_str().unwrap().contains("unknown unit kind"),
            "got: {}",
            results[0]["error"]
        );
        // 第二个单元（fs.exists 缺路径/权限会失败或恒 false——此处只断言批次继续执行）
        assert_eq!(results[1]["id"], "ok");
    }
}
