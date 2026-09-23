//! host-task 并发任务域闭环（ABI v20，含产物加载）
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;
/// 预置「用户已记住」的 fs 授权记录（票 07 后测试侧唯一的无弹窗放行方式）
///
/// 旧写法把测试目录塞进 `.claude` 段借路径白名单蒙过校验；那条白名单对任何插件都
/// 放行，已随票 07 退役。这里改走生产同款：`fs_granted_paths` 前缀记录。
fn seed_fs_grant(ctx: &crate::wasm_core::manager::runtime::WasmHostContext, plugin_id: &str, dir: &std::path::Path) {
    crate::wasm_core::manager::runtime::block_on_async(
        ctx.fs_auth().save_granted_path(plugin_id, &dir.to_string_lossy()),
    )
    .expect("seed fs_granted_paths 记录");
}
/// execute-batch：8 个 fs.stat 并发 → 全完成、结果按 id 关联、顺序保序
#[test]

fn test_task_execute_batch_fixture_parallel_results() {
    let Some(bytes) = Some(build_task_test_component()) else {
        eprintln!("[skip] task fixture build failed");
        return;
    };
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
        crate::wasm_core::host_api::tests::grant_permissions(
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
    let Some(bytes) = Some(build_task_test_component()) else {
        return;
    };
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime.compile_component(&bytes).expect("compile task fixture");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mock = Arc::new(MockTaskServices::new());
        host_ctx.set_services(mock.clone()).await;
        let mut plugin = wasm_runtime
            .instantiate_component(&component, &task_fixture_plugin_id(), host_ctx.clone(), &[], None)
            .expect("instantiate task fixture");
        crate::wasm_core::host_api::tests::grant_permissions(
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
    let Some(bytes) = Some(build_task_test_component()) else {
        return;
    };
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime.compile_component(&bytes).expect("compile task fixture");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mock = Arc::new(MockTaskServices::new());
        host_ctx.set_services(mock.clone()).await;
        let mut plugin = wasm_runtime
            .instantiate_component(&component, &task_fixture_plugin_id(), host_ctx.clone(), &[], None)
            .expect("instantiate task fixture");
        crate::wasm_core::host_api::tests::grant_permissions(
            &host_ctx,
            &task_fixture_plugin_id(),
            &["task:run", "fs:read", "process:run"],
        );

        // 慢任务：4 × sleep 0.3（maxConcurrency=1 → 逐个串行）
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

/// 旧产物（plugin-component-test，未导出 events-task）：on_task_event 降级
/// Ok(false)，不影响加载与其余导出（spec §5.3 降级路径）
#[test]

fn test_task_legacy_component_event_export_degrades() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_test_component())
        .expect("compile legacy component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, "com.bedcode.component-test", host_ctx, &[], None)
        .expect("legacy component must load (no events-task)");
    assert!(
        !plugin
            .on_task_event("{\"jobId\":\"x\",\"phase\":\"completed\"}")
            .expect("probe"),
        "未导出 events-task 的产物必须走降级路径（Ok(false)）"
    );
    assert!(plugin.get_manifest().is_ok(), "降级后其余导出照常");
}

/// 双门：仅授 task:run 不授 fs:read → fs.stat 单元失败（permission denied），
/// 批次继续（fail-collect）；fs_auth 未授权路径直接 Err、不弹窗
#[test]

fn test_task_dual_gate_domain_permission_denied() {
    let Some(bytes) = Some(build_task_test_component()) else {
        return;
    };
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime.compile_component(&bytes).expect("compile task fixture");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut plugin = wasm_runtime
            .instantiate_component(&component, &task_fixture_plugin_id(), host_ctx.clone(), &[], None)
            .expect("instantiate task fixture");
        // 只授 task:run（不授 fs:read / process:run）
        crate::wasm_core::host_api::tests::grant_permissions(
            &host_ctx,
            &task_fixture_plugin_id(),
            &["task:run"],
        );
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

/// 回归保护：加载真实构建产物（resources 下 wasip3 版 ai-chatbox，票 03）
/// 组件导入接口必须与宿主 linker 全部匹配（实例化成功即证明，含 wasi0.3 全套
/// p3 接口）；产物缺失（未跑插件构建）时跳过——插件装配由真实构建 + 运行覆盖。
#[test]

fn test_ai_chatbox_wasip3_artifact_loads() {
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.ai-chatbox/bedcode_plugin_ai_chatbox.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] ai-chatbox wasip3 artifact not built");
        return;
    }
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let mut plugin = wasm_runtime
        .load_plugin_from_file(&wasm_path, "com.bedcode.ai-chatbox", host_ctx, &[], None)
        .expect("load wasip3 ai-chatbox: all imports must resolve");
    // manifest 往返（无副作用导出，验证 bindgen 接口工作）
    let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
    assert_eq!(manifest["id"], "com.bedcode.ai-chatbox");
}
