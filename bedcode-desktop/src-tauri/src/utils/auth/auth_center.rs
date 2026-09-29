//! 认证中心宿主桥接（票 11 C2 落地 → 终端会话中心票 04/05 改指 → 2026-09-29
//! ADR 0031 v32：注册表显式登记取代能力探测）
//!
//! 认证中心 ≡ 微服务架构的 auth server（用户裁定）：唯一裁决者、注册进网关、可提供
//! 多种认证方式（grant）。与微服务的唯一结构差异是**没有也不需要服务发现**——发现
//! 协议就是「插件激活时调 `auth-center-register` + 宿主唯一性仲裁」这一个函数
//! （`wasm_core/host_api/auth_center.rs` 单中心注册表，D2）。**两套发现路径已合一**：
//! 本文件的桥接门（[`session_active`]）与裁决面（[`enforce_connection_policy`]）都查
//! 注册表；退役 api_registry 锚点 [`SESSION_MARKER_API`]（K7：留锚点 = 留第二套
//! 发现机制 = 留 2026-09-29 选错中心的同型病灶）。
//!
//! 认证语义（配对码 / QR / trust / consent / 生物 / JWT 编排）实现在认证中心插件
//! `com.bedcode.terminal-session`（[`SESSION_PLUGIN_ID`]），本文件是宿主侧唯一桥接门：
//! - **裁决面**（[`enforce_connection_policy`]，K2/K3）：验签后经注册表找到中心，再
//!   调中心 `auth-policy` 策略（结构/claims/时效 + 信任撤销）；**无中心 / 调用失败
//!   一律拒绝**（fail-closed，取代两条 fail-open 降级）——三类拒绝以结构化字段
//!   `deny_kind = no_center | unavailable | policy` 区分（AGENTS §8 结构化字段红线）
//! - **桥接门**（[`session_active`]，K7）：查注册表（替代旧锚点判据）
//! - **组合式认证**（[`invoke_auth_method`]，K6）：零解析窄转发到中心 `auth-grant`
//!   互调 api（宿主不拆 `params`、不解释 `method` 语义，只校验「method 在注册表内」
//!   ——安全闸门判据非业务解释，B1 不命中）
//!
//! 消费方：
//! - Tauri 命令面：`commands/system.rs`（配对码）、`commands/qr.rs`（QR）
//! - server 配对端点：`controllers/auth_controller.rs`（/api/auth/pairing ·
//!   /verify · /qr-connect）——移动端验签与前端展示必须同源
//! - server 认证中间件：WS 插件端点 + HTTP 网关取 `auth-policy` 策略
//! - `utils/session_gateway.rs`：会话互调窄转发的激活门（[`session_active`]）
//!
//! 边界：
//! - TTL 配置（`pairing_code_ttl` / `qr_token_ttl`）留宿主 DB 设置（配置域；生成
//!   命令把配置值传插件），TTL get/set 命令不转发。插件可经 host-auth
//!   `auth-setting-set` 写这两项（白名单 + 正整数校验），读取仍走宿主配置 / 命令面
//! - 认证记录真源在认证中心私有库（v24 下沉；宿主主库 `pairings` 等退役表不读不迁
//!   不清理）；宿主只剩 host-auth 密钥托管 / 生物验签 / JWT 签发验签原语（K5 不动）
//!
//! 互调调用约定：请求 topic `bedcode.api.<plugin-id>.<method>`，回复 topic
//! `bedcode.api.reply.<caller>.<request-id>`；caller 为宿主虚拟身份
//! [`crate::wasm_core::host_api::api::HOST_API_CALLER_ID`]
//! （互调门禁只校验目标 api 声明，不校验调用方）。

use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::{AppError, Result};

// 票 12 C3：认证策略取认证中心 capability 导出（验签执行留宿主中间件）；
// 票 06 起具体导出名与调用模型收口在宿主门面 `PluginHost::call_auth_policy` 内
use crate::wasm_core::manager::host::PluginHost;
use crate::wasm_core::runtime_util::block_on_async;

/// 终端会话中心插件 ID（票 04 起为配对 / QR 语义的权威实现方，票 05 起兼管
/// trust / consent / 认证策略；v32 起为**认证中心**，经注册表显式登记，ADR 0031）
pub const SESSION_PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 宿主→插件互调超时（毫秒）：单次操作远快于此，超时视为故障走拒绝（fail-closed）
pub const AUTH_CENTER_TIMEOUT_MS: u64 = 5_000;

/// 认证中心是否在册（K7 桥接门：替代退役的 api_registry 锚点判据）。
///
/// 注册表 = 宿主进程内的发现协议：中心激活时调 `auth-center-register` 登记、
/// 停用时注销 / 宿主 purge 回收，故在册 ⇔ 中心已就绪。`host_ctx` 保留为参数
/// 以兼容既有调用方（历史锚点判据需要它；注册表是全局，不再需要）。
pub fn session_active(host_ctx: &WasmHostContext) -> bool {
    let _ = host_ctx;
    crate::wasm_core::host_api::auth_center::is_registered()
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

// ==================== server 认证策略（票 12 C3） ====================

/// server 连接建立认证策略：**验签执行留宿主中间件**（密码学引擎不移动，spec
/// §3「不动」表——中间件已用宿主 `JwtService` 验签），验签通过后经注册表找到
/// 认证中心，再调中心 `auth-policy.verify-device-token` 策略导出（claims 结构/
/// 时效 + 信任撤销检查，见认证中心插件 `policy` 模块）做裁决。
///
/// **v32（ADR 0031）替换旧实现**：角色发现从「能力探测 + 排序取首个」改为**查
/// 注册表**（`wasm_core/host_api/auth_center.rs`，O(1)，不复刻 2026-09-29 不住
/// 选错中心的病灶）；两条 fail-open 降级（无候选放行 / 传输失败放行）删除，改
/// **fail-closed**（K3）：
/// - 无中心在册 → 拒绝（`no auth center registered`，`deny_kind=no_center`）
/// - 中心策略放行 → `Ok(())`（调用方保留自身验签 claims 作为连接身份）
/// - 中心策略拒绝（guest 自报 Err）→ 上抛拒绝原因（`deny_kind=policy`）
/// - 能力调用传输失败（实例缺失/trap/超时）→ 拒绝（`auth center unavailable: …`，
///   `deny_kind=unavailable`）
///
/// 三类拒绝必须可区分（规格 §5.1）：`no_center` / `unavailable` 是部署/故障问题，
/// `policy` 是产品语义问题（如设备被撤销），排障路径完全不同。
///
/// 调用方：`server/websocket/channel/plugin.rs::verify_endpoint_jwt`（WS 插件端点
/// 首消息认证）+ `server/middleware/jwt_auth.rs::extract_and_verify_jwt`
/// （HTTP /api 网关）。
pub fn enforce_connection_policy(plugin_host: &PluginHost, token: &str) -> std::result::Result<(), String> {
    // 注册表查询（O(1)，无候选遍历、无“取第一个”启发式）：无中心 = 拒绝（K3）
    let Some(entry) = crate::wasm_core::host_api::auth_center::center() else {
        tracing::warn!(
            deny_kind = "no_center",
            "auth center not registered, denying connection (fail-closed)"
        );
        return Err("no auth center registered".to_string());
    };
    let center_owner = entry.owner.clone();
    let center_id = entry.center_id.clone();
    let token = token.to_string();
    // 闭包 move 用克隆副本（center_owner 在 match 分支还要读）
    let call_owner = center_owner.clone();
    let result = block_on_async(async move { plugin_host.call_auth_policy(&call_owner, token).await });
    match result {
        Ok(Ok(_claims_json)) => Ok(()), // 认证中心策略放行（claims 以宿主验签结果为准）
        Ok(Err(reason)) => {
            // 中心策略拒绝（撤销/其他）：deny_kind=policy，原因透出（F4）
            tracing::warn!(
                deny_kind = "policy",
                center_id = %center_id,
                center_owner = %center_owner,
                %reason,
                "auth center policy rejected connection"
            );
            Err(reason)
        }
        Err(e) => {
            // 调用传输失败（实例缺失/trap/超时）：deny_kind=unavailable，拒绝（K3），
            // 不得降级成放行——中心不可用就拒（exp：fail-safe 默认“无应答/超时即拒”）
            tracing::error!(
                deny_kind = "unavailable",
                center_id = %center_id,
                center_owner = %center_owner,
                error = %e,
                "auth center call failed, denying connection (fail-closed)"
            );
            Err(format!("auth center unavailable: {e}"))
        }
    }
}

/// 组合式认证原语（K6）：经认证中心执行一次认证方式调用，**零解析窄转发**。
///
/// 宿主不拆 `params`、不解释 `method` 的业务语义（B1 不命中），只校验「method 在
/// 注册表内」（安全闸门判据）后把调用转发到中心 `auth-grant` 互调 api（ADR 0017
/// JSON-RPC 2.0 over host-bus），并把 result 序列化回串原样透回。
///
/// 边界（spec §4.3.1 矩阵）：无中心 → fail-closed；method 不在注册表 → 点名 method
/// 与在册列表（不猜、不回退到“试试别的”）；传输失败 → `auth center unavailable:
/// <原因>`；中心返回错误信封 → **原样透传**（业务拒绝不吞成宿主错误）。
pub(crate) fn invoke_auth_method(
    host_ctx: &WasmHostContext,
    method: &str,
    params: &str,
) -> std::result::Result<String, String> {
    let Some(entry) = crate::wasm_core::host_api::auth_center::center() else {
        tracing::warn!(
            deny_kind = "no_center",
            method = %method,
            "auth-method-invoke without a registered auth center (fail-closed)"
        );
        return Err("no auth center registered".to_string());
    };
    if !entry.methods.iter().any(|m| m == method) {
        return Err(format!(
            "auth center '{}' does not provide method '{}' (registered: {})",
            entry.owner,
            method,
            entry.methods.join(", ")
        ));
    }
    // params 是调用方给的 JSON 串：解析成 Value 只是为了装进 JSON-RPC 信封，
    // 语义上仍是“原样透传”（宿主不解释字段）。
    let params_value: serde_json::Value =
        serde_json::from_str(params).map_err(|e| format!("auth method invoke params must be valid JSON: {e}"))?;
    let api = format!("{}.auth-grant", entry.owner);
    let request_topic = format!("bedcode.api.{api}");
    let method_short = api.rsplit_once('.').map(|(_, m)| m).unwrap_or(&api);
    let id = next_host_request_id();
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method_short,
        "params": { "method": method, "params": params_value },
    });
    let reply_json = host_ctx
        .call_plugin_api_host(&request_topic, &payload.to_string(), AUTH_CENTER_TIMEOUT_MS)
        .map_err(|e| format!("auth center unavailable: {e}"))?;
    let reply: serde_json::Value =
        serde_json::from_str(&reply_json).map_err(|e| format!("auth center unavailable (invalid reply): {e}"))?;
    if let Some(err) = reply.get("error") {
        // 中心返回错误信封：业务拒绝原样透传（ADR 0030 错误码由中心自持）
        let message = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("rpc error")
            .to_string();
        return Err(message);
    }
    let result = reply
        .get("result")
        .cloned()
        .ok_or_else(|| "auth center unavailable (reply missing result/error)".to_string())?;
    serde_json::to_string(&result).map_err(|e| format!("auth center reply serialize failed: {e}"))
}

/// 格式化设备显示名称：名称 + 首次连接 IP（原 `auth_service` 同名函数，
/// 票 07 后 WS 重认证路径仍在宿主使用——HTTP 路径的同构实现在插件 auth_http）
pub fn format_device_display_name(device_name: &str, address: &str) -> String {
    // address 格式为 "IP:PORT"，提取 IP 部分
    let ip = address.rsplit_once(':').map(|(ip, _)| ip).unwrap_or(address);
    format!("{} ({})", device_name, ip)
}
