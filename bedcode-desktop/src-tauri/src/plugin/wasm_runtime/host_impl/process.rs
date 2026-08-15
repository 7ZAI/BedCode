//! 进程域宿主实现（外部进程执行，v8 host-process）
//!
//! 调度框架类插件（如计划任务）经 `process_run` 在桌面端宿主进程内 spawn
//! 外部命令/脚本：异步执行立即返回 run-id，进程结束后宿主经
//! `PluginServices::dispatch_process_done` 回调插件（携带 exit_code / 超时标记）。
//!
//! 架构要点：
//! - **Child 由执行任务独占持有**：进程注册表（`ProcessRegistry`）只记录 pid
//!   而非 Child 句柄 —— `Child::wait` 在整个进程生命周期内独占 `&mut self`，
//!   若注册表同时持 Child 句柄，kill 路径将阻塞到进程自然退出（死锁）。
//!   按 pid 杀进程组（unix `kill -9 -pgid` / Windows `taskkill /T /F /PID`）
//!   与 wait 天然无冲突，且能连带终止子进程树（超时 kill 与插件取消同语义）。
//! - **权限门禁**：`process:run`（高危：执行任意命令），manifest 声明即信任，
//!   每次执行由宿主全量审计日志（命令/参数/cwd/env/结果）。

use crate::plugin::permission::PERMISSION_PROCESS;
use crate::plugin::wasm_runtime::{block_on_async, kill_process_group, WasmHostContext};
use crate::system::error_boundary::spawn_with_error_boundary;
use uuid::Uuid;

/// 默认超时：10 分钟（SDK 契约与插件侧默认值一致）
const DEFAULT_TIMEOUT_MS: u64 = 600_000;

/// process_run 请求（对应 SDK `HostProcess::process_run` 的 request JSON）
///
/// 字段语义见 SDK `host/process.rs` 文档；未知字段忽略（serde 默认行为）。
#[derive(Debug, serde::Deserialize)]
struct ProcessRequest {
    /// 必填：可执行程序（含路径或 PATH 查找）
    command: String,
    /// 可选：参数列表
    #[serde(default)]
    args: Vec<String>,
    /// 可选：工作目录（缺省继承宿主进程）
    #[serde(default)]
    cwd: Option<String>,
    /// 可选：附加环境变量（合并到继承的 env）
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
    /// 可选：超时（毫秒），超时 kill 并标记 timed_out
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
    /// 必填：stdout/stderr 合并落盘文件路径
    output_path: String,
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

/// 启动进程（权限 + 校验 + 注册 + 异步执行），立即返回 run-id
///
/// 异步执行模式与 `session_create` 相同：wasm 调用栈内同步等待子进程会
/// 阻塞 Store；此处 spawn 后台任务执行，wasm 调用立即返回。
pub(crate) fn process_run(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    request_json: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PROCESS, "host_process_run") {
        return Err("permission denied".to_string());
    }
    let request: ProcessRequest = serde_json::from_str(request_json)
        .map_err(|e| format!("process error: invalid request JSON: {}", e))?;
    if request.command.trim().is_empty() {
        return Err("process error: empty command".to_string());
    }
    if request.output_path.trim().is_empty() {
        return Err("process error: empty output_path".to_string());
    }

    // stdout/stderr 合并写同一文件：两个句柄共享同一文件偏移，可交错追加
    // （顺序不保证，但调度脚本日志场景可接受，避免管道缓冲丢尾部输出）
    // 父目录不存在时自动创建：插件按 exec_id 命名输出文件（如
    // <home>/.bedcode/scheduler/<exec_id>.log），目录通常需首次创建。
    // 安全上无新增面：拥有 process:run 的插件本就可执行任意命令
    if let Some(parent) = std::path::Path::new(&request.output_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "process error: create output dir '{}' failed: {}",
                    parent.display(),
                    e
                )
            })?;
        }
    }
    let output_file = std::fs::File::create(&request.output_path).map_err(|e| {
        format!(
            "process error: create output file '{}' failed: {}",
            request.output_path, e
        )
    })?;
    let stderr_file = output_file
        .try_clone()
        .map_err(|e| format!("process error: clone output file handle failed: {}", e))?;

    let mut cmd = tokio::process::Command::new(&request.command);
    cmd.args(&request.args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(output_file))
        .stderr(std::process::Stdio::from(stderr_file));
    if let Some(cwd) = &request.cwd {
        cmd.current_dir(cwd);
    }
    cmd.envs(&request.env);

    // 独立进程组：kill 时连带子进程树（超时 / 插件取消共用同一语义）
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        // CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW：
        // 前者与 taskkill /T 配合整树终止；后者抑制控制台窗口——任务多为
        // cmd /C、.bat、python 等控制台程序，无标志会弹出黑窗一闪而过
        cmd.creation_flags(0x0000_0200 | 0x0800_0000);
    }

    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "process error: spawn '{}' failed: {}",
            request.command, e
        )
    })?;
    // process_group(0) 后 pgid == pid；spawn 成功即应有 pid（极端情况兜底 0）
    let pid = child.id().unwrap_or(0);
    let run_id = Uuid::new_v4().to_string();

    let registry = host_ctx.process_registry().clone();
    registry.register(run_id.clone(), plugin_id.to_string(), pid);

    let pid_str = plugin_id.to_string();
    let rid = run_id.clone();
    let timeout_ms = request.timeout_ms.max(1);
    // 提前捕获 services（进程运行时宿主必然已完成注入；测试/无头为 None）：
    // 避免把 &WasmHostContext 引用送入 'static 任务（无法克隆 Arc）
    let services = block_on_async(host_ctx.services());
    spawn_with_error_boundary("host_process_run", async move {
        let timeout = std::time::Duration::from_millis(timeout_ms);
        let (exit_code, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) => (status.code(), false),
            Ok(Err(e)) => {
                tracing::error!(
                    plugin_id = %pid_str,
                    run_id = %rid,
                    error = %e,
                    "host_process_run: wait failed"
                );
                (None, false)
            }
            Err(_) => {
                // 超时：先杀进程组（连带子进程），再 kill + wait 回收防僵尸
                tracing::warn!(
                    plugin_id = %pid_str,
                    run_id = %rid,
                    timeout_ms = timeout_ms,
                    "host_process_run: timed out, killing process group"
                );
                kill_process_group(pid).await;
                let _ = child.kill().await;
                let _ = child.wait().await;
                (None, true)
            }
        };
        registry.remove(&rid);

        // 事件回灌插件；services 为 None（测试/无头环境）仅记日志
        let event = serde_json::json!({
            "run_id": rid,
            "exit_code": exit_code,
            "timed_out": timed_out,
        });
        match services {
            Some(services) => services.dispatch_process_done(pid_str, event),
            None => {
                tracing::debug!(
                    plugin_id = %pid_str,
                    "host_process_run: no PluginServices, done event skipped"
                );
            }
        }
    });

    tracing::info!(
        plugin_id = %plugin_id,
        run_id = %run_id,
        command = %request.command,
        "host_process_run: started"
    );
    Ok(run_id)
}

/// 终止进程（权限 + 注册表查找 + 进程组 kill）
///
/// 尽力而为：进程可能已结束/未被找到（SDK 契约约定此时返回 Ok）。
/// kill 成功后执行任务侧的 `wait` 随即返回，完成事件照常分发。
pub(crate) fn process_kill(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    run_id: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PROCESS, "host_process_kill") {
        return Err("permission denied".to_string());
    }
    if run_id.is_empty() {
        return Err("process error: empty run_id".to_string());
    }
    let registry = host_ctx.process_registry().clone();
    let rid = run_id.to_string();
    let found = block_on_async(registry.kill(&rid));
    if !found {
        // 进程已结束/已移除属预期内（kill 与完成事件竞态），仅记 debug
        tracing::debug!(
            plugin_id = %plugin_id,
            run_id = %run_id,
            "host_process_kill: run not found, best-effort ok"
        );
    }
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};

    const PLUGIN: &str = "test-plugin";

    fn request(command: &str, args: Vec<&str>, output_path: &str) -> String {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        serde_json::json!({
            "command": command,
            "args": args,
            "output_path": output_path,
            "timeout_ms": 10_000,
        })
        .to_string()
    }

    /// 无 process:run 权限：run 被拒绝
    #[test]
    fn process_run_permission_denied() {
        let ctx = build_host_ctx();
        let err = process_run(&ctx, PLUGIN, "{}").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 空 command：权限通过后参数校验拒绝
    #[test]
    fn process_run_empty_command_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_PROCESS]);
        let err = process_run(&ctx, PLUGIN, &request("", vec![], "/tmp/x.log")).unwrap_err();
        assert!(err.contains("empty command"), "got: {}", err);
    }

    /// 空 output_path：权限通过后参数校验拒绝
    #[test]
    fn process_run_empty_output_path_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_PROCESS]);
        let err = process_run(&ctx, PLUGIN, &request("echo", vec!["hi"], "")).unwrap_err();
        assert!(err.contains("empty output_path"), "got: {}", err);
    }

    /// 非法请求 JSON：拒绝并给出可读错误
    #[test]
    fn process_run_invalid_json_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_PROCESS]);
        let err = process_run(&ctx, PLUGIN, "not-json").unwrap_err();
        assert!(err.contains("invalid request JSON"), "got: {}", err);
    }

    /// 无 process:run 权限：kill 被拒绝
    #[test]
    fn process_kill_permission_denied() {
        let ctx = build_host_ctx();
        let err = process_kill(&ctx, PLUGIN, "r1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 空 run_id：权限通过后参数校验拒绝（防误杀全量）
    #[test]
    fn process_kill_empty_run_id_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_PROCESS]);
        let err = process_kill(&ctx, PLUGIN, "").unwrap_err();
        assert!(err.contains("empty run_id"), "got: {}", err);
    }

    /// 真实 spawn：stdout 落盘（父目录不存在时宿主自动创建）+ 注册表移除
    ///
    /// services 为 None（测试上下文）→ 完成事件仅记日志，不影响结果。
    /// 输出目录故意不预创建：验证 host-process 对嵌套路径的自动建目录。
    #[tokio::test]
    async fn process_run_spawns_and_writes_output() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_PROCESS]);
        let dir = std::env::temp_dir().join(format!("bedcode-proc-run-{}", Uuid::new_v4()));
        let out = dir.join("nested").join("deeper").join("out.log");
        let (cmd, args) = if cfg!(target_os = "windows") {
            ("cmd", vec!["/C", "echo hello-from-process"])
        } else {
            ("sh", vec!["-c", "echo hello-from-process"])
        };
        let run_id = process_run(&ctx, PLUGIN, &request(cmd, args, out.to_str().unwrap()))
            .expect("run ok");
        assert_eq!(run_id.len(), 36);

        // 等待后台任务完成（输出落盘 + 注册表移除）
        let registry = ctx.process_registry().clone();
        for _ in 0..200 {
            if registry.running_count() == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(registry.running_count(), 0, "registry entry not removed");

        let content = std::fs::read_to_string(&out).expect("read output file");
        assert!(content.contains("hello-from-process"), "output: {}", content);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// kill 路径：spawn 长跑进程 → process_kill → 进程组被终止 → 注册表移除
    #[tokio::test]
    async fn process_kill_terminates_process_group() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_PROCESS]);
        let dir = std::env::temp_dir().join(format!("bedcode-proc-kill-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let out = dir.join("out.log");
        let (cmd, args) = if cfg!(target_os = "windows") {
            // ping 阻塞 60s，由 cmd /C 拉起（进程树：cmd → ping）
            ("cmd", vec!["/C", "ping -n 60 127.0.0.1 >nul"])
        } else {
            // sh 拉起 sleep（进程组：sh → sleep），kill 组须连带终止
            ("sh", vec!["-c", "sleep 60"])
        };
        let run_id = process_run(&ctx, PLUGIN, &request(cmd, args, out.to_str().unwrap()))
            .expect("run ok");

        // 等待注册完成，确认进程在跑
        let registry = ctx.process_registry().clone();
        for _ in 0..100 {
            if registry.running_count() == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(registry.running_count(), 1, "process not registered");

        process_kill(&ctx, PLUGIN, &run_id).expect("kill ok");

        // 进程组被终止 → 执行任务 wait 返回并移除注册表项
        for _ in 0..200 {
            if registry.running_count() == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(registry.running_count(), 0, "process group not killed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
