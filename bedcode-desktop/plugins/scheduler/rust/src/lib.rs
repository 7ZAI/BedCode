//! Task Scheduler Plugin (WASM)
//!
//! 通用调度框架（spec：`.scratch/task-scheduler-plugin/spec.md`）：
//! cron 6 段表达式触发，经 host-process 在桌面端宿主进程内执行 shell 脚本/
//! 内联命令，与 agent 会话无关。管理主入口为 CLI（bedtask，issue 03），
//! 本插件提供 HTTP 端点 + tick 调度引擎（issue 02）。
//!
//! 模块划分：
//! - `cron`：cron 6 段解析器（纯函数，本地时区字符串基准）
//! - `engine`：任务/执行记录数据模型 + tick 状态机 + HTTP 端点 + 事件广播
//! - `api`：互调 api（spec §9.5，`#[plugin_api]` 宏生成 JSON-RPC 分派）
//!
//! 时间基准：WASM 无系统时钟，tick 的 now_local 由宿主注入，时间戳由
//! 宿主 DB（SQLite datetime('now','localtime')）计算（spec §13）。

mod api;
mod cron;
mod engine;

use bedcode_plugin_api::events::ProcessDoneEvent;
use bedcode_plugin_api::host::{HostApp, HostLog, HostPluginDatabase, HostTimer};
use bedcode_plugin_api::http_response;
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::{BusMessage, CommandArgs, WasmHost, WasmPlugin};

struct SchedulerPlugin;

impl WasmPlugin for SchedulerPlugin {
    const ID: &'static str = "com.bedcode.scheduler";

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Scheduler plugin activated");

        // 错过恢复（幂等，spec §5.2）：重启残留 waiting/running → missed；
        // 过期任务（超过宽限）→ missed 执行 + next_at 推进到下一未来时刻
        engine::recover(&host);

        // 安装随包 CLI（bedtask，spec §8.1）：复制到用户 bin 目录 + 注册 PATH。
        // 幂等（宿主侧去重/覆盖）；失败不阻断插件（CLI 是管理入口，调度照常）
        match host.cli_install("bedtask", "") {
            Ok(bin_dir) => host.log_info(&format!("bedtask CLI installed to {}", bin_dir)),
            Err(e) => host.log_error(&format!("bedtask CLI install failed: {}", e)),
        }

        // 宿主周期定时器：到点回调 task-scheduler.tick（附本地/UTC 时间）。
        // activate 与 on_startup 都注册：幂等（重复注册替换旧实例），
        // 覆盖"应用启动时已启用"与"稍后手动启用"两种路径
        match host.timer_register(engine::SCHEDULER_INTERVAL_SECS, "task-scheduler.tick") {
            Ok(()) => host.log_info(&format!(
                "Scheduler timer registered: interval={}s",
                engine::SCHEDULER_INTERVAL_SECS
            )),
            Err(e) => host.log_error(&format!("Failed to register scheduler timer: {}", e)),
        }

        // 订阅互调请求 topic（宏生成）：`bedcode.api.<api>` 逐个订阅（宿主去重），
        // 其他插件才能经 JSON-RPC 调用本插件声明的 api（spec §9.5）
        api::ScheduleApiDispatcher::register()?;

        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Scheduler plugin deactivated");
        // 卸载随包 CLI（spec §8.1）：删文件 + 移除仅本插件的 PATH 条目。
        // 应用关闭流程中宿主自动跳过（CLI 保留，下次激活幂等重装）；
        // 失败不阻断（残留由下次 activate 的幂等安装覆盖/去重）
        if let Err(e) = host.cli_uninstall("bedtask", "") {
            host.log_warn(&format!("bedtask CLI uninstall failed: {}", e));
        }

        // 停用 = 定时器回调被宿主门禁跳过（插件未激活时 invoke 返回 Err 仅记日志），
        // 已有运行中进程由宿主进程注册表继续执行至完成（事件回灌失败仅记日志）
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let host = WasmHost;
        let args = CommandArgs::new(args);

        match name {
            "_http_endpoint" => {
                let method = args.str_or("method", "");
                let path = args.str_or("path", "");
                let body = args.value_owned("body").unwrap_or(serde_json::Value::Null);
                let query = args.value_owned("query").unwrap_or(serde_json::json!({}));

                if let Some(scheduler_path) = path.strip_prefix("task-scheduler/") {
                    Ok(engine::handle_scheduler_http(
                        &host, &method, scheduler_path, &body, &query,
                    ))
                } else {
                    Ok(http_response::error(404, &format!("Not found: {}", path)))
                }
            }
            "task-scheduler.tick" => {
                // 宿主定时器到点回调：now_local 为本地时间字符串（spec §5.1），
                // 与 SQLite datetime('now','localtime') 同格式，字典序可比
                let now_local = args.str_or("now_local", "");
                if now_local.is_empty() {
                    return Err(anyhow::anyhow!("task-scheduler.tick: missing now_local"));
                }
                engine::handle_tick(&host, &now_local);
                Ok(serde_json::json!({ "ticked": true }))
            }
            _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
        }
    }

    /// 总线消息入口：互调请求先经宏生成的分派器（命中 `bedcode.api.*` 请求
    /// topic 则调用对应方法并回响应，返回 true）；其余消息不处理返回 false，
    /// 保持总线既有语义（本插件无其他订阅）
    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        let _ = api::ScheduleApiDispatcher::dispatch::<Self>(msg)?;
        Ok(())
    }

    fn on_startup() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Scheduler plugin on_startup");

        // 初始化插件独立数据库（宿主 plugin_db_execute 仅执行单条语句，
        // schema 按语句数组逐条执行，见 auto-task 同模式）
        for stmt in engine::SCHEDULED_JOBS_SCHEMA {
            match host.plugin_db_execute(stmt) {
                Ok(_) => {}
                Err(e) => {
                    host.log_error(&format!("Failed to initialize scheduled_jobs table: {}", e));
                    break;
                }
            }
        }
        host.log_info("scheduled_jobs table initialized");

        for stmt in engine::JOB_EXECUTIONS_SCHEMA {
            match host.plugin_db_execute(stmt) {
                Ok(_) => {}
                Err(e) => {
                    host.log_error(&format!("Failed to initialize job_executions table: {}", e));
                    break;
                }
            }
        }
        host.log_info("job_executions table initialized");

        Ok(())
    }

    fn on_shutdown() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Scheduler plugin on_shutdown");
        // 运行中进程随宿主进程终止（孤儿回收交给 OS）；残留执行记录
        // 由下次 activate 的 recover() 置 missed（重启语义）
        Ok(())
    }

    fn on_process_done(event: &ProcessDoneEvent) -> anyhow::Result<()> {
        let host = WasmHost;
        // host-process 完成事件：回写执行记录 + 推进 next_at + 继续调度排队
        engine::handle_process_done(&host, &event.run_id, event.exit_code, event.timed_out);
        Ok(())
    }
}

bedcode_plugin_api::wasm_entry!(SchedulerPlugin);
