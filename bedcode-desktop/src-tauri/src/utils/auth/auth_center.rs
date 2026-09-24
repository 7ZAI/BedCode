//! 认证中心宿主桥接（票 11 C2 落地 → 终端会话中心票 04/05 改指）
//!
//! 双轨并存期的桥接层：认证语义生效时，配对码 / QR token 生命周期操作经互调
//! api（ADR 0017、JSON-RPC 2.0 over host-bus）转发插件实现（状态以插件为准）；
//! 插件未激活或互调失败时**降级宿主实现**（迁移前行为，无单点 —— D7）。
//!
//! **票 04 改指 pairing（expand 步）**：配对码 / QR token 语义搬入合并插件
//! `com.bedcode.terminal-session`。
//!
//! **票 05 收敛（本文件的目标常量收敛为一个）**：trust / consent / `auth-policy`
//! 亦已搬入会话中心，故旧认证中心的 `AUTH_CENTER_PLUGIN_ID` / `AUTH_CENTER_MARKER_API`
//! 与 `auth_center_active()` 一并删除——转发目标与探活锚点统一为
//! [`SESSION_PLUGIN_ID`] / [`SESSION_MARKER_API`]（`com.bedcode.terminal-session.trust-list`）。
//! 锚点取 trust 域只读 api：它是「插件已激活且互调面已登记」的稳定判据，且不与
//! 任何即将演进/退役的域绑定（旧口径用 pairing-code-status 探 trust 面，属借来的判据）。
//! **票 06 退役**：独立认证中心插件已整体删除（认证语义全部归会话中心），本文件是
//! 认证语义的唯一桥接门。模块名 `auth_center` 作为历史命名保留——重命名要牵动 server
//! 中间件 / 命令面 / 端点多处调用点，无行为收益。
//!
//! 消费方：
//! - Tauri 命令面：`commands/system.rs`（配对码）、`commands/qr.rs`（QR）
//! - server 配对端点：`controllers/auth_controller.rs`（/api/auth/pairing ·
//!   /verify · /qr-connect）——移动端验签与前端展示必须同源，否则生成与验证
//!   落在不同状态存储上会破坏配对流程
//! - server 认证中间件：WS 终端/事件通道 + HTTP 网关取 `auth-policy` 策略
//!
//! 边界：
//! - TTL 配置（`pairing_code_ttl` / `qr_token_ttl`）留宿主 DB 设置（配置域；生成
//!   命令把配置值传插件），TTL get/set 命令不转发。票 05 起插件可经 host-auth
//!   `auth-setting-set` 写这两项（白名单 + 正整数校验），读取仍走宿主配置 / 命令面
//! - 连接历史 / 已配对设备表（DB `pairings`）/ 在线设备列表（WS 注册表）留宿主
//!   内核存储（spec D3「不动」表）；票 05 起插件经 host-auth 记录面**读原始记录 +
//!   撤销**，宿主/插件不再是两套账本
//!
//! 互调调用约定：请求 topic `bedcode.api.<plugin-id>.<method>`，回复 topic
//! `bedcode.api.reply.<caller>.<request-id>`；caller 为宿主虚拟身份
//! [`crate::wasm_core::host_api::api::HOST_API_CALLER_ID`]
//! （互调门禁只校验目标 api 声明，不校验调用方）。

use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::{AppError, Result};

// 票 12 C3：认证策略取认证中心 capability 导出（验签执行留宿主中间件）
use crate::wasm_core::manager::capability::{CAP_AUTH_POLICY, EXPORT_AUTH_VERIFY_DEVICE_TOKEN};
use crate::wasm_core::manager::host::PluginHost;
use crate::wasm_core::runtime_util::block_on_async;

/// 终端会话中心插件 ID（票 04 起为配对 / QR 语义的权威实现方，票 05 起兼管
/// trust / consent / 认证策略）
pub const SESSION_PLUGIN_ID: &str = "com.bedcode.terminal-session";
/// 桥接探活锚点（票 05 收敛后的唯一锚点）：注册表含它 ⇔ 会话中心已激活且互调面
/// 已声明（激活登记 / 停用注销，见 ApiRegistry）。
///
/// 取 trust 域只读 api 而非 pairing-code-status：锚点是「插件可用」的判据，绑在
/// 即将退役的域上会随该域消失而静默失效（旧口径正是拿配对码状态探 trust 面）。
pub const SESSION_MARKER_API: &str = "com.bedcode.terminal-session.trust-list";

/// 宿主→插件互调超时（毫秒）：单次操作远快于此，超时视为故障走降级
pub const AUTH_CENTER_TIMEOUT_MS: u64 = 5_000;

/// api 注册表是否含该锚点（锚点只由激活态插件登记，故等价于「插件可用」）
fn api_registered(host_ctx: &WasmHostContext, marker_api: &str) -> bool {
    host_ctx.api_registry().contains(marker_api)
}

/// 会话中心是否可用（配对 / QR / trust / policy 四条桥接路径的**同一**桥接门）
pub fn session_active(host_ctx: &WasmHostContext) -> bool {
    api_registered(host_ctx, SESSION_MARKER_API)
}

/// 宿主互调请求 id 计数器（全局单调；宿主多线程并发调用，reply topic 含
/// caller+id，id 必须全局唯一防止并发调用串台——SDK 侧用 thread_local（wasm
/// 单线程），宿主必须 Atomic）
static HOST_REQUEST_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_host_request_id() -> String {
    let n = HOST_REQUEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    format!("host-req-{n}")
}

/// 调用认证中心互调 api：构造 JSON-RPC 请求 → 发布等待 → 解码 reply → result 值
///
/// wire 形状与 SDK `api_call` 约定一致（spec §9.3）：请求
/// `{jsonrpc, id, method, params}`，响应 `{jsonrpc, id, result | error}`；
/// SDK api_call 模块被 `wasm` feature 门禁（guest 侧），宿主按同一约定本地实现。
pub(crate) fn call_api(host_ctx: &WasmHostContext, api: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let request_topic = format!("bedcode.api.{api}");
    // JSON-RPC `method` 字段 = 短方法名（与 SDK client 同约定：topic 全限定、
    // payload 短名——插件宏分派 `match req.method` 按短名匹配）
    let method = api.rsplit_once('.').map(|(_, m)| m).unwrap_or(api);
    let id = next_host_request_id();
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let reply_json = host_ctx
        .call_plugin_api_host(&request_topic, &payload.to_string(), AUTH_CENTER_TIMEOUT_MS)
        .map_err(|e| AppError::Plugin(format!("auth center api call '{api}' failed: {e}")))?;
    let reply: serde_json::Value = serde_json::from_str(&reply_json)
        .map_err(|e| AppError::Plugin(format!("auth center api '{api}' reply invalid JSON: {e}")))?;
    if let Some(err) = reply.get("error") {
        let message = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("rpc error")
            .to_string();
        return Err(AppError::Plugin(format!("auth center api '{api}' error: {message}")));
    }
    reply
        .get("result")
        .cloned()
        .ok_or_else(|| AppError::Plugin(format!("auth center api '{api}' reply missing result/error")))
}

/// 插件侧不可用时记录降级（结构化字段；双轨并存期未激活是常态，静默跳过）
fn log_fallback(api: &str, err: &AppError) {
    tracing::warn!(
        api = %api,
        error = %err,
        "plugin auth surface unavailable, fallback to host implementation"
    );
}

// ==================== server 认证策略（票 12 C3） ====================

/// server 连接建立认证策略：**验签执行留宿主中间件**（密码学引擎不移动，spec
/// §3「不动」表——中间件已用宿主 `JwtService` 验签），验签通过后经认证中心
/// capability 导出（`auth-policy.verify-device-token`）取策略裁决（claims 结构/
/// 时效 + 信任撤销检查，见会话中心插件 `policy` 模块）。
///
/// 降级语义（无单点）：
/// - 认证中心未激活（api 注册表无标记）→ 宿主策略（迁移前行为：验签通过即放行）
/// - 认证中心策略放行 → `Ok(())`（调用方保留自身验签 claims 作为连接身份）
/// - 认证中心策略拒绝（guest 自报 Err）→ 上抛拒绝原因（调用方拒绝连接）
/// - 能力调用传输失败（实例缺失/trap）→ 宿主策略回退（防认证中心故障误杀全部连接）
///
/// 调用方：`server/websocket/conn.rs::authenticate_jwt`（WS 终端/事件通道首消息认证）+
/// `server/middleware/jwt_auth.rs::extract_and_verify_jwt`（HTTP /api 网关）。
pub fn enforce_connection_policy(plugin_host: &PluginHost, token: &str) -> std::result::Result<(), String> {
    let host_ctx = plugin_host.wasm_host_ctx();
    if !session_active(host_ctx) {
        // 会话中心未激活：宿主策略回退（迁移前行为），无单点
        return Ok(());
    }
    let token = token.to_string();
    let result = block_on_async(async move {
        plugin_host
            .call_plugin_capability_export::<(String,), (std::result::Result<String, String>,)>(
                SESSION_PLUGIN_ID,
                CAP_AUTH_POLICY,
                EXPORT_AUTH_VERIFY_DEVICE_TOKEN,
                (token,),
            )
            .await
    });
    match result {
        Ok((Ok(_claims_json),)) => Ok(()), // 认证中心策略放行（claims 以宿主验签结果为准）
        Ok((Err(reason),)) => Err(reason), // 认证中心策略拒绝 → 上抛原因
        Err(e) => {
            // 能力调用传输失败（实例缺失/trap）：宿主策略回退，防认证中心故障
            // 误杀全部连接（无单点）
            log_fallback("auth-policy.verify-device-token", &e);
            Ok(())
        }
    }
}

// ==================== 认证记录通知（v24 认证记录下沉，WS 中间件回调） ====================
//
// 认证记录（配对设备 / 连接历史）真源下沉认证中心后，宿主 WS 认证中间件不再
// 直写主库：认证成功（last_seen / connect_count）与断开回填（disconnected_at）
// 经互调 api 通知认证中心插件（`connection-touch` / `connection-close`）。
// 降级语义（无单点）：插件未激活 / 互调失败 → warn + 跳过（记录缺失不阻断
// 认证与断连——与 v24 前 update_pairing_last_seen / close_open 失败 warn 同语义）。

/// WS 认证成功：通知认证中心刷新配对记录（last_seen / connect_count）
///
/// **异步 fire-and-forget**（ambient runtime 后台执行）：WS 认证/断连常在
/// actix current_thread runtime 的驱动线程上运行——若在此同步等待互调 reply
/// 会自锁（reply 的 MessageBus 投递 spawn 到同一 current_thread runtime，而
/// 该 runtime 的调度线程正被本调用阻塞，投递永不执行 → 超时）。记录刷新是
/// 降级语义（失败 warn + 跳过，不阻断认证），后台执行即可。
pub fn notify_connection_touch(plugin_host: &PluginHost, fingerprint: &str) {
    let host_ctx = plugin_host.wasm_host_ctx().clone();
    let fp = fingerprint.to_string();
    if !session_active(&host_ctx) {
        return; // 插件未激活：记录更新跳过（不阻断认证）
    }
    let params = serde_json::json!(fp);
    crate::wasm_core::runtime_util::ambient_handle().spawn(async move {
        match call_api(&host_ctx, "com.bedcode.terminal-session.connection-touch", params) {
            Ok(_) => tracing::debug!(
                fingerprint = %fp,
                "WS auth: connection touch notified to auth center"
            ),
            Err(e) => log_fallback("auth center connection-touch", &e),
        }
    });
}

/// WS 断链：通知认证中心回填最近 open 连接的断开时间
///
/// 与 touch 同口径：异步 fire-and-forget（ambient runtime），避免 actix
/// current_thread 上下文同步等互调 reply 自锁；失败 warn + 跳过。
pub fn notify_connection_close(plugin_host: &PluginHost, fingerprint: &str) {
    let host_ctx = plugin_host.wasm_host_ctx().clone();
    let fp = fingerprint.to_string();
    if !session_active(&host_ctx) {
        return; // 插件未激活：回填跳过（不阻断断开语义）
    }
    let params = serde_json::json!(fp);
    crate::wasm_core::runtime_util::ambient_handle().spawn(async move {
        match call_api(&host_ctx, "com.bedcode.terminal-session.connection-close", params) {
            Ok(_) => tracing::debug!(
                fingerprint = %fp,
                "WS disconnect: connection close notified to auth center"
            ),
            Err(e) => log_fallback("auth center connection-close", &e),
        }
    });
}

/// 格式化设备显示名称：名称 + 首次连接 IP（原 `auth_service` 同名函数，
/// 票 07 后 WS 重认证路径仍在宿主使用——HTTP 路径的同构实现在插件 auth_http）
pub fn format_device_display_name(device_name: &str, address: &str) -> String {
    // address 格式为 "IP:PORT"，提取 IP 部分
    let ip = address.rsplit_once(':').map(|(ip, _)| ip).unwrap_or(address);
    format!("{} ({})", device_name, ip)
}
