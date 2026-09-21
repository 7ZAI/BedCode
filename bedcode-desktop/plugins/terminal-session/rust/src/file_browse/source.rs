//! 文件浏览域的宿主能力端口（票 03）
//!
//! 端口按**能力语义**暴露（目录直读 / canonicalize / stat / 读文本 / 存在性 /
//! git 同步执行），wasm 实现委托 `host-fs` / `host-process`（v19 追加函数）；
//! native 单测注入内存实现。与 `config/store.rs` 同模式——native 链接不引用
//! wasm 专属 import 符号。

use bedcode_plugin_api::host::{FsDirEntry, FsStat};

/// 文件系统端口（host-fs，权限 fs:read）
pub trait FsPort {
    /// 目录直读（`[{name, nodeType}]`；symlink 等为 "other"）
    fn read_dir(&self, path: &str) -> Result<Vec<FsDirEntry>, String>;
    /// canonicalize 绝对路径；不存在返回 Ok(None)
    fn canonicalize(&self, path: &str) -> Result<Option<String>, String>;
    /// 文件元数据；不存在返回 Ok(None)
    fn stat(&self, path: &str) -> Result<Option<FsStat>, String>;
    /// 读文本文件；不存在返回 Ok(None)；二进制返回 Err
    fn read(&self, path: &str) -> Result<Option<String>, String>;
    /// 路径是否存在
    fn exists(&self, path: &str) -> Result<bool, String>;
}

/// git 同步执行端口（host-process run-sync，权限 process:run）
pub trait GitPort {
    /// 同步执行并返回结果（stdout/stderr/exit_code/timed_out）
    fn run(
        &self,
        cwd: &str,
        args: &[&str],
    ) -> Result<bedcode_plugin_api::host::ProcessSyncResult, String>;

    /// 批量只读执行（v20 host-task：生产实现用 `execute-batch` 真并行——status
    /// 树等互相独立的只读 git 命令并发执行，替代串行排队）；结果按入参顺序返回
    /// （fail-collect：单条失败不中断其余，错误定位与该命令单测语义一致）。
    /// native 测试的 Mock 实现保持默认串行（确定性注入不变）。
    fn run_batch(
        &self,
        cwd: &str,
        batch: &[Vec<String>],
    ) -> Vec<Result<bedcode_plugin_api::host::ProcessSyncResult, String>> {
        batch
            .iter()
            .map(|args| self.run(cwd, &args.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
            .collect()
    }
}

// ==================== wasm 实现（host-fs / host-process） ====================

#[cfg(target_arch = "wasm32")]
pub mod wasm_impl {
    use super::*;
    use bedcode_plugin_api::host::{HostFs, HostProcess};
    use bedcode_plugin_api::wasm_host::WasmHost;

    impl FsPort for WasmHost {
        fn read_dir(&self, path: &str) -> Result<Vec<FsDirEntry>, String> {
            HostFs::fs_read_dir(self, path)
                .map_err(|e| format!("file browse: read_dir '{}' failed: {}", path, e.message))
        }

        fn canonicalize(&self, path: &str) -> Result<Option<String>, String> {
            HostFs::fs_canonicalize(self, path)
                .map_err(|e| format!("file browse: canonicalize '{}' failed: {}", path, e.message))
        }

        fn stat(&self, path: &str) -> Result<Option<FsStat>, String> {
            HostFs::fs_stat(self, path)
                .map_err(|e| format!("file browse: stat '{}' failed: {}", path, e.message))
        }

        fn read(&self, path: &str) -> Result<Option<String>, String> {
            HostFs::fs_read(self, path)
                .map_err(|e| format!("file browse: read '{}' failed: {}", path, e.message))
        }

        fn exists(&self, path: &str) -> Result<bool, String> {
            HostFs::fs_exists(self, path)
                .map_err(|e| format!("file browse: exists '{}' failed: {}", path, e.message))
        }
    }

    impl GitPort for WasmHost {
        fn run(
            &self,
            cwd: &str,
            args: &[&str],
        ) -> Result<bedcode_plugin_api::host::ProcessSyncResult, String> {
            let request = serde_json::json!({
                "command": "git",
                "args": args,
                "cwd": cwd,
                "timeout_ms": 60_000,
            });
            HostProcess::process_run_sync(self, &request.to_string())
                .map_err(|e| format!("file browse: git run failed: {}", e.message))
        }

        /// 批量只读执行：host-task `execute-batch` 真并行（v20）。
        ///
        /// 单元 params = run-sync request 原样（command=git），宿主池线程并发；
        /// 结果按入参顺序回填——transport 级失败（execute-batch 全拒）按序铺 Err，
        /// 单元级 succeed/fail 走 fail-collect（单条失败不中断其余，错误随该单元）。
        /// 错误文案与 [`Self::run`] 同源（同一个 process_run_sync 宿主实现），调用方
        /// 逐条映射时保持既有的 `Internal error:` 前缀不变。
        fn run_batch(
            &self,
            cwd: &str,
            batch: &[Vec<String>],
        ) -> Vec<Result<bedcode_plugin_api::host::ProcessSyncResult, String>> {
            use bedcode_plugin_api::host::{HostTask, TaskPlan, TaskUnit};

            let units: Vec<TaskUnit> = batch
                .iter()
                .enumerate()
                .map(|(i, args)| {
                    TaskUnit::process_run_sync(
                        &format!("git{i}"),
                        serde_json::json!({
                            "command": "git",
                            "args": args,
                            "cwd": cwd,
                            "timeout_ms": 60_000,
                        }),
                    )
                })
                .collect();
            let raw = match HostTask::execute_batch(self, &TaskPlan::new(units).to_json()) {
                Ok(r) => r,
                Err(e) => {
                    // 批次级失败（如宿主拒绝 plan / 权限缺失）：全部置同样的传输错误
                    let msg = format!("file browse: git batch failed: {}", e.message);
                    return (0..batch.len()).map(|_| Err(msg.clone())).collect();
                }
            };

            let parsed: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => {
                    let msg = "file browse: git batch: invalid host response".to_string();
                    return (0..batch.len()).map(|_| Err(msg.clone())).collect();
                }
            };
            let results = parsed["results"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            // 按入参顺序取单元结果（host 保序）：ok → 恢复 ProcessSyncResult；
            // !ok → Err（与 run 的 transport 错误同语义，前缀由调用方按 run_git_lines 规则加）
            let mut out = Vec::with_capacity(batch.len());
            for (i, args) in batch.iter().enumerate() {
                let entry = results.get(i).cloned().unwrap_or(serde_json::Value::Null);
                if entry["ok"] == true {
                    match entry["value"].as_str().and_then(|v| serde_json::from_str::<bedcode_plugin_api::host::ProcessSyncResult>(v).ok()) {
                        Some(r) => out.push(Ok(r)),
                        None => out.push(Err(format!(
                            "file browse: git batch unit {} ({}): invalid result payload",
                            i,
                            args.join(" ")
                        ))),
                    }
                } else {
                    let err = entry["error"]
                        .as_str()
                        .unwrap_or("unknown batch unit error")
                        .to_string();
                    out.push(Err(err));
                }
            }
            out
        }
    }
}

// ==================== Tests：内存实现（native 单测注入） ====================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// 内存 fs（native 单测注入）：行语义与 host-fs 对齐
    ///
    /// 结构：真实临时目录上的薄封装——canonicalize/stat/read_dir 走 std::fs，
    /// 与宿主 host_impl/fs.rs 的语义一致（canonicalize 后 component 判定）。
    pub struct MockFs;

    impl FsPort for MockFs {
        fn read_dir(&self, path: &str) -> Result<Vec<FsDirEntry>, String> {
            let read = std::fs::read_dir(path)
                .map_err(|e| format!("read_dir '{}' failed: {}", path, e))?;
            let mut entries = Vec::new();
            for entry in read {
                let entry = entry.map_err(|e| format!("read_dir entry failed: {}", e))?;
                let name = entry.file_name().to_string_lossy().to_string();
                let ft = entry
                    .file_type()
                    .map_err(|e| format!("file type failed: {}", e))?;
                let node_type = if ft.is_dir() {
                    bedcode_plugin_api::host::FsNodeType::Folder
                } else if ft.is_file() {
                    bedcode_plugin_api::host::FsNodeType::File
                } else {
                    bedcode_plugin_api::host::FsNodeType::Other
                };
                entries.push(FsDirEntry { name, node_type });
            }
            Ok(entries)
        }

        fn canonicalize(&self, path: &str) -> Result<Option<String>, String> {
            match std::fs::canonicalize(path) {
                Ok(c) => Ok(Some(c.to_string_lossy().to_string())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("canonicalize '{}' failed: {}", path, e)),
            }
        }

        fn stat(&self, path: &str) -> Result<Option<FsStat>, String> {
            match std::fs::metadata(path) {
                Ok(m) => Ok(Some(FsStat {
                    size: m.len(),
                    is_file: m.is_file(),
                    is_dir: m.is_dir(),
                })),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("stat '{}' failed: {}", path, e)),
            }
        }

        fn read(&self, path: &str) -> Result<Option<String>, String> {
            match std::fs::read_to_string(path) {
                Ok(s) => Ok(Some(s)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("read '{}' failed: {}", path, e)),
            }
        }

        fn exists(&self, path: &str) -> Result<bool, String> {
            Ok(std::path::Path::new(path).exists())
        }
    }

    /// 内存 git（native 单测注入）：按 (cwd, args 拼接) 查预置输出表
    pub struct MockGit {
        /// (cwd, args-joined) → stdout
        pub outputs: Mutex<HashMap<(String, String), String>>,
        /// 命令执行记录（断言调用顺序与参数）
        pub calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl MockGit {
        pub fn new(outputs: HashMap<(String, String), String>) -> Self {
            Self {
                outputs: Mutex::new(outputs),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl GitPort for MockGit {
        fn run(
            &self,
            cwd: &str,
            args: &[&str],
        ) -> Result<bedcode_plugin_api::host::ProcessSyncResult, String> {
            self.calls.lock().unwrap().push((
                cwd.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
            ));
            let key = (cwd.to_string(), args.join(" "));
            let stdout = self
                .outputs
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .unwrap_or_default();
            Ok(bedcode_plugin_api::host::ProcessSyncResult {
                exit_code: Some(0),
                stdout,
                stderr: String::new(),
                timed_out: false,
            })
        }
    }
}
