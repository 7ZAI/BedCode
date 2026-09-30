//! 认证中心宿主桥接（票 11 C2 落地 → 终端会话中心票 04/05 改指 → 2026-09-29
//! ADR 0031 v32：注册表显式登记取代能力探测 → v33 / ADR 0033：验签执行一并下沉）
//!
//! 认证中心 ≡ 微服务架构的 auth server（用户裁定）：唯一裁决者、注册进网关、可提供
//! 多种认证方式（grant）。与微服务的唯一结构差异是**没有也不需要服务发现**——发现
//! 协议就是「插件激活时调 `auth-center-register` + 宿主唯一性仲裁」这一个函数
//! （`wasm_core/host_api/auth_center.rs` 单中心注册表，ADR 0031 D2）。**两套发现路径已合一**：
//! 本文件的桥接门（[`session_active`]）与裁决面（[`enforce_connection_policy`]）都查
//! 注册表；退役 api_registry 锚点 `SESSION_MARKER_API`（K7：留锚点 = 留第二套
//! 发现机制 = 留 2026-09-29 选错中心的同型病灶）。
//!
//! 认证语义（配对码 / QR / trust / consent / 生物 / **入场 JWT 签发与验签**）实现在
//! 认证中心插件 `com.bedcode.terminal-session`（[`SESSION_PLUGIN_ID`]），本文件是宿主
//! 侧唯一桥接门：
//! - **裁决面**（[`enforce_connection_policy`]，K2/K3 + ADR 0033）：**验签与策略收为
//!   一次调用**——中心内部先验签（入场密钥自持）再逐条做策略，宿主只拿一份
//!   [`AuthenticatedIdentity`]。**无中心 / 调用失败 一律拒绝**（fail-closed，取代
//!   两条 fail-open 降级）——三类拒绝以结构化字段
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
//! - server 认证面：WS 插件端点 + HTTP 网关（**都只调本文件的裁决面**）
//! - `utils/session_gateway.rs`：会话互调窄转发的激活门（[`session_active`]）
//!
//! 边界（ADR 0033 后）：
//! - TTL 配置（`pairing_code_ttl` / `qr_token_ttl`）留宿主 DB 设置（配置域；生成
//!   命令把配置值传插件），TTL get/set 命令不转发。插件可经 host-auth
//!   `auth-setting-set` 写这两项（白名单 + 正整数校验），读取仍走宿主配置 / 命令面
//! - 认证记录真源在认证中心私有库（v24 下沉）；宿主只剩 host-auth 密钥托管 /
//!   生物验签 / 链路身份原语。**设备入场密码学已完全离开宿主**（ADR 0033 D1）
//!
//! 互调调用约定与通用客户端：见 [`crate::wasm_core::intercall`]（ADR 0033 从本
//! 文件上提——原住这里的 `call_api` 实际是通用 JSON-RPC 客户端，被会话面也当通用
//! 互调用，归属错位）。

use crate::utils::auth::identity::AuthenticatedIdentity;
use crate::wasm_core::intercall::AUTH_CENTER_TIMEOUT_MS;
use crate::wasm_core::manager::runtime::WasmHostContext;

// 票 06 起具体导出名与调用模型收口在宿主门面 `PluginHost::call_auth_policy` 内
use crate::wasm_core::manager::host::PluginHost;
use crate::wasm_core::runtime_util::block_on_async;

/// 终端会话中心插件 ID（票 04 起为配对 / QR 语义的权威实现方，票 05 起兼管
/// trust / consent / 认证策略；v32 起为**认证中心**，经注册表显式登记，ADR 0031）
pub const SESSION_PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 认证中心是否在册（K7 桥接门：替代退役的 api_registry 锚点判据）。
///
/// 注册表 = 宿主进程内的发现协议：中心激活时调 `auth-center-register` 登记、
/// 停用时注销 / 宿主 purge 回收，故在册 ⇔ 中心已就绪。
///
/// 无参：注册表是**进程级单例**（`wasm_core/host_api/auth_center.rs`），不需要
/// `WasmHostContext`。ADR 0033 同批去掉了那个只为历史锚点判据保留、实际
/// `let _ = host_ctx;` 的参数——留着会让人以为「在册与否」和某个上下文有关。
pub fn session_active() -> bool {
    crate::wasm_core::host_api::auth_center::is_registered()
}

// ==================== server 认证面（票 12 C3 → ADR 0031 v32 → ADR 0033 v33） ====================

/// 连接建立认证：**问认证中心一次**（验签 + 策略在中心内部完成，ADR 0033）
///
/// 返回：
/// - `Ok(identity)` = 中心放行，并交回连接身份（`device_id` / `device_name` /
///   `fingerprint`，字段集被 L2 锁钉死）
/// - `Err(reason)` = 拒绝原因
///
/// **v32（ADR 0031）**把角色发现从「能力探测 + 排序取首个」改为**查注册表**
/// （`wasm_core/host_api/auth_center.rs`，O(1)，不复刻 2026-09-29 不住选错中心的
/// 病灶）；两条 fail-open 降级（无候选放行 / 传输失败放行）删除，改 **fail-closed**（K3）：
/// - 无中心在册 → 拒绝（`no auth center registered`，`deny_kind=no_center`）
/// - 中心调用失败（实例缺失 / trap / 超时）→ 拒绝（`auth center unavailable: …`，
///   `deny_kind=unavailable`）
/// - 中心策略拒绝（guest 自报 Err）→ 上抛拒绝原因（`deny_kind=policy`）
/// - 中心放行但给不出身份 → 拒绝（`deny_kind=unavailable`，见下方注释）
///
/// **v33（ADR 0033）**：中心内部先验签（入场密钥自持）再裁决，宿主**不再有任何
/// JWT 密码学**。失败面从迁移前的「宿主验签 → 问中心策略」两次调用收窄为一次。
/// **失败面收窄的代价**：宿主不再有独立的验签层，故中心**必须**先把签名验过才
/// 谈策略；中心放行却给不出 `sub` 视为中心侧故障（`deny_kind=unavailable`），
/// 宿主绝不回查本地凭据表——那正是 ADR 0022 §5.1.4 的「宿主侧回查」红线。
///
/// 三类拒绝必须可区分（ADR 0031 §规格）：`no_center` / `unavailable` 是部署/故障问题，
/// `policy` 是产品语义问题（如设备被撤销），排障路径完全不同。
///
/// 调用方：`bedcode-server-websocket` 的 `channel/plugin.rs::authenticate_endpoint_connection`（WS 插件端点
/// 首消息认证）+ `bedcode-server-http` 的 `middleware/auth_gateway.rs::authenticate_with_center`
/// （HTTP `/api` 网关）。
pub fn enforce_connection_policy(
    plugin_host: &PluginHost,
    token: &str,
) -> std::result::Result<AuthenticatedIdentity, String> {
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
        Ok(Ok(claims_json)) => AuthenticatedIdentity::from_center_claims(&claims_json).map_err(|e| {
            tracing::error!(
                deny_kind = "unavailable",
                center_id = %center_id,
                center_owner = %center_owner,
                error = %e,
                "auth center admitted a token without a usable identity, denying (fail-closed)"
            );
            e
        }),
        Ok(Err(reason)) => {
            // 中心策略拒绝（撤销 / 签名无效 / 过期 / 结构非法）：deny_kind=policy，
            // 原因透出（中心侧才是判定方，宿主不解释）
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
    let id = crate::wasm_core::intercall::next_host_request_id();
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
