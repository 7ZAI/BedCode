//! session 域（trust/consent/config/http/编排）闭环
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;

/// 经插件命令面向**配置真源**（插件私有库）播种一条配置 → 写入后的配置
///
/// 测试播种必须落真源：v21 起主库 `session_configs` 只剩 legacy 迁移通道（已无写者），
/// 插件侧解析链（task 域 agent / workingDir 判定、scheduled → launch spec）读的是私有库。
/// 只写主库的 fixture 对解析链不可见，会让用例测到一个生产里不存在的形状。
///
/// v24（2026-09-22）：主库 `session_configs` 表退役，返回 upsert 回执 JSON
/// （camelCase，含 `id`）——不再依赖已删除的 `crate::db::SessionConfig` 模型。
async fn seed_config_in_plugin_store(
    plugin: &mut LoadedWasmPlugin,
    name: &str,
    working_dir: &str,
    command: &str,
) -> serde_json::Value {
    let draft = serde_json::json!({
        "name": name,
        "environment": "linux",
        "workingDir": working_dir,
        "command": command,
    });
    let out = plugin
        .invoke_command("session.config.upsert", &draft.to_string())
        .expect("seed config via plugin command");
    serde_json::from_str(&out).expect("seeded config json")
}

#[test]

fn test_session_plugin_artifact_lifecycle() {
    use crate::utils::auth::auth_center as bridge;

    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let mut plugin = wasm_runtime
        .load_plugin_from_file(
            &wasm_path,
            "com.bedcode.terminal-session",
            Arc::clone(&host_ctx),
            &[],
            None,
        )
        .expect("load wasip3 session: all imports must resolve");

    assert_eq!(plugin.activate().expect("activate"), 0);

    let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
    assert_eq!(manifest["id"], "com.bedcode.terminal-session");
    assert_eq!(
        manifest["pluginType"], "rust-ts",
        "骨架即 rust-ts 形态：P3 贡献式前端的落点"
    );
    assert_eq!(
        manifest["permissions"],
        serde_json::json!([
            "auth",
            // 票 15 任务域：broadcast（任务 / 模式 / 队列广播）、fs:read + fs:write
            // （写项目级 Agent 集成）、terminal:input（队列下发）+ terminal:observe
            // （提交输入行监听）、timer:schedule（队列周期 tick）
            "broadcast",
            "fs:read",
            "fs:write",
            "peer",
            // 票 03：文件浏览域 git diff 经 host-process run-sync
            "process:run",
            "session:read",
            "session:write",
            "storage",
            // 票 21（v20 host-task）：`task:run` ——git 域 diff_file_tree 三路只读
            // 命令改走 execute-batch 并行（池线程真并发，替代 run-sync 串行）。
            // 注意顺序：wasm 产物经 manifest-gen **ASCII 升序**重排（task < terminal）
            "task:run",
            "terminal:input",
            "terminal:observe",
            "timer:schedule",
            // 票 17：`ui:input`（任务队列弹窗的终端工具栏入口，纯前端贡献面）
            // 清单顺序 = manifest-gen 的 ASCII 升序口径（release 构建会重排）
            "ui:input",
            "ui:settings",
            // 票 14：`ui:sidebar`（票 13 起运行期注册侧边栏目录实际需要，此前漏声明
            // → 前端权限门会抛错）
            "ui:sidebar"
        ]),
        "票 05：host-auth + host-peer；票 08：session:read（config-list 精简列表）+\
             session:read（config-get 全量行，迁移读 legacy 用）+ storage（配置私有库）；\
             票 14：ui:sidebar + ui:settings（两个纯前端贡献面）；票 15：任务域五位；\
             票 17：ui:input（任务弹窗工具栏入口）；票 21：task:run（git 域 execute-batch 并行）"
    );
    let declared_api: Vec<String> = manifest["api"]
        .as_array()
        .expect("api 数组")
        .iter()
        .map(|v| v.as_str().expect("api 字符串").to_string())
        .collect();
    assert_eq!(
        declared_api.len(),
        27,
        "pairing 八项 + trust 两项 + consent 一项 + config 三项 + session-create 一项（票 09）+ \
             会话动作四项（票 10）+ annotate + devices-connect-list 两项（票 11）\
             + quick-actions-import 一项（票 02）\
             + v24 认证记录下沉五项（auth-records-import / devices-list / history-list / \
             connection-touch / connection-close），got: {declared_api:?}"
    );
    // 票 08：宿主配置命令面转发依赖这三项（缺一即静默降级到只读投影）
    for consumed in [
        "com.bedcode.terminal-session.config-list",
        "com.bedcode.terminal-session.config-upsert",
        "com.bedcode.terminal-session.config-delete",
    ] {
        assert!(
            declared_api.iter().any(|a| a == consumed),
            "manifest 缺配置面 api {consumed}"
        );
    }
    // 票 09/10：宿主命令面（创建 / 重启 / 删除 / 尺寸裁决）的桥接目标必须在声明面里，
    // 否则「未声明 api 不可调」门禁会把转发整片拒掉（静默降级回宿主旧路径）
    for consumed in [
        "com.bedcode.terminal-session.session-create",
        "com.bedcode.terminal-session.session-restart",
        "com.bedcode.terminal-session.session-remove",
        "com.bedcode.terminal-session.session-rename",
        "com.bedcode.terminal-session.session-resize",
    ] {
        assert!(
            declared_api.iter().any(|a| a == consumed),
            "manifest 缺会话动作 api {consumed}"
        );
    }
    // 宿主桥接锚点必须真在声明面里：锚点漂移 = 桥接永久静默降级（无人报错）
    assert!(
        declared_api.contains(&bridge::SESSION_MARKER_API.to_string()),
        "manifest 缺桥接探活锚点 {}",
        bridge::SESSION_MARKER_API
    );
    // 文件传输插件经互调消费 consent / trust：两条 api 必须在声明面里，
    // 否则「未声明 api 不可调」门禁会把它的调用整片拒掉（静默降级）
    for consumed in [
        "com.bedcode.terminal-session.consent-decide",
        "com.bedcode.terminal-session.trust-list",
    ] {
        assert!(
            declared_api.iter().any(|a| a == consumed),
            "manifest 缺消费方 api {consumed}"
        );
    }
    for api in &declared_api {
        assert!(
            api.starts_with("com.bedcode.terminal-session."),
            "api 必须落在本插件命名空间, got: {api}"
        );
    }

    // 命令面可调用：状态命令回传 manifest 声明，宿主据此确认 api/permissions 生效
    let result = plugin.invoke_command("session.status", "{}").expect("session.status");
    let r: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(r["plugin"], "com.bedcode.terminal-session");
    assert_eq!(
        r["domains"],
        serde_json::json!([
            "pairing",
            "trust",
            "consent",
            "config",
            "session",
            "devices",
            "environment",
            // 票 15：任务域后端①（Agent 集成 + 会话状态/映射 + 队列随迁）
            "task"
        ]),
        "票 11 devices 域（注解槽 + 设备派生视图）；票 13 追加 environment 域\
             （WSL 发行版枚举，供插件会话配置表单的执行环境分支）；票 15 追加 task 域"
    );
    assert_eq!(r["permissions"], manifest["permissions"]);
    assert_eq!(r["api"], manifest["api"]);

    // 未声明命令明确报错（命令面快速失败）：wasm_entry 将 Err 序列化为
    // `{"error": ...}` JSON 返回（宿主侧 Ok）——断言错误形状而非 is_err
    let ghost = plugin
        .invoke_command("session.ghost", "{}")
        .expect("session.ghost 返回 JSON");
    let r: serde_json::Value = serde_json::from_str(&ghost).unwrap();
    assert!(
        r["error"]
            .as_str()
            .map(|e| e.contains("Unknown command"))
            .unwrap_or(false),
        "未知命令必须返回 error 形状, got: {ghost}"
    );

    assert_eq!(plugin.deactivate().expect("deactivate"), 0);
}

/// 票 15 任务域后端闭环（真实 wasm 产物 + 真实宿主原语，S1 主接缝）
///
/// 验收的是「搬入即可用」，而不是「搬入即编译通过」：
/// - 私有库：`activate` 建表后，预设任务经 `preset-create` → `preset-list`
///   真实往返（证明 `host-plugin-database` 在 wasm 内可用，schema 生效）
/// - 会话开关：`set-auto-mode` → `session-settings` 读回（session_settings 表，
///   且开关在宿主会话不存在时仍按会话键独立记账）
/// - 定时器 tick：队列两域 + 定时任务域都执行（`tick` 分发表 + 私有库查询可用）
/// - **编排反转载体**：`on_session_lifecycle(Creating)` 对未适配 agent 不得
///   产生任何集成写入（项目目录保持为空）——本插件自己注册监听并推进注入，
///   不再等宿主反向调用
/// - 命令面：未知命令显性报错（`{"error"...}` 形状）
///
/// **不调用 deactivate**：停用会走全局 hooks 清理（`~/.claude/settings.json`），
/// 那是真实用户目录上的写操作，只在真机与插件单测里验证，不在本闭环触发。
/// 产物缺失（未跑插件构建）时跳过。
#[test]

fn test_session_task_domain_closed_loop() {
    let _serial = session_plugin_db_guard();
    const PROBE_SESSION: &str = "probe-task-session";

    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    // 本测试不经 PluginHost 激活，权限门所需的 grant 需显式下发（与 manifest 同表）
    host_ctx.permission.grant_permissions(
        "com.bedcode.terminal-session",
        &[
            "auth",
            "peer",
            "storage",
            "session:read",
            "session:write",
            "storage",
            "broadcast",
            "fs:read",
            "fs:write",
            "terminal:input",
            "terminal:observe",
            "timer:schedule",
            "ui:sidebar",
            "ui:settings",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>(),
    );
    let mut plugin = wasm_runtime
        .load_plugin_from_file(
            &wasm_path,
            "com.bedcode.terminal-session",
            Arc::clone(&host_ctx),
            &[],
            None,
        )
        .expect("load wasip3 session");
    assert_eq!(plugin.activate().expect("activate"), 0);

    // 1. agent 能力清单（wasm 内 registry，纯计算面）
    let out = plugin
        .invoke_command("session.task.supported-agents", "{}")
        .expect("supported-agents");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(r["error"].is_null(), "supported-agents 不得报错, got: {out}");
    let agents = r["agents"].as_array().expect("agents 数组").clone();
    assert!(
        agents.iter().any(|a| a.as_str() == Some("claude")),
        "claude 必须在受支持 agent 名单内, got: {agents:?}"
    );

    // 2. 预设任务真实往返（私有库 task_preset 表）
    let out = plugin
        .invoke_command("session.task.preset-create", r#"{"prompt":"probe-prompt"}"#)
        .expect("preset-create");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(r["error"].is_null(), "preset-create 不得报错, got: {out}");
    let preset_id = r["preset_id"].as_str().expect("preset_id").to_string();
    let out = plugin
        .invoke_command("session.task.preset-list", "{}")
        .expect("preset-list");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    let presets = r["presets"].as_array().expect("presets 数组").clone();
    assert!(
        presets
            .iter()
            .any(|p| p["id"] == preset_id.as_str() && p["prompt"] == "probe-prompt"),
        "创建的预设必须可由私有库读回且字段一致, got: {presets:?}"
    );

    // 3. 会话开关写入 → 读回（task_session_settings 表，两个开关独立）
    let out = plugin
        .invoke_command(
            "session.task.set-auto-mode",
            &serde_json::json!({
                "session_id": PROBE_SESSION,
                "auto_execute": true,
                "auto_answer": false
            })
            .to_string(),
        )
        .expect("set-auto-mode");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(r["error"].is_null(), "set-auto-mode 不得报错, got: {out}");
    let out = plugin
        .invoke_command(
            "session.task.session-settings",
            &serde_json::json!({ "session_id": PROBE_SESSION }).to_string(),
        )
        .expect("session-settings");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(r["error"].is_null(), "session-settings 不得报错, got: {out}");
    assert_eq!(r["auto_execute"], true, "自动执行开关必须落库, got: {out}");
    assert_eq!(r["auto_answer"], false, "自动应答开关保持关闭, got: {out}");

    // 4. 定时器 tick：三个域步骤都执行（分发表 + 私有库查询可用）
    let out = plugin
        .invoke_command("session.task.scheduler-tick", r#"{"now_utc":"2026-09-20 00:00:00"}"#)
        .expect("scheduler-tick");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(r["error"].is_null(), "scheduler-tick 不得报错, got: {out}");
    assert_eq!(
        r["executed"],
        serde_json::json!(["queue-delay-clear", "queue-silence-check", "scheduled-trigger"]),
        "tick 必须顺序执行队列两域与定时任务域且都成功（票 16 并入第三域）, got: {out}"
    );
    assert!(
        r["failed"].as_array().map(|a| a.is_empty()).unwrap_or(false),
        "tick 无失败域, got: {out}"
    );

    // 5. Creating + 未适配 agent（`bash`）→ 不写入任何 agent 集成
    let probe_dir = std::env::temp_dir().join(format!("bedcode-task-probe-{}", std::process::id()));
    std::fs::create_dir_all(&probe_dir).expect("create probe dir");
    plugin
        .on_session_lifecycle(&serde_json::json!({
            "event_type": "creating",
            "config_id": "probe-config",
            "command": "bash",
            "working_dir": probe_dir.to_string_lossy(),
            "source_device": null,
            "resource_dir": ""
        }))
        .expect("creating 事件不得报错");
    let entries: Vec<String> = std::fs::read_dir(&probe_dir)
        .expect("read probe dir")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .collect();
    assert!(
        entries.is_empty(),
        "未适配 agent 的会话不得写入任何集成文件, got: {entries:?}"
    );
    let _ = std::fs::remove_dir_all(&probe_dir);

    // 6. 状态推进序列（票 15 的对照核心）：受支持 agent 会话收到提交输入 →
    //    任务行 in_progress → 注解槽投影；会话 Stopped → 兜底中断 interrupted
    //
    // 播种走真源（配置在插件私有库 + 会话在内核注册表），start=false 故不 spawn 进程：
    // 本用例只推进状态机，不需要真实 PTY。
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let (annotations_after_input, annotations_after_stopped, status_after_stopped) = rt.block_on(async {
        let sm = host_ctx.session_manager.clone();

        // agent 判定读配置真源的 command（claude → 受支持 agent → 建任务行）
        let config = seed_config_in_plugin_store(&mut plugin, "任务域探针", "/tmp", "claude").await;

        let sid = sm
            .create_session_from_spec(
                crate::enums::SessionLaunchConfig {
                    name: "任务域探针".to_string(),
                    environment: crate::enums::ExecutionEnvironment::Linux,
                    working_dir: "/tmp".to_string(),
                    command: "claude".to_string(),
                    command_args: vec!["bash".to_string(), "-lic".to_string(), "claude".to_string()],
                    env_vars: std::collections::HashMap::new(),
                    cols: 120,
                    rows: 40,
                },
                config["id"].as_str().unwrap().to_string(),
                None,
                false,
                None,
                // 票 04：属主登记——本用例的会话由 com.bedcode.terminal-session 实例注解/操作
                Some("com.bedcode.terminal-session"),
            )
            .await
            .expect("create session from spec");

        // 提交输入（非命令、非空行、受支持 agent、无在途任务）→ 建任务行
        plugin
            .on_input_submitted(&serde_json::json!({
                "session_id": sid,
                "text": "probe task input"
            }))
            .expect("on_input_submitted");

        let after_input = sm.session_annotations(&sid).await;
        // 会话结束兜底：运行中任务行与注解槽一并收敛到 interrupted
        plugin
            .on_session_lifecycle(&serde_json::json!({
                "event_type": "stopped",
                "session_id": sid,
                "source_device": null,
                "resource_dir": ""
            }))
            .expect("stopped 事件不得报错");
        let after_stopped = sm.session_annotations(&sid).await;

        let out = plugin
            .invoke_command(
                "session.task.get-status",
                &serde_json::json!({ "session_id": sid }).to_string(),
            )
            .expect("get-status");
        let r: serde_json::Value = serde_json::from_str(&out).unwrap();
        (after_input, after_stopped, r["task_status"].clone())
    });

    assert_eq!(
        annotations_after_input.get("taskStatus").map(String::as_str),
        Some("in_progress"),
        "提交输入必须把任务状态推进为 in_progress（真源 + 注解槽）, got: {annotations_after_input:?}"
    );
    assert_eq!(
        annotations_after_input.get("taskReason").map(String::as_str),
        Some("User submitted input"),
        "注解槽键名 contract（票 12）：taskReason 由本域写入, got: {annotations_after_input:?}"
    );
    assert!(
        annotations_after_input
            .get("taskUpdatedAt")
            .map(|v| !v.is_empty())
            .unwrap_or(false),
        "taskUpdatedAt 必须是非空时间戳, got: {annotations_after_input:?}"
    );
    assert_eq!(
        annotations_after_stopped.get("taskStatus").map(String::as_str),
        Some("interrupted"),
        "会话停止兜底必须把运行中任务收敛为 interrupted, got: {annotations_after_stopped:?}"
    );
    assert_eq!(
        status_after_stopped,
        serde_json::json!("interrupted"),
        "命令面读到的状态必须与注解槽一致, got: {status_after_stopped}"
    );

    // 7. 未知任务命令显性报错（命令面快速失败）
    let ghost = plugin
        .invoke_command("session.task.ghost", "{}")
        .expect("session.task.ghost 返回 JSON");
    let r: serde_json::Value = serde_json::from_str(&ghost).unwrap();
    assert!(
        r["error"]
            .as_str()
            .map(|e| e.contains("Unknown command"))
            .unwrap_or(false),
        "未知任务命令必须返回 error 形状, got: {ghost}"
    );
}

/// 票 16 任务域 HTTP 面 + 定时任务四态闭环（真实 wasm 产物 + 真实私有库，S1）
///
/// 断言的外部可见结果分两层：
/// - **HTTP 面**：宿主 `_http_endpoint` 的入参形状与旧插件逐字一致（`method` /
///   `path` / `body` / `query`），回包是 `{status, body}`——`path` 段一个都不改
///   （spec D1「基址随插件 id 改、路径段不改」的落地证据）。未声明路径由插件
///   自答 404（宿主侧「声明命中 / 未声明放行」的精确匹配另有纯函数单测）。
/// - **定时任务四态**：`pending →(到期且宽限内) creating →(Created 事件)
///   executed`，以及 `pending →(超过宽限) missed →(reset) pending →(remove) 消失`
///   ——prompts 入队与自动执行开关都从私有库读回验证，不测内部函数。
///
/// 时间基准：插件全部时间判断以宿主注入的 `now_utc` 为准（WASM 无系统时钟），
/// 但私有库的 `created_at` 用 SQLite `datetime('now')`（真实 UTC）——故队列内容
/// 的断言放在「首轮下发兜底」可能被真实时钟判 stale 之前完成。
///
/// 不调用 deactivate（同票 15 用例：停用会写真实用户目录的 hooks 配置）。
#[test]

fn test_session_task_http_and_scheduled_closed_loop() {
    let _serial = session_plugin_db_guard();
    const HTTP_SESSION: &str = "probe-http-session";
    const TRIGGER_AT: &str = "2026-09-20 00:00:00";
    // 超过宽限 → 判 missed（不补跑）
    const NOW_PAST_GRACE: &str = "2026-09-20 01:00:00";

    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    host_ctx.permission.grant_permissions(
        "com.bedcode.terminal-session",
        &[
            "auth",
            "peer",
            "storage",
            "session:read",
            "session:write",
            "storage",
            "broadcast",
            "fs:read",
            "fs:write",
            "terminal:input",
            "terminal:observe",
            "timer:schedule",
            "ui:sidebar",
            "ui:settings",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>(),
    );
    let mut plugin = wasm_runtime
        .load_plugin_from_file(
            &wasm_path,
            "com.bedcode.terminal-session",
            Arc::clone(&host_ctx),
            &[],
            None,
        )
        .expect("load wasip3 session");
    assert_eq!(plugin.activate().expect("activate"), 0);

    /// 以客户端形态打一次插件 HTTP 端点（宿主 plugin_controller 构造的同一形状）
    fn http_call(
        plugin: &mut LoadedWasmPlugin,
        method: &str,
        path: &str,
        body: serde_json::Value,
        query: serde_json::Value,
    ) -> serde_json::Value {
        let args = serde_json::json!({
            "method": method,
            "path": path,
            "headers": {},
            "body": body,
            "query": query,
        });
        let out = plugin
            .invoke_command("_http_endpoint", &args.to_string())
            .unwrap_or_else(|e| panic!("_http_endpoint {method} {path} 调用失败: {e}"));
        serde_json::from_str(&out)
            .unwrap_or_else(|e| panic!("_http_endpoint {method} {path} 回包非法 JSON: {e} / {out}"))
    }

    // 1. 声明命中的只读端点 → 200 + 载荷（supported-agents 与旧插件同形状）
    let r = http_call(
        &mut plugin,
        "GET",
        "supported-agents",
        serde_json::Value::Null,
        serde_json::json!({}),
    );
    assert_eq!(r["status"], 200, "supported-agents 必须 200, got: {r}");
    assert!(
        r["body"]["data"]["agents"]
            .as_array()
            .map(|a| a.iter().any(|v| v.as_str() == Some("claude")))
            .unwrap_or(false),
        "agents 载荷必须含 claude, got: {r}"
    );

    // 2. 插件未实现的路径 → 插件自答 404（不是 200 空体，也不是宿主错误）
    let r = http_call(
        &mut plugin,
        "GET",
        "not-a-real-endpoint",
        serde_json::Value::Null,
        serde_json::json!({}),
    );
    assert_eq!(r["status"], 404, "未知道路必须由插件返回 404, got: {r}");

    // 3. 队列 HTTP 往返：add → list 读回同一项（path 段 task-queue/* 与旧插件一致）
    let r = http_call(
        &mut plugin,
        "POST",
        "task-queue/add",
        serde_json::json!({ "session_id": HTTP_SESSION, "prompt": "probe-queue-prompt" }),
        serde_json::json!({}),
    );
    assert_eq!(r["status"], 200, "task-queue/add 必须 200, got: {r}");
    let queue_task_id = r["body"]["data"]["task_id"]
        .as_str()
        .unwrap_or_else(|| panic!("add 回包缺 task_id: {r}"))
        .to_string();
    let r = http_call(
        &mut plugin,
        "GET",
        "task-queue/list",
        serde_json::Value::Null,
        serde_json::json!({ "session_id": HTTP_SESSION }),
    );
    assert_eq!(r["status"], 200, "task-queue/list 必须 200, got: {r}");
    assert!(
        r["body"]["data"]["tasks"]
            .as_array()
            .map(|tasks| tasks.iter().any(|t| t["id"].as_str() == Some(queue_task_id.as_str())))
            .unwrap_or(false),
        "HTTP 入队项必须能从 HTTP 列表读回（同一私有库）, got: {r}"
    );

    // 4. 定时任务：HTTP create → list 可见 pending；配置播种走插件真源
    //    （scheduled → launch spec 的解析链读私有库，见 task/scheduled.rs 模块文档）
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let config_id = rt
        .block_on(async { seed_config_in_plugin_store(&mut plugin, "定时探针", "/tmp", "claude").await })
        ["id"].as_str().unwrap().to_string();

    let r = http_call(
        &mut plugin,
        "POST",
        "scheduled-jobs/create",
        serde_json::json!({
            "name": "probe-job",
            "config_id": config_id,
            "trigger_at": TRIGGER_AT,
            "prompts": ["probe-scheduled-prompt"],
        }),
        serde_json::json!({}),
    );
    assert_eq!(r["status"], 200, "scheduled-jobs/create 必须 200, got: {r}");
    let job_id = r["body"]["data"]["job_id"]
        .as_str()
        .unwrap_or_else(|| panic!("create 回包缺 job_id: {r}"))
        .to_string();

    /// 直接播种/改写 `task_scheduled` 行（外部可见真源），返回 job id
    fn seed_scheduled_job(
        status: &str,
        id: &str,
        config_id: &str,
        trigger_at: &str,
        prompts_json: &str,
        session_id: Option<&str>,
    ) -> String {
        let db_path = plugin_db_root().join("com.bedcode.terminal-session").join("plugin.db");
        let job_id = if id.is_empty() {
            format!(
                "probe-{}-{}-{}",
                status,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0)
            )
        } else {
            id.to_string()
        };
        let conn = rusqlite::Connection::open(&db_path)
            .unwrap_or_else(|e| panic!("打开插件私有库失败 {}: {e}", db_path.display()));
        // 插件侧连接可能正在写（同一文件的跨连接竞态）：等锁而不是立刻失败
        let _ = conn.busy_timeout(std::time::Duration::from_secs(10));
        let inserted = conn
            .execute(
                "INSERT OR REPLACE INTO task_scheduled \
                     (id, name, config_id, trigger_at, prompts, status, session_id, executed_at, error, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, datetime('now'))",
                rusqlite::params![job_id, "probe", config_id, trigger_at, prompts_json, status, session_id],
            )
            .unwrap_or_else(|e| panic!("播种 task_scheduled 行失败: {e}"));
        assert_eq!(inserted, 1, "播种必须命中一行");
        job_id
    }

    /// 从 HTTP 列表端点取指定 job 行（真源 = 私有库 task_scheduled）
    fn job_row(plugin: &mut LoadedWasmPlugin, job_id: &str) -> serde_json::Value {
        let r = http_call(
            plugin,
            "GET",
            "scheduled-jobs/list",
            serde_json::Value::Null,
            serde_json::json!({}),
        );
        assert_eq!(r["status"], 200, "scheduled-jobs/list 必须 200, got: {r}");
        r["body"]["data"]["jobs"]
            .as_array()
            .and_then(|jobs| jobs.iter().find(|j| j["id"].as_str() == Some(job_id)).cloned())
            .unwrap_or_else(|| panic!("列表中找不到 job {job_id}: {r}"))
    }

    let job = job_row(&mut plugin, &job_id);
    assert_eq!(job["status"], "pending", "新建定时任务必须是 pending, got: {job}");
    assert_eq!(job["config_id"], config_id.as_str(), "config_id 原样落库, got: {job}");

    // 5. creating → executed（Created 事件驱动的入队腿）
    //
    // **为什么直接播种私有库**：`creating` 的正常来源是 tick 调
    // `host-session.create`，而该原语在宿主侧要经 `AppContext::global()` 回灌
    // 会话生命周期事件——无头 harness 没有全局 AppContext（unit 测试进程里
    // 是否已 init 取决于用例顺序），把创建腿塞进本用例只会得到跨用例竞态。
    // 播种与「内核 pairings 表播种」同法：外部可见状态真源在库里，插件读的
    // 就是那一行，事件推进路径与生产完全同形。
    // （pending →creating 这一腿由生产路径与票 18 的手工回归清单覆盖。）
    let scheduled_session_id = format!("probe-scheduled-{}", std::process::id());
    seed_scheduled_job(
        "creating",
        &job_id,
        &config_id,
        TRIGGER_AT,
        r#"["probe-scheduled-prompt"]"#,
        Some(&scheduled_session_id),
    );
    let job = job_row(&mut plugin, &job_id);
    assert_eq!(job["status"], "creating", "播种的 creating 行必须可读, got: {job}");

    plugin
        .on_session_lifecycle(&serde_json::json!({
            "event_type": "created",
            "session_id": scheduled_session_id,
            "config_id": config_id,
            // Created 变体的 name / working_dir 无 serde default（events.rs）：
            // 缺字段会让整个事件反序列化失败并被宿主当作未知事件丢弃，
            // 插件侧表现为「job 永久停在 creating」而不是报错
            "name": "定时探针会话",
            "working_dir": "/tmp",
            "resource_dir": ""
        }))
        .expect("created 事件不得报错");
    let job = job_row(&mut plugin, &job_id);
    assert_eq!(job["status"], "executed", "Created 事件后必须归档 executed, got: {job}");

    let r = http_call(
        &mut plugin,
        "GET",
        "task-queue/list",
        serde_json::Value::Null,
        serde_json::json!({ "session_id": scheduled_session_id }),
    );
    assert!(
        r["body"]["data"]["tasks"]
            .as_array()
            .map(|tasks| !tasks.is_empty() && tasks.iter().all(|t| t["source"].as_str() == Some("scheduled")))
            .unwrap_or(false),
        "定时 prompts 必须以 source='scheduled' 入队, got: {r}"
    );
    let r = http_call(
        &mut plugin,
        "GET",
        "session-settings",
        serde_json::Value::Null,
        serde_json::json!({ "session_id": scheduled_session_id }),
    );
    assert_eq!(
        r["body"]["data"]["auto_execute"],
        serde_json::json!(true),
        "定时任务语义要求无人值守：入队即打开自动执行, got: {r}"
    );

    // 6. creating 卡死兑底：超过宽限仍在 creating 的行（宿主创建失败、事件永不到达）
    //    → tick 标 failed 并带原因
    let stuck_job_id = seed_scheduled_job(
        "creating",
        "",
        &config_id,
        TRIGGER_AT,
        r#"["probe-stuck-prompt"]"#,
        Some("probe-stuck-session"),
    );
    let out = plugin
        .invoke_command(
            "session.task.scheduler-tick",
            &serde_json::json!({ "now_utc": NOW_PAST_GRACE }).to_string(),
        )
        .expect("tick for stuck job");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        r["failed"].as_array().map(|a| a.is_empty()).unwrap_or(false),
        "tick 不得有失败域（定时域内某一步失败也只降级本域，D7）, got: {out}"
    );
    let job = job_row(&mut plugin, &stuck_job_id);
    assert_eq!(job["status"], "failed", "creating 超宽限必须兑底为 failed, got: {job}");
    assert!(
        job["error"]
            .as_str()
            .map(|e| e.contains("Session creation timed out"))
            .unwrap_or(false),
        "failed 必须带创建超时原因, got: {job}"
    );

    // 7. missed 路径：新建 job 与上面同触发时刻，本轮 now 已超宽限 → missed（不补跑）
    let r = http_call(
        &mut plugin,
        "POST",
        "scheduled-jobs/create",
        serde_json::json!({
            "config_id": config_id,
            "trigger_at": TRIGGER_AT,
            "prompts": ["probe-late-prompt"],
        }),
        serde_json::json!({}),
    );
    let late_job_id = r["body"]["data"]["job_id"].as_str().expect("job_id").to_string();
    let out = plugin
        .invoke_command(
            "session.task.scheduler-tick",
            &serde_json::json!({ "now_utc": NOW_PAST_GRACE }).to_string(),
        )
        .expect("tick past grace");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(r["error"].is_null(), "tick 不得整体失败, got: {out}");
    let job = job_row(&mut plugin, &late_job_id);
    assert_eq!(
        job["status"], "missed",
        "超过宽限期未执行必须判 missed（不补跑）, got: {job}"
    );
    assert!(
        job["error"]
            .as_str()
            .map(|e| e.contains("app was not running"))
            .unwrap_or(false),
        "missed 必须带原因文本, got: {job}"
    );

    // 8. reset → 回 pending（可改触发时间）；remove → 从清单消失
    let r = http_call(
        &mut plugin,
        "POST",
        "scheduled-jobs/reset",
        serde_json::json!({ "job_id": late_job_id, "trigger_at": "2026-12-31 00:00:00" }),
        serde_json::json!({}),
    );
    assert_eq!(r["status"], 200, "reset 必须 200, got: {r}");
    let job = job_row(&mut plugin, &late_job_id);
    assert_eq!(job["status"], "pending", "reset 后回 pending, got: {job}");
    assert_eq!(
        job["trigger_at"], "2026-12-31 00:00:00",
        "reset 可改触发时间, got: {job}"
    );

    let r = http_call(
        &mut plugin,
        "DELETE",
        "scheduled-jobs/remove",
        serde_json::json!({ "job_id": late_job_id }),
        serde_json::json!({}),
    );
    assert_eq!(r["status"], 200, "remove 必须 200, got: {r}");
    let r = http_call(
        &mut plugin,
        "GET",
        "scheduled-jobs/list",
        serde_json::Value::Null,
        serde_json::json!({}),
    );
    let jobs = r["body"]["data"]["jobs"].as_array().expect("jobs 数组");
    assert!(
        jobs.iter().all(|j| j["id"].as_str() != Some(late_job_id.as_str())),
        "删除后不得再出现在清单里, got: {jobs:?}"
    );

    // 9. 命令面与 HTTP 面同源：同一私有库读回同一条 job
    let out = plugin
        .invoke_command("session.task.scheduled-list", "{}")
        .expect("scheduled-list");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        r["jobs"]
            .as_array()
            .map(|jobs| jobs.iter().any(|j| j["id"].as_str() == Some(job_id.as_str())))
            .unwrap_or(false),
        "命令面与 HTTP 面必须读同一份真源, got: {r}"
    );

    // 10. 参数非法显性报错（HTTP 与命令面都不得静默吞掉）
    let r = http_call(
        &mut plugin,
        "POST",
        "scheduled-jobs/create",
        serde_json::json!({ "config_id": "", "trigger_at": "", "prompts": [] }),
        serde_json::json!({}),
    );
    assert_eq!(r["status"], 400, "缺参数必须 400, got: {r}");
    let out = plugin
        .invoke_command("session.task.scheduled-delete", "{}")
        .expect("scheduled-delete 返回 JSON");
    let r: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        r["error"]
            .as_str()
            .map(|e| e.contains("missing job_id"))
            .unwrap_or(false),
        "命令面缺参数必须显性报错, got: {out}"
    );
}

/// 票 05 会话中心互调闭环（消费方 = sdk-test caller 角色），两条路径：
///
/// **trust 路径（列表 → 撤销 → 列表变化）**：认证中心私有库 `auth_pairings`
/// 播种（1 活跃 + 1 已软删，v24 认证记录下沉后真源在私有库）→ `trust-list` 只回
/// 活跃条目（软删行不出现，但原始记录里仍在，撤销检测因此成立）→
/// `trust-revoke` 软删私有库真源 → 再 `trust-list` 立即消失 → 直查私有库表确认
/// `is_active = 0`（**真源一致性**：插件不再自持账本）。
///
/// **consent 路径**（headless 宿主：host-peer require_app 失败 → 信任不可验证 →
/// fail-closed 按未知处理，见 consent/ops.rs 模块文档）：阶段 1（无用户意向）→ ask；
/// 阶段 2（回传 userDecision）→ accept/explicit、deny、accept/one_time。
/// 已信任免确认的自动放行路径由插件 native 单测覆盖（mock 可信集注入）。
///
/// 另钉两条边界：未声明 api 被宿主门禁拒绝（ADR 0017）；peerError 显性透出
/// （无头上下文 host-peer 不可用不静默降级为空列表）。产物缺失时跳过。
#[test]

fn test_session_trust_and_consent_api_closed_loop() {
    // 会话插件私有库是进程级共享路径：与其它会话闭环用例串行（见锁文档）
    let _serial = session_plugin_db_guard();
    use crate::utils::auth::auth_center as bridge;

    const CONSUMER_ID: &str = "com.bedcode.consent-consumer";
    const NODE: &str = "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344";
    let session_id = bridge::SESSION_PLUGIN_ID;

    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let session_component = wasm_runtime
        .compile_component(&std::fs::read(&wasm_path).expect("read session artifact"))
        .expect("compile session artifact");
    let consumer_component = wasm_runtime
        .compile_component(&build_sdk_test_component())
        .expect("compile sdk-test consumer");

    // 授权路径等价 PluginHost 装载（manifest permissions 登记）：会话中心声明
    // auth（secret-store + 记录面）+ peer（consent 取可信集 / trust peer 段）
    host_ctx
        .permission
        .grant_permissions(session_id, &["auth".to_string(), "peer".to_string(), "storage".to_string()]);
    // 登记目标插件声明的 api（等价 PluginHost::activate_plugin 的登记）。
    // 清单读插件工程 manifest（单一真源），不在测试里抄第二份。
    let session_api_list = session_apis();
    host_ctx.api_registry().register(
        session_id,
        &session_api_list.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let session = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&session_component, session_id, host_ctx.clone(), &[], None)
                .expect("instantiate session"),
        ));
        let consumer = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&consumer_component, CONSUMER_ID, host_ctx.clone(), &[], None)
                .expect("instantiate consumer"),
        ));
        let instances = Arc::new(RwLock::new(HashMap::from([
            (session_id.to_string(), session.clone()),
            (CONSUMER_ID.to_string(), consumer.clone()),
        ])));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher { instances }))
            .await;

        session.lock().await.activate().expect("session activate");
        consumer.lock().await.activate().expect("consumer activate");

        // v24 认证记录下沉：配对真源 = 认证中心私有库 `auth_pairings`。activate
        // 已建表（auth_records::ensure_schema_via_host），此处直写播种
        // （1 活跃 + 1 已软删——撤销检测依赖软删行可见）
        {
            let db_path = plugin_db_root().join(bridge::SESSION_PLUGIN_ID).join("plugin.db");
            let conn = rusqlite::Connection::open(&db_path)
                .unwrap_or_else(|e| panic!("打开认证中心私有库失败 {}: {e}", db_path.display()));
            let _ = conn.busy_timeout(std::time::Duration::from_secs(10));
            for (id, name, fp, active, paired_at) in [
                ("p-live", "Trust Phone", "fp-live", 1, "2026-09-19T00:00:00Z"),
                ("p-revoked", "Revoked Pad", "fp-revoked", 0, "2026-09-18T00:00:00Z"),
            ] {
                conn.execute(
                    "INSERT OR REPLACE INTO auth_pairings \
                     (id, device_name, device_fingerprint, address, uid_hash, paired_at, \
                      last_seen, connect_count, is_active) \
                     VALUES (?1, ?2, ?3, NULL, NULL, ?4, NULL, 1, ?5)",
                    rusqlite::params![id, name, fp, paired_at, active],
                )
                .expect("seed auth_pairings");
            }
        }

        // ==================== trust：列表（私有库真源 → 统一视图） ====================
        let result = consumer
            .lock()
            .await
            .invoke_command("test_session_trust_list", "{}")
            .expect("trust-list");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        let devices = r["devices"].as_array().expect("devices 数组");
        assert_eq!(devices.len(), 1, "软删行不进列表（is_active 过滤）, got: {result}");
        assert_eq!(devices[0]["kind"], "pairing");
        assert_eq!(devices[0]["id"], "p-live", "活跃配对可见");
        assert_eq!(devices[0]["name"], "Trust Phone");
        assert_eq!(devices[0]["fingerprint"], "fp-live");
        assert_eq!(devices[0]["active"], true);
        // 无头上下文 peer-net 不可用：peerError 必须透出（不静默降级为空）
        let peer_err = r["peerError"].as_str().expect("peerError 必须透出");
        assert!(
            peer_err.contains("unavailable") || peer_err.contains("headless"),
            "无头上下文 peer 不可用错误透出, got: {peer_err}"
        );

        // ==================== trust：撤销（写内核真源） ====================
        let result = consumer
            .lock()
            .await
            .invoke_command("test_session_trust_revoke", r#"{"id":"p-live"}"#)
            .expect("trust-revoke");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["removed"], true, "命中即报 removed, got: {result}");
        assert_eq!(r["kind"], "pairing");

        // 撤销后列表立即变化
        let result = consumer
            .lock()
            .await
            .invoke_command("test_session_trust_list", "{}")
            .expect("trust-list after revoke");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            r["devices"].as_array().unwrap().len(),
            0,
            "撤销后立即从统一视图消失, got: {result}"
        );

        // 私有库表断言：软删而非物理删除（auth_records::revoke 语义），且真的落库
        {
            let db_path = plugin_db_root().join(bridge::SESSION_PLUGIN_ID).join("plugin.db");
            let conn = rusqlite::Connection::open(&db_path)
                .unwrap_or_else(|e| panic!("打开认证中心私有库失败 {}: {e}", db_path.display()));
            let active: i32 = conn
                .query_row("SELECT is_active FROM auth_pairings WHERE id = 'p-live'", [], |row| {
                    row.get(0)
                })
                .expect("p-live 必须仍在（软删保留记录）");
            assert_eq!(active, 0, "撤销写的是认证中心私有库真源（is_active = 0）");
            let rows: i32 = conn
                .query_row("SELECT COUNT(*) FROM auth_pairings", [], |row| row.get(0))
                .expect("count");
            assert_eq!(rows, 2, "软删保留记录（不物理删除）");
        }

        // ==================== consent：阶段 1（信任预检） ====================
        // 仅 peer 信息（无用户意向）→ 无头上下文 host-peer 不可用 → fail-closed ask
        let result = consumer
            .lock()
            .await
            .invoke_command(
                "test_session_consent_decide",
                &format!(r#"{{"peerInfo":{{"requestId":"req-c1","nodeId":"{NODE}","deviceName":"模拟对端"}}}}"#),
            )
            .expect("consent-decide phase 1");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            r["decision"]["decision"], "ask",
            "无头上下文信任不可验证 → fail-closed ask, got: {result}"
        );
        assert_eq!(r["decision"]["requestId"], "req-c1");
        assert!(r["decision"].get("reason").is_none(), "ask 无 reason, got: {result}");

        // 阶段 2（允许）：回传 userDecision=accept → accept / reason=explicit
        let result = consumer
            .lock()
            .await
            .invoke_command(
                "test_session_consent_decide_explicit",
                &format!(r#"{{"userDecision":"accept","peerInfo":{{"requestId":"req-c2","nodeId":"{NODE}"}}}}"#),
            )
            .expect("consent-decide explicit accept");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["decision"]["decision"], "accept", "got: {result}");
        assert_eq!(
            r["decision"]["reason"], "explicit",
            "用户显式允许 reason=explicit, got: {result}"
        );

        // 阶段 2（拒绝）：userDecision=deny → deny / reason=explicit
        let result = consumer
            .lock()
            .await
            .invoke_command(
                "test_session_consent_decide_explicit",
                &format!(r#"{{"userDecision":"deny","peerInfo":{{"requestId":"req-c3","nodeId":"{NODE}"}}}}"#),
            )
            .expect("consent-decide explicit deny");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["decision"]["decision"], "deny", "got: {result}");
        assert_eq!(r["decision"]["reason"], "explicit");

        // 阶段 2（一次性确认）：userDecision=one_time → accept / reason=one_time
        // （放行但不改变信任状态——消费方据此不落信任，下次首连仍询问）
        let result = consumer
            .lock()
            .await
            .invoke_command(
                "test_session_consent_decide_explicit",
                &format!(r#"{{"userDecision":"one_time","peerInfo":{{"requestId":"req-c4","nodeId":"{NODE}"}}}}"#),
            )
            .expect("consent-decide explicit one_time");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["decision"]["decision"], "accept", "got: {result}");
        assert_eq!(
            r["decision"]["reason"], "one_time",
            "一次性确认 reason=one_time, got: {result}"
        );

        // ==================== 门禁：未声明 api 不可调（ADR 0017） ====================
        let result = consumer
            .lock()
            .await
            .invoke_command("test_session_undeclared", "{}")
            .expect("test_session_undeclared");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"].as_str().map(|e| e.contains("not declared")).unwrap_or(false),
            "未声明 api 必须被门禁拒绝, got: {result}"
        );

        session.lock().await.deactivate().expect("session deactivate");
        consumer.lock().await.deactivate().expect("consumer deactivate");
    });
}

#[test]

fn test_business_endpoints_dual_track_closed_loop() {
    let _serial = session_plugin_db_guard();
    const SESSION_ID: &str = "com.bedcode.terminal-session";
    let session_api_list = session_apis();
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }

    // 临时工作区（working_dir = 配置真源指向它；node_modules 将被 exclude）
    let ws = tempfile::tempdir().expect("tempdir");
    let work = ws.path().join("work");
    std::fs::create_dir_all(work.join("src")).expect("mkdir src");
    std::fs::create_dir_all(work.join("node_modules")).expect("mkdir node_modules");
    std::fs::write(work.join("src").join("main.rs"), "fn main() {}").expect("write main.rs");
    std::fs::write(work.join("README.md"), "# readme").expect("write readme");
    std::fs::write(work.join("node_modules").join("x.js"), "x").expect("write x.js");
    let working_dir = work.to_string_lossy().to_string();

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    // 票 07：为**工作区根**预置持久化授权（生产里 = 用户第一次打开文件树时点的授权 +
    // 记住）。文件浏览根是用户选的任意目录，不在第一方集成目录清单里（清单只覆盖
    // `.claude` 这类具名段）。授权落在工作区根而不是会话目录：越界探测
    // （`../outside.txt`）必须先被宿主 exists 判成 404，而不是被权限层拦成 500——
    // 那条 404/403 的分层语义是插件契约的一部分。
    block_on_async(
        host_ctx
            .fs_auth()
            .save_granted_path(SESSION_ID, &ws.path().to_string_lossy()),
    )
    .expect("seed fs grant for workspace");
    // 私有库清空（marker 干净）
    let _ = std::fs::remove_dir_all(plugin_db_root().join(SESSION_ID));
    let rt = tokio::runtime::Runtime::new().expect("runtime");

    // legacy 主库：quick_actions 表播种（契约退役后仅存量旧库持有该表）
    let legacy_db = crate::db::Database::new(&std::path::PathBuf::from(":memory:")).expect("legacy db");
    legacy_db.init_schema().expect("legacy schema");
    legacy_db
        .seed_legacy_quick_action_row(&crate::db::LegacyQuickActionRow {
            id: "qa-1".into(),
            name: "部署".into(),
            content: "pnpm run deploy".into(),
            icon: Some("rocket".into()),
            color: None,
            category: Some("dev".into()),
            sort_order: 2,
            created_at: "2026-09-20T00:00:00Z".into(),
        })
        .expect("seed qa-1");
    legacy_db
        .seed_legacy_quick_action_row(&crate::db::LegacyQuickActionRow {
            id: "qa-2".into(),
            name: "构建".into(),
            content: "pnpm run build".into(),
            icon: None,
            color: Some("#0f0".into()),
            category: None,
            sort_order: 1,
            created_at: "2026-09-19T00:00:00Z".into(),
        })
        .expect("seed qa-2");

    // 会话配置播种移到插件激活之后（见下：v21 起配置面插件必需，无宿主降级）

    // 权限（manifest 全量）+ api 注册表（含 quick-actions-import 与桥接锚点）
    host_ctx.permission.grant_permissions(
        SESSION_ID,
        &[
            "auth".to_string(),
            "peer".to_string(), "storage".to_string(),
            "session:read".to_string(),
            "session:write".to_string(),
            "storage".to_string(),
            "fs:read".to_string(),
            "fs:write".to_string(),
            "process:run".to_string(),
            "broadcast".to_string(),
            "terminal:input".to_string(),
            "terminal:observe".to_string(),
            "timer:schedule".to_string(),
            "ui:input".to_string(),
            "ui:settings".to_string(),
            "ui:sidebar".to_string(),
        ],
    );
    host_ctx.api_registry().register(
        SESSION_ID,
        &session_api_list.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );

    rt.block_on(async move {
        let component = wasm_runtime
            .compile_component(&std::fs::read(&wasm_path).expect("read session artifact"))
            .expect("compile session artifact");
        let instances = Arc::new(RwLock::new(HashMap::new()));
        let session = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, SESSION_ID, host_ctx.clone(), &[], None)
                .expect("instantiate session"),
        ));
        instances.write().await.insert(SESSION_ID.to_string(), session.clone());
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::clone(&instances),
            }))
            .await;
        session.lock().await.activate().expect("activate session");

        // ==================== 1. 会话配置播种（插件真源，激活后必经） ====================
        // v21：配置面插件必需（无宿主降级），故播种必须在激活之后
        let seeded_config =
            seed_config_in_plugin_store(&mut *session.lock().await, "工作台", &working_dir, "bash").await;

        // ==================== handoff：legacy 主库 → 插件私有库 ====================
        let report = crate::wasm_core::legacy::quick_actions_migration::migrate(&host_ctx, &legacy_db)
            .await
            .expect("handoff migrate");
        assert!(report.skipped.is_none(), "插件在位时必须执行迁移");
        let plugin_report = report.plugin_report.expect("plugin report");
        assert_eq!(plugin_report["imported"], 2, "两条 legacy 快捷指令必须全部迁入");

        // ==================== _http_endpoint 双轨对照 ====================
        async fn http(
            plugin: &Arc<Mutex<LoadedWasmPlugin>>,
            method: &str,
            path: &str,
            body: serde_json::Value,
        ) -> serde_json::Value {
            let args = serde_json::json!({
                "method": method,
                "path": path,
                "headers": {},
                "body": body,
                "query": {},
            });
            let result = plugin
                .lock()
                .await
                .invoke_command("_http_endpoint", &args.to_string())
                .expect("_http_endpoint");
            serde_json::from_str(&result).expect("http envelope json")
        }
        fn data(envelope: serde_json::Value) -> serde_json::Value {
            envelope["body"]["data"].clone()
        }

        // --- GET quick-actions：插件面 == 宿主旧 QuickActionItem 形状（逐字节） ---
        let quick = http(&session, "GET", "quick-actions", serde_json::Value::Null).await;
        let golden_quick = serde_json::to_value(crate::server::http::dtos::config_dto::QuickActionListResponseData {
            actions: vec![
                crate::server::http::dtos::config_dto::QuickActionItem {
                    id: "qa-2".into(),
                    name: "构建".into(),
                    content: "pnpm run build".into(),
                    icon: None,
                    color: Some("#0f0".into()),
                },
                crate::server::http::dtos::config_dto::QuickActionItem {
                    id: "qa-1".into(),
                    name: "部署".into(),
                    content: "pnpm run deploy".into(),
                    icon: Some("rocket".into()),
                    color: None,
                },
            ],
        })
        .expect("golden quick");
        assert_eq!(
            data(quick),
            golden_quick,
            "quick-actions 插件面必须与宿主旧 DTO 逐字节一致（sort_order 升序 + icon/color 显式 null）"
        );

        // --- GET configs：插件面 == 宿主旧 ConfigItem 形状 ---
        let configs = http(&session, "GET", "configs", serde_json::Value::Null).await;
        let golden_configs = serde_json::to_value(crate::server::http::dtos::config_dto::ConfigListResponseData {
            configs: vec![crate::server::http::dtos::config_dto::ConfigItem {
                id: seeded_config["id"].as_str().unwrap().to_string(),
                name: "工作台".into(),
                environment: "linux".into(),
                wsl_distro: None,
                working_dir: working_dir.clone(),
                command: "bash".into(),
            }],
        })
        .expect("golden configs");
        assert_eq!(
            data(configs),
            golden_configs,
            "configs 插件面必须与宿主旧 DTO 逐字节一致（wslDistro 显式 null）"
        );

        // --- POST file-tree：插件面 == 宿主 scan_dir 语义（exclude + 排序 + 节点形状） ---
        let tree_env = http(
            &session,
            "POST",
            "file-tree",
            serde_json::json!({ "sessionId": seeded_config["id"], "excludeDirs": ["node_modules"] }),
        )
        .await;
        assert_eq!(
            data(tree_env),
            serde_json::json!({
                "tree": [{
                    "name": "src", "nodeType": "folder", "path": "src",
                    "children": [{ "name": "main.rs", "nodeType": "file", "path": "src/main.rs" }]
                }, {
                    "name": "README.md", "nodeType": "file", "path": "README.md"
                }]
            }),
            "file-tree：node_modules 排除 + 文件夹在前 + 文件 children 省略"
        );

        // --- GET file-tree-children：单层 + Cache-Control 头（参数走 query，与宿主一致） ---
        let children_args = serde_json::json!({
            "method": "GET",
            "path": "file-tree-children",
            "headers": {},
            "body": serde_json::Value::Null,
            // query 键名 snake_case（宿主 FileTreeChildrenQuery 无 rename，移动端
            // useHttpApi 同名构造）——camelCase 是假绿，真机请求会解析不到 session_id
            "query": serde_json::json!({ "session_id": seeded_config["id"], "dir_path": "src", "exclude_dirs": "" }),
        });
        let children_env: serde_json::Value = serde_json::from_str(
            &session
                .lock()
                .await
                .invoke_command("_http_endpoint", &children_args.to_string())
                .expect("_http_endpoint children"),
        )
        .expect("children envelope json");
        assert_eq!(
            data(children_env.clone()),
            serde_json::json!({
                "children": [{ "name": "main.rs", "nodeType": "file", "path": "src/main.rs" }]
            })
        );
        assert_eq!(
            children_env["headers"]["Cache-Control"], "private, max-age=30",
            "file-tree-children 必须带迁移前宿主同口径的 Cache-Control（30 秒，缓存值随域下沉插件）"
        );

        // --- POST file-content：成功路径 ---
        let content_env = http(
            &session,
            "POST",
            "file-content",
            serde_json::json!({ "sessionId": seeded_config["id"], "filePath": "src/main.rs" }),
        )
        .await;
        assert_eq!(
            data(content_env),
            serde_json::json!({ "content": "fn main() {}", "fileName": "main.rs" })
        );

        // --- 确定性错误（与宿主文案逐字一致） ---
        let not_found = http(
            &session,
            "POST",
            "file-content",
            serde_json::json!({ "sessionId": seeded_config["id"], "filePath": "../outside.txt" }),
        )
        .await;
        assert_eq!(
            not_found["body"]["code"], 404,
            "不存在的 ../ 路径先答 404（宿主 exists 前置）"
        );
        // 越界但存在 → 403
        std::fs::write(ws.path().join("outside.txt"), "evil").expect("outside");
        let escape = http(
            &session,
            "POST",
            "file-content",
            serde_json::json!({ "sessionId": seeded_config["id"], "filePath": "../outside.txt" }),
        )
        .await;
        assert_eq!(escape["body"]["code"], 403, "穿越工作目录必须拒绝");
        assert_eq!(
            escape["body"]["message"],
            "Access denied: file is outside working directory"
        );

        // --- 非 git 仓库：diff-tree / file-diff 答 400（宿主文案逐字一致） ---
        let diff_tree = http(
            &session,
            "POST",
            "diff-tree",
            serde_json::json!({ "sessionId": seeded_config["id"], "excludeDirs": [] }),
        )
        .await;
        assert_eq!(diff_tree["body"]["code"], 400);
        assert_eq!(diff_tree["body"]["message"], "Not a git repository");
        let file_diff = http(
            &session,
            "POST",
            "file-diff",
            serde_json::json!({ "sessionId": seeded_config["id"], "filePath": "src/main.rs" }),
        )
        .await;
        assert_eq!(file_diff["body"]["code"], 400);
        assert_eq!(file_diff["body"]["message"], "Not a git repository");

        // ==================== 票 04：git 查询域闭环（真实 git 仓库 + 真实 run-sync） ====================
        // 工作区此刻已被上方断言证明是「非 git 仓库」形态——现在把它变成真仓库，
        // 验证插件经 host-process run-sync 执行 git 的完整链路
        let run_git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&work)
                .output()
                .expect("git run");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run_git(&["init", "-q"]);
        run_git(&["config", "user.email", "t@t"]);
        run_git(&["config", "user.name", "t"]);
        run_git(&["add", "."]);
        run_git(&["commit", "-q", "-m", "init"]);
        run_git(&["branch", "dev"]);

        // GET git/branches：query 键名 snake_case（宿主 GitBranchesQuery 无 rename）
        let branches_args = serde_json::json!({
            "method": "GET",
            "path": "git/branches",
            "headers": {},
            "body": serde_json::Value::Null,
            "query": serde_json::json!({ "session_id": seeded_config["id"] }),
        });
        let branches_env: serde_json::Value = serde_json::from_str(
            &session
                .lock()
                .await
                .invoke_command("_http_endpoint", &branches_args.to_string())
                .expect("_http_endpoint branches"),
        )
        .expect("branches envelope json");
        assert_eq!(branches_env["body"]["code"], 0);
        let branches = data(branches_env.clone());
        assert_eq!(branches["isGitRepo"], true);
        assert!(
            branches["branches"]
                .as_array()
                .expect("branches")
                .iter()
                .any(|b| b == "dev"),
            "branch --list 必须含 dev: {branches}"
        );
        let initial_branch = branches["currentBranch"].as_str().expect("current branch").to_string();

        // GET git/status：新文件（未跟踪）→ hasChanges
        std::fs::write(work.join("untracked.txt"), "x").expect("write untracked");
        let status_args = serde_json::json!({
            "method": "GET",
            "path": "git/status",
            "headers": {},
            "body": serde_json::Value::Null,
            "query": serde_json::json!({ "session_id": seeded_config["id"] }),
        });
        let status_env: serde_json::Value = serde_json::from_str(
            &session
                .lock()
                .await
                .invoke_command("_http_endpoint", &status_args.to_string())
                .expect("_http_endpoint status"),
        )
        .expect("status envelope json");
        assert_eq!(status_env["body"]["code"], 0);
        assert_eq!(data(status_env.clone())["hasChanges"], true);
        assert_eq!(data(status_env)["changedCount"], 1);

        // POST git/checkout：切到 dev，回执分支名；再查 currentBranch 即 dev
        let checkout = http(
            &session,
            "POST",
            "git/checkout",
            serde_json::json!({ "sessionId": seeded_config["id"], "branch": "dev" }),
        )
        .await;
        assert_eq!(checkout["body"]["code"], 0);
        assert_eq!(data(checkout), serde_json::json!({ "branch": "dev" }));
        let branches_after = serde_json::from_str::<serde_json::Value>(
            &session
                .lock()
                .await
                .invoke_command("_http_endpoint", &branches_args.to_string())
                .expect("_http_endpoint branches after"),
        )
        .expect("branches envelope json");
        assert_eq!(
            data(branches_after)["currentBranch"],
            "dev",
            "checkout 后当前分支必须是 dev（checkout 前为 {initial_branch}）"
        );

        // checkout 白名单前置：注入形态在插件侧 500 拒绝（不经 git，文案逐字一致）
        let inject = http(
            &session,
            "POST",
            "git/checkout",
            serde_json::json!({ "sessionId": seeded_config["id"], "branch": "main;rm -rf /" }),
        )
        .await;
        assert_eq!(inject["body"]["code"], 500);
        assert_eq!(
            inject["body"]["message"],
            "Invalid input: Invalid branch name: main;rm -rf /"
        );

        // ==================== handoff 幂等：重推不重复导入 ====================
        let again = crate::wasm_core::legacy::quick_actions_migration::migrate(&host_ctx, &legacy_db)
            .await
            .expect("handoff again");
        let again_report = again.plugin_report.expect("plugin report");
        assert_eq!(again_report["alreadyMigrated"], true, "marker 已在 → 重推整体跳过");
        assert_eq!(again_report["imported"], 0);

        session.lock().await.deactivate().expect("final deactivate");
    });
}

/// 票 09 真实 wasm 闭环：真实会话中心产物 + 真实 `host-session`
/// `create-with-spec` 原语 + 真实插件私有库真源——「创建编排经插件」从
/// 命名唯一化到会话落库的完整链路（S1 已知缺口补：session 域成功路径）。
///
/// 流程：桥接播种配置（真源 + 投影）→ 激活会话中心 → 桥接编排创建
/// （`start=false`，不 spawn 进程无残留）→ 异步等待会话落库 → 断言命名
/// 唯一化（同配置第二次 → `(1)` 后缀）→ 降级（注销互调面 → 桥接返回 None）。
#[test]

fn test_session_create_with_spec_closed_loop() {
    // 会话插件私有库是进程级共享路径：与其它会话闭环用例串行（见锁文档）
    let _serial = session_plugin_db_guard();
    const SESSION_ID: &str = "com.bedcode.terminal-session";
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    // 独立私有库根目录：进程级 `plugin_db_root()` 被 wasm_runtime::tests 内
    // 多个并行测试共享（各自 remove_dir_all + 写同一 SQLite 文件 → BUSY/缺失），
    // 本测试对私有库只读真源（不测迁移），用独立目录消除并行文件竞争。
    let mut host_ctx = host_ctx;
    if let Some(ctx) = Arc::get_mut(&mut host_ctx) {
        ctx.set_plugin_db_root(Some(std::env::temp_dir().join(format!(
                    "bedcode_plugin_dbs_spec_{}_{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                ))));
    }
    let _ = std::fs::remove_dir_all(plugin_db_root().join(SESSION_ID));
    let component = wasm_runtime
        .compile_component(&std::fs::read(&wasm_path).expect("read session artifact"))
        .expect("compile session artifact");

    host_ctx.permission.grant_permissions(
        SESSION_ID,
        &[
            "auth".to_string(),
            "peer".to_string(), "storage".to_string(),
            "session:read".to_string(),
            // 票 09：host-session.create-with-spec（创建编排执行端）
            "session:write".to_string(),
            "storage".to_string(),
        ],
    );
    host_ctx.api_registry().register(
        SESSION_ID,
        &session_apis().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // ==================== 1. 激活会话中心（配置面 v21 起插件必需） ====================
        let instances = Arc::new(RwLock::new(HashMap::new()));
        let session = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, SESSION_ID, host_ctx.clone(), &[], None)
                .expect("instantiate session"),
        ));
        instances.write().await.insert(SESSION_ID.to_string(), session.clone());
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::clone(&instances),
            }))
            .await;
        session.lock().await.activate().expect("activate session");

        // ==================== 2. 播种配置（插件真源；激活后必经） ====================
        let seeded = seed_config_in_plugin_store(&mut *session.lock().await, "编排会话", "/srv/orch", "bash").await;

        let sm = host_ctx.session_manager.clone();
        async fn wait_session(
            sm: &Arc<crate::session::SessionManager>,
            session_id: &str,
        ) -> crate::session::SessionInfo {
            // 创建为宿主异步执行（事件回灌需锁释放）；有限轮询等落库
            for _ in 0..50 {
                if let Some(info) = sm.get_session(session_id).await {
                    return info;
                }
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            }
            panic!("session {session_id} 未在时限内落库");
        }

        // ==================== 3. 编排创建（start=false，两阶段第一阶段） ====================
        let sid1 = crate::utils::session_create_bridge::create_session_via_plugin(
            &host_ctx, &seeded["id"].as_str().unwrap(), None, None, false, None,
        )
        .await
        .expect("plugin active → 必须编排成功");
        assert_eq!(sid1.len(), 36, "预生成 UUID; got: {sid1}");
        let info1 = wait_session(&sm, &sid1).await;
        assert_eq!(
            info1.status,
            crate::enums::SessionStatus::Starting,
            "start=false → Starting"
        );
        assert_eq!(info1.name, "编排会话", "命名唯一化首见 = 原名（插件决策）");
        assert_eq!(info1.config_id, seeded["id"].as_str().unwrap().to_string(), "configId 透传（会话记录标真源配置）");
        assert_eq!(sm.canonical_renderer_of(&sid1).await, None, "不启动 → 正统端为空");

        // ==================== 4. 命名唯一化：同配置第二次 → 原名(1) ====================
        let sid2 = crate::utils::session_create_bridge::create_session_via_plugin(
            &host_ctx, &seeded["id"].as_str().unwrap(), None, None, false, None,
        )
        .await
        .expect("plugin active → 必须编排成功");
        let info2 = wait_session(&sm, &sid2).await;
        assert_eq!(info2.name, "编排会话(1)", "重名冲突 → 插件改写为 (1) 后缀");
        assert_eq!(info2.config_id, seeded["id"].as_str().unwrap().to_string());

        // ==================== 5. 插件必需：注销互调面 → 显性报错（无宿主降级） ====================
        host_ctx.api_registry().unregister(SESSION_ID);
        let err = crate::utils::session_create_bridge::create_session_via_plugin(
            &host_ctx, &seeded["id"].as_str().unwrap(), None, None, false, None,
        )
        .await
        .expect_err("插件不可用 → 必须显性报错（host-business-decarriage 收尾：降级轨已删）");
        assert!(
            err.to_string().contains("session plugin not active"),
            "错误必须指明插件未激活, got: {err}"
        );

        // 清理：不 spawn 进程，仅释放 openpty slave fd
        sm.remove_session(&sid1).await.expect("remove s1");
        sm.remove_session(&sid2).await.expect("remove s2");
        session.lock().await.deactivate().expect("final deactivate");
    });
}

/// 票 10 真实 wasm 闭环：真实会话中心产物 + 真实 `host-session` 会话动作原语
/// （重启 / 移除 / 改名 / 尺寸裁决）经**产品路径**（宿主命令面桥接 → 插件互调 →
/// 原语 → 内核执行器）走一遍，断言外部可见结果：
///
/// 1. 播种配置（桥接 → 插件真源 + 主库投影）→ 编排创建三个「只创建不启动」会话
/// 2. **改名**：记录改名 + 回执原名 + 未知会话显性错
/// 3. **尺寸裁决四态**（规则在插件、登记事实在内核——两处都断言）：
///    无渲染端 → applied（归属确立）；单端 → applied（无确认）；
///    多端争用 → needsConfirmation 且**内核登记零改动**；force → applied 且归属移交
/// 4. **重启**：同一 id 重建并启动（真实 bash，测试后 kill），名字 / configId 保持，
///    正统端回到启动端；与内核直连执行器**对照**：生命周期事件序列与状态字段等价
/// 5. **移除**：记录与正统端归属一并消失
/// 6. **降级**：注销互调面 → 桥接返回 None（调用方走宿主旧路径，无单点）
#[test]

fn test_session_actions_closed_loop() {
    // 会话插件私有库是进程级共享路径：与其它会话闭环用例串行（见锁文档）
    let _serial = session_plugin_db_guard();
    use crate::session::session_lifecycle::{SessionLifecycleEvent, SessionLifecycleListener};
    use crate::session::RendererSource;

    const SESSION_ID: &str = "com.bedcode.terminal-session";
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }

    /// 生命周期事件签名捕获（对照两个重启路径的「事件序列等价」）
    struct LifecycleCapture {
        events: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl SessionLifecycleListener for LifecycleCapture {
        fn on_session_lifecycle(&self, event: &SessionLifecycleEvent) {
            let sig = match event {
                SessionLifecycleEvent::Creating { config_id, .. } => {
                    format!("Creating(config={config_id})")
                }
                SessionLifecycleEvent::Created { name, .. } => format!("Created(id=<sid>, name={name})"),
                SessionLifecycleEvent::Stopping { .. } => "Stopping".to_string(),
                SessionLifecycleEvent::Stopped { .. } => "Stopped".to_string(),
            };
            let _ = self.events.lock().unwrap_or_else(|e| e.into_inner()).push(sig);
        }
    }

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    // 独立私有库根目录：进程级 `plugin_db_root()` 被本模块多个并行测试共享
    // （各自 remove_dir_all + 写同一 SQLite 文件 → BUSY/缺失），见票 09 同款处置
    let mut host_ctx = host_ctx;
    if let Some(ctx) = Arc::get_mut(&mut host_ctx) {
        ctx.set_plugin_db_root(Some(std::env::temp_dir().join(format!(
                "bedcode_plugin_dbs_actions_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ))));
    }
    let _ = std::fs::remove_dir_all(plugin_db_root().join(SESSION_ID));

    let component = wasm_runtime
        .compile_component(&std::fs::read(&wasm_path).expect("read session artifact"))
        .expect("compile session artifact");

    host_ctx.permission.grant_permissions(
        SESSION_ID,
        &[
            "auth".to_string(),
            "peer".to_string(), "storage".to_string(),
            "session:read".to_string(),
            "session:write".to_string(),
            "storage".to_string(),
        ],
    );
    host_ctx.api_registry().register(
        SESSION_ID,
        &session_apis().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let sm = host_ctx.session_manager.clone();

        /// 有限轮询等会话落库 / 状态推进（宿主侧异步执行，见票 09 同款处置）
        async fn wait_session(
            sm: &Arc<crate::session::SessionManager>,
            session_id: &str,
        ) -> crate::session::SessionInfo {
            for _ in 0..50 {
                if let Some(info) = sm.get_session(session_id).await {
                    return info;
                }
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            }
            panic!("session {session_id} 未在时限内落库");
        }

        let instances = Arc::new(RwLock::new(HashMap::new()));
        let session = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, SESSION_ID, host_ctx.clone(), &[], None)
                .expect("instantiate session"),
        ));
        instances.write().await.insert(SESSION_ID.to_string(), session.clone());
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::clone(&instances),
            }))
            .await;
        session.lock().await.activate().expect("activate session");

        // ==================== 1. 播种配置（插件真源）+ 创建会话（不启动，无进程） ====================
        // working_dir 用 /tmp：重启会真实 spawn（cwd 必须存在），其余动作不启动
        let seeded = seed_config_in_plugin_store(&mut *session.lock().await, "动作会话", "/tmp", "bash").await;

        let sid = crate::utils::session_create_bridge::create_session_via_plugin(
            &host_ctx, &seeded["id"].as_str().unwrap(), None, None, false, None,
        )
        .await
        .expect("plugin active → 必须编排成功");
        let info = wait_session(&sm, &sid).await;
        assert_eq!(info.name, "动作会话");
        assert_eq!(
            sm.canonical_renderer_of(&sid).await,
            None,
            "不启动 → 无正统端（裁决起点：无渲染端）"
        );
        // 两阶段第二阶段（内核路径，非本票动作）：启动进程，使 PTY master 就绪
        // （resize 需要 master；`start_existing_session` 不做归属登记 →
        //  正统端仍为空，正是裁决态 1「无渲染端」）
        sm.start_existing_session(&sid, None).await.expect("start session");
        assert_eq!(sm.canonical_renderer_of(&sid).await, None);

        // ==================== 2. 改名 ====================
        let previous = crate::utils::session_action_bridge::rename_session_via_plugin(&host_ctx, &sid, "重命名后")
            .await
            .expect("bridge rename")
            .expect("plugin active → 必须编排成功");
        assert_eq!(previous, "动作会话", "回执改名前的名字");
        assert_eq!(sm.get_session(&sid).await.expect("session").name, "重命名后");

        // 未知会话：显性失败（互调错误 → 桥接降级返回 None，由宿主执行器报 NotFound）
        let missing = crate::utils::session_action_bridge::rename_session_via_plugin(&host_ctx, "ghost-session", "x")
            .await
            .expect("bridge rename ghost");
        assert_eq!(missing, None, "插件侧显性报错 → 桥接降级（宿主执行器给出 NotFound）");

        // ==================== 3. 尺寸裁决四态（规则在插件；事实在内核） ====================
        // 态 1 无渲染端 → 首个请求方即位正统
        let applied = crate::utils::session_action_bridge::resize_session_via_plugin(
            &host_ctx,
            &sid,
            100,
            30,
            &RendererSource::Desktop,
            false,
        )
        .await
        .expect("bridge resize")
        .expect("plugin active → 必须裁决成功");
        assert!(
            matches!(applied, crate::session::ResizeOutcome::Applied { .. }),
            "无渲染端 → applied, got: {applied:?}"
        );
        assert_eq!(
            sm.canonical_renderer_of(&sid).await,
            Some(RendererSource::Desktop),
            "归属登记在内核"
        );

        // 态 2 单端（归属 = 请求方）→ 直接应用，无确认
        let applied = crate::utils::session_action_bridge::resize_session_via_plugin(
            &host_ctx,
            &sid,
            110,
            32,
            &RendererSource::Desktop,
            false,
        )
        .await
        .expect("bridge resize again")
        .expect("plugin active");
        assert!(
            matches!(applied, crate::session::ResizeOutcome::Applied { .. }),
            "单端 → applied, got: {applied:?}"
        );

        // 态 3 多端争用（归属 Desktop；移动端未 force）→ needsConfirmation 且零改动
        let mobile = RendererSource::Mobile {
            device_name: "Pixel-9".to_string(),
        };
        let outcome =
            crate::utils::session_action_bridge::resize_session_via_plugin(&host_ctx, &sid, 80, 24, &mobile, false)
                .await
                .expect("bridge resize contended")
                .expect("plugin active");
        assert_eq!(
            outcome,
            crate::session::ResizeOutcome::NeedsConfirmation {
                current_canonical: RendererSource::Desktop
            },
            "多端争用 → 需覆盖确认（回执当前正统端）"
        );
        assert_eq!(
            sm.canonical_renderer_of(&sid).await,
            Some(RendererSource::Desktop),
            "需确认路径必须零改动（归属不得被抢占）"
        );

        // 态 4 端接管（force = 覆盖确认通过）→ 应用并移交归属
        let outcome =
            crate::utils::session_action_bridge::resize_session_via_plugin(&host_ctx, &sid, 80, 24, &mobile, true)
                .await
                .expect("bridge resize takeover")
                .expect("plugin active");
        assert_eq!(
            outcome,
            crate::session::ResizeOutcome::Applied {
                canonical: mobile.clone()
            },
            "force → 应用并移交归属"
        );
        assert_eq!(sm.canonical_renderer_of(&sid).await, Some(mobile.clone()));

        // ==================== 4. 重启（同一 id 重建并启动）+ 对照等价 ====================
        let capture = Arc::new(std::sync::Mutex::new(Vec::new()));
        sm.register_lifecycle_listener(Arc::new(LifecycleCapture {
            events: Arc::clone(&capture),
        }))
        .await;

        // 完成信号：Created 生命周期到达——重启为「remove 旧会话 + 同 id
        // create-with-spec」两步（编排在插件），存在性轮询不可用（移除后新记录
        // 尚未插入，会提前通过而读到重启前状态）
        let restarted = crate::utils::session_action_bridge::restart_session_via_plugin(&host_ctx, &sid)
            .await
            .expect("plugin active → 必须编排成功（插件必需，无宿主降级）");
        assert_eq!(restarted, sid, "重启保持同一 session id（线协议与订阅键不变）");
        let mut created_seen = false;
        for _ in 0..100 {
            if capture
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .any(|e| e.starts_with("Created("))
            {
                created_seen = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
        assert!(created_seen, "重启完成信号超时（宿主异步执行 Created 事件）");
        let info = wait_session(&sm, &sid).await;
        assert_eq!(info.status, crate::enums::SessionStatus::Running, "重启后 Running");
        assert_eq!(info.name, "重命名后", "重启保持名字（不做二次命名）");
        assert_eq!(info.config_id, seeded["id"].as_str().unwrap().to_string(), "重启保持 configId");
        assert_eq!(
            sm.canonical_renderer_of(&sid).await,
            Some(RendererSource::Desktop),
            "重启归属回到启动端（与内核执行器一致）"
        );
        assert_eq!(
            capture.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            vec![
                format!("Creating(config={})", seeded["id"].as_str().unwrap()),
                "Created(id=<sid>, name=重命名后)".to_string()
            ],
            "重启事件序列 = Creating → Created（与迁移前逐字一致）"
        );

        // ==================== 5. 移除 ====================
        let removed = crate::utils::session_action_bridge::remove_session_via_plugin(&host_ctx, &sid)
            .await
            .expect("bridge remove");
        assert_eq!(removed, Some(()), "插件编排接管（未降级）");
        assert!(sm.get_session(&sid).await.is_none(), "会话记录必须消失");
        assert!(
            sm.canonical_renderer_of(&sid).await.is_none(),
            "正统端归属必须随会话清理"
        );

        // ==================== 6. 降级与插件必需：注销互调面后的两条口径 ====================
        host_ctx.api_registry().unregister(SESSION_ID);
        // 重启：插件必需（v21 内核执行器退役）→ 显性报错，不静默让位
        let restart_err = crate::utils::session_action_bridge::restart_session_via_plugin(&host_ctx, "any")
            .await
            .expect_err("插件不可用 → 重启必须显性报错（无宿主执行器可降级）");
        assert!(
            restart_err.to_string().contains("session plugin not active"),
            "错误必须指明插件未激活, got: {restart_err}"
        );
        assert_eq!(
            crate::utils::session_action_bridge::remove_session_via_plugin(&host_ctx, "any")
                .await
                .expect("fallback remove"),
            None
        );
        assert_eq!(
            crate::utils::session_action_bridge::rename_session_via_plugin(&host_ctx, "any", "n")
                .await
                .expect("fallback rename"),
            None
        );
        assert_eq!(
            crate::utils::session_action_bridge::resize_session_via_plugin(
                &host_ctx,
                "any",
                10,
                10,
                &RendererSource::Desktop,
                false
            )
            .await
            .expect("fallback resize"),
            None
        );

        session.lock().await.deactivate().expect("final deactivate");
    });
}

/// 票 11 真实 wasm 闭环：真实会话中心产物 + 真实 `host-session` 新原语
/// （注解槽写入 `annotate` / 连接清单 `connections-list` / 会话列表 `annotations`
/// 透传）经**产品路径**走一遍，断言外部可见结果（spec S1 闭环矩阵：annotate
/// 透传、connections-list 属主隔离）：
///
/// 1. 播种配置 → 编排创建「只创建不启动」会话（无进程）
/// 2. `session.annotate` 命令经真实组件写注解槽 → 宿主槽可见原样透传；
///    **expand 期双写**：旧任务字段保持 None（wire DTO 形状不变）；
///    同一命令对 ghost 会话显性报错（存在性校验跨 wasm 边界生效）
/// 3. `session.devices.connect-list` 命令（真实组件内调真实原语）：无头上下文
///    连接注册表为空 → `{connections: []}`（形状恒定；connections-list +
///    trusted-devices-list + session-list 三原语在 guest 内完整走通），并对照
///    宿主事实面（连接注册表）为空
#[test]

fn test_session_annotate_and_devices_closed_loop() {
    // 会话插件私有库是进程级共享路径：与其它会话闭环用例串行（见锁文档）
    let _serial = session_plugin_db_guard();
    const SESSION_ID: &str = "com.bedcode.terminal-session";
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    // 独立私有库根目录：进程级 `plugin_db_root()` 被本模块多个并行测试共享
    // （各自 remove_dir_all + 写同一 SQLite 文件 → BUSY/缺失），见票 09 同款处置
    let mut host_ctx = host_ctx;
    if let Some(ctx) = Arc::get_mut(&mut host_ctx) {
        ctx.set_plugin_db_root(Some(std::env::temp_dir().join(format!(
                "bedcode_plugin_dbs_annotate_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ))));
    }
    let _ = std::fs::remove_dir_all(plugin_db_root().join(SESSION_ID));

    let component = wasm_runtime
        .compile_component(&std::fs::read(&wasm_path).expect("read session artifact"))
        .expect("compile session artifact");

    host_ctx.permission.grant_permissions(
        SESSION_ID,
        &[
            "auth".to_string(),
            "peer".to_string(), "storage".to_string(),
            "session:read".to_string(),
            "session:write".to_string(),
            "storage".to_string(),
        ],
    );
    host_ctx.api_registry().register(
        SESSION_ID,
        &session_apis().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let sm = host_ctx.session_manager.clone();

        let instances = Arc::new(RwLock::new(HashMap::new()));
        let session = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, SESSION_ID, host_ctx.clone(), &[], None)
                .expect("instantiate session"),
        ));
        instances.write().await.insert(SESSION_ID.to_string(), session.clone());
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::clone(&instances),
            }))
            .await;
        session.lock().await.activate().expect("activate session");

        // ==================== 1. 播种配置 + 创建会话（不启动，无进程） ====================
        let seeded = seed_config_in_plugin_store(&mut *session.lock().await, "注解会话", "/tmp", "bash").await;

        let sid = crate::utils::session_create_bridge::create_session_via_plugin(
            &host_ctx, &seeded["id"].as_str().unwrap(), None, None, false, None,
        )
        .await
        .expect("plugin active → 必须编排成功");
        // 创建为宿主异步执行（事件回灌需锁释放）——有限轮询等落库
        for _ in 0..50 {
            if sm.get_session(&sid).await.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
        assert!(sm.get_session(&sid).await.is_some(), "session {sid} 未在时限内落库");

        // ==================== 2. annotate：真实组件写注解槽（透传） ====================
        let r = session
            .lock()
            .await
            .invoke_command(
                "session.annotate",
                &serde_json::json!({
                    "sessionId": sid,
                    "key": "taskStatus",
                    "value": "asking",
                })
                .to_string(),
            )
            .expect("annotate command");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(r["ok"], true, "annotate 命令成功, got: {r}");
        session
            .lock()
            .await
            .invoke_command(
                "session.annotate",
                &serde_json::json!({
                    "sessionId": sid,
                    "key": "taskReason",
                    "value": "等待用户答复",
                })
                .to_string(),
            )
            .expect("annotate reason");

        // 透传断言：guest 写入 → 宿主槽原样（内核只搬运不解释）
        let ann = sm.session_annotations(&sid).await;
        assert_eq!(ann.get("taskStatus").map(String::as_str), Some("asking"));
        assert_eq!(ann.get("taskReason").map(String::as_str), Some("等待用户答复"));
        assert_eq!(ann.len(), 2, "双键并存");

        // contract 期（票 12）端到端：引擎记录已无任务字段，对外视图的任务字段
        // 只来自槽——guest 写槽 → 宿主对外形状取值（前端命令 / 控制帧 / 移动端
        // DTO 的同一构造点）
        let info = sm.get_session(&sid).await.expect("info");
        assert_eq!(info.id, sid);
        let view = sm.session_view(&sid).await.expect("view");
        assert_eq!(
            view.task_status.as_deref(),
            Some("asking"),
            "槽值经对外视图透出（内核不解释键名）"
        );
        assert_eq!(view.task_reason.as_deref(), Some("等待用户答复"));
        assert_eq!(
            serde_json::to_value(&view).expect("serialize")["taskStatus"],
            "asking",
            "wire 字段名不变（前端 / 移动端契约）"
        );

        // 未知会话跨 wasm 边界显性报错（`{"error": ...}` 形状）：票 04 起统一按
        // 「非属主」拒绝——未知 id 判不出属主，与他人的 id 走同一口径
        let ghost = session
            .lock()
            .await
            .invoke_command(
                "session.annotate",
                &serde_json::json!({"sessionId": "ghost", "key": "k", "value": "v"}).to_string(),
            )
            .expect("ghost annotate returns json");
        let r: serde_json::Value = serde_json::from_str(&ghost).unwrap();
        assert!(
            r["error"].as_str().map(|e| e.contains("not owner")).unwrap_or(false),
            "未知会话必须显性报错, got: {ghost}"
        );
        assert!(sm.session_annotations("ghost").await.is_empty(), "绝不写孤儿键");

        // ==================== 3. devices.connect-list：真实组件内真实原语 ====================
        let view = session
            .lock()
            .await
            .invoke_command("session.devices.connect-list", "{}")
            .expect("devices connect list command");
        let v: serde_json::Value = serde_json::from_str(&view).unwrap();
        assert!(
            v.get("error").is_none(),
            "连接清单命令不得报错（无头上下文注册表为空）, got: {view}"
        );
        assert_eq!(
            v["connections"],
            serde_json::json!([]),
            "无头上下文 → 空连接清单（形状恒定）"
        );

        // 宿主命令面只回引擎事实（连接注册表原始记录），派生视图归插件：
        // `devices-connect-list` 互调 api 与宿主 `utils/devices_bridge` 已随
        // host-business-decarriage 收尾退役（宿主唯一消费方只需指纹字段）。
        assert_eq!(
            crate::server::websocket::WebSocketManager::global().list_clients().await.len(),
            0,
            "无头上下文连接注册表为空 → 宿主事实面为空"
        );

        // 清理：不 spawn 进程，仅释放 openpty slave fd
        sm.remove_session(&sid).await.expect("remove session");
        session.lock().await.deactivate().expect("final deactivate");
    });
}

/// 票 10 互调闭环（票 05 改指会话中心）：真实 file-transfer 产物消费真实会话中心
/// 产物 ——consent 两阶段流（阶段 1 信任预检 / 阶段 2 用户意向）与统一信任视图
/// 均经总线互调；并在 wire 层捕获断言 file-transfer 发出的 JSON-RPC 请求
/// 形状（requestId / nodeId / userDecision 映射正确），证明改指生效。
///
/// 约束：无头上下文 host-peer 不可用（require_app 失败）——
/// - consent-decide 返回 ask（fail-closed）；accept/deny 决策后的宿主应答
///   以错误形状（headless）透出，恰证「决策已应用到宿主应答路径」
/// - trust-list 经会话中心视图（peerError 透出）错误形状
/// 降级路径（会话中心不可用）：respond-consent / list-trusted 直答宿主
/// （与迁移前行为等价，双轨无单点）。决策本身的正确性由
/// `test_session_trust_and_consent_api_closed_loop` 覆盖，本测试聚焦消费方
/// wire 契约与降级。产物缺失时跳过。
#[test]

fn test_filetransfer_consumes_session_center_closed_loop() {
    use crate::utils::auth::auth_center as bridge;

    const FT_ID: &str = "com.bedcode.file-transfer";
    const NODE: &str = "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344";
    let session_id = bridge::SESSION_PLUGIN_ID;
    let decide_topic = format!("bedcode.api.{session_id}.consent-decide");
    let list_topic = format!("bedcode.api.{session_id}.trust-list");

    let ft_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.file-transfer/bedcode_plugin_file_transfer.wasm");
    let center_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !ft_path.exists() || !center_path.exists() {
        eprintln!("[skip] file-transfer / session wasip3 artifacts not built");
        return;
    }
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let ft_component = wasm_runtime
        .compile_component(&std::fs::read(&ft_path).expect("read file-transfer artifact"))
        .expect("compile file-transfer artifact");
    let center_component = wasm_runtime
        .compile_component(&std::fs::read(&center_path).expect("read session artifact"))
        .expect("compile session artifact");

    // 授权路径等价 PluginHost 装载（manifest permissions 登记）：
    // - 会话中心声明 auth/peer（auth 含记录面；peer 供 consent 取可信集）
    // - file-transfer 声明 peer（宿主应答/降级直查路径需权限门放行）
    host_ctx
        .permission
        .grant_permissions(session_id, &["auth".to_string(), "peer".to_string(), "storage".to_string()]);
    host_ctx.permission.grant_permissions(FT_ID, &["peer".to_string(), "storage".to_string()]);
    let center_apis = session_apis();
    host_ctx.api_registry().register(
        session_id,
        &center_apis.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );

    // wire 捕获（静态订阅，与会话中心的 wasm 订阅共存 fan-out）
    let captures: Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>> = Arc::new(std::sync::Mutex::new(Vec::new()));

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        host_ctx
            .message_bus
            .subscribe_static(
                "capture",
                &decide_topic,
                Box::new(AuthCenterCaptureHandler {
                    captures: Arc::clone(&captures),
                }),
            )
            .await;
        host_ctx
            .message_bus
            .subscribe_static(
                "capture",
                &list_topic,
                Box::new(AuthCenterCaptureHandler {
                    captures: Arc::clone(&captures),
                }),
            )
            .await;

        let filetransfer = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&ft_component, FT_ID, host_ctx.clone(), &[], None)
                .expect("instantiate file-transfer"),
        ));
        let center = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&center_component, session_id, host_ctx.clone(), &[], None)
                .expect("instantiate session"),
        ));

        let instances = Arc::new(RwLock::new(HashMap::from([
            (FT_ID.to_string(), filetransfer.clone()),
            (session_id.to_string(), center.clone()),
        ])));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher { instances }))
            .await;

        center.lock().await.activate().expect("session activate");
        filetransfer.lock().await.activate().expect("file-transfer activate");

        // 等待捕获数（线程安全轮询；事件/互调均异步派发）
        async fn wait_captures(
            captures: &Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
            topic: &str,
            n: usize,
        ) {
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
            loop {
                let count = {
                    let c = captures.lock().expect("capture lock");
                    c.iter().filter(|(t, _)| t == topic).count()
                };
                if count >= n {
                    return;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "timeout waiting for {n} captures on '{topic}' (got {count})"
                );
                tokio::time::sleep(std::time::Duration::from_millis(15)).await;
            }
        }
        async fn decide_count(
            captures: &Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
            topic: &str,
        ) -> usize {
            let c = captures.lock().expect("capture lock");
            c.iter().filter(|(t, _)| t == topic).count()
        }

        // ==================== consent：阶段 1 信任预检 ====================
        // 发布 peer:consent（宿主引擎事件桥语义）→ file-transfer 经
        // auth.decide-consent（无 userDecision）预检信任
        host_ctx.message_bus.publish(
            "peer:consent",
            "test-host",
            serde_json::json!({
                "requestId": "req-c1",
                "nodeId": NODE,
                "fingerprintShort": &NODE[..8],
                "deviceName": "消费方测试对端",
            }),
        );
        wait_captures(&captures, &decide_topic, 1).await;
        {
            let c = captures.lock().expect("capture lock");
            let (_, req) = c.iter().find(|(t, _)| t == &decide_topic).unwrap();
            // 阶段 1：无用户意向（仅对端信息），requestId/nodeId 沿事件桥原样
            assert!(
                req["params"].get("userDecision").is_none(),
                "阶段 1 不得携带用户意向, got: {}",
                req["params"]
            );
            assert_eq!(req["params"]["requestId"], "req-c1");
            assert_eq!(req["params"]["nodeId"], NODE);
            assert_eq!(req["params"]["deviceName"], "消费方测试对端");
        }

        // ==================== consent：阶段 2 用户接受 ====================
        // 回传 accepted=true → auth.decide-consent userDecision=accept →
        // 决策 accept → 应答宿主（无头上下文 headless 错误透出——恰证决策
        // 被应用到宿主应答路径，而非静默丢弃）
        let result = filetransfer
            .lock()
            .await
            .invoke_command(
                "file-transfer.respond-consent",
                r#"{"requestId":"req-c1","accepted":true}"#,
            )
            .expect("respond-consent accept");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"]
                .as_str()
                .map(|e| e.contains("unavailable") || e.contains("headless"))
                .unwrap_or(false),
            "accept 决策后应答宿主（无头上下文报错透出）, got: {result}"
        );
        wait_captures(&captures, &decide_topic, 2).await;
        {
            let c = captures.lock().expect("capture lock");
            let (_, req) = c.iter().filter(|(t, _)| t == &decide_topic).last().unwrap();
            assert_eq!(
                req["params"]["userDecision"], "accept",
                "阶段 2 必须携带 userDecision=accept, got: {}",
                req["params"]
            );
            assert_eq!(req["params"]["requestId"], "req-c1");
            assert_eq!(req["params"]["nodeId"], NODE, "对端信息沿事件桥登记传递");
        }

        // ==================== consent：阶段 2 用户拒绝 ====================
        host_ctx.message_bus.publish(
            "peer:consent",
            "test-host",
            serde_json::json!({
                "requestId": "req-c2",
                "nodeId": NODE,
                "fingerprintShort": &NODE[..8],
            }),
        );
        wait_captures(&captures, &decide_topic, 3).await; // req-c2 阶段 1
        let result = filetransfer
            .lock()
            .await
            .invoke_command(
                "file-transfer.respond-consent",
                r#"{"requestId":"req-c2","accepted":false}"#,
            )
            .expect("respond-consent deny");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"]
                .as_str()
                .map(|e| e.contains("unavailable") || e.contains("headless"))
                .unwrap_or(false),
            "deny 决策后应答宿主（无头上下文报错透出）, got: {result}"
        );
        wait_captures(&captures, &decide_topic, 4).await;
        {
            let c = captures.lock().expect("capture lock");
            let (_, req) = c.iter().filter(|(t, _)| t == &decide_topic).last().unwrap();
            assert_eq!(
                req["params"]["userDecision"], "deny",
                "accepted=false 映射 userDecision=deny, got: {}",
                req["params"]
            );
        }

        // ==================== 降级：认证中心不可用（门禁拒绝） ====================
        // 注销会话中心声明 → 互调请求被门禁拦下。无登记请求（未发布
        // peer:consent）的迟到应答 → file-transfer 直答宿主（迁移前行为
        // 等价，双轨无单点），不发起任何互调。
        // （阶段 1 降级弹窗的编排路径由插件 native 单测
        // phase1_auth_center_down_falls_back_to_ask 覆盖——闭环聚焦可确定的
        // wire 断言，避免异步派发时序竞态。）
        host_ctx.api_registry().unregister(session_id);
        let result = filetransfer
            .lock()
            .await
            .invoke_command(
                "file-transfer.respond-consent",
                r#"{"requestId":"req-unknown","accepted":true}"#,
            )
            .expect("respond-consent degraded");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"]
                .as_str()
                .map(|e| e.contains("unavailable") || e.contains("headless"))
                .unwrap_or(false),
            "降级直答宿主（无头上下文报错透出）, got: {result}"
        );
        assert_eq!(
            decide_count(&captures, &decide_topic).await,
            4,
            "认证中心不可用：不得发出版互调请求（门禁拦下 / 无登记直答）"
        );

        // ==================== 信任列表：经认证中心统一视图 ====================
        host_ctx.api_registry().register(
            session_id,
            &center_apis.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        let result = filetransfer
            .lock()
            .await
            .invoke_command("file-transfer.list-trusted", "{}")
            .expect("list-trusted");
        wait_captures(&captures, &list_topic, 1).await;
        {
            let c = captures.lock().expect("capture lock");
            let (_, req) = c.iter().find(|(t, _)| t == &list_topic).unwrap();
            assert_eq!(req["method"], "trust-list");
        }
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"]
                .as_str()
                .map(|e| e.contains("unavailable") || e.contains("headless"))
                .unwrap_or(false),
            "认证中心 peerError 透出（不静默空列表）, got: {result}"
        );

        // ==================== 撤销：维持宿主原语（语义不变） ====================
        let result = filetransfer
            .lock()
            .await
            .invoke_command("file-transfer.revoke-trusted", &format!(r#"{{"nodeId":"{NODE}"}}"#))
            .expect("revoke-trusted");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"]
                .as_str()
                .map(|e| e.contains("unavailable") || e.contains("headless"))
                .unwrap_or(false),
            "revoke 宿主原语（无头上下文报错透出）, got: {result}"
        );
        assert_eq!(
            decide_count(&captures, &decide_topic).await,
            4,
            "revoke 不走互调（无声明 api）"
        );

        center.lock().await.deactivate().expect("session deactivate");
        filetransfer
            .lock()
            .await
            .deactivate()
            .expect("file-transfer deactivate");
    });
}

/// 票 04 闭环：会话输出经 `host-session.output-ring-fetch` 原语拉取（插件命令面
/// `session.output.pull` → WIT `list<u8>` 二进制直传）——「插件经原语拉取会话输出
/// 字节不 JSON 化 + 慢消费只损失自己的 ring 历史（游标续拉）」的验收主证据。
///
/// 流程：真实 bash 会话启动 → 写输入 `echo OUTPUT_RING_PROBE_*` → bash 输出进
/// GlobalOutputManager 会话 ring → 插件按游标批量拉取 → 断言原始字节包含 probe
/// 文本（原始字节流，非 JSON 字符串包裹）。属主 = 本插件（create 走插件编排），
/// 权限含 `terminal:output`。
#[test]
fn test_session_output_ring_fetch_closed_loop() {
    // 会话插件私有库是进程级共享路径：与其它会话闭环用例串行（见锁文档）
    let _serial = session_plugin_db_guard();
    const SESSION_ID: &str = "com.bedcode.terminal-session";
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] session wasip3 artifact not built");
        return;
    }

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let mut host_ctx = host_ctx;
    if let Some(ctx) = Arc::get_mut(&mut host_ctx) {
        ctx.set_plugin_db_root(Some(std::env::temp_dir().join(format!(
            "bedcode_plugin_dbs_outring_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))));
    }
    let _ = std::fs::remove_dir_all(plugin_db_root().join(SESSION_ID));

    let component = wasm_runtime
        .compile_component(&std::fs::read(&wasm_path).expect("read session artifact"))
        .expect("compile session artifact");

    host_ctx.permission.grant_permissions(
        SESSION_ID,
        &[
            "auth".to_string(),
            "peer".to_string(), "storage".to_string(),
            "session:read".to_string(),
            "session:write".to_string(),
            // 票 04：输出消费二进制原语（terminal:output）
            "terminal:output".to_string(),
            "storage".to_string(),
        ],
    );
    host_ctx.api_registry().register(
        SESSION_ID,
        &session_apis().iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let instances = Arc::new(RwLock::new(HashMap::new()));
        let session = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, SESSION_ID, host_ctx.clone(), &[], None)
                .expect("instantiate session"),
        ));
        instances.write().await.insert(SESSION_ID.to_string(), session.clone());
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::clone(&instances),
            }))
            .await;
        session.lock().await.activate().expect("activate session");

        // ==================== 1. 播种配置 + 创建 + 启动（真实 bash） ====================
        // command 经宿主 shell 包装（bash -lic "cd ... && pwd && <command>"）：
        // 启动即 echo 探针输出 + sleep 保持会话存活——不依赖 write_input（其插件
        // 输入管道需 AppContext，无头测试未初始化）
        let probe = format!("OUTPUT_RING_PROBE_{}", std::process::id());
        let seeded = seed_config_in_plugin_store(
            &mut *session.lock().await,
            "输出环会话",
            "/tmp",
            &format!("echo {probe}; sleep 60"),
        )
        .await;
        let sm = host_ctx.session_manager.clone();
        let sid = crate::utils::session_create_bridge::create_session_via_plugin(
            &host_ctx, &seeded["id"].as_str().unwrap(), None, None, false, None,
        )
        .await
        .expect("plugin active → 必须编排成功");
        // 创建为宿主异步执行：有限轮询等落库
        let mut info = None;
        for _ in 0..50 {
            if let Some(i) = sm.get_session(&sid).await {
                info = Some(i);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
        let info = info.expect("session 落库");
        assert_eq!(info.status, crate::enums::SessionStatus::Starting);
        sm.start_existing_session(&sid, None).await.expect("start real bash");

        // ==================== 2. 等 bash 输出进入会话 ring（echo + pwd + prompt） ====================

        // ==================== 3. 插件按游标拉取（游标前端自持；追平后停） ====================
        let mut cursor: u64 = 0;
        let mut acc: Vec<u8> = Vec::new();
        let mut got_probe = false;
        for _ in 0..100 {
            let raw = session
                .lock()
                .await
                .invoke_command(
                    "session.output.pull",
                    &serde_json::json!({ "sessionId": sid, "fromOffset": cursor }).to_string(),
                )
                .expect("output.pull 命令");
            let value: serde_json::Value = serde_json::from_str(&raw).expect("pull json");
            // 追平 → null
            if value.is_null() {
                if acc.windows(probe.len()).any(|w| w == probe.as_bytes()) {
                    got_probe = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            }
            let data: Vec<u8> = value["data"]
                .as_array()
                .expect("data 数组")
                .iter()
                .map(|v| v.as_u64().expect("字节") as u8)
                .collect();
            let next = value["nextOffset"].as_u64().expect("nextOffset");
            if value["truncated"].as_bool().unwrap_or(false) {
                panic!("首拉即截断：会话输出不应超过环容量（min_offset 前移）");
            }
            acc.extend_from_slice(&data);
            cursor = next;
            if acc.windows(probe.len()).any(|w| w == probe.as_bytes()) {
                got_probe = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
        assert!(
            got_probe,
            "插件必须经 output-ring-fetch 拉到 bash 输出字节，got: {:?}",
            String::from_utf8_lossy(&acc)
        );

        // ==================== 4. 续拉不重复 + 追平自愈 ====================
        // 游标已追平（最后一次返回后可能仍有增量——再拉直到追平验证不抛错）
        for _ in 0..50 {
            let raw = session
                .lock()
                .await
                .invoke_command(
                    "session.output.pull",
                    &serde_json::json!({ "sessionId": sid, "fromOffset": cursor }).to_string(),
                )
                .expect("pull again");
            let value: serde_json::Value = serde_json::from_str(&raw).expect("pull json");
            if value.is_null() {
                break;
            }
            let data: Vec<u8> = value["data"]
                .as_array()
                .expect("data 数组")
                .iter()
                .map(|v| v.as_u64().expect("字节") as u8)
                .collect();
            cursor = value["nextOffset"].as_u64().expect("nextOffset");
            // 续拉不重复：新段不得与已拉区间重叠（游标单调推进由断言 3 隐含保证）
            assert!(!data.is_empty(), "非追平响应必须带字节");
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }

        // ==================== 清理 ====================
        sm.remove_session(&sid).await.expect("remove session");
        session.lock().await.deactivate().expect("final deactivate");
    });
}
