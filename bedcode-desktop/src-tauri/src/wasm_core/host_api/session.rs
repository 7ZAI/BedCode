//! 会话域宿主实现（会话查询、配置 CRUD、配置列表与会话创建）

use crate::wasm_core::manager::runtime::{block_on_async, WasmHostContext};
use crate::wasm_core::permission::{PERMISSION_SESSION_READ, PERMISSION_SESSION_WRITE, PERMISSION_TERMINAL_OUTPUT};
use crate::session::RingFetchOutput;
use crate::system::constants::PLUGIN_SESSION_RING_FETCH_MAX_BYTES;
use crate::system::error_boundary::spawn_with_error_boundary;
use uuid::Uuid;

/// 属主拒绝统一文案（票 04，与 pty / ws / mdns 的 `not owner of ...` 同形）
pub(crate) const NOT_OWNER: &str = "not owner of session";

/// 先权限门（调用方已过）、后属主：只有创建方插件能操作自己的会话（票 04，P0-3）
///
/// 此前 `close` / `remove` / `rename` / `resize` / `annotate` 与 `terminal_send` 只看
/// 权限位——任意声明了 `session:write` / `terminal:input` 的插件都能关掉用户正在用的
/// 会话、向它的终端注入按键。pty / ws / mdns / core-task 早有同一判定，本函数把它
/// 补齐到会话域，不发明新机制。
///
/// 无属主（内核/宿主自建）同样拒绝：fail-closed，宁可不给操作也不放行陌生人。
/// 错误文案只报「不是属主」，不回带真实属主 id（避免把别的插件身份泄露给调用方）。
pub(crate) fn ensure_session_owner(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
) -> Result<(), String> {
    match block_on_async(host_ctx.session_manager.session_owner(session_id)) {
        Some(owner) if owner == plugin_id => Ok(()),
        other => {
            tracing::warn!(
                plugin_id = %plugin_id,
                session_id = %session_id,
                owned = other.is_some(),
                "host-session 属主校验拒绝"
            );
            Err(format!("{NOT_OWNER}: {session_id}"))
        }
    }
}

/// 列出所有会话（权限 + SessionManager 查询），返回 JSON 数组字符串
///
/// 返回的 JSON 是宿主 `SessionInfo` 序列化的**超集**：每个会话对象额外带
/// * `canonicalRenderer`（当前正统渲染端归属，无归属为 `null`）——理由同
///   [`session_get`]（票 10 尺寸裁决读取通道，list 与 get 保持同形超集）
/// * `annotations`（票 11 会话注解槽透传：`session-id → key → value` 不透明键值对，
///   空槽为 `{}`）——**内核只搬运透传、绝不解释键名**（spec D5），插件据此读回
///   自己写入的任务态注解（如 `taskStatus`），不另开读原语
///
/// 两字段均为增量追加，既有消费者（`parse_sessions` 等宽容解析）不受影响。
pub(crate) fn session_list(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_READ, "host_session_list") {
        return Err("permission denied".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    let sessions = block_on_async(sm.list_sessions());
    let mut values: Vec<serde_json::Value> = Vec::with_capacity(sessions.len());
    for info in sessions {
        let canonical = block_on_async(sm.canonical_renderer_of(&info.id));
        let annotations = block_on_async(sm.session_annotations(&info.id));
        let mut value =
            serde_json::to_value(&info).map_err(|e| format!("session error: JSON serialization failed: {}", e))?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "canonicalRenderer".to_string(),
                serde_json::to_value(canonical)
                    .map_err(|e| format!("session error: JSON serialization failed: {}", e))?,
            );
            obj.insert(
                "annotations".to_string(),
                serde_json::to_value(annotations)
                    .map_err(|e| format!("session error: JSON serialization failed: {}", e))?,
            );
        }
        values.push(value);
    }
    serde_json::to_string(&values)
        .map(Some)
        .map_err(|e| format!("session error: JSON serialization failed: {}", e))
}

/// 获取单个会话（权限 + 查询），不存在返回 None
///
/// 返回的 JSON 是宿主 `SessionInfo` 的**超集**：额外带 `canonicalRenderer`
/// （当前正统渲染端归属，无归属为 `null`，wire 形状同宿主 `RendererSource`）
/// 与 `annotations`（票 11 会话注解槽透传，`session-id → key → value` 不透明
/// 键值对，空槽 `{}`——内核只搬运透传、绝不解释键名）。
/// 这是票 10「尺寸裁决规则归插件、登记事实留内核」的读取通道——插件据此判断
/// 「无渲染端 / 单端 / 多端争用」，再自己决定是否调用 `resize` 原语执行；
/// 不另开读原语（既有 interface 上追加函数已足够，spec D4）。字段为**增量追加**，
/// 既有消费者（`parse_sessions` 等宽容解析）不受影响。
pub(crate) fn session_get(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_READ, "host_session_get") {
        return Err("permission denied".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    let Some(info) = block_on_async(sm.get_session(session_id)) else {
        return Ok(None);
    };
    let canonical = block_on_async(sm.canonical_renderer_of(session_id));
    let annotations = block_on_async(sm.session_annotations(session_id));
    let mut value =
        serde_json::to_value(&info).map_err(|e| format!("session error: JSON serialization failed: {}", e))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "canonicalRenderer".to_string(),
            serde_json::to_value(canonical).map_err(|e| format!("session error: JSON serialization failed: {}", e))?,
        );
        obj.insert(
            "annotations".to_string(),
            serde_json::to_value(annotations)
                .map_err(|e| format!("session error: JSON serialization failed: {}", e))?,
        );
    }
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|e| format!("session error: JSON serialization failed: {}", e))
}

/// 会话启动规格创建（v19，权限 session:write） ====================

/// `create-with-spec` 入参结构（插件算好的 launch spec，camelCase）
///
/// 字段语义（spec D4）：`environment` 与宿主 `ExecutionEnvironment` serde 同形
/// （`{"type":"Wsl2","distro":...}` | `{"type":"Linux"}` |
/// `{"type":"Windows","shell":"PowerShell"}`）；`start=false` 走只创建不启动
/// 的两阶段第一阶段。映射决策（命名唯一化 / config→launch / 何时启动）由插件完成，
/// 宿主只做执行与输入仲裁。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchSpec {
    /// 会话名（插件已唯一化；宿主不二次改名）
    pub name: String,
    /// 命令串（**仅诊断**：人类可读命令行；宿主不解释它，实际 exec 的是 `commandArgs`）
    pub command: String,
    /// 完整 argv（**必填**，pty 票 1 起宿主唯一命令形态）：非空 → 宿主按 argv 数组
    /// 原样 exec（**不做 shell 包装 / WSL 转换**，插件侧 `launch.rs::build_argv`
    /// 已算好完整 argv）。此处用 `Option` 只为把「旧产物未送该字段」变成可读的
    /// **显性错误**而不是 serde 反序列化错误；缺省或空数组 → 拒绝（宿主旧
    /// shell 包装路径已于 2026-09-23 PTY 解耦票退役，不再静默回退）。
    /// 字段为追加语义，函数签名不变故不 bump ABI。
    #[serde(default)]
    pub command_args: Option<Vec<String>>,
    /// 追加参数（**旧路径字段，已退役**）：旧语义是「以空格拼接追加到 command 后
    /// 交 shell 解释」——宿主已无 shell，故非空即显性拒绝（插件产物 `args` 恒空）
    #[serde(default)]
    pub args: Vec<String>,
    /// 工作目录
    pub cwd: String,
    /// 启动网格（可选；缺省或为 0 时用宿主默认网格——D4「尺寸缺省」）
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    /// 环境变量（可选）
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// 执行环境（发行版转换 / shell 分支的依据）
    pub environment: crate::enums::ExecutionEnvironment,
    /// 来源配置 id（可选；无配置直接启动时缺省为空串）——会话记录的 config_id 字段
    #[serde(default)]
    pub config_id: Option<String>,
    /// 是否创建即启动（缺省 true；false = 两阶段第一阶段）
    #[serde(default = "default_spec_start")]
    pub start: bool,
    /// 启动端设备名（可选，非业务解释的纯事实）：移动端经 HTTP/WS 触发启动时由插件
    /// 透传设备名，内核据此把「正统渲染端」初始归属固定为启动端（与旧宿主路径
    /// `create_session_with_source` 的归属语义逐字一致）；桌面本地启动缺省为 None
    /// （归属 Desktop）。
    #[serde(default)]
    pub source_device: Option<String>,
    /// 指定会话 id（可选）：**重启编排**用——插件先 `remove` 旧会话，再以同一 id
    /// 重建（线协议与终端订阅键不变）。缺省由宿主预生成 UUID；指定 id 已被在册
    /// 会话占用时显性拒绝（不覆盖，避免与重启语义冲突）。
    #[serde(default)]
    pub session_id: Option<String>,
}

fn default_spec_start() -> bool {
    true
}

/// 解析 launch spec-json → `(LaunchSpec, SessionLaunchConfig)`：输入仲裁 + 映射
/// 决策（不含任何业务语义，D4「宿主只做执行」）。
///
/// 分离为 pub(crate) 便于纯单测覆盖参数矩阵（非法环境取值 / 空字段 / commandArgs
/// 必填与可 exec 边界 / 尺寸缺省），不用起 PTY、不用异步。
pub(crate) fn resolve_launch_spec(
    spec_json: &str,
) -> std::result::Result<(LaunchSpec, crate::enums::SessionLaunchConfig), String> {
    if spec_json.trim().is_empty() {
        return Err("session error: empty launch spec".to_string());
    }
    let spec: LaunchSpec =
        serde_json::from_str(spec_json).map_err(|e| format!("session error: invalid launch spec: {}", e))?;
    // 输入仲裁（§8 红线：前端/插件校验只作 UX，最终仲裁在 Rust 端）
    if spec.name.trim().is_empty() {
        return Err("session error: launch spec name is empty".to_string());
    }
    if spec.cwd.trim().is_empty() {
        return Err("session error: launch spec cwd is empty".to_string());
    }

    // commandArgs 必填（2026-09-23 PTY 解耦票）：宿主不做 shell 包装 / WSL 转换，
    // 命令的唯一形态是插件算好的 argv。缺省或空 = 旧产物 → **显性拒绝**（不静默
    // 回退旧路径）。引擎只守可 exec 边界：argv[0] 非空 + 无 NUL 字节（exec 注入面）；
    // 元素自身的 shell 语义不校验（raw exec 天然免注入，转义防护归插件 `build_argv`）。
    let command_args = match &spec.command_args {
        Some(args) if !args.is_empty() => {
            if args[0].trim().is_empty() {
                return Err("session error: launch spec commandArgs[0] is empty".to_string());
            }
            if args.iter().any(|a| a.contains('\0')) {
                return Err("session error: launch spec commandArgs contains NUL byte".to_string());
            }
            args.clone()
        }
        _ => {
            return Err(
                "session error: launch spec commandArgs is required (host shell wrapping retired; \
                 rebuild the plugin artifact with launch.rs::build_argv)"
                    .to_string(),
            )
        }
    };
    // 旧路径字段退役：`args` 的「空格拼接交 shell 解释」语义已无宿主实现
    if !spec.args.is_empty() {
        return Err("session error: launch spec args is retired (send commandArgs instead)".to_string());
    }

    // 构造启动配置：尺寸缺省 = 默认网格（与既有 DefaultConfigMapper::to_launch_config
    // 的 120x40 基准一致，D4「尺寸缺省」）；`command` 仅作诊断字段透传
    let launch_config = crate::enums::SessionLaunchConfig {
        name: spec.name.clone(),
        environment: spec.environment.clone(),
        working_dir: spec.cwd.clone(),
        command: spec.command.clone(),
        command_args,
        env_vars: spec.env.clone(),
        cols: spec.cols.filter(|c| *c > 0).unwrap_or(120),
        rows: spec.rows.filter(|r| *r > 0).unwrap_or(40),
    };
    Ok((spec, launch_config))
}

/// 按启动规格创建会话（v19，权限 `session:write`）→ 返回预生成的 session_id
///
/// 与 [`session_create`] 同一异步执行设计：wasm 调用栈内同步创建会死锁（生命周期
/// 事件回灌同一插件实例需重新获取 wasm_plugins 写锁，该锁正被当前调用持有，tokio
/// RwLock 不可重入；且 wasmtime Store 不可重入）。因此预生成 ID 立即返回，实际创建
/// 在宿主上下文异步执行。创建失败无同步返回通道：插件侧以 created 超时看门狗置
/// failed（与 session_create 同语义）。
///
/// 宿主只做执行：spec → `SessionLaunchConfig`（命令 = 插件算好的 `commandArgs` argv，
/// 宿主**不做** shell 包装 / 发行版转换；尺寸缺省用默认网格）；不做命名、不做映射决策。
pub(crate) fn session_create_with_spec(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    spec_json: &str,
) -> Result<String, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_SESSION_WRITE,
        "host_session_create_with_spec",
    ) {
        return Err(format!("permission denied: {}", PERMISSION_SESSION_WRITE));
    }
    let (spec, launch_config) = resolve_launch_spec(spec_json)?;
    // 会话 id：spec 指定（重启 = 同一 id 重建）或宿主预生成
    let session_id = match spec.session_id.as_deref().map(str::trim) {
        Some("") => return Err("session error: empty session id".to_string()),
        Some(id) => id.to_string(),
        None => Uuid::new_v4().to_string(),
    };
    // id 冲突仲裁（§8：最终仲裁在 Rust 端）：指定 id 已被在册会话占用 → 显性拒绝，
    // 不静默覆盖（重启编排的正确序是「先 remove 再 create」，宿主只做事实校验）
    if let Some(existing) = block_on_async(host_ctx.session_manager.get_session(&session_id)) {
        return Err(format!(
            "session error: session id already exists: {} (name={})",
            session_id, existing.name
        ));
    }

    let sm = host_ctx.session_manager.clone();
    let sid = session_id.clone();
    let pid = plugin_id.to_string();
    let cid = spec.config_id.unwrap_or_default();
    let start = spec.start;
    // 启动端归属（票 09/D3）：spec 可携带 source_device，缺省 None = 桌面本地启动
    let source_device = spec.source_device.clone();
    spawn_with_error_boundary("host_session_create_with_spec", async move {
        match sm
            .create_session_from_spec(launch_config, cid.clone(), source_device, start, Some(&sid), Some(&pid))
            .await
        {
            Ok(_) => {
                tracing::info!(
                    plugin_id = %pid,
                    session_id = %sid,
                    config_id = %cid,
                    start,
                    "host_session_create_with_spec: session created (async)"
                );
            }
            Err(e) => {
                // 创建失败无同步返回通道：插件侧由 creating 超时看门狗置 failed
                tracing::error!(
                    plugin_id = %pid,
                    session_id = %sid,
                    config_id = %cid,
                    error = %e,
                    "host_session_create_with_spec: create_session_from_spec failed (async)"
                );
            }
        }
    });
    Ok(session_id)
}

/// 关闭（终止）会话（v7，需要 `session:write` 权限）
///
/// 包一层核心已有的 `SessionManager::kill_session_with_source`，供插件
/// （如 com.bedcode.terminal-session 定时任务）在执行完毕后关闭自己创建的会话。
/// 停止 PTY 并置 Stopped，会话记录保留（与用户手动关闭一致）。
///
/// **异步执行**：`kill_session_with_source` 会同步分发 Stopping/Stopped
/// 生命周期事件，事件回灌同一插件实例需要重新获取 `wasm_plugins` 写锁
/// （tokio RwLock 不可重入），故此处 spawn 异步执行、wasm 调用立即返回。
pub(crate) fn session_close(host_ctx: &WasmHostContext, plugin_id: &str, session_id: &str) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_WRITE, "host_session_close") {
        return Err("permission denied".to_string());
    }
    if session_id.is_empty() {
        return Err("session error: empty session_id".to_string());
    }
    // 参数合法后再查属主（票 04：先权限、再参数、后属主）
    ensure_session_owner(host_ctx, plugin_id, session_id)?;
    let sm = host_ctx.session_manager.clone();
    let sid = session_id.to_string();
    let pid = plugin_id.to_string();
    spawn_with_error_boundary("host_session_close", async move {
        match sm.kill_session_with_source(&sid, None).await {
            Ok(_) => {
                tracing::info!(
                    plugin_id = %pid,
                    session_id = %sid,
                    "host_session_close: session closed (async)"
                );
            }
            Err(e) => {
                tracing::error!(
                    plugin_id = %pid,
                    session_id = %sid,
                    error = %e,
                    "host_session_close: kill_session failed (async)"
                );
            }
        }
    });
    Ok(())
}

// ==================== 会话动作（v19，权限 session:write，票 10） ====================
//
// 四项「今天插件做不到」的会话动作原语（spec D4）：重启 / 移除 / 改名 / 带请求端
// 标识的尺寸调整。分工（spec D3）：
//
// - **编排与裁决在插件**：重启前存在性预检与失败可见、移除的调用顺序、尺寸的正统端
//   判定与覆盖确认策略（`plugins/terminal-session` 的 `actions` 模块）
// - **执行与登记在内核**：`SessionManager::remove_session` / `resize_session`
//   执行器保留（移动端 HTTP/WS 路径与插件未激活时的降级轨仍直连它们），本层只做
//   权限门 / 参数仲裁 / 原语形状适配
//
// 重启不再有独立原语（v21 退役）：插件编排 = `remove` + `create-with-spec`
// （spec 带 `sessionId` 保住同 id 重建语义），见 host-business-decarriage 收尾。
//
// 权限门一律先于参数处理（AGENTS §7/§8：最终仲裁在 Rust 端）。

/// 移除会话（v19，权限 `session:write`）
///
/// 同步执行（`remove_session_with_source` 只做注册表清理 + 删除同步事件，不派发生命
/// 周期事件 → 无插件回灌死锁），故失败对调用方**可见**：未知会话幂等成功
/// （与宿主 `remove_session` 同语义——删了不存在的会话不是错误）。
pub(crate) fn session_remove(host_ctx: &WasmHostContext, plugin_id: &str, session_id: &str) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_WRITE, "host_session_remove") {
        return Err("permission denied".to_string());
    }
    if session_id.trim().is_empty() {
        return Err("session error: empty session_id".to_string());
    }
    // 参数合法后再查属主（票 04：先权限、再参数、后属主）
    ensure_session_owner(host_ctx, plugin_id, session_id)?;
    let sm = host_ctx.session_manager.clone();
    block_on_async(sm.remove_session_with_source(session_id, None))
        .map_err(|e| format!("session error: remove session failed: {}", e))?;
    tracing::info!(
        plugin_id = %plugin_id,
        session_id = %session_id,
        "host_session_remove: session removed"
    );
    Ok(())
}

/// 改名（v19，权限 `session:write`）→ 返回改名前的名字
///
/// 未知 `session_id` 显性 `session error: session not found: <id>`（不静默）；
/// 空名显性报错。**不新增线协议事件**（spec D1 自守边界：线协议形状不变）——
/// 改名结果经会话列表拉取（`list-sessions`）即可见。
pub(crate) fn session_rename(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
    name: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_WRITE, "host_session_rename") {
        return Err("permission denied".to_string());
    }
    if session_id.trim().is_empty() {
        return Err("session error: empty session_id".to_string());
    }
    if name.trim().is_empty() {
        return Err("session error: empty session name".to_string());
    }
    // 参数合法后再查属主（票 04：先权限、再参数、后属主）
    ensure_session_owner(host_ctx, plugin_id, session_id)?;
    let sm = host_ctx.session_manager.clone();
    let previous = block_on_async(sm.rename_session(session_id, name))
        .map_err(|e| format!("session error: rename session failed: {}", e))?;
    tracing::info!(
        plugin_id = %plugin_id,
        session_id = %session_id,
        "host_session_rename: session renamed"
    );
    Ok(previous)
}

/// 带请求端标识的尺寸调整（v19，权限 `session:write`）
///
/// **只登记与执行，不做裁决**（spec D3：正统端判定与覆盖确认策略归插件）：
/// 透传 PTY winsize + 把正统端归属置为请求方，返回
/// `{previousCanonical, canonical}`（`canonical` 为 `RendererSource` wire 形状）。
///
/// 实现上复用内核执行器并**显式携带覆盖信号**（`force = true`）：内核的裁决分支
/// 因此不参与本原语（那是插件侧职责），而执行 + 登记 + 失败语义（未知会话
/// `NotFound`、PTY 不存在报错）与迁移前逐字一致。
///
/// `requester-json` = `{"kind":"desktop"}` | `{"kind":"mobile","deviceName":"..."}`
/// （与宿主 `RendererSource` serde 同形）；非法取值 / 尺寸非正数显性报错。
pub(crate) fn session_resize(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
    cols: u16,
    rows: u16,
    requester_json: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_WRITE, "host_session_resize") {
        return Err("permission denied".to_string());
    }
    if session_id.trim().is_empty() {
        return Err("session error: empty session_id".to_string());
    }
    if cols == 0 || rows == 0 {
        return Err("session error: invalid size (cols/rows must be > 0)".to_string());
    }
    let requester: crate::session::RendererSource =
        serde_json::from_str(requester_json).map_err(|e| format!("session error: invalid requester: {}", e))?;
    // 参数合法后再查属主（票 04：先权限、再参数、后属主）
    ensure_session_owner(host_ctx, plugin_id, session_id)?;

    let sm = host_ctx.session_manager.clone();
    let previous = block_on_async(sm.canonical_renderer_of(session_id));
    let outcome = block_on_async(sm.resize_session(session_id, cols, rows, requester.clone(), true))
        .map_err(|e| format!("session error: resize session failed: {}", e))?;
    // force=true ⇒ 内核不裁决，恒为 Applied；非 Applied 只可能是内核契约变更
    let crate::session::ResizeOutcome::Applied { canonical } = outcome else {
        return Err("session error: resize executor returned non-applied outcome".to_string());
    };
    tracing::debug!(
        plugin_id = %plugin_id,
        session_id = %session_id,
        cols,
        rows,
        "host_session_resize: size applied and canonical renderer registered"
    );
    serde_json::to_string(&serde_json::json!({
        "previousCanonical": previous,
        "canonical": canonical,
    }))
    .map_err(|e| format!("session error: JSON serialization failed: {}", e))
}

// ==================== 会话注解槽与连接清单（v19，票 11） ====================
//
// 票 11 两原语（spec D4 / D5）：
// - `annotate`：会话注解槽写入——内核只搬运透传、绝不解释键名（`session-id →
//   key → value` 扁平不透明 map）。权限门（`session:write`，属主边界）+ 会话存在性
//   （未知会话显性报错，不写孤儿键）+ 参数形状（session-id / key 非空）三件事之后
//   原样落槽；写入按调用方插件记录归属（结构化日志，值不落日志）。expand 期与旧
//   任务字段（`SessionInfo.task_*`）并存双写，互不干扰；contract 期（票 12）字段
//   摘除后对外 DTO/事件构造点改从槽取值。读取面是 `list-sessions` / `get` 回执的
//   `annotations` 字段（同槽透传）。
// - `connections-list`：连接注册表**原始记录**清单（addr / 设备标识 / 指纹），
//   无排序无解读（不过滤未认证、不合并配对、不加派生字段）——在线判定 / 会话数 /
//   任务状态合并是插件侧派生视图（`plugins/terminal-session` 的 `devices` 模块）的职责。

/// 会话注解槽写入（v19，权限 `session:write`，票 11）
///
/// 见本区块头注释；回调自 `SessionManager::annotate_session`（会话存在 → 落槽并
/// 返回 true；不存在 → false → 宿主显性报错）。同步执行（无事件回灌，无死锁风险），
/// 失败对调用方可见。
pub(crate) fn session_annotate(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_WRITE, "host_session_annotate") {
        return Err("permission denied".to_string());
    }
    if session_id.trim().is_empty() {
        return Err("session error: empty session_id".to_string());
    }
    if key.trim().is_empty() {
        return Err("session error: empty annotation key".to_string());
    }
    // 参数合法后再查属主（票 04：先权限、再参数、后属主）
    ensure_session_owner(host_ctx, plugin_id, session_id)?;
    let sm = host_ctx.session_manager.clone();
    let written = block_on_async(sm.annotate_session(session_id, key, value));
    if !written {
        return Err(format!("session error: session not found: {}", session_id));
    }
    // 属主留痕：记录写入方插件与键名；值不落日志（AGENTS §8 凭据/明文红线，
    // 值可能是任务文本等敏感内容，只记 key 与长度）
    tracing::debug!(
        plugin_id = %plugin_id,
        session_id = %session_id,
        key = %key,
        value_len = value.len(),
        "host_session_annotate: annotation written"
    );
    Ok(())
}

/// 连接注册表原始记录清单（v19，权限 `session:read`，票 11）
///
/// **无排序无解读**：直取内核 WS 连接注册表（`WebSocketManager::list_clients`）的
/// 全部原始条目序列化返回，不排序（保留注册表存储序）、不过滤（含未认证连接）、
/// 不合并（不关联配对记录）、不加派生字段。JSON 数组，元素字段名 = 注册表原始
/// 字段（camelCase）：`{clientId, deviceName?, fingerprint?, addr, authenticated,
/// connectedAt}`。排序 / 在线判定 / 会话数 / 任务状态合并是插件侧派生视图的职责
/// （spec D3「派生视图（在线判定 + 会话数 + 任务状态合并）」）。
pub(crate) fn session_connections_list(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_SESSION_READ,
        "host_session_connections_list",
    ) {
        return Err("permission denied".to_string());
    }
    let manager = crate::server::websocket::WebSocketManager::global();
    let clients = block_on_async(manager.list_clients());
    let values: Vec<serde_json::Value> = clients
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "clientId": c.client_id,
                "deviceName": c.device_name,
                "fingerprint": c.fingerprint,
                "addr": c.addr,
                "authenticated": c.authenticated,
                "connectedAt": c.connected_at,
            })
        })
        .collect();
    serde_json::to_string(&values).map_err(|e| format!("session error: JSON serialization failed: {}", e))
}

/// 会话输出环拉取（票 04，权限 `terminal:output` + 属主校验）：按游标拉取会话输出
/// 原始字节（WIT `list<u8>` 直传，不 JSON 化）。
///
/// `Ok(None)` = 游标已追平产出端（无新字节）；`Ok(Some)` = 自游标起的字节 + 续拉
/// 游标；游标落后于环驻留起点时 `truncated = true`（缺口如实上报，不静默补洞）。
/// 单次返回不超过 [`PLUGIN_SESSION_RING_FETCH_MAX_BYTES`]（约束一次 wasm 边界拷贝量）。
///
/// 环本体归宿主 [`GlobalOutputManager`]（内核保有环、插件注册游标 + 自有水位），
/// 背压语义沿用 2026-09-17 pull 模型：慢消费只损失自己的 ring 历史（`truncated`
/// 重锚），绝不回传到产出端。
pub(crate) fn session_output_ring_fetch(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
    from_offset: u64,
    max_bytes: u32,
) -> Result<Option<RingFetchOutput>, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_TERMINAL_OUTPUT,
        "host_session_output_ring_fetch",
    ) {
        return Err("permission denied".to_string());
    }
    // 属主校验（与 terminal_send 同形态，票 04 P0-3）：会话输出是会话域的私密数据面
    // ——只有创建方插件能拉取自己会话的输出，任意持 `terminal:output` 位的插件不得
    // 读走别人会话的原始字节。
    ensure_session_owner(host_ctx, plugin_id, session_id)?;

    // 会话输出环：GlobalOutputManager 单例按 session_id 取会话管理器；注册表读锁内
    // 只 clone 会话管理器句柄，应答在锁外完成（不交叉持锁）
    let manager = crate::session::GlobalOutputManager::global();
    let Some(session) = block_on_async(manager.session(session_id)) else {
        return Err(format!("session output not found: {session_id}"));
    };
    let budget = max_bytes.min(PLUGIN_SESSION_RING_FETCH_MAX_BYTES) as usize;
    let fetched = block_on_async(async {
        let ring = session.ring();
        let queue = ring.read().await;
        queue.fetch(from_offset, budget)
    });
    if fetched.data.is_empty() && !fetched.truncated {
        return Ok(None);
    }
    Ok(Some(fetched))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};

    const PLUGIN: &str = "test-plugin";

    /// 无 session:read 权限：会话列表被拒绝
    #[test]
    fn session_list_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_list(&ctx, PLUGIN).unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 session:read 权限：单会话查询被拒绝
    #[test]
    fn session_get_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_get(&ctx, PLUGIN, "s1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 session:read 权限：配置列表被拒绝
    #[test]
    fn session_close_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_close(&ctx, PLUGIN, "s1").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 空 session_id：权限通过后参数校验拒绝（防误杀全量会话）
    #[test]
    fn session_close_empty_session_id_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        let err = session_close(&ctx, PLUGIN, "").unwrap_err();
        assert_eq!(err, "session error: empty session_id");
    }

    // ==================== create-with-spec（v19，票 09） ====================

    /// 无 session:write 权限：launch spec 创建会话被拒绝（权限门先于一切参数处理）
    #[test]
    fn session_create_with_spec_permission_denied() {
        let ctx = build_host_ctx();
        let err = session_create_with_spec(
            &ctx,
            PLUGIN,
            r#"{"name":"x","command":"bash","cwd":"/tmp","environment":{"type":"Linux"}}"#,
        )
        .unwrap_err();
        assert!(err.starts_with("permission denied"), "unexpected: {err}");
    }

    /// 成功路径：合法 spec → 预生成 UUID v4 立即返回（实际创建异步执行）。
    /// 用 `start=false` 避免真实 spawn 进程（不启动则无进程可残留），
    /// 与 `session_create_returns_pre_generated_uuid` 同一回退语义。
    #[tokio::test]
    async fn session_create_with_spec_returns_pre_generated_uuid() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        let spec = r#"{
            "name": "dev(2)",
            "command": "bash",
            "commandArgs": ["wsl.exe", "-d", "Ubuntu", "--", "bash", "-lic", "pwd"],
            "cwd": "/home/u",
            "environment": {"type": "Wsl2", "distro": "Ubuntu"},
            "configId": "cfg-1",
            "start": false
        }"#;
        let sid = session_create_with_spec(&ctx, PLUGIN, spec).expect("pre-generated id");
        assert_eq!(sid.len(), 36);
        let uuid = Uuid::parse_str(&sid).expect("valid uuid");
        assert_eq!(uuid.get_version_num(), 4);
    }

    /// 参数门矩阵（resolve_launch_spec 纯函数）：空 spec / 非对象 / 缺 name /
    /// 缺 command / 缺 cwd / 非法 environment 一律显性报错（§8：最终仲裁在 Rust 端）
    #[test]
    fn resolve_launch_spec_param_gates_are_explicit() {
        let valid = |name: &str, command: &str, env: &str| {
            format!(r#"{{"name":"{name}","command":"{command}","cwd":"/tmp","environment":{env}}}"#)
        };
        // 空 spec
        assert!(resolve_launch_spec("").unwrap_err().contains("empty launch spec"));
        // 非法 JSON
        assert!(resolve_launch_spec("not-json")
            .unwrap_err()
            .contains("invalid launch spec"));
        // 缺 name（serde 必填字段缺失 → invalid launch spec）
        assert!(
            resolve_launch_spec(r#"{"command":"ls","cwd":"/tmp","environment":{"type":"Linux"}}"#)
                .unwrap_err()
                .contains("invalid launch spec")
        );
        // 缺 command（必填字段缺失）
        assert!(
            resolve_launch_spec(r#"{"name":"x","cwd":"/tmp","environment":{"type":"Linux"}}"#)
                .unwrap_err()
                .contains("invalid launch spec")
        );
        // 缺 cwd（必填字段缺失）
        assert!(
            resolve_launch_spec(r#"{"name":"x","command":"ls","environment":{"type":"Linux"}}"#)
                .unwrap_err()
                .contains("invalid launch spec")
        );
        // 空 name（字段存在但 trim 后为空 → 空串仲裁）
        assert!(
            resolve_launch_spec(r#"{"name":"  ","command":"ls","cwd":"/tmp","environment":{"type":"Linux"}}"#)
                .unwrap_err()
                .contains("name is empty")
        );
        // 非法 environment 取值（serde 拒绝未知变体）
        assert!(resolve_launch_spec(&valid("x", "ls", r#"{"type":"MacOs"}"#))
            .unwrap_err()
            .contains("invalid launch spec"));
        // 缺 environment（必填字段缺失）
        assert!(resolve_launch_spec(r#"{"name":"x","command":"ls","cwd":"/tmp"}"#)
            .unwrap_err()
            .contains("invalid launch spec"));
    }

    /// 映射决策（resolve_launch_spec）：spec 字段逐一到 SessionLaunchConfig；
    /// commandArgs 透传、尺寸缺省、env 透传、start 缺省 true、configId 透传
    #[test]
    fn resolve_launch_spec_maps_spec_to_launch_config() {
        use crate::enums::{ExecutionEnvironment, WindowsShell};

        // Wsl2 + distro + 尺寸合法 + env + argv
        let (spec, lc) = resolve_launch_spec(
            r#"{
                "name": "dev(1)",
                "command": "powershell",
                "commandArgs": ["wsl.exe", "-d", "Ubuntu-22.04", "--", "bash", "-lic", "pwd"],
                "cwd": "/home/u",
                "cols": 100,
                "rows": 30,
                "env": {"FOO": "bar"},
                "environment": {"type": "Wsl2", "distro": "Ubuntu-22.04"},
                "configId": "cfg-2"
            }"#,
        )
        .expect("resolve ok");
        assert_eq!(spec.start, true, "start 缺省 true");
        assert_eq!(spec.config_id.as_deref(), Some("cfg-2"));
        assert_eq!(lc.name, "dev(1)");
        assert_eq!(lc.command, "powershell", "command 仅诊断透传（不做 args 拼接）");
        assert_eq!(lc.command_args.len(), 7, "argv 整体透传");
        assert_eq!(lc.working_dir, "/home/u");
        assert_eq!(lc.cols, 100);
        assert_eq!(lc.rows, 30);
        assert_eq!(lc.env_vars.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(
            lc.environment,
            ExecutionEnvironment::Wsl2 {
                distro: "Ubuntu-22.04".to_string()
            }
        );

        // Linux + 尺寸缺省（0 / 缺省 → 默认网格 120x40，与 DefaultConfigMapper 基准一致）
        let (_, lc) = resolve_launch_spec(
            r#"{"name":"s","command":"bash","commandArgs":["bash"],"cwd":"/","environment":{"type":"Linux"},"cols":0}"#,
        )
        .expect("resolve ok");
        assert_eq!(lc.environment, ExecutionEnvironment::Linux);
        assert_eq!(lc.cols, 120, "cols=0 视为缺省");
        assert_eq!(lc.rows, 40, "rows 缺省");

        // Windows + PowerShell shell
        let (_, lc) = resolve_launch_spec(
            r#"{"name":"w","command":"echo hi","commandArgs":["powershell.exe","-NoExit","-Command","echo hi"],"cwd":"D:\\work","environment":{"type":"Windows","shell":"PowerShell"}}"#,
        )
        .expect("resolve ok");
        assert_eq!(
            lc.environment,
            ExecutionEnvironment::Windows {
                shell: WindowsShell::PowerShell
            }
        );
    }

    /// start=false 透传：spec 显式不启动时映射结果携带 start=false（供两阶段编排）
    #[test]
    fn resolve_launch_spec_passes_through_start_false() {
        let (spec, _) = resolve_launch_spec(
            r#"{"name":"p","command":"bash","commandArgs":["bash"],"cwd":"/","environment":{"type":"Linux"},"start":false}"#,
        )
        .expect("resolve ok");
        assert!(!spec.start, "start=false 透传");
    }

    /// `commandArgs` 是唯一命令形态（2026-09-23 PTY 解耦票）：非空 argv 透传
    /// （宿主 raw exec）；**缺省 / 空数组 / 旧 `args` 字段 → 显性拒绝**（宿主旧
    /// shell 包装路径已退役，不静默回退）；argv[0] 空 / NUL → 拒绝（可 exec 边界）
    #[test]
    fn resolve_launch_spec_requires_command_args() {
        // ① argv 透传（宿主不解释）：Linux 环境 + 插件算好的 bash -lic 包装 argv
        let (_, lc) = resolve_launch_spec(
            r#"{
                "name": "dev(1)",
                "command": "bash",
                "commandArgs": ["bash", "-lic", "cd '/home/u' && pwd && pnpm dev"],
                "cwd": "/home/u",
                "environment": {"type": "Linux"}
            }"#,
        )
        .expect("resolve ok");
        assert_eq!(
            lc.command_args,
            vec![
                "bash".to_string(),
                "-lic".to_string(),
                "cd '/home/u' && pwd && pnpm dev".to_string()
            ],
            "commandArgs 必须整体透传（宿主不解释 argv）"
        );
        assert_eq!(lc.command, "bash", "command 字段仅诊断透传，原样保留");

        // ② 缺省 / 空数组 → 显性拒绝（旧产物必须按新 SDK 重建，而不是静默走旧路径）
        for json in [
            r#"{"name":"s","command":"bash","cwd":"/","environment":{"type":"Linux"}}"#,
            r#"{"name":"s","command":"bash","commandArgs":[],"cwd":"/","environment":{"type":"Linux"}}"#,
        ] {
            let err = resolve_launch_spec(json).unwrap_err();
            assert!(
                err.contains("commandArgs is required"),
                "缺 commandArgs 必须显性拒绝，got: {err}（json: {json}）"
            );
        }

        // ③ 旧 `args` 字段（空格拼接交 shell 解释）→ 显性拒绝
        let err = resolve_launch_spec(
            r#"{"name":"s","command":"bash","commandArgs":["bash"],"args":["-l"],"cwd":"/","environment":{"type":"Linux"}}"#,
        )
        .unwrap_err();
        assert!(err.contains("args is retired"), "旧 args 字段必须显性拒绝，got: {err}");

        // ④ command 为空字符串但 argv 齐备 → 放行（command 仅诊断）
        let (_, lc) = resolve_launch_spec(
            r#"{"name":"s","command":"","commandArgs":["wsl.exe","-d","Ubuntu","--","bash","-lic","pwd"],"cwd":"/","environment":{"type":"Wsl2","distro":"Ubuntu"}}"#,
        )
        .expect("raw path with empty command ok");
        assert_eq!(lc.command_args.len(), 7, "argv 齐备即放行（command 空不再是拒绝理由）");

        // ⑤ 可 exec 边界：argv[0] 空 / 含 NUL → 显性拒绝
        let err = resolve_launch_spec(
            r#"{"name":"s","command":"","commandArgs":["","-lic"],"cwd":"/","environment":{"type":"Linux"}}"#,
        );
        assert!(
            err.unwrap_err().contains("commandArgs[0] is empty"),
            "空 argv0 必须拒绝"
        );
        let err = resolve_launch_spec(
            r#"{"name":"s","command":"","commandArgs":["bash","-c","echo \u0000"],"cwd":"/","environment":{"type":"Linux"}}"#,
        );
        assert!(err.unwrap_err().contains("contains NUL"), "NUL 字节必须拒绝");
    }

    /// 空会话库：列表返回空 JSON 数组（内存 SessionManager）
    #[tokio::test]
    async fn session_list_empty_ok() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_READ]);
        let json = session_list(&ctx, PLUGIN).expect("list ok").expect("some value");
        assert_eq!(json, "[]");
    }

    /// 不存在的会话：session_get 返回 Ok(None)（非错误）
    #[tokio::test]
    async fn session_get_missing_returns_none() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_READ]);
        let result = session_get(&ctx, PLUGIN, "no-such-session").expect("get ok");
        assert!(result.is_none());
    }

    /// 关闭未知会话：票 04 起按「非属主」显性拒绝（旧契约的幂等成功会让
    /// 陌生插件对任意 id 试探而不留痕；未知 id 与他人的 id 同样不可操作）
    #[tokio::test]
    async fn session_close_of_unowned_session_is_denied() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        let err = session_close(&ctx, PLUGIN, "no-such-session").unwrap_err();
        assert!(err.contains("not owner"), "got: {err}");
        // 属主关闭自己的会话仍同步返回 Ok（异步执行 kill）
        let sid = seed_session(&ctx).await;
        session_close(&ctx, PLUGIN, &sid).expect("属主关闭应放行");
    }

    // ==================== 会话动作（v19，票 10） ====================

    /// 播种会话并登记属主（票 04）：`owner = None` 模拟内核/宿主自建的无主会话
    async fn seed_owned_session(ctx: &WasmHostContext, owner: Option<&str>) -> String {
        use crate::enums::{ExecutionEnvironment, SessionLaunchConfig};
        let launch_config = SessionLaunchConfig {
            name: "属主用会话".to_string(),
            environment: ExecutionEnvironment::Linux,
            working_dir: "/tmp".to_string(),
            command: "bash".to_string(),
            command_args: vec!["bash".to_string()],
            env_vars: std::collections::HashMap::new(),
            cols: 120,
            rows: 40,
        };
        block_on_async(ctx.session_manager.create_session_from_spec(
            launch_config,
            "cfg-owner".to_string(),
            None,
            false,
            None,
            owner,
        ))
        .expect("seed owned session")
    }

    /// 播种一个「只创建不启动」会话（openpty 就绪、无进程），返回 session id
    ///
    /// 走 `create_session_from_spec`（票 09 执行端）：**不读配置表**——本模块的
    /// `build_host_ctx` 里会话管理器与配置管理器各持一个内存库（会话库未建
    /// schema），走配置表会撞 `no such table: session_configs`。会话记录形状与
    /// 生产一致（configId 透传、Starting、无正统端归属）。
    async fn seed_session(ctx: &WasmHostContext) -> String {
        use crate::enums::{ExecutionEnvironment, SessionLaunchConfig};
        let launch_config = SessionLaunchConfig {
            name: "动作会话".to_string(),
            environment: ExecutionEnvironment::Linux,
            working_dir: "/tmp".to_string(),
            command: "bash".to_string(),
            command_args: vec!["bash".to_string()],
            env_vars: std::collections::HashMap::new(),
            cols: 120,
            rows: 40,
        };
        block_on_async(ctx.session_manager.create_session_from_spec(
            launch_config,
            "cfg-actions".to_string(),
            None,
            false,
            None,
            // 票 04：本模块既有用例都以 PLUGIN 身份操作会话，播种即登记其为属主
            Some(PLUGIN),
        ))
        .expect("seed session")
    }

    /// 播种一个**已启动**的会话（真实 bash；openpty master 就绪 → resize 才可用），
    /// 且正统端归属仍为空——`start_existing_session` 不做归属登记，与生产
    /// 「两阶段启动后首个 resize 才确立归属」一致（即裁决四态的起点）。
    async fn seed_running_session(ctx: &WasmHostContext) -> String {
        let sid = seed_session(ctx).await;
        block_on_async(ctx.session_manager.start_existing_session(&sid, None)).expect("start seeded session");
        sid
    }

    /// 三原语缺权限：一律**权限门先于参数处理**（非法 id 也报权限错，不泄漏参数面）
    /// 属主闭环（票 04 红测）：持 `session:write` 的**他插件**不得关闭 / 移除 /
    /// 改名 / 调整 / 注解**别人创建**的会话——此前这些函数只查权限位，
    /// 任意插件拿到 `session:write` 就能杀掉用户正在用的会话。
    #[tokio::test]
    async fn session_actions_by_non_owner_are_denied() {
        let ctx = build_host_ctx();
        let owner = "com.bedcode.owner-a";
        let intruder = "com.bedcode.intruder-b";
        let sid = seed_owned_session(&ctx, Some(owner)).await;
        grant_permissions(&ctx, owner, &[PERMISSION_SESSION_WRITE]);
        grant_permissions(&ctx, intruder, &[PERMISSION_SESSION_WRITE]);

        // 属主自己可用（正例对照，防「恒拒绝假绿」）
        session_rename(&ctx, owner, &sid, "属主改名").expect("属主改名应放行");

        for (label, outcome) in [
            ("close", session_close(&ctx, intruder, &sid).map(|_| ())),
            ("remove", session_remove(&ctx, intruder, &sid)),
            ("rename", session_rename(&ctx, intruder, &sid, "越权改名").map(|_| ())),
            (
                "resize",
                session_resize(&ctx, intruder, &sid, 80, 24, r#"{"kind":"desktop"}"#).map(|_| ()),
            ),
            (
                "annotate",
                session_annotate(&ctx, intruder, &sid, "taskStatus", "running"),
            ),
        ] {
            let err = outcome.err().unwrap_or_else(|| panic!("非属主 {label} 必须被拒"));
            assert!(err.contains("not owner"), "{label} 应按属主拒绝，got: {err}");
        }

        // 越权失败零副作用：会话仍在册、名字未变、注解未被写入
        let info = ctx.session_manager.get_session(&sid).await.expect("会话应仍在册");
        assert_eq!(info.name, "属主改名", "越权改名不得生效");
        assert_eq!(
            ctx.session_manager.session_annotations(&sid).await.get("taskStatus"),
            None,
            "越权注解不得生效"
        );
    }

    /// 无属主会话（内核/宿主自建）一律不可操作（fail-closed，票 04）
    #[tokio::test]
    async fn session_actions_on_ownerless_session_are_denied() {
        let ctx = build_host_ctx();
        let sid = seed_owned_session(&ctx, None).await;
        grant_permissions(&ctx, "com.bedcode.anyone", &[PERMISSION_SESSION_WRITE]);
        let err = session_remove(&ctx, "com.bedcode.anyone", &sid).unwrap_err();
        assert!(err.contains("not owner"), "无主会话不得被插件删: {err}");
    }

    /// 属主随会话销毁注销：同 id 重建后由新创建方重新持有
    #[tokio::test]
    async fn session_owner_is_released_on_remove() {
        let ctx = build_host_ctx();
        let a = "com.bedcode.owner-a";
        let b = "com.bedcode.owner-b";
        let sid = seed_owned_session(&ctx, Some(a)).await;
        assert_eq!(ctx.session_manager.session_owner(&sid).await.as_deref(), Some(a));
        grant_permissions(&ctx, a, &[PERMISSION_SESSION_WRITE]);
        session_remove(&ctx, a, &sid).expect("属主移除应放行");
        assert_eq!(ctx.session_manager.session_owner(&sid).await, None);

        grant_permissions(&ctx, b, &[PERMISSION_SESSION_WRITE]);
        let sid2 = seed_owned_session(&ctx, Some(b)).await;
        assert_eq!(ctx.session_manager.session_owner(&sid2).await.as_deref(), Some(b));
    }

    #[test]
    fn session_actions_permission_denied() {
        let ctx = build_host_ctx();
        assert_eq!(session_remove(&ctx, PLUGIN, "s1").unwrap_err(), "permission denied");
        assert_eq!(
            session_rename(&ctx, PLUGIN, "s1", "n").unwrap_err(),
            "permission denied"
        );
        assert_eq!(
            session_resize(&ctx, PLUGIN, "s1", 80, 24, r#"{"kind":"desktop"}"#).unwrap_err(),
            "permission denied"
        );
    }

    /// 参数门（权限通过后）：空 id / 空名 / 非正尺寸 / 非法 requester 一律显性报错，
    /// 且**零副作用**（会话表仍为空）
    #[tokio::test]
    async fn session_actions_param_gates_are_explicit() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);

        assert_eq!(
            session_remove(&ctx, PLUGIN, "  ").unwrap_err(),
            "session error: empty session_id"
        );
        assert_eq!(
            session_rename(&ctx, PLUGIN, "s1", " ").unwrap_err(),
            "session error: empty session name"
        );
        assert_eq!(
            session_resize(&ctx, PLUGIN, "s1", 0, 24, r#"{"kind":"desktop"}"#).unwrap_err(),
            "session error: invalid size (cols/rows must be > 0)"
        );
        let err = session_resize(&ctx, PLUGIN, "s1", 80, 24, "not-json").unwrap_err();
        assert!(err.contains("invalid requester"), "got: {err}");
        let err = session_resize(&ctx, PLUGIN, "s1", 80, 24, r#"{"kind":"tablet"}"#).unwrap_err();
        assert!(err.contains("invalid requester"), "got: {err}");

        assert!(
            block_on_async(ctx.session_manager.list_sessions()).is_empty(),
            "参数门拒绝不得产生任何会话"
        );
    }

    /// 移除：真实会话移除后记录与归属一并消失；**未知/无主会话不再幂等成功**
    /// （票 04 起按非属主显性拒绝——幂等成功等于允许对任意 id 盲发删除）
    #[tokio::test]
    async fn session_remove_is_idempotent_and_clears_state() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        let err = session_remove(&ctx, PLUGIN, "ghost").unwrap_err();
        assert!(err.contains("not owner"), "未知会话应按非属主拒绝: {err}");

        let sid = seed_running_session(&ctx).await;
        // 预置归属（模拟曾 resize）：移除必须连带清理正统端归属
        let sm = ctx.session_manager.clone();
        block_on_async(sm.resize_session(&sid, 100, 30, crate::session::RendererSource::Desktop, true))
            .expect("claim canonical");
        assert!(block_on_async(sm.canonical_renderer_of(&sid)).is_some());

        session_remove(&ctx, PLUGIN, &sid).expect("remove ok");
        assert!(block_on_async(sm.get_session(&sid)).is_none(), "会话记录必须消失");
        assert!(
            block_on_async(sm.canonical_renderer_of(&sid)).is_none(),
            "正统端归属必须随会话清理"
        );
    }

    /// 改名成功闭环（回执改名前的名字）+ 未知会话显性报错；改名后 `get` 可见新名
    #[tokio::test]
    async fn session_rename_roundtrip_and_missing_session() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE, PERMISSION_SESSION_READ]);
        let sid = seed_session(&ctx).await;
        let info: serde_json::Value =
            serde_json::from_str(&session_get(&ctx, PLUGIN, &sid).expect("get").expect("exists")).expect("json");
        let original = info["name"].as_str().expect("name").to_string();

        let previous = session_rename(&ctx, PLUGIN, &sid, "改过的名字").expect("rename");
        assert_eq!(previous, original, "回执改名前的名字");

        let after: serde_json::Value =
            serde_json::from_str(&session_get(&ctx, PLUGIN, &sid).expect("get").expect("exists")).expect("json");
        assert_eq!(after["name"], "改过的名字");

        let err = session_rename(&ctx, PLUGIN, "ghost", "x").unwrap_err();
        // 票 04：未知 id 无从判定属主 → 与非属主同一拒绝口径（fail-closed）
        assert!(err.contains("not owner"), "未知会话按非属主显性拒绝, got: {err}");
    }

    /// 尺寸原语 = **登记 + 执行**（不裁决）：首次请求方即位正统、回执 previousCanonical
    /// 为 null；他端请求也照旧执行并移交归属（裁决在插件侧，内核不拦）
    #[tokio::test]
    async fn session_resize_registers_and_executes_without_arbitration() {
        use crate::session::RendererSource;
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        let sid = seed_running_session(&ctx).await;
        let sm = ctx.session_manager.clone();

        let first: serde_json::Value = serde_json::from_str(
            &session_resize(&ctx, PLUGIN, &sid, 100, 30, r#"{"kind":"desktop"}"#).expect("resize"),
        )
        .expect("json");
        assert!(first["previousCanonical"].is_null(), "首次无线索端归属");
        assert_eq!(first["canonical"], serde_json::json!({"kind": "desktop"}));
        assert_eq!(
            block_on_async(sm.canonical_renderer_of(&sid)),
            Some(RendererSource::Desktop)
        );

        // 他端（移动端）请求：原语不裁决（裁决归插件）→ 直接执行并移交归属
        let second: serde_json::Value = serde_json::from_str(
            &session_resize(&ctx, PLUGIN, &sid, 80, 24, r#"{"kind":"mobile","deviceName":"Pixel"}"#)
                .expect("resize from other renderer"),
        )
        .expect("json");
        assert_eq!(
            second["previousCanonical"],
            serde_json::json!({"kind": "desktop"}),
            "回执前归属（供插件回执/审计）"
        );
        assert_eq!(
            second["canonical"],
            serde_json::json!({"kind": "mobile", "deviceName": "Pixel"})
        );
        assert_eq!(
            block_on_async(sm.canonical_renderer_of(&sid)),
            Some(RendererSource::Mobile {
                device_name: "Pixel".to_string()
            })
        );

        block_on_async(sm.remove_session(&sid)).expect("cleanup");
    }

    /// `get` 携带登记事实（票 10 新增字段）：无归属为 null，resize 后为该请求方——
    /// 这是插件侧裁决规则的数据来源
    #[tokio::test]
    async fn session_get_exposes_canonical_renderer_fact() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE, PERMISSION_SESSION_READ]);
        let sid = seed_running_session(&ctx).await;

        let before: serde_json::Value =
            serde_json::from_str(&session_get(&ctx, PLUGIN, &sid).expect("get").expect("exists")).expect("json");
        assert!(before["canonicalRenderer"].is_null(), "无归属 → null");

        session_resize(&ctx, PLUGIN, &sid, 100, 30, r#"{"kind":"desktop"}"#).expect("resize");
        let after: serde_json::Value =
            serde_json::from_str(&session_get(&ctx, PLUGIN, &sid).expect("get").expect("exists")).expect("json");
        assert_eq!(after["canonicalRenderer"], serde_json::json!({"kind": "desktop"}));
        assert_eq!(after["id"], sid, "既有字段（宽解析）不受追加字段影响");

        block_on_async(ctx.session_manager.remove_session(&sid)).expect("cleanup");
    }

    // ==================== 注解槽与连接清单（v19，票 11） ====================

    /// annotate 权限门：缺 `session:write` → 显性拒绝，且零副作用
    #[tokio::test]
    async fn session_annotate_permission_gate() {
        let ctx = build_host_ctx();
        let err = session_annotate(&ctx, PLUGIN, "s1", "taskStatus", "x").unwrap_err();
        assert_eq!(err, "permission denied");
        assert!(
            block_on_async(ctx.session_manager.session_annotations("s1")).is_empty(),
            "越权拒绝不得留下任何注解"
        );
    }

    /// annotate 参数门（权限通过后）：空 session-id / 空 key 显性报错；
    /// 未知会话 → `session not found`（不写孤儿键）
    #[tokio::test]
    async fn session_annotate_param_and_existence_gates() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);

        assert_eq!(
            session_annotate(&ctx, PLUGIN, " ", "k", "v").unwrap_err(),
            "session error: empty session_id"
        );
        assert_eq!(
            session_annotate(&ctx, PLUGIN, "s1", "", "v").unwrap_err(),
            "session error: empty annotation key"
        );
        // 票 04：未知会话先撞上属主判定（不再暴露「存在与否」的差异）
        let err = session_annotate(&ctx, PLUGIN, "ghost", "taskStatus", "x").unwrap_err();
        assert!(err.contains("not owner"), "got: {err}");
        assert!(
            block_on_async(ctx.session_manager.session_annotations("ghost")).is_empty(),
            "未知会话不写孤儿键"
        );
    }

    /// annotate 成功闭环：写入 → `session_list` / `session_get` 回执的 `annotations`
    /// 字段原样透传（不透明键值对）；contract 期（票 12）回执只有引擎记录 + 注解槽，
    /// 引擎记录里不再有任何任务字段（对外取值走 `session_view`）
    #[tokio::test]
    async fn session_annotate_passthrough_via_list_and_get() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE, PERMISSION_SESSION_READ]);
        let sid = seed_session(&ctx).await;

        session_annotate(&ctx, PLUGIN, &sid, "taskStatus", "asking").expect("annotate ok");
        session_annotate(&ctx, PLUGIN, &sid, "taskReason", "等待用户答复").expect("annotate ok");

        // get 回执透传
        let got: serde_json::Value =
            serde_json::from_str(&session_get(&ctx, PLUGIN, &sid).expect("get").expect("exists")).expect("json");
        assert_eq!(got["annotations"]["taskStatus"], "asking");
        assert_eq!(got["annotations"]["taskReason"], "等待用户答复");
        // list 回执透传
        let list: serde_json::Value =
            serde_json::from_str(&session_list(&ctx, PLUGIN).expect("list").expect("some")).expect("json");
        let row = list
            .as_array()
            .expect("array")
            .iter()
            .find(|s| s["id"] == sid)
            .expect("row");
        assert_eq!(row["annotations"]["taskStatus"], "asking");
        // 与 get 同形超集：canonicalRenderer 也一并透传
        assert!(row.get("canonicalRenderer").is_some(), "list 与 get 同形超集");

        // contract 期（票 12）：宿主原语回执 = 引擎记录 + `annotations`，记录里不再有
        // 任何任务字段（对外取值走 `session_view` / 前端命令，见 commands/session.rs）
        assert!(
            got.get("taskStatus").is_none() && got.get("taskReason").is_none(),
            "引擎记录不得再序列化任务字段"
        );
        assert!(
            row.get("taskStatus").is_none(),
            "list 回执与 get 同口径（任务态只在 annotations 里）"
        );
        let ann = block_on_async(ctx.session_manager.session_annotations(&sid));
        assert_eq!(ann.get("taskStatus").map(String::as_str), Some("asking"));

        // 清理：不 spawn 进程，仅释放 openpty slave fd
        block_on_async(ctx.session_manager.remove_session(&sid)).expect("remove");
    }

    /// connections-list 权限门：缺 `session:read` → 显性拒绝；授权后可读，
    /// 无头上下文注册表为空 → 合法空数组（形状恒定）
    #[tokio::test]
    async fn session_connections_list_permission_and_empty_shape() {
        let ctx = build_host_ctx();
        let err = session_connections_list(&ctx, PLUGIN).unwrap_err();
        assert_eq!(err, "permission denied");

        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_READ]);
        let raw = session_connections_list(&ctx, PLUGIN).expect("connections list");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json array");
        assert!(parsed.is_array(), "必须为 JSON 数组（无头注册表为空）");
        assert_eq!(parsed, serde_json::json!([]));
    }

    // ==================== 票 04：output-ring-fetch（输出消费二进制原语） ====================

    /// 播种会话（属主 = PLUGIN）并注册输出管理器 + 推入一段输出（模拟 PTY 读线程产出）；
    /// 调用方负责 `unregister_session` / `remove_session` 清理
    async fn seed_session_with_output(ctx: &WasmHostContext, bytes: &[u8]) -> String {
        let sid = seed_session(ctx).await;
        let manager = crate::session::GlobalOutputManager::global();
        let _ = block_on_async(manager.register_session(&sid));
        block_on_async(manager.on_output(crate::session::OutputEvent::new(
            sid.clone(),
            bytes.to_vec(),
            0,
            0,
            false,
        )));
        sid
    }

    /// 权限门：缺 `terminal:output` → 显性拒绝（权限门先于属主/会话存在性——
    /// 未授权插件连「会话输出是否存在」都不应可探知）
    #[tokio::test]
    async fn output_ring_fetch_permission_denied_without_terminal_output() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_READ]);
        let err = session_output_ring_fetch(&ctx, PLUGIN, "any-session", 0, 1024).unwrap_err();
        assert_eq!(err, "permission denied", "缺 terminal:output 必须报权限拒绝");
    }

    /// 属主闭环（票 04 红测同形态）：持 `terminal:output` 的**他插件**不得拉取
    /// 别人会话的输出字节——输出是会话域的私密数据面，只凭权限位即可越权读走
    #[tokio::test]
    async fn output_ring_fetch_by_non_owner_is_denied() {
        let ctx = build_host_ctx();
        // seed_session 播种属主 = PLUGIN（见 seed_session 尾部 Some(PLUGIN)）
        let intruder = "com.bedcode.intruder-b";
        let sid = seed_session(&ctx).await;
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_WRITE]);
        // 越权方权限齐备——只有属主判定能拦住它
        grant_permissions(&ctx, intruder, &[PERMISSION_TERMINAL_OUTPUT, PERMISSION_SESSION_READ]);

        let err = session_output_ring_fetch(&ctx, intruder, &sid, 0, 1024).unwrap_err();
        assert!(err.contains("not owner"), "非属主拉取输出必须被拒，got: {err}");
        // 属主可拉（输出未注册 → 报输出不存在而非权限/属主错，分档可辨）
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_TERMINAL_OUTPUT]);
        let err = session_output_ring_fetch(&ctx, PLUGIN, &sid, 0, 1024).unwrap_err();
        assert!(
            err.contains("session output not found"),
            "属主 + 权限齐备时只报输出不存在，got: {err}"
        );
        block_on_async(ctx.session_manager.remove_session(&sid)).expect("remove");
    }

    /// 正路径 + 追平：推 20 字节 → 16+4 两批拉净 → 第三次 Ok(None)（游标已追平产出端）
    #[tokio::test]
    async fn output_ring_fetch_roundtrip_and_catchup() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_TERMINAL_OUTPUT]);
        let payload: Vec<u8> = (0..20u8).collect();
        let sid = seed_session_with_output(&ctx, &payload).await;
        let manager = crate::session::GlobalOutputManager::global();

        // 第一批：max-bytes=16 截断（宿主钳位后按 16 返回）
        let first = session_output_ring_fetch(&ctx, PLUGIN, &sid, 0, 16)
            .expect("first fetch")
            .expect("first has data");
        assert_eq!(first.data, payload[..16].to_vec());
        assert_eq!(first.next_offset, 16);
        assert!(!first.truncated);

        // 第二批：续拉不重复
        let second = session_output_ring_fetch(&ctx, PLUGIN, &sid, first.next_offset, 16)
            .expect("second fetch")
            .expect("second has data");
        assert_eq!(second.data, payload[16..].to_vec());
        assert_eq!(second.next_offset, 20);
        assert!(!second.truncated);

        // 第三批：游标已追平 → Ok(None)
        let catchup = session_output_ring_fetch(&ctx, PLUGIN, &sid, second.next_offset, 16).expect("catchup");
        assert!(catchup.is_none(), "追平后必须返回 None");

        // 未来游标自愈：from > max_offset 按追平处理（不报错）
        let future = session_output_ring_fetch(&ctx, PLUGIN, &sid, 999, 16).expect("future");
        assert!(future.is_none(), "未来游标按追平自愈");

        block_on_async(manager.unregister_session(&sid));
    }

    /// max-bytes 钳位：单次返回不超宿主上限（16 KiB）；超上限传值只截断不报错
    #[tokio::test]
    async fn output_ring_fetch_clamps_to_host_budget() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_TERMINAL_OUTPUT]);
        let sid = seed_session(&ctx).await;
        let manager = crate::session::GlobalOutputManager::global();
        let _ = block_on_async(manager.register_session(&sid));
        // 20 KiB 驻留（超出单批上限 16 KiB）
        let payload: Vec<u8> = (0..=255u8).cycle().take(20 * 1024).collect();
        block_on_async(manager.on_output(crate::session::OutputEvent::new(
            sid.clone(),
            payload.clone(),
            0,
            0,
            false,
        )));

        let fetched = session_output_ring_fetch(&ctx, PLUGIN, &sid, 0, u32::MAX)
            .expect("fetch")
            .expect("has data");
        assert!(
            fetched.data.len() <= 16 * 1024,
            "单批不得超过宿主钳位上限，got {}",
            fetched.data.len()
        );
        assert_eq!(fetched.data, payload[..16 * 1024].to_vec(), "截断返回的是头段");
        assert_eq!(fetched.next_offset, 16 * 1024 as u64);
        assert!(!fetched.truncated);

        block_on_async(manager.unregister_session(&sid));
        block_on_async(ctx.session_manager.remove_session(&sid)).expect("remove");
    }
}
