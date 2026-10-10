//! host-task 宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数 + 自报，四件同处）
//!
//! wasm-core 纯净性收口票 02 批次 05 自内核迁出（`host_api/task.rs` 整文件删除，
//! 内核反向锁 `path_b_domains_must_not_return_to_wasm_core` 防回接）。与 auth /
//! crypto 同款：**没有独立能力 crate**——装配方就是宿主本 crate（孤儿规则见
//! [`super::bindings`] 模块文档）。
//!
//! 入口职责（spec §6「双门结构」，语义逐字保留）：
//! - `task:run` 权限门：管「占用宿主线程池资源」这件事本身；
//! - 解析 / 配额仲裁 / 单元执行全部在**留内核的任务引擎**（`manager::task` 的
//!   `CoreTaskEngine`，经 `TaskEngine` 接口两阶段注入）——本文件只做权限门 +
//!   引擎转发。**引擎不迁则接口不迁（C4）**：`TaskEngine` trait 与执行器注册表
//!   留内核 `host_api/context.rs` / `unit_executor.rs`，消费方（core-task）在内核。
//!
//! 单元 kind 的域权限门在单元执行时由目标域函数内部再校验（fs:read / fs:write /
//! process:run / network:http + fs_auth）——仅授 `task:run` 不授域权限的插件
//! 所有单元都会失败（并发能力与数据访问能力解耦授权、解耦审计）。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api::permission::PERMISSION_TASK_RUN;
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::WasmHostContext;
use bedcode_wasm_core::runtime_util::block_on_async;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报，见
// `host_module_whitelist`）。路径 B 域的自报静态住在本 crate（宿主 lib 即最终
// 二进制）⇒ 无需能力 crate 那样的 `use <crate> as _;` 强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "task";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-task"];

/// 本域的权限位（必须与 `bedcode.wit` / SDK 权限表逐字一致）
pub const MODULE_PERMISSIONS: &[&str] = &["task:run"];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 20`：host-task 并发任务域在 ABI v20 引入。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 20,
};

/// host-task 能力模块
pub struct TaskModule;

impl HostModule for TaskModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_task::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: TaskModule = TaskModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 域函数转发） ====================

impl bedcode::plugin::host_task::Host for WasmPluginState {
    fn execute_batch(&mut self, plan_json: String) -> Result<String, String> {
        execute_batch(ctx_of(self), &self.plugin_id, &plan_json)
    }

    fn submit(&mut self, plan_json: String) -> Result<String, String> {
        submit(ctx_of(self), &self.plugin_id, &plan_json)
    }

    fn status(&mut self, job_id: String) -> Result<Option<String>, String> {
        status(ctx_of(self), &self.plugin_id, &job_id)
    }

    fn cancel(&mut self, job_id: String) -> Result<bool, String> {
        cancel(ctx_of(self), &self.plugin_id, &job_id)
    }

    fn list_jobs(&mut self) -> Result<String, String> {
        list_jobs(ctx_of(self), &self.plugin_id)
    }
}

/// 取本实例的宿主上下文（与内核 `HostCtxOf::host_ctx` 同一转型；类型不符即 panic
/// ——装配期编程错误，fail-visible，不静默降级）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

// ==================== 域函数（权限门 + 引擎转发；自 `host_api/task.rs` 迁入） ====================

/// 取注入的执行引擎（PluginHost 构造后两阶段注入；未注入 fail-visible，不静默降级）
fn engine(
    host_ctx: &WasmHostContext,
) -> Result<std::sync::Arc<dyn bedcode_wasm_core::host_api::context::TaskEngine>, String> {
    block_on_async(host_ctx.task_engine())
        .ok_or_else(|| "task: engine not available (host-task 执行引擎未注入)".to_string())
}

/// `execute-batch`：同步批（扇出 → join → 一次性返回全部单元结果）。
/// 阻塞 Store 至全部单元终态（或超时）——同 `run-sync` 语义，仅限快操作。
///
/// **pub**：宿主集成测试 `tests/task_e2e.rs` 的两个引擎语义用例（空 units / 未知
/// kind fail-collect）直调本入口（迁移前内核 task_e2e 同款直调；其余四函数仍
/// `pub(crate)`——e2e 走 guest 命令路径）。
pub fn execute_batch(host_ctx: &WasmHostContext, plugin_id: &str, plan_json: &str) -> Result<String, String> {
    if !check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_execute_batch") {
        return Err("permission denied".to_string());
    }
    engine(host_ctx)?.execute_batch(plugin_id, plan_json)
}

/// `submit`：异步任务，登记后立即返回 `task-<hex>` 句柄；进度/终态经
/// `events-task#on-task-event` 回调；`cancel` 协作式取消。
pub(crate) fn submit(host_ctx: &WasmHostContext, plugin_id: &str, plan_json: &str) -> Result<String, String> {
    if !check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_submit") {
        return Err("permission denied".to_string());
    }
    engine(host_ctx)?.submit(plugin_id, plan_json)
}

/// `status`：任务状态自愈快照（事件丢失后查询）。`Ok(None)` = 不存在 / 非属主。
pub(crate) fn status(host_ctx: &WasmHostContext, plugin_id: &str, job_id: &str) -> Result<Option<String>, String> {
    if !check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_status") {
        return Err("permission denied".to_string());
    }
    engine(host_ctx)?.status(plugin_id, job_id)
}

/// `cancel`：协作式取消（正在执行的单元跑完或超时，未开始单元 skipped）；幂等。
pub(crate) fn cancel(host_ctx: &WasmHostContext, plugin_id: &str, job_id: &str) -> Result<bool, String> {
    if !check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_cancel") {
        return Err("permission denied".to_string());
    }
    engine(host_ctx)?.cancel(plugin_id, job_id)
}

/// `list-jobs`：本插件在册任务清单（自愈快照）
pub(crate) fn list_jobs(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !check_permission(host_ctx, plugin_id, PERMISSION_TASK_RUN, "host_task_list_jobs") {
        return Err("permission denied".to_string());
    }
    engine(host_ctx)?.list_jobs(plugin_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_wasm_core::host_api::grant_permissions;
    use bedcode_wasm_core::test_support::build_host_ctx_at;
    use std::sync::Arc;

    /// 无头（AppHandle=None）宿主上下文——轻量构建器，不建 WasmRuntime
    fn ctx() -> Arc<WasmHostContext> {
        build_host_ctx_at(None)
    }

    #[test]
    fn task_requires_permission() {
        // 无 task:run：全部入口拒绝（五同步点之一：host_impl 权限门，先于引擎）
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
    fn task_engine_missing_fails_visible() {
        // 无头上下文未注入引擎（build_host_ctx_at 无 set_task_engine）：
        // 过权限门后必须显性报错，不得静默降级成空/成功
        let ctx = ctx();
        grant_permissions(&ctx, "com.bedcode.task-test", &[PERMISSION_TASK_RUN]);
        let err = execute_batch(&ctx, "com.bedcode.task-test", r#"{"units":[]}"#).unwrap_err();
        assert!(err.contains("engine not available"), "got: {err}");
        assert!(submit(&ctx, "com.bedcode.task-test", r#"{"units":[]}"#).is_err());
        assert!(status(&ctx, "com.bedcode.task-test", "task-x").is_err());
        assert!(cancel(&ctx, "com.bedcode.task-test", "task-x").is_err());
        assert!(list_jobs(&ctx, "com.bedcode.task-test").is_err());
    }

    // 真实执行语义（空 units 校验 / 未知 kind fail-collect / 执行器分发）由
    // 宿主集成测试覆盖（tests/task_e2e.rs，经 setup_wasm_runtime 注入真实引擎
    // + 执行器自报收集）——本文件单测只守门禁与注入契约。

    // ==================== 白名单 / 自报三件一致（与 crypto/auth 样板同款） ====================

    /// 白名单声明、接口路径、权限位三件与本域常量逐字一致
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(MODULE_NAME, "task", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin/host-task"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            MODULE_PERMISSIONS,
            &["task:run"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致（装载期一致性核对用）"
        );
    }
}
