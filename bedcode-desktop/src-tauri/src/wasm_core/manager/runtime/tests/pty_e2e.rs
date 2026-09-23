//! host-pty 创建→拉取 / IO / 事件 / 背压 / 隔离矩阵（ABI v16）
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;
use bedcode_plugin_api::host::{pty_event_topic, PTY_EXIT};
/// 端到端（最高 seam）：WIT 契约 → 宿主实现 → 组件接线 → 权限两域 → SDK → 真 PTY 输出
///
/// 覆盖票 02 主干：spawn 真实命令拿句柄 → `ring-fetch` 拉到输出字节 → 二次按
/// `next-offset` 续拉不重复 → spawn 不发布任何事件 → 跨插件属主隔离 →
/// 未授权 `pty:spawn` 被宿主拒绝（Rust 端最终仲裁）。
#[test]

fn test_pty_spawn_ring_fetch_roundtrip() {
    // `setup_wasm_runtime` 内部自建 runtime 并 block_on，必须在 `rt.block_on`
    // 之外调用（嵌套 block_on 会 panic "Cannot start a runtime from within a runtime"）
    let (runtime_a, ctx_a) = setup_wasm_runtime();
    let (runtime_b, ctx_b) = setup_wasm_runtime();
    let _e2e_guard = lock_pty_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("host-pty spawn→ring-fetch e2e", async {
        const PLUGIN_A: &str = "com.bedcode.pty-test";
        // 同一产物以第二个属主 id 实例化：属主域完全隔离
        const PLUGIN_B: &str = "com.bedcode.pty-test.peer";

        ctx_a.permission.grant_permissions(
            PLUGIN_A,
            &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()],
        );
        // B 只授数据域：跨插件负向断言必须先过权限门才落到属主仲裁；
        // 同时它没有 pty:spawn，正好端到端验证两域独立
        ctx_b
            .permission
            .grant_permissions(PLUGIN_B, &["storage".to_string(), "pty:io".to_string()]);

        let component_a = runtime_a
            .compile_component(&build_pty_test_component())
            .expect("compile pty fixture component for A");
        let plugin_a = Arc::new(Mutex::new(
            runtime_a
                .instantiate_component(&component_a, PLUGIN_A, ctx_a.clone(), &[], None)
                .expect("instantiate pty fixture A"),
        ));
        ctx_a
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_A.to_string(), plugin_a.clone())]))),
            }))
            .await;
        plugin_a
            .lock()
            .await
            .activate()
            .expect("activate A = 订阅 <A>::pty:exit");

        // wasmtime 不支持跨 Engine 实例化，B 用自己 runtime 编译的组件
        let component_b = runtime_b
            .compile_component(&build_pty_test_component())
            .expect("compile pty fixture component for B");
        let plugin_b = Arc::new(Mutex::new(
            runtime_b
                .instantiate_component(&component_b, PLUGIN_B, ctx_b.clone(), &[], None)
                .expect("instantiate pty fixture B"),
        ));
        ctx_b
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_B.to_string(), plugin_b.clone())]))),
            }))
            .await;
        plugin_b.lock().await.activate().expect("activate B");

        // ==================== A：spawn 真实命令 → 句柄 ====================
        let marker = format!(
            "BEDCODE_PTY_E2E_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let spawned = {
            let mut guard = plugin_a.lock().await;
            guard
                .invoke_command(
                    "pty-spawn",
                    // 载体常驻（`echo` 后阻塞在 `read`）：票 04 起进程一退出即摘环，
                    // 短命命令会让后续断言撞上「句柄已不存在」而不是它要测的行为
                    &serde_json::json!({ "command": "/bin/sh", "args": ["-c", format!("echo {marker}; read go")] })
                        .to_string(),
                )
                .expect("pty-spawn")
        };
        let pty_id = serde_json::from_str::<serde_json::Value>(&spawned).expect("spawn json")["ptyId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(pty_id.starts_with("pty-"), "句柄形状应为 pty-<uuid>，got: {pty_id}");

        // ==================== ring-fetch：输出字节流到插件侧 ====================
        let first = pty_fetch_until(&plugin_a, &pty_id, &marker).await;
        let text = pty_fetched_text(&first);
        assert!(text.contains(&marker), "真 PTY 输出必须经环流到插件侧，got: {text}");
        assert_eq!(first["truncated"], false, "首次全量拉取不存在缺口: {first}");
        let next_offset = first["nextOffset"].as_u64().unwrap_or(0);
        assert_eq!(
            next_offset,
            first["data"].as_array().map(|a| a.len()).unwrap_or(0) as u64,
            "nextOffset 应等于本次返回区间末端: {first}"
        );

        // 续拉不重复（游标追平 → none）
        let second = pty_fixture_fetch(&plugin_a, &pty_id, next_offset).await;
        assert_eq!(second["none"], true, "游标追平时不得重复投递已消费字节: {second}");

        // spawn 不发任何事件（成功面无事件；退出事件须先授权/订阅时序契约，见票 04）
        let state_a = {
            let mut guard = plugin_a.lock().await;
            guard.invoke_command("pty-state", "{}").expect("pty-state")
        };
        let events_a: serde_json::Value = serde_json::from_str(&state_a).expect("pty-state json");
        assert_eq!(
            events_a["events"].as_array().map(|a| a.len()).unwrap_or(0),
            0,
            "spawn 不得发布任何总线事件: {state_a}"
        );

        // ==================== 属主隔离（WIT 端到端） ====================
        let denied_by_owner = {
            let mut guard = plugin_b.lock().await;
            guard
                .invoke_command(
                    "pty-ring-fetch",
                    &serde_json::json!({ "ptyId": pty_id, "fromOffset": 0, "maxBytes": 4096 }).to_string(),
                )
                .expect("跨插件调用以 error 载荷回传")
        };
        assert!(
            pty_command_error(&denied_by_owner).contains("not owner of pty handle"),
            "B 拉 A 的句柄必须被属主仲裁拒绝，got: {denied_by_owner}"
        );

        // ==================== 权限两域独立（Rust 端最终仲裁） ====================
        let denied_by_permission = {
            let mut guard = plugin_b.lock().await;
            guard
                .invoke_command("pty-spawn", &serde_json::json!({ "command": "/bin/true" }).to_string())
                .expect("未授权调用以 error 载荷回传")
        };
        assert!(
            pty_command_error(&denied_by_permission).contains("permission denied: pty:spawn"),
            "未声明 pty:spawn 的插件不得创建 PTY，got: {denied_by_permission}"
        );

        // 收尾：杀掉常驻载体，避免测试进程退出前挂着无用子进程（AGENTS §3）
        {
            let mut guard = plugin_a.lock().await;
            guard
                .invoke_command("pty-kill", &serde_json::json!({ "ptyId": pty_id }).to_string())
                .expect("pty-kill 收尾");
        }
    }));
}

/// 端到端（票 03 数据面）：spawn 交互进程 → write 输入 → ring-fetch 拉回**进程响应**
/// → resize 生效 → is-running 快照，全程跨 WIT / SDK 边界
#[test]

fn test_pty_interactive_io_loop_roundtrip() {
    let (runtime, ctx) = setup_wasm_runtime();
    let _e2e_guard = lock_pty_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("host-pty 数据面 e2e", async {
        const PLUGIN_ID: &str = "com.bedcode.pty-test";
        ctx.permission.grant_permissions(
            PLUGIN_ID,
            &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()],
        );
        let plugin = pty_activate_fixture(&runtime, &ctx, PLUGIN_ID).await;

        // sed 由进程自己加前缀：拉到的 `OUT:` 只能来自进程，而非 tty 本地回显
        let marker = format!(
            "BEDCODE_PTY_IO_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let spawned = {
            let mut guard = plugin.lock().await;
            guard
                .invoke_command(
                    "pty-spawn",
                    &serde_json::json!({ "command": "/bin/sed", "args": ["s/^/OUT:/"] }).to_string(),
                )
                .expect("pty-spawn")
        };
        let pty_id = serde_json::from_str::<serde_json::Value>(&spawned).expect("spawn json")["ptyId"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        // ==================== write：输入进进程，响应回环到环 ====================
        let input = format!("in-{marker}\n");
        let written = {
            let mut guard = plugin.lock().await;
            guard
                .invoke_command(
                    "pty-write",
                    &serde_json::json!({
                        "ptyId": pty_id,
                        "bytes": input.as_bytes().to_vec(),
                    })
                    .to_string(),
                )
                .expect("pty-write")
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&written).expect("write json")["len"],
            input.len() as u64,
            "pty-write 应回报实际写入字节数"
        );
        let response = pty_fetch_until(&plugin, &pty_id, &format!("OUT:in-{marker}")).await;
        assert!(
            pty_fetched_text(&response).contains(&format!("OUT:in-{marker}")),
            "写进 PTY 的输入必须被进程消费并以其输出回环: {response}"
        );

        // ==================== resize：无错即生效（时序不承诺） ====================
        let resized = {
            let mut guard = plugin.lock().await;
            guard
                .invoke_command(
                    "pty-resize",
                    &serde_json::json!({ "ptyId": pty_id, "cols": 90, "rows": 25 }).to_string(),
                )
                .expect("pty-resize")
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&resized).expect("resize json")["ok"],
            true,
            "resize 成功必须回 ok"
        );

        // ==================== is-running：进程存活期快照 ====================
        let state = {
            let mut guard = plugin.lock().await;
            guard
                .invoke_command("pty-is-running", &serde_json::json!({ "ptyId": pty_id }).to_string())
                .expect("pty-is-running")
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&state).expect("running json")["running"],
            true,
            "sed 仍在等待下一行输入时应为 running: {state}"
        );

        // 收尾：杀掉常驻 sed（其退出事件同时是票 04 的一次真实投递，不断言只清场）
        {
            let mut guard = plugin.lock().await;
            guard
                .invoke_command("pty-kill", &serde_json::json!({ "ptyId": pty_id }).to_string())
                .expect("pty-kill 收尾");
        }
    }));
}

/// 端到端（票 04 生命面）：kill / 自然退出 / 停用回收三条路径的退出事件与摘除
///
/// 覆盖 WIT → 宿主 → SDK → guest 事件回调的完整一圈：`pty-kill` 后属主经
/// `on_message` 收到 `<owner>::pty:exit`（reason=killed）且句柄不可再寻址；进程
/// 自然退出带出真实退出码；`purge_for_plugin`（deactivate 路径调用的同一函数）
/// 只回收本人，它插件的 PTY 与其事件流不受影响。
#[test]

fn test_pty_exit_event_and_purge_roundtrip() {
    let (runtime_a, ctx_a) = setup_wasm_runtime();
    let (runtime_b, ctx_b) = setup_wasm_runtime();
    let _e2e_guard = lock_pty_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("host-pty 生命面 e2e", async {
        const PLUGIN_A: &str = "com.bedcode.pty-test";
        const PLUGIN_B: &str = "com.bedcode.pty-test.peer";

        // 接线锁：停用路径必须调用插件 PTY 回收（本夹具没有 PluginHost，行为侧由
        // 下面的 purge 断言兜住，调用点存在性在此锁死——AGENTS §7 停用回收契约）。
        // 注意：deactivate_plugin_inner 经 P2 拆至 host/activation.rs，此处锁其源码
        let host_src = include_str!("../../host/activation.rs");
        assert!(
            host_src.contains("pty::purge_for_plugin(plugin_id, &self.message_bus)"),
            "deactivate_plugin_inner 未接线 host-pty 停用回收"
        );

        for (ctx, id) in [(&ctx_a, PLUGIN_A), (&ctx_b, PLUGIN_B)] {
            ctx.permission.grant_permissions(
                id,
                &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()],
            );
        }
        let plugin_a = pty_activate_fixture(&runtime_a, &ctx_a, PLUGIN_A).await;
        let plugin_b = pty_activate_fixture(&runtime_b, &ctx_b, PLUGIN_B).await;

        // ==================== kill：终止 + 摘除 + killed 事件 ====================
        let killed = {
            let mut guard = plugin_a.lock().await;
            guard
                .invoke_command(
                    "pty-spawn",
                    &serde_json::json!({ "command": "/bin/sh", "args": ["-c", "read go"] }).to_string(),
                )
                .expect("pty-spawn")
        };
        let killed_id = serde_json::from_str::<serde_json::Value>(&killed).expect("spawn json")["ptyId"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let kill_result = pty_fixture_call(&plugin_a, "pty-kill", serde_json::json!({ "ptyId": killed_id })).await;
        assert_eq!(kill_result["ok"], true, "kill 必须回报成功");

        let event = pty_wait_exit_event(&plugin_a, &killed_id).await;
        assert_eq!(
            event["topic"],
            pty_event_topic(PTY_EXIT, PLUGIN_A),
            "topic 为属主私有命名空间"
        );
        assert_eq!(event["sender"], "host", "事件由宿主发布");
        assert_eq!(event["payload"]["reason"], "killed", "kill 路径 reason 固定");

        // 摘除即不可寻址（句柄与环一并释放）
        let after_kill = {
            let mut guard = plugin_a.lock().await;
            guard
                .invoke_command("pty-is-running", &serde_json::json!({ "ptyId": killed_id }).to_string())
                .expect("以 error 载荷回传")
        };
        assert!(
            pty_command_error(&after_kill).contains("not found"),
            "kill 摘除后句柄必须不可寻址，got: {after_kill}"
        );

        // ==================== 自然退出：stopped + 真实退出码 ====================
        let exited = {
            let mut guard = plugin_a.lock().await;
            guard
                .invoke_command(
                    "pty-spawn",
                    &serde_json::json!({ "command": "/bin/sh", "args": ["-c", "exit 3"] }).to_string(),
                )
                .expect("pty-spawn")
        };
        let exited_id = serde_json::from_str::<serde_json::Value>(&exited).expect("spawn json")["ptyId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let event = pty_wait_exit_event(&plugin_a, &exited_id).await;
        assert_eq!(event["payload"]["reason"], "stopped", "自然退出不得报 killed");
        assert_eq!(
            event["payload"]["exitCode"], 3,
            "退出码必须经 WIT/bus 原样送达插件: {event}"
        );

        // ==================== 停用回收：只碰本人 ====================
        let mine = {
            let mut guard = plugin_a.lock().await;
            guard
                .invoke_command(
                    "pty-spawn",
                    &serde_json::json!({ "command": "/bin/sh", "args": ["-c", "read go"] }).to_string(),
                )
                .expect("pty-spawn")
        };
        let mine_id = serde_json::from_str::<serde_json::Value>(&mine).expect("spawn json")["ptyId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let theirs = {
            let mut guard = plugin_b.lock().await;
            guard
                .invoke_command(
                    "pty-spawn",
                    &serde_json::json!({ "command": "/bin/sh", "args": ["-c", "read go"] }).to_string(),
                )
                .expect("pty-spawn")
        };
        let theirs_id = serde_json::from_str::<serde_json::Value>(&theirs).expect("spawn json")["ptyId"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        // A 停用：宿主回收其全部在册 PTY（此刻 A 有 1 条活着的那条 + 已终态的两条已摘除）
        // 走 spawn_blocking：`purge_for_plugin` 内含 `block_on_async`（kill 要 await
        // 引擎），在 `rt.block_on` 的驱动线程上直接调用会撞 block_in_place 约束——
        // 无 handle 的阻塞线程才是它的真实调用形态（同生产 deactivate 路径）
        let purged = {
            let bus = Arc::clone(&ctx_a.message_bus);
            tokio::task::spawn_blocking(move || {
                crate::wasm_core::host_api::pty::purge_for_plugin(PLUGIN_A, &bus)
            })
            .await
            .expect("purge 任务不得 panic")
        };
        assert_eq!(purged, 1, "回收数应为 A 当前在册的 PTY 数（已终态者早被摘除）");

        let event = pty_wait_exit_event(&plugin_a, &mine_id).await;
        assert_eq!(
            event["payload"]["reason"], "killed",
            "停用回收对插件表现为一次被宿主终止: {event}"
        );
        assert_eq!(
            pty_fixture_events(&plugin_a)
                .await
                .iter()
                .filter(|e| e["payload"]["ptyId"] == serde_json::Value::String(mine_id.clone()))
                .count(),
            1,
            "恰好一次：停用回收与退出监听不得各发一条"
        );

        // B 完全不受影响：句柄可查、事件流里没有 A 的任何一条
        let peer_state = pty_fixture_call(&plugin_b, "pty-is-running", serde_json::json!({ "ptyId": theirs_id })).await;
        assert_eq!(peer_state["running"], true, "它插件的 PTY 不得被连带终止");
        let peer_events = pty_fixture_events(&plugin_b).await;
        assert!(
            peer_events.is_empty(),
            "非属主物理上收不到他人的退出事件: {peer_events:?}"
        );

        // 收尾：清场 B 的常驻进程（同一回收函数对 B 亦只碰本人）
        let purged_peer = {
            let bus = Arc::clone(&ctx_b.message_bus);
            tokio::task::spawn_blocking(move || {
                crate::wasm_core::host_api::pty::purge_for_plugin(PLUGIN_B, &bus)
            })
            .await
            .expect("peer purge 任务不得 panic")
        };
        assert_eq!(purged_peer, 1, "B 的回收同样只清自己那一条");
    }));
}

/// 端到端（票 05 限额与背压）：插件声明的 `ringBytes` 经 WIT 生效，落后游标得到
/// `truncated` 并可按 `next-offset` 续拉；超上限的声明被宿主拒绝（不静默降级）
#[test]

fn test_pty_declared_ring_backpressure_roundtrip() {
    let (runtime, ctx) = setup_wasm_runtime();
    let _e2e_guard = lock_pty_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("host-pty 背压 e2e", async {
        const PLUGIN_ID: &str = "com.bedcode.pty-test";
        ctx.permission.grant_permissions(
            PLUGIN_ID,
            &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()],
        );
        let plugin = pty_activate_fixture(&runtime, &ctx, PLUGIN_ID).await;

        // ==================== 声明超上限：宿主拒绝且不静默夹取 ====================
        let oversized = {
            let mut guard = plugin.lock().await;
            guard
                .invoke_command(
                    "pty-spawn",
                    &serde_json::json!({
                        "command": "/bin/sh",
                        "args": ["-c", "read go"],
                        // 宿主上限 4 MiB（PLUGIN_PTY_RING_MAX_BYTES）
                        "ringBytes": 4 * 1024 * 1024 + 1,
                    })
                    .to_string(),
                )
                .expect("以 error 载荷回传")
        };
        let err = pty_command_error(&oversized);
        assert!(
            err.contains("ringBytes") && err.contains("too large"),
            "声明超上限必须可见: {err}"
        );

        // ==================== 小环 + 持续产出：落后游标 truncated + 可续拉 ====================
        let spawned = {
            let mut guard = plugin.lock().await;
            guard
                .invoke_command(
                    "pty-spawn",
                    &serde_json::json!({
                        "command": "/bin/sh",
                        // 关掉 tty 回显：环内内容即进程产出；每 ~1ms 一行
                        "args": ["-c", "stty -echo; while true; do echo line; sleep 0.001; done"],
                        "ringBytes": 512,
                    })
                    .to_string(),
                )
                .expect("pty-spawn")
        };
        let pty_id = serde_json::from_str::<serde_json::Value>(&spawned).expect("spawn json")["ptyId"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        // 消费者（本用例）先不拉取，让产出远超 512 字节
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let stale = pty_fixture_fetch(&plugin, &pty_id, 0).await;
        assert_eq!(stale["truncated"], true, "游标 0 必已落后于驻留起点: {stale}");
        let first_end = stale["nextOffset"].as_u64().unwrap_or(0);
        assert!(first_end > 512, "产出必须已远超声明容量（源侧未被拖住）: {stale}");
        assert!(
            first_end - pty_fetched_text(&stale).len() as u64 > 0,
            "返回段必须从产出中段起（此前字节已淘汰）: {stale}"
        );
        assert!(
            pty_fetched_text(&stale).contains("line"),
            "返回的必须是真实输出: {stale}"
        );

        // 续拉不重复、不再报缺口
        let resume = pty_fixture_fetch(&plugin, &pty_id, first_end).await;
        if resume.get("none").is_none() {
            assert_eq!(resume["truncated"], false, "从 nextOffset 起续拉不得再报缺口: {resume}");
            assert!(
                resume["nextOffset"].as_u64().unwrap_or(0) > first_end,
                "游标必须前进: {resume}"
            );
        }

        // 收尾：杀掉流式载体
        {
            let mut guard = plugin.lock().await;
            guard
                .invoke_command("pty-kill", &serde_json::json!({ "ptyId": pty_id }).to_string())
                .expect("pty-kill 收尾");
        }
    }));
}

/// 端到端契约矩阵（票 06，最高 seam 固化为回归基线）
///
/// 一条用例冻结五类断言，**每个分格独立可定位**（不打包成大 assert）：
/// ① 正路径闭环 spawn → ring-fetch → write → resize → is-running → kill → exit 事件；
/// ② 属主隔离矩阵——他人句柄上**每一个**带句柄入参的函数都验一次 `not owner`
///    （漏一个函数即契约破口），并验拒绝零副作用；
/// ③ 事件定向——非属主的事件流里不得出现他人事件；
/// ④ ADR 0017——`api: []` 的插件被互调时宿主门禁拒绝；
/// ⑤ 权限两域独立（未声明 `pty:spawn` 的插件不得创建）。
/// 权限「完全未授权」分格与配额/回收分格分持在宿主层单测（`every_api_without_any_permission_is_denied_before_any_lookup`
/// 与 `pty_quota_*` / `purge_for_plugin_*`），此处不重复造轮子。
#[test]

fn test_pty_isolation_and_contract_matrix_roundtrip() {
    let (runtime_a, ctx_a) = setup_wasm_runtime();
    let (runtime_b, ctx_b) = setup_wasm_runtime();
    let _e2e_guard = lock_pty_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("host-pty 契约矩阵 e2e", async {
        const PLUGIN_A: &str = "com.bedcode.pty-test";
        const PLUGIN_B: &str = "com.bedcode.pty-test.peer";

        // A：双域；B：只授数据域——既做「越权创建」的负向载体，也让属主负向断言
        // 必须先过权限门（不致于把权限拒绝误当成属主拒绝）
        ctx_a.permission.grant_permissions(
            PLUGIN_A,
            &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()],
        );
        ctx_b
            .permission
            .grant_permissions(PLUGIN_B, &["storage".to_string(), "pty:io".to_string()]);
        let plugin_a = pty_activate_fixture(&runtime_a, &ctx_a, PLUGIN_A).await;
        let plugin_b = pty_activate_fixture(&runtime_b, &ctx_b, PLUGIN_B).await;

        // ==================== ① 正路径闭环 ====================
        // `-u` 让 sed 行缓冲（非 tty 输出下默认块缓冲会把响应压到最后一次吐出）；
        // `OUT:` 前缀只能由进程加上，故拉到的前缀即「输入真的进了进程」的证据
        let spawned = pty_fixture_call(
            &plugin_a,
            "pty-spawn",
            serde_json::json!({ "command": "/bin/sed", "args": ["-u", "s/^/OUT:/"], "cols": 100, "rows": 30 }),
        )
        .await;
        let pty_id = spawned["ptyId"].as_str().unwrap_or_default().to_string();
        assert!(pty_id.starts_with("pty-"), "①-a 句柄形状: {spawned}");

        let tag = format!(
            "MATRIX_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let written = pty_fixture_call(
            &plugin_a,
            "pty-write",
            serde_json::json!({ "ptyId": pty_id, "bytes": format!("in-{tag}\n").into_bytes() }),
        )
        .await;
        assert_eq!(
            written["len"],
            format!("in-{tag}\n").len() as u64,
            "①-b write 必须回报实际写入字节数"
        );
        let response = pty_fetch_until(&plugin_a, &pty_id, &format!("OUT:in-{tag}")).await;
        assert!(
            pty_fetched_text(&response).contains(&format!("OUT:in-{tag}")),
            "①-c ring-fetch 必须拉到进程响应: {response}"
        );

        let resized = pty_fixture_call(
            &plugin_a,
            "pty-resize",
            serde_json::json!({ "ptyId": pty_id, "cols": 120, "rows": 40 }),
        )
        .await;
        assert_eq!(resized["ok"], true, "①-d resize 生效必须回 ok: {resized}");

        let alive = pty_fixture_call(&plugin_a, "pty-is-running", serde_json::json!({ "ptyId": pty_id })).await;
        assert_eq!(alive["running"], true, "①-e 存活快照必须为 true: {alive}");

        let killed = pty_fixture_call(&plugin_a, "pty-kill", serde_json::json!({ "ptyId": pty_id })).await;
        assert_eq!(killed["ok"], true, "①-f kill 必须回 ok: {killed}");
        let event = pty_wait_exit_event(&plugin_a, &pty_id).await;
        assert_eq!(
            event["topic"],
            pty_event_topic(PTY_EXIT, PLUGIN_A),
            "①-g 退出事件必须落在属主私有 topic: {event}"
        );
        assert_eq!(
            event["payload"]["reason"], "killed",
            "①-h kill 路径 reason 固定: {event}"
        );
        let after_kill = {
            let mut guard = plugin_a.lock().await;
            guard
                .invoke_command("pty-is-running", &serde_json::json!({ "ptyId": pty_id }).to_string())
                .expect("以 error 载荷回传")
        };
        assert!(
            pty_command_error(&after_kill).contains("not found"),
            "①-i 退出即摘除：句柄必须不再可寻址: {after_kill}"
        );

        // ==================== ② 属主隔离矩阵（每一个句柄型函数） ====================
        let foreign = pty_fixture_call(
            &plugin_a,
            "pty-spawn",
            serde_json::json!({ "command": "/bin/sh", "args": ["-c", "read go"] }),
        )
        .await;
        let foreign_id = foreign["ptyId"].as_str().unwrap_or_default().to_string();
        assert!(!foreign_id.is_empty(), "②-a 属主 spawn 应先成功");

        // 第 5 个句柄型函数 `kill` 属创建域：B 无 `pty:spawn`，其属主分格由宿主层
        // `kill_still_enforces_owner_before_its_own_gate` 锁，⑤ 在此锁它的权限门优先级
        let matrix: [(&str, serde_json::Value); 4] = [
            (
                "pty-write",
                serde_json::json!({ "ptyId": foreign_id, "bytes": b"ls\n".to_vec() }),
            ),
            (
                "pty-resize",
                serde_json::json!({ "ptyId": foreign_id, "cols": 80, "rows": 24 }),
            ),
            (
                "pty-ring-fetch",
                serde_json::json!({ "ptyId": foreign_id, "fromOffset": 0, "maxBytes": 4096 }),
            ),
            ("pty-is-running", serde_json::json!({ "ptyId": foreign_id })),
        ];
        for (command, args) in &matrix {
            let raw = {
                let mut guard = plugin_b.lock().await;
                guard
                    .invoke_command(command, &args.to_string())
                    .unwrap_or_else(|e| panic!("②-b {command} 调用应失败而非 trap: {e}"))
            };
            // 失败信息必须自带分格名（命令），成功载荷同样视为破口
            let err =
                pty_error_of(&raw).unwrap_or_else(|| panic!("②-b {command} 必须被属主仲裁拒绝，got 成功载荷: {raw}"));
            assert!(
                err.contains("not owner of pty handle"),
                "②-b {command} 拒绝文案应为 not owner（而非权限/查表），got: {err}"
            );
        }
        // 拒绝必须零副作用：属主的句柄照旧可用、内容照旧可拉
        let still_ours =
            pty_fixture_call(&plugin_a, "pty-is-running", serde_json::json!({ "ptyId": foreign_id })).await;
        assert_eq!(
            still_ours["running"], true,
            "②-c 越权拒绝不得影响属主句柄: {still_ours}"
        );

        // ==================== ③ 事件定向（B 的物理订阅窗口里没有 A 的事件） ====================
        let peer_events = pty_fixture_events(&plugin_b).await;
        assert!(
            peer_events
                .iter()
                .all(|e| e["payload"]["ptyId"] != serde_json::Value::String(foreign_id.clone())),
            "③ 非属主事件流里不得出现他人事件: {peer_events:?}"
        );
        assert!(
            peer_events.is_empty(),
            "③ B 全程未拥有 PTY，事件流必须为空: {peer_events:?}"
        );

        // ==================== ④ ADR 0017：未声明 api 的互调被宿主门禁拒绝 ====================
        let call_undeclared = {
            let mut guard = plugin_b.lock().await;
            guard
                .invoke_command(
                    "pty-call-undeclared-api",
                    &serde_json::json!({ "api": "pty-spawn" }).to_string(),
                )
                .expect("以 error 载荷回传")
        };
        let gate_err = pty_command_error(&call_undeclared);
        assert!(
            gate_err.contains("not declared"),
            "④ fixture 的 manifest `api: []`，互调必须被门禁拒绝，got: {call_undeclared}"
        );

        // ==================== ⑤ 权限两域独立（无 pty:spawn 的插件不得创建） ====================
        let denied_spawn = {
            let mut guard = plugin_b.lock().await;
            guard
                .invoke_command("pty-spawn", &serde_json::json!({ "command": "/bin/true" }).to_string())
                .expect("以 error 载荷回传")
        };
        assert!(
            pty_command_error(&denied_spawn).contains("permission denied: pty:spawn"),
            "⑤ 只授 pty:io 的插件不得创建 PTY，got: {denied_spawn}"
        );
        // 同属主 B 的 kill（创建域）同样先撞权限门——与 ② 的属主判据区分开
        let denied_kill = {
            let mut guard = plugin_b.lock().await;
            guard
                .invoke_command("pty-kill", &serde_json::json!({ "ptyId": foreign_id }).to_string())
                .expect("以 error 载荷回传")
        };
        assert!(
            pty_command_error(&denied_kill).contains("permission denied: pty:spawn"),
            "⑤ kill 属创建域：B 缺 pty:spawn 时必须先被权限门拒绝: {denied_kill}"
        );

        // 收尾：A 的在册句柄回收（B 无在册句柄）
        let bus = Arc::clone(&ctx_a.message_bus);
        let purged = tokio::task::spawn_blocking(move || {
            crate::wasm_core::host_api::pty::purge_for_plugin(PLUGIN_A, &bus)
        })
        .await
        .expect("purge 任务不得 panic");
        assert_eq!(purged, 1, "收尾回收应只剩 A 的那条常驻 PTY");
    }));
}
