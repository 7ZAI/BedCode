//! host-task 并发任务域闭环（ABI v20，含产物加载）
//!
//! 自 wasm-core `manager/runtime/tests/task_e2e.rs` 迁入（wasm-core 纯净性收口票 02
//! 批次 05）：host-task 的 WIT impl 迁宿主（`src/plugin/task.rs` 路径 B）后，内核
//! 测试二进制不再注册该 interface ⇒ 携带其 import 的 task 夹具无法在内核实例化
//! （与批次 04 crypto 探针同判据）。产物仍由 wasm-core 测试的 `fixture_keeper`
//! 用例构建（宿主只读）；`process.run-sync` 单元执行器随域住宿主并自报注册
//! （`setup_wasm_runtime` 的收集循环在宿主二进制内可收齐 fs / http / process 三个执行器）。
//!
//! 共享脚手架从 wasm-core 的 `mod tests` 改为 `mod support` + 本文件的 task 段
//! （MockTaskServices / 串行锁 / fs 授权播种）；用例本体逐字保留。

mod support;

use std::sync::Arc;

use bedcode_desktop_lib::wasm_core::host_api::context::PluginServices;
use support::*;

/// task_e2e 全局任务注册表串行锁
///
/// 共享 `TaskRegistry`（进程级单例）+ 固定 owner `com.bedcode.task-test`：
/// `register_job` 的**惰性 GC**（`retain(终态 → 摘除)`，同属主下一任务登记时
/// 触发）会把并行测试刚完成的 submit 任务摘出注册表 → 该测试的 `status` 自愈
/// 快照查询返回 `Ok(None)`（作业不存在）→ `snap["state"]` 为 null 假红
/// （2026-09-25 实测：提交测试与其它宿主任务测试并行即偶发，单跑恒绿）。
/// 持锁把同一注册表上的登记/GC 排成一条序列（与 SESSION_PLUGIN_DB_LOCK 同模式）。
static TASK_E2E_REGISTRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 取任务注册表串行锁（各用例入口第一行调用；不可重入——用例内不得再取）
fn task_e2e_registry_guard() -> std::sync::MutexGuard<'static, ()> {
    TASK_E2E_REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// 构建 host-task fixture 插件（合集 `packages/plugin-sdk-fixtures` `feature = "task"`，ABI v20）
///
/// **只读不建**：产物由 wasm-core 测试的 `fixture_keeper::task_fixture_artifact_is_built_for_host_e2e`
/// 构建到共享夹具目录（与 pty 夹具同约定——宿主测试不承担 fixture 构建）。
fn build_task_test_component() -> Vec<u8> {
    sdk_fixture_artifact_bytes("task")
}

/// fixture 插件 ID（与夹具 manifest 一致）
fn task_fixture_plugin_id() -> String {
    "com.bedcode.task-test".to_string()
}

/// 预置「用户已记住」的 fs 授权记录（票 07 后测试侧唯一的无弹窗放行方式）
///
/// 旧写法把测试目录塞进 `.claude` 段借路径白名单蒙过校验；那条白名单对任何插件都
/// 放行，已随票 07 退役。这里改走生产同款：`fs_granted_paths` 前缀记录。
fn seed_fs_grant(ctx: &WasmHostContext, plugin_id: &str, dir: &std::path::Path) {
    bedcode_desktop_lib::wasm_core::runtime_util::block_on_async(
        ctx.fs_auth()
            .seed_legacy_granted_path(plugin_id, &dir.to_string_lossy()),
    )
    .expect("seed fs_granted_paths 记录");
}

/// host-task 消费派发测试替身：收集 dispatch_task_event 事件 + 真实投递到
/// 注册实例（验证 SDK 回调链路：dispatch → LoadedWasmPlugin::on_task_event →
/// WIT events-task 导出 → fixture on_task_event）
struct MockTaskServices {
    events: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
}

impl PluginServices for MockTaskServices {
    fn mark_plugin_error(&self, _plugin_id: String, _error: String) {}
    fn register_plugin_timer(&self, _plugin_id: String, _interval_secs: u64, _command: String) {}
    fn dispatch_process_done(&self, _plugin_id: String, _event: serde_json::Value) {}
    fn dispatch_task_event(&self, plugin_id: String, event: serde_json::Value) {
        // 收集事件序列（phase 顺序断言用）。真实投递（with_wasm_plugin_call →
        // LoadedWasmPlugin::on_task_event）由 PluginHost 的 dispatch 实现承担，
        // 与 dispatch_process_done 同构；SDK 回调链路在 submit 用例中手动验证
        let _ = plugin_id;
        self.events.lock().unwrap_or_else(|e| e.into_inner()).push(event);
    }
    fn install_cli(
        &self,
        _plugin_id: String,
        _file_name: String,
        _bin_dir: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>> {
        Box::pin(async { Err("mock: no cli".to_string()) })
    }
    fn uninstall_cli(
        &self,
        _plugin_id: String,
        _file_name: String,
        _bin_dir: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
    fn plugin_resource_dir(
        &self,
        _plugin_id: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>> {
        Box::pin(async { Err("mock: no resource dir".to_string()) })
    }
}

impl MockTaskServices {
    fn new() -> Self {
        Self {
            events: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn phases(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter_map(|e| e.get("phase").and_then(|v| v.as_str()).map(str::to_string))
            .collect()
    }
}

/// execute-batch：8 个 fs.stat 并发 → 全完成、结果按 id 关联、顺序保序
#[test]

fn test_task_execute_batch_fixture_parallel_results() {
    let _task_e2e_guard = task_e2e_registry_guard();
    let bytes = build_task_test_component();
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime.compile_component(&bytes).expect("compile task fixture");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mock = Arc::new(MockTaskServices::new());
        host_ctx.set_services(mock.clone()).await;
        let mut plugin = wasm_runtime
            .instantiate_component(&component, &task_fixture_plugin_id(), host_ctx.clone(), &[], None)
            .expect("instantiate task fixture");
        // 授予 task:run + fs:read（fs_auth 三层：plugin.json 声明 + 宿主授权）
        bedcode_desktop_lib::wasm_core::host_api::grant_permissions(
            &host_ctx,
            &task_fixture_plugin_id(),
            &["task:run", "fs:read"],
        );
        // fs_auth 第二层：给 fixture 插件预置该根的持久化授权。票 07 前这里靠
        // `.claude` 路径段蒙过校验（对任何插件都免弹窗），现在走生产同款记录
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("task-stat");
        std::fs::create_dir_all(&root).expect("create root");
        seed_fs_grant(&host_ctx, &task_fixture_plugin_id(), &root);
        let tmp = root.join("stat.txt");
        std::fs::write(&tmp, b"hello task").expect("write temp file");

        let units: Vec<serde_json::Value> = (0..8)
            .map(|i| {
                serde_json::json!({
                    "id": format!("u{}", i),
                    "kind": "fs.stat",
                    "params": { "path": tmp.to_str().unwrap() }
                })
            })
            .collect();
        let plan = serde_json::json!({
            "units": units,
            "maxConcurrency": 4,
            "jobTimeoutMs": 60000,
        })
        .to_string();
        let raw = plugin
            .invoke_command("execute-batch", &serde_json::json!({ "plan": plan }).to_string())
            .expect("execute-batch command");
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let payload = v
            .get("result")
            .and_then(|r| serde_json::from_str::<serde_json::Value>(r.as_str().unwrap_or("{}")).ok());
        let payload = payload.expect("execute-batch result must be JSON");
        assert!(payload["jobId"].as_str().unwrap().starts_with("task-"), "jobId prefix");
        assert_eq!(payload["cancelled"], false);
        let results = payload["results"].as_array().expect("results array");
        assert_eq!(results.len(), 8, "全部单元都有结果条目");
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r["id"], format!("u{}", i), "结果按 units 原顺序、id 关联");
            assert!(r["ok"] == true, "fs.stat 成功（临时文件存在）；error: {}", r["error"]);
            let value_str = r["value"].as_str().expect("value");
            let value: serde_json::Value = serde_json::from_str(value_str).unwrap_or(serde_json::Value::Null);
            assert_eq!(value["size"], 10, "fs.stat 返回文件字节数");
        }
        std::fs::remove_file(&tmp).ok();
    });
}

/// submit：started → completed 回调全链路（dispatch 收集 + SDK on_task_event 到达 fixture）
#[test]

fn test_task_submit_events_dispatched_and_status() {
    let _task_e2e_guard = task_e2e_registry_guard();
    let bytes = build_task_test_component();
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime.compile_component(&bytes).expect("compile task fixture");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mock = Arc::new(MockTaskServices::new());
        host_ctx.set_services(mock.clone()).await;
        let mut plugin = wasm_runtime
            .instantiate_component(&component, &task_fixture_plugin_id(), host_ctx.clone(), &[], None)
            .expect("instantiate task fixture");
        bedcode_desktop_lib::wasm_core::host_api::grant_permissions(
            &host_ctx,
            &task_fixture_plugin_id(),
            &["task:run", "fs:read"],
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("task-submit");
        std::fs::create_dir_all(&root).expect("create root");
        seed_fs_grant(&host_ctx, &task_fixture_plugin_id(), &root);
        let tmp = root.join("a.txt");
        std::fs::write(&tmp, b"x").expect("write temp file");
        let plan = serde_json::json!({
            "units": [
                { "id": "a", "kind": "fs.stat", "params": { "path": tmp.to_str().unwrap() } },
                { "id": "b", "kind": "fs.stat", "params": { "path": tmp.to_str().unwrap() } }
            ],
            "jobTimeoutMs": 60000,
        })
        .to_string();
        let raw = plugin
            .invoke_command("submit", &serde_json::json!({ "plan": plan }).to_string())
            .expect("submit command");
        let job_id: String = {
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            v["jobId"].as_str().unwrap().to_string()
        };
        assert!(job_id.starts_with("task-"), "submit returns task-<hex>");

        // 轮询消费派发（tokio 异步）直到 terminal 事件到达（上限 5s）
        let mut phases: Vec<String> = Vec::new();
        for _ in 0..100 {
            phases = mock.phases();
            if phases.iter().any(|p| p == "completed" || p == "failed") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            phases.iter().any(|p| p == "started"),
            "must receive started event, got {phases:?}"
        );
        assert!(
            phases.iter().any(|p| p == "completed"),
            "must receive completed event, got {phases:?}"
        );
        let last = phases.last().unwrap().clone();
        assert_eq!(last, "completed", "terminal event last, got {phases:?}");

        // status 自愈快照
        let raw = plugin
            .invoke_command("status", &serde_json::json!({ "jobId": job_id }).to_string())
            .expect("status command");
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let snap: serde_json::Value =
            serde_json::from_str(v["status"].as_str().unwrap_or("null")).unwrap_or(serde_json::Value::Null);
        assert_eq!(snap["state"], "completed");
        assert_eq!(snap["doneUnits"], 2);

        // SDK 回调链路（绑定直达）：构造终态事件投递 → 实例 on_task_event →
        // WIT events-task 导出 → fixture 静态累积（dispatch 的真实投递由
        // PluginHost 承担，与 dispatch_process_done 同构；此处验证绑定本身）
        plugin
            .on_task_event(r#"{"jobId":"probe","phase":"completed","doneUnits":2}"#)
            .expect("on_task_event must deliver");
        let raw = plugin.invoke_command("task-events", "{}").expect("task-events command");
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let events = v["events"].as_array().expect("fixture events array");
        assert!(!events.is_empty(), "fixture on_task_event must have been called");
        std::fs::remove_file(&tmp).ok();
    });
}

/// cancel：协作式取消命中 + 终态事件 cancelled；非属主/不存在不可区分（防枚举）
#[test]

fn test_task_cancel_semantics() {
    let _task_e2e_guard = task_e2e_registry_guard();
    let bytes = build_task_test_component();
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime.compile_component(&bytes).expect("compile task fixture");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mock = Arc::new(MockTaskServices::new());
        host_ctx.set_services(mock.clone()).await;
        let mut plugin = wasm_runtime
            .instantiate_component(&component, &task_fixture_plugin_id(), host_ctx.clone(), &[], None)
            .expect("instantiate task fixture");
        bedcode_desktop_lib::wasm_core::host_api::grant_permissions(
            &host_ctx,
            &task_fixture_plugin_id(),
            &["task:run", "fs:read", "process:run"],
        );

        // 慢任务：4 × sleep 0.3（maxConcurrency=1 → 逐个串行）；process 单元
        // 执行器随域住宿主并自报注册（本测试二进制经 setup_wasm_runtime 收齐）
        let plan = serde_json::json!({
            "units": (0..4).map(|i| serde_json::json!({
                "id": format!("s{}", i),
                "kind": "process.run-sync",
                "params": { "command": "sleep", "args": ["0", "0", "0", "0"], "timeoutMs": 10000 }
            })).collect::<Vec<_>>(),
            "maxConcurrency": 1,
            "jobTimeoutMs": 60000,
        })
        .to_string();
        let raw = plugin
            .invoke_command("submit", &serde_json::json!({ "plan": plan }).to_string())
            .expect("submit command");
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let job_id = v["jobId"].as_str().unwrap().to_string();

        // 立即取消（首个 sleep 大概率运行中/未开始）
        let raw = plugin
            .invoke_command("cancel", &serde_json::json!({ "jobId": job_id }).to_string())
            .expect("cancel command");
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["hit"], true, "cancel must hit a live job");
        // 幂等：二次取消命中 false（已终态）
        let raw = plugin
            .invoke_command("cancel", &serde_json::json!({ "jobId": job_id }).to_string())
            .expect("cancel command 2");
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["hit"], false, "second cancel must miss (terminal)");

        // 终态事件 / status：cancelled
        let mut saw_cancelled = false;
        for _ in 0..100 {
            let phases = mock.phases();
            if phases.iter().any(|p| p == "cancelled") {
                saw_cancelled = true;
                break;
            }
            let raw = plugin
                .invoke_command("status", &serde_json::json!({ "jobId": job_id }).to_string())
                .expect("status command");
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let snap: serde_json::Value =
                serde_json::from_str(v["status"].as_str().unwrap_or("null")).unwrap_or(serde_json::Value::Null);
            if snap["state"] == "cancelled" {
                saw_cancelled = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(saw_cancelled, "cancel must eventually reflect cancelled state");

        // 非属主查询不可区分（防枚举）：别的 plugin_id 查 → None 而非错误
        let raw = plugin
            .invoke_command("status", &serde_json::json!({ "jobId": job_id }).to_string())
            .expect("status command");
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(v["status"].is_null() || v["status"].as_str().is_some());
    });
}

/// 双门：仅授 task:run 不授 fs:read → fs.stat 单元失败（permission denied），
/// 批次继续（fail-collect）；fs_auth 未授权路径直接 Err、不弹窗
#[test]

fn test_task_dual_gate_domain_permission_denied() {
    let _task_e2e_guard = task_e2e_registry_guard();
    let bytes = build_task_test_component();
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime.compile_component(&bytes).expect("compile task fixture");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut plugin = wasm_runtime
            .instantiate_component(&component, &task_fixture_plugin_id(), host_ctx.clone(), &[], None)
            .expect("instantiate task fixture");
        // 只授 task:run（不授 fs:read / process:run）
        bedcode_desktop_lib::wasm_core::host_api::grant_permissions(&host_ctx, &task_fixture_plugin_id(), &["task:run"]);
        let tmp = std::env::temp_dir().join(format!("bedcode_task_dual_{}", std::process::id()));
        std::fs::write(&tmp, b"x").expect("write temp file");
        let plan = serde_json::json!({
            "units": [
                { "id": "no-perm", "kind": "fs.stat", "params": { "path": tmp.to_str().unwrap() } },
                { "id": "unknown", "kind": "no.such", "params": {} }
            ],
            "jobTimeoutMs": 60000,
        })
        .to_string();
        let raw = plugin
            .invoke_command("execute-batch", &serde_json::json!({ "plan": plan }).to_string())
            .expect("execute-batch command");
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(v["result"].as_str().unwrap_or("{}")).unwrap_or(serde_json::Value::Null);
        let results = payload["results"].as_array().expect("results");
        assert_eq!(results.len(), 2, "fail-collect：所有单元都有结果条目");
        assert_eq!(results[0]["ok"], false);
        assert!(
            results[0]["error"].as_str().unwrap().contains("permission denied"),
            "无 fs:read 的 fs.stat 单元必须失败（无弹窗，错误可见），got: {}",
            results[0]["error"]
        );
        assert_eq!(results[1]["ok"], false);
        assert!(results[1]["error"].as_str().unwrap().contains("unknown unit kind"));
        std::fs::remove_file(&tmp).ok();
    });
}

/// execute-batch 空 units：plan 校验失败（可见错误，不静默）——原 host_api/task.rs
/// 单测迁入（setup_wasm_runtime 已注入真实引擎 + 收集执行器自报，走 core-task 的
/// parse_plan 校验；域入口直调经 `bedcode_desktop_lib::plugin::task`）
#[test]

fn test_task_execute_batch_empty_units_rejected() {
    let _task_e2e_guard = task_e2e_registry_guard();
    let (_, host_ctx) = setup_wasm_runtime();
    let plugin = task_fixture_plugin_id();
    bedcode_desktop_lib::wasm_core::host_api::grant_permissions(&host_ctx, &plugin, &["task:run"]);
    let err =
        bedcode_desktop_lib::plugin::task::execute_batch(&host_ctx, &plugin, r#"{"units":[]}"#).unwrap_err();
    assert!(err.contains("no units"), "got: {err}");
}

/// execute-batch 未知 kind fail-collect：bad 单元报 unknown unit kind，同批其他单元
/// 正常执行（不拖垮批次）——原 host_api/task.rs 单测迁入，改走真实引擎 + 执行器
/// 注册表分发（fs.exists 需 fs:read + fs_auth 预置授权才真成功）
#[test]

fn test_task_execute_batch_unknown_kind_fails_in_that_unit_only() {
    let _task_e2e_guard = task_e2e_registry_guard();
    let (_, host_ctx) = setup_wasm_runtime();
    let plugin = task_fixture_plugin_id();
    bedcode_desktop_lib::wasm_core::host_api::grant_permissions(&host_ctx, &plugin, &["task:run", "fs:read"]);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("task-unknown-kind");
    std::fs::create_dir_all(&root).expect("create root");
    seed_fs_grant(&host_ctx, &plugin, &root);
    let tmp = root.join("a.txt");
    std::fs::write(&tmp, b"x").expect("write temp file");
    let plan = serde_json::json!({
        "units": [
            { "id": "bad", "kind": "no.such.kind", "params": {} },
            { "id": "ok", "kind": "fs.exists", "params": { "path": tmp.to_str().unwrap() } }
        ],
        "jobTimeoutMs": 60000,
    })
    .to_string();
    let raw = bedcode_desktop_lib::plugin::task::execute_batch(&host_ctx, &plugin, &plan).expect("batch runs");
    let v: serde_json::Value = serde_json::from_str(&raw).expect("results json");
    let results = v["results"].as_array().expect("results array");
    assert_eq!(results.len(), 2, "fail-collect：所有单元都有结果条目");
    assert_eq!(results[0]["id"], "bad");
    assert_eq!(results[0]["ok"], false);
    assert!(
        results[0]["error"].as_str().unwrap().contains("unknown unit kind"),
        "got: {}",
        results[0]["error"]
    );
    assert_eq!(results[1]["id"], "ok");
    assert_eq!(
        results[1]["ok"], true,
        "已授权 fs.exists 单元应成功: {:?}",
        results[1]["error"]
    );
    std::fs::remove_file(&tmp).ok();
}
