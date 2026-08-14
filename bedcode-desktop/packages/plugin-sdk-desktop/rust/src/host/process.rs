//! 宿主能力：外部进程执行（v8，host-process）
//!
//! 供调度框架类插件（如计划任务）执行 shell 脚本 / 内联命令。
//! 进程在桌面端宿主进程内 spawn（WASM 插件无法直接创建进程），
//! 异步执行：`process_run` 立即返回 run-id，进程结束后宿主经
//! [`WasmPlugin::on_process_done`](crate::wasm::WasmPlugin::on_process_done)
//! 回调插件（携带 exit_code / 超时标记）。
//!
//! 需要 `process:run` 权限（manifest `permissions` 声明，安装即信任）。

use super::HostError;

/// 宿主进程执行
pub trait HostProcess {
    /// 启动进程（异步执行，立即返回 run-id）
    ///
    /// `request_json` 结构：
    /// ```json
    /// {
    ///   "command": "bash",            // 必填：可执行程序（含路径或 PATH 查找）
    ///   "args": ["-c", "echo hi"],    // 可选：参数列表
    ///   "cwd": "/path/to/dir",        // 可选：工作目录（缺省继承宿主进程）
    ///   "env": { "K": "V" },          // 可选：附加环境变量（合并到继承的 env）
    ///   "timeout_ms": 600000,         // 可选：超时（毫秒），超时 kill 并标记 timed_out
    ///   "output_path": "/abs/out.log" // 必填：stdout/stderr 合并落盘文件路径
    /// }
    /// ```
    ///
    /// 进程结束后宿主调用 `on_process_done` 回调：
    /// `{ run_id, exit_code, timed_out }`（exit_code 为 None = 被信号终止）。
    fn process_run(&self, request_json: &str) -> Result<String, HostError>;

    /// 终止进程（超时/取消）
    ///
    /// 尽力而为：进程可能已结束（此时返回 Ok）。终止的是进程组
    /// （含子进程），与超时 kill 同一语义。
    fn process_kill(&self, run_id: &str) -> Result<(), HostError>;
}
