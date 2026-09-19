//! 认证中心消费（票 10 C1）—— peer consent / 信任决策改经互调认证中心 API
//!
//! 语义迁移（auth-center-spec §3「动」表：peer consent / 设备信任列表归
//! 认证中心单一权威）：
//!
//! - **consent**：首连确认决策经 `com.bedcode.devices.decide-consent`
//!   （`auth.decide-consent`）两阶段流——
//!   阶段 1（`peer:consent` 事件到达）：无用户意向预检信任，已信任免确认
//!   自动放行（不弹窗）；未知 peer → ask，照旧弹窗询问；
//!   阶段 2（`file-transfer.respond-consent` 回传用户意向）：最终 accept/deny，
//!   消费方按决策应答宿主 peer 引擎（引擎原语留在宿主，本插件只做决策映射）。
//! - **信任列表**：`file-transfer.list-trusted` 经
//!   `com.bedcode.devices.list-trusted-devices`（`auth.list-trusted-devices`）
//!   取统一视图，映射回旧的 peer-only 数组 wire（行为等价，前端零改动）。
//! - **撤销**：无互调 api（认证中心 manifest api 未声明 revoke），维持宿主
//!   `peer_revoke_trusted` 原语直通——与认证中心 peer 段共享同一宿主 trust
//!   store，数据一致。
//!
//! 双轨兜底（无单点）：认证中心未激活 / 互调超时 / 门禁拒绝时降级为迁移前
//! 行为——consent 直接弹窗、respond 直答宿主、list 直查宿主。宿主实现
//! （peer_net 引擎原语）保留作对照基线（票 10 验收：与迁移前行为等价）。
//!
//! 测试策略：
//! - 编排逻辑经 [`AuthCenterGateway`] / [`PeerRespondOps`] 注入假实现 native
//!   直测（对照场景：同一对端同一决策）；wire 映射 / 登记表纯函数直测。
//! - 端到端互调闭环（真实产物 + 总线 wire 捕获）在宿主测试
//!   `test_filetransfer_consumes_auth_center_closed_loop` 覆盖。

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostBus;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostLog, HostPeer};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::plugin_api;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

/// 认证中心插件 ID（互调目标；仅 wasm 运行时使用）
#[cfg(target_arch = "wasm32")]
pub(crate) const AUTH_CENTER_ID: &str = "com.bedcode.devices";

/// consent 决策互调超时（毫秒）：认证中心无响应时快速降级，避免阻塞总线回调
#[cfg(target_arch = "wasm32")]
const DECIDE_TIMEOUT_MS: u64 = 3_000;
/// 信任列表互调超时（毫秒）
#[cfg(target_arch = "wasm32")]
const LIST_TIMEOUT_MS: u64 = 5_000;

// ==================== wire 契约（镜像认证中心 consent/model.rs） ====================
// 与 devices 插件 serde 形状逐字段对齐（camelCase / snake_case）；插件间不
// 相互依赖 crate（ADR 0022 高内聚低耦合），契约共享唯一真源 = WIT/JSON wire。

/// 用户显式意向（wire snake_case；与认证中心 `UserDecision` 同形状）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UserDecision {
    /// 允许：本连接放行（认证中心 accept 路径带名落库信任）
    Accept,
    /// 拒绝：本连接拒绝
    Deny,
    /// 一次性确认：放行但不变更信任状态（本插件 UI 暂未暴露，wire 对齐保兼容）
    #[serde(rename = "one_time")]
    OneTimeAccept,
}

/// 决策请求（camelCase；`peer:consent` 事件桥 payload 可直接反序列化）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsentRequest {
    /// 宿主 peer:consent 事件的 requestId（应答透传寻址）
    pub request_id: String,
    /// 对端节点 ID（64 位小写 hex）
    pub node_id: String,
    /// 短指纹（前 8 位；对端展示兜底）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint_short: Option<String>,
    /// 对端展示名
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// 用户显式意向（缺省 = 仅策略评估：已信任自动放行 / 未知 → ask）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_decision: Option<UserDecision>,
}

/// 决策种类（wire snake_case）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionKind {
    Accept,
    Deny,
    Ask,
}

/// 决策结果（camelCase）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsentDecision {
    pub decision: DecisionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub request_id: String,
}

// ==================== 互调 client（ADR 0017：manifest 声明即契约） ====================
// 以认证中心 manifest 为防漂移比对源：trait 方法推导的 api 清单与其 `api` 字段
// 精确集合比对（构建期不一致直接编译失败）。client 类型化调用，wire 形状与本
// 模块类型经 serde 对齐。
//
// 仅 wasm 目标声明：native（cargo test）不消费 client，跳过宏展开避免
// 死代码告警；防漂移比对由 wasm 构建（CI 插件构建链）强制执行。

#[cfg(target_arch = "wasm32")]
#[plugin_api(manifest = "../../devices/plugin.json")]
// 声明即契约（ADR 0017 防漂移）：trait 自身不被运行引用，仅承载构建期比对
// （宏生成 Dispatcher/Client，Client 由 WasmAuthGateway 使用）
#[allow(dead_code)]
pub(crate) trait AuthCenterApi {
    /// 探活（manifest 契约全量对齐；本插件不消费）
    fn hello() -> Result<String, String>;

    /// 首连确认决策（两阶段流：阶段 1 无意向评估信任；阶段 2 回传 userDecision）
    #[api("decide-consent")]
    fn decide_consent(request: ConsentRequest) -> Result<ConsentDecision, String>;

    /// 统一信任视图（pairing + peer 合并；本插件只取 peer 段）
    #[api("list-trusted-devices")]
    fn list_trusted_devices() -> Result<serde_json::Value, String>;

    // ============ 票 11 命令面桥接新增（宿主命令面消费；本插件不消费） ============
    // 构建期防漂移要求 trait 方法集与 manifest.api 精确一致，以下方法仅承载
    // 比对（宿主侧经原始 JSON-RPC 调用，不经本 client）。
    #[api("pairing-code-generate")]
    fn pairing_code_generate(ttl: u64) -> Result<serde_json::Value, String>;
    #[api("pairing-code-status")]
    fn pairing_code_status() -> Result<Option<serde_json::Value>, String>;
    #[api("pairing-code-verify")]
    fn pairing_code_verify(code: String) -> Result<bool, String>;
    #[api("pairing-code-clear")]
    fn pairing_code_clear() -> Result<(), String>;
    #[api("qr-code-generate")]
    fn qr_code_generate(ttl: u64) -> Result<serde_json::Value, String>;
    #[api("qr-code-status")]
    fn qr_code_status() -> Result<Option<serde_json::Value>, String>;
    #[api("qr-code-verify")]
    fn qr_code_verify(token: String) -> Result<serde_json::Value, String>;
    #[api("qr-code-clear")]
    fn qr_code_clear() -> Result<(), String>;
}

// ==================== 可注入面（native 单测驱动编排） ====================

/// 认证中心互调面（wasm 下经宏生成 client 直连；native 单测注入假实现）
pub(crate) trait AuthCenterGateway {
    fn decide_consent(&self, request: ConsentRequest) -> Result<ConsentDecision, String>;
    fn list_trusted_devices(&self) -> Result<serde_json::Value, String>;
}

/// 宿主 peer 引擎应答面（wasm 下直通 WasmHost；native 单测注入记录式假实现）
pub(crate) trait PeerRespondOps {
    /// 应答首连确认（返回是否命中待确认项）
    fn respond_consent(&self, request_id: &str, accepted: bool) -> Result<bool, String>;
    /// 可信对端列表（TrustedPeerDto JSON 数组；降级直查宿主用）
    fn list_trusted(&self) -> Result<serde_json::Value, String>;
}

/// wasm 真实现：宏生成 client 直连认证中心
#[cfg(target_arch = "wasm32")]
pub(crate) struct WasmAuthGateway;

#[cfg(target_arch = "wasm32")]
impl AuthCenterGateway for WasmAuthGateway {
    fn decide_consent(&self, request: ConsentRequest) -> Result<ConsentDecision, String> {
        AuthCenterApiClient::new(AUTH_CENTER_ID)
            .with_timeout(DECIDE_TIMEOUT_MS)
            .decide_consent(request)
            .map_err(|e| e.to_string())
    }

    fn list_trusted_devices(&self) -> Result<serde_json::Value, String> {
        AuthCenterApiClient::new(AUTH_CENTER_ID)
            .with_timeout(LIST_TIMEOUT_MS)
            .list_trusted_devices()
            .map_err(|e| e.to_string())
    }
}

/// wasm 真实现：直通独立宿主 peer 引擎原语
#[cfg(target_arch = "wasm32")]
pub(crate) struct WasmPeerOps<'a> {
    h: &'a WasmHost,
}

#[cfg(target_arch = "wasm32")]
impl PeerRespondOps for WasmPeerOps<'_> {
    fn respond_consent(&self, request_id: &str, accepted: bool) -> Result<bool, String> {
        self.h
            .peer_respond_consent(request_id, accepted)
            .map_err(|e| e.message)
    }

    fn list_trusted(&self) -> Result<serde_json::Value, String> {
        self.h.peer_list_trusted().map_err(|e| e.message)
    }
}

// ==================== 待确认登记表（阶段 2 数据源） ====================
// `respond-consent` 命令只带 requestId，阶段 2 决策需要对端信息——`peer:consent`
// 事件到达时登记，应答时消费（FIFO，容量封顶防事件风暴）。wasip3 的
// thread_local 按宿主调用线程隔离，实例状态必须 static Mutex（教训见 devices
// lib.rs 模块文档）。

/// 登记表容量封顶：consent 请求 30s 超时窗内峰值并发极小，防御性上限
const PENDING_CAP: usize = 32;

static PENDING: OnceLock<Mutex<VecDeque<ConsentRequest>>> = OnceLock::new();

fn pending() -> &'static Mutex<VecDeque<ConsentRequest>> {
    PENDING.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// 登记核心（纯队列操作，native 单测直测；静态包装经 [`pending`] 加锁）
fn remember_in(q: &mut VecDeque<ConsentRequest>, request: ConsentRequest) {
    if q.iter().any(|r| r.request_id == request.request_id) {
        return;
    }
    if q.len() >= PENDING_CAP {
        q.pop_front();
    }
    q.push_back(request);
}

/// 取出核心（消费性；未知 requestId 返回 None）
fn take_in(q: &mut VecDeque<ConsentRequest>, request_id: &str) -> Option<ConsentRequest> {
    let idx = q.iter().position(|r| r.request_id == request_id)?;
    q.remove(idx)
}

/// 登记（同 requestId 去重幂等；超限 FIFO 淘汰最旧）
fn remember(request: ConsentRequest) {
    let mut q = pending().lock().expect("pending consent registry lock");
    remember_in(&mut q, request);
}

/// 取出登记（消费性；未知 requestId 返回 None）
fn take_pending(request_id: &str) -> Option<ConsentRequest> {
    let mut q = pending().lock().expect("pending consent registry lock");
    take_in(&mut q, request_id)
}

// ==================== 纯函数（native 直测） ====================

/// 解析宿主 `peer:consent` 事件载荷 → 决策请求
///
/// 校验与认证中心同口径：`requestId` / `nodeId` 必填（无身份/寻址的请求
/// 没有可裁决语义）；畸形载荷返回 None（调用方照旧弹窗，前端解析有同等保护）。
pub(crate) fn parse_consent_payload(payload: &serde_json::Value) -> Option<ConsentRequest> {
    let request: ConsentRequest = serde_json::from_value(payload.clone()).ok()?;
    if request.request_id.is_empty() || request.node_id.is_empty() {
        return None;
    }
    Some(request)
}

/// 用户显式意向映射：命令面布尔 → 认证中心 userDecision
pub(crate) fn user_decision_from_accepted(accepted: bool) -> UserDecision {
    if accepted {
        UserDecision::Accept
    } else {
        UserDecision::Deny
    }
}

/// 决策 → 宿主应答布尔：accept → Some(true)、deny → Some(false)、ask → None
pub(crate) fn decide_accepts_connection(decision: &ConsentDecision) -> Option<bool> {
    match decision.decision {
        DecisionKind::Accept => Some(true),
        DecisionKind::Deny => Some(false),
        DecisionKind::Ask => None,
    }
}

/// 认证中心统一视图 → 旧 file-transfer 列表 wire（TrustedPeerDto 数组）
///
/// 规则：
/// - `peerError` 非空 → Err（host-peer 数据不可用；不静默吞错——迁移前直查
///   宿主的同场景也是报错，行为等价）
/// - 仅保留 `kind = peer` 条目（旧列表语义 = peer 信任集；pairing 不并入，
///   防展示面行为漂移——pairing 设备走设置面统一视图，不经本列表）
/// - 畸形视图（缺 devices 数组）→ Err（显性失败，不猜测）
pub(crate) fn map_trusted_list(view: &serde_json::Value) -> Result<serde_json::Value, String> {
    let devices = view
        .get("devices")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            "auth center list-trusted-devices: malformed view (missing devices array)".to_string()
        })?;
    if let Some(err) = view
        .get("peerError")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Err(format!(
            "auth center trusted peers unavailable (peerError): {err}"
        ));
    }
    let peers: Vec<serde_json::Value> = devices
        .iter()
        .filter(|e| e.get("kind").and_then(|k| k.as_str()) == Some("peer"))
        .map(|e| {
            let id = e.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            serde_json::json!({
                "nodeId": id,
                "displayName": e.get("name").and_then(|v| v.as_str()),
                "fingerprintShort": e
                    .get("fingerprintShort")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .or_else(|| {
                        (!id.is_empty()).then(|| id[..id.len().min(8)].to_string())
                    }),
                "addedAt": e.get("addedAt").and_then(|v| v.as_str()).unwrap_or_default(),
            })
        })
        .collect();
    Ok(serde_json::Value::Array(peers))
}

// ==================== 编排（native 可测；wasm 入口直通） ====================

/// 阶段 1 评估编排：`peer:consent` 事件到达时的信任预检。
///
/// 返回 `(handled, allow)`：
/// - `handled = true`：认证中心已裁决（accept → 应答宿主放行 / deny → 拒绝），
///   调用方不应再弹窗；`allow` 为裁决结果
/// - `handled = false`：需要用户确认（ask），或认证中心不可用降级为询问
///   （双轨兜底：与迁移前行为一致——事件照常弹窗）
///
/// 无论结果，先登记待确认请求（阶段 2 需要对端信息；已自动应答场景条目存而
/// 不用——迟到的应答落到宿主 hit:false，静默无害）。
pub(crate) fn run_phase1(
    gw: &dyn AuthCenterGateway,
    peer: &dyn PeerRespondOps,
    payload: &serde_json::Value,
) -> (bool, bool) {
    let Some(request) = parse_consent_payload(payload) else {
        return (false, false);
    };
    remember(request.clone());
    let Ok(decision) = gw.decide_consent(request) else {
        // 认证中心不可用（未激活/超时/门禁拒绝）→ 降级照旧询问（无单点）
        return (false, false);
    };
    match decision.decision {
        DecisionKind::Ask => (false, false),
        DecisionKind::Accept => {
            let _ = peer.respond_consent(&decision.request_id, true);
            (true, true)
        }
        DecisionKind::Deny => {
            let _ = peer.respond_consent(&decision.request_id, false);
            (true, false)
        }
    }
}

/// 阶段 2 决策编排：用户意向（accept/deny）经认证中心最终裁决后应答宿主。
///
/// 返回宿主应答 `hit`（是否命中待确认项；false = 已超时/已应答/未知，前端
/// 应关闭弹窗）。
/// 降级路径（与迁移前行为等价，双轨无单点）：
/// - 认证中心不可用 → 直答宿主（旧行为）
/// - 无登记请求（事件丢失 / 插件重启）→ 直答宿主（旧行为）
/// - 认证中心返回 ask（带显式意向不可能，协议异常）→ 显性报错不猜测
pub(crate) fn run_decide(
    gw: &dyn AuthCenterGateway,
    peer: &dyn PeerRespondOps,
    request_id: &str,
    accepted: bool,
) -> Result<bool, String> {
    match take_pending(request_id) {
        Some(mut request) => {
            request.user_decision = Some(user_decision_from_accepted(accepted));
            match gw.decide_consent(request) {
                Ok(decision) => match decide_accepts_connection(&decision) {
                    Some(allow) => peer.respond_consent(request_id, allow),
                    None => Err(format!(
                        "auth center returned ask for explicit decision (request {request_id})"
                    )),
                },
                Err(_) => {
                    // 认证中心不可用 → 双轨降级直答宿主（旧行为）
                    peer.respond_consent(request_id, accepted)
                }
            }
        }
        None => {
            // 无登记请求 → 双轨降级直答宿主（旧行为）
            peer.respond_consent(request_id, accepted)
        }
    }
}

/// 信任列表编排：认证中心统一视图 → 旧 wire；不可用降级直查宿主（旧行为）
pub(crate) fn run_list_trusted(
    gw: &dyn AuthCenterGateway,
    peer: &dyn PeerRespondOps,
) -> Result<serde_json::Value, String> {
    match gw.list_trusted_devices() {
        Ok(view) => map_trusted_list(&view),
        Err(_) => {
            // 认证中心不可用 → 双轨降级直查宿主（旧行为）
            peer.list_trusted()
        }
    }
}

// ==================== wasm 入口（lib.rs 调用） ====================
// 模式对齐 devices（keys.rs / consent/ops.rs）：native 显性失败，不引用
// wasm 专属符号（macros 生成的 client 仅在 wasm 分支被调用）。

/// 阶段 1 预检（wasm）：true = 认证中心已裁决并应答（不弹窗）
#[cfg(target_arch = "wasm32")]
pub(crate) fn evaluate_and_maybe_respond(h: &WasmHost, payload: &serde_json::Value) -> bool {
    let (handled, allow) = run_phase1(&WasmAuthGateway, &WasmPeerOps { h }, payload);
    if handled {
        h.log_info(&format!(
            "consent auto-resolved via auth center (allow={allow})"
        ));
    }
    handled
}

/// 阶段 1 预检（native 桩：单测经 run_phase1 注入假实现直测）
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn evaluate_and_maybe_respond(_h: &WasmHost, _payload: &serde_json::Value) -> bool {
    false
}

/// 阶段 2 决策并应答宿主（wasm）：返回宿主应答 hit
#[cfg(target_arch = "wasm32")]
pub(crate) fn decide_and_respond(
    h: &WasmHost,
    request_id: &str,
    accepted: bool,
) -> Result<bool, String> {
    run_decide(&WasmAuthGateway, &WasmPeerOps { h }, request_id, accepted)
}

/// 阶段 2 决策并应答宿主（native 桩）
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn decide_and_respond(
    _h: &WasmHost,
    _request_id: &str,
    _accepted: bool,
) -> Result<bool, String> {
    Err("consent decide unavailable outside wasm runtime".to_string())
}

/// 信任列表（wasm）：经认证中心统一视图映射；不可用降级直查宿主
#[cfg(target_arch = "wasm32")]
pub(crate) fn list_trusted(h: &WasmHost) -> Result<serde_json::Value, String> {
    run_list_trusted(&WasmAuthGateway, &WasmPeerOps { h })
}

/// 信任列表（native 桩）
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn list_trusted(_h: &WasmHost) -> Result<serde_json::Value, String> {
    Err("list-trusted unavailable outside wasm runtime".to_string())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    const NODE_A: &str = "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344";
    const NODE_B: &str = "11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd";

    fn payload(request_id: &str, node_id: &str) -> serde_json::Value {
        serde_json::json!({
            "requestId": request_id,
            "nodeId": node_id,
            "fingerprintShort": &node_id[..8],
            "deviceName": "模拟对端",
        })
    }

    fn decision(decision: DecisionKind, request_id: &str) -> ConsentDecision {
        ConsentDecision {
            decision,
            reason: None,
            request_id: request_id.to_string(),
        }
    }

    /// 认证中心假实现：按脚本返回决策 / 视图 / 错误，并记录收到的请求
    struct FakeGateway {
        decide_results: Vec<Result<ConsentDecision, String>>,
        list_result: Result<serde_json::Value, String>,
        received: Mutex<Vec<ConsentRequest>>,
    }

    impl FakeGateway {
        fn new(decide_results: Vec<Result<ConsentDecision, String>>) -> Self {
            Self {
                decide_results,
                // 默认认证中心不可用（列表侧失败 → 触发降级路径）；需要时注入视图
                list_result: Err("mock auth center list-trusted-devices unavailable".to_string()),
                received: Mutex::new(Vec::new()),
            }
        }
    }

    impl AuthCenterGateway for FakeGateway {
        fn decide_consent(&self, request: ConsentRequest) -> Result<ConsentDecision, String> {
            self.received.lock().unwrap().push(request.clone());
            let idx = self.received.lock().unwrap().len() - 1;
            self.decide_results
                .get(idx)
                .cloned()
                .unwrap_or_else(|| Err("unexpected extra decide_consent call".to_string()))
        }

        fn list_trusted_devices(&self) -> Result<serde_json::Value, String> {
            self.list_result.clone()
        }
    }

    /// 宿主应答假实现：记录应答参数与列表直查次数，可注入返回
    struct FakePeer {
        responses: Mutex<Vec<(String, bool)>>,
        list_calls: Mutex<usize>,
        list_result: Result<serde_json::Value, String>,
    }

    impl FakePeer {
        fn new() -> Self {
            Self {
                responses: Mutex::new(Vec::new()),
                list_calls: Mutex::new(0),
                list_result: Err("mock host list_trusted unavailable".to_string()),
            }
        }
    }

    impl PeerRespondOps for FakePeer {
        fn respond_consent(&self, request_id: &str, accepted: bool) -> Result<bool, String> {
            self.responses
                .lock()
                .unwrap()
                .push((request_id.to_string(), accepted));
            Ok(true)
        }

        fn list_trusted(&self) -> Result<serde_json::Value, String> {
            *self.list_calls.lock().unwrap() += 1;
            self.list_result.clone()
        }
    }

    // ==================== wire 解析 / 映射 ====================

    #[test]
    fn parse_consent_payload_valid_event() {
        // peer:consent 载荷（宿主事件桥 camelCase）→ 决策请求；无 userDecision
        let parsed = parse_consent_payload(&payload("req-1", NODE_A)).expect("parse");
        assert_eq!(parsed.request_id, "req-1");
        assert_eq!(parsed.node_id, NODE_A);
        assert_eq!(parsed.fingerprint_short.as_deref(), Some(&NODE_A[..8]));
        assert_eq!(parsed.device_name.as_deref(), Some("模拟对端"));
        assert_eq!(parsed.user_decision, None, "事件载荷不含用户意向");
    }

    #[test]
    fn parse_consent_payload_rejects_missing_identity() {
        // requestId / nodeId 必填（与认证中心同口径）：缺失或空 → None
        assert!(parse_consent_payload(&serde_json::json!({ "nodeId": NODE_A })).is_none());
        assert!(parse_consent_payload(&serde_json::json!({ "requestId": "req-1" })).is_none());
        assert!(parse_consent_payload(&serde_json::json!({
            "requestId": "",
            "nodeId": NODE_A,
        }))
        .is_none());
        assert!(parse_consent_payload(&serde_json::json!("garbage")).is_none());
    }

    #[test]
    fn user_decision_from_accepted_maps_bool() {
        assert_eq!(user_decision_from_accepted(true), UserDecision::Accept);
        assert_eq!(user_decision_from_accepted(false), UserDecision::Deny);
    }

    #[test]
    fn decide_accepts_connection_maps_decision() {
        // 决策 → 宿主应答布尔（对照：迁移前 accept↔true / deny↔false）
        assert_eq!(
            decide_accepts_connection(&decision(DecisionKind::Accept, "r")),
            Some(true)
        );
        assert_eq!(
            decide_accepts_connection(&decision(DecisionKind::Deny, "r")),
            Some(false)
        );
        assert_eq!(
            decide_accepts_connection(&decision(DecisionKind::Ask, "r")),
            None
        );
    }

    #[test]
    fn map_trusted_list_filters_peer_entries_to_old_wire() {
        // 统一视图（pairing + peer 混合）→ 仅 peer 段映射为旧 TrustedPeerDto 数组
        let view = serde_json::json!({
            "devices": [
                {
                    "kind": "pairing",
                    "id": "pair-1",
                    "name": "配对设备",
                    "fingerprint": "fp-pairing",
                    "addedAt": "2026-01-01T00:00:00+00:00",
                    "active": true,
                },
                {
                    "kind": "peer",
                    "id": NODE_A,
                    "name": "可信对端A",
                    "fingerprint": NODE_A,
                    "fingerprintShort": &NODE_A[..8],
                    "addedAt": "2026-09-19T08:00:00+00:00",
                    "active": true,
                },
                {
                    "kind": "peer",
                    "id": NODE_B,
                    "addedAt": "2026-09-19T09:00:00+00:00",
                    "active": true,
                },
            ],
            "peerError": null,
        });
        let mapped = map_trusted_list(&view).expect("map");
        let arr = mapped.as_array().expect("array");
        assert_eq!(arr.len(), 2, "pairing 条目不得并入旧 peer 列表");
        assert_eq!(arr[0]["nodeId"], NODE_A);
        assert_eq!(arr[0]["displayName"], "可信对端A");
        assert_eq!(arr[0]["fingerprintShort"], &NODE_A[..8]);
        assert_eq!(arr[0]["addedAt"], "2026-09-19T08:00:00+00:00");
        // 缺名/缺短指纹：displayName=null、短指纹以 id 前 8 位兜底
        assert_eq!(arr[1]["displayName"], serde_json::Value::Null);
        assert_eq!(arr[1]["fingerprintShort"], &NODE_B[..8]);
        assert_eq!(arr[1]["addedAt"], "2026-09-19T09:00:00+00:00");
    }

    #[test]
    fn map_trusted_list_peer_error_surfaces() {
        // peerError 透出（不静默吞错）：与迁移前直查宿主报错的场景等价
        let view = serde_json::json!({
            "devices": [],
            "peerError": "peer-net unavailable in headless context (no app_handle)",
        });
        let err = map_trusted_list(&view).unwrap_err();
        assert!(
            err.contains("peerError") && err.contains("unavailable"),
            "got: {err}"
        );
    }

    #[test]
    fn map_trusted_list_malformed_view_errors() {
        // 畸形视图（缺 devices 数组）显性失败
        assert!(map_trusted_list(&serde_json::json!({ "peerError": null })).is_err());
    }

    // ==================== 待确认登记表 ====================

    #[test]
    fn registry_roundtrip_and_consume() {
        // 本地队列直测核心（并行测试共享静态登记表，容量/淘汰断言须确定性）
        let req = ConsentRequest {
            request_id: "req-t1".to_string(),
            node_id: NODE_A.to_string(),
            fingerprint_short: Some(NODE_A[..8].to_string()),
            device_name: None,
            user_decision: None,
        };
        let mut q = VecDeque::new();
        remember_in(&mut q, req.clone());
        let taken = take_in(&mut q, "req-t1").expect("take");
        assert_eq!(taken, req);
        assert!(take_in(&mut q, "req-t1").is_none(), "消费性取出");
    }

    #[test]
    fn registry_dedupes_and_caps_fifo() {
        let mut q = VecDeque::new();
        for i in 0..PENDING_CAP + 4 {
            remember_in(
                &mut q,
                ConsentRequest {
                    request_id: format!("req-cap-{i}"),
                    node_id: NODE_A.to_string(),
                    fingerprint_short: None,
                    device_name: None,
                    user_decision: None,
                },
            );
        }
        assert_eq!(q.len(), PENDING_CAP, "容量封顶");
        // 同 id 重复登记被去重：不改变总量（req-cap-10 在淘汰窗内仍在队列）
        let before = q.len();
        remember_in(
            &mut q,
            ConsentRequest {
                request_id: "req-cap-10".to_string(),
                node_id: NODE_A.to_string(),
                fingerprint_short: None,
                device_name: None,
                user_decision: None,
            },
        );
        assert_eq!(q.len(), before, "去重幂等");
        // 超限 FIFO 淘汰最旧：req-cap-0 被逐出（未被重复登记回队），最新仍在
        assert!(take_in(&mut q, "req-cap-0").is_none(), "最旧应被淘汰");
        assert!(
            take_in(&mut q, &format!("req-cap-{}", PENDING_CAP + 3)).is_some(),
            "最新仍在"
        );
        assert_eq!(q.len(), PENDING_CAP - 1, "取出后同步收缩");
    }

    // ==================== 阶段 1 评估（对照测试：同一场景同一决策） ====================

    #[test]
    fn phase1_trusted_peer_auto_allows_without_ui() {
        // 场景：peer 已在可信列表 → 认证中心 accept(trusted) → 自动放行、
        // 不应弹窗（handled=true），宿主应答 allow
        let mut d = decision(DecisionKind::Accept, "req-p1");
        d.reason = Some("trusted".to_string());
        let gw = FakeGateway::new(vec![Ok(d)]);
        let peer = FakePeer::new();
        let (handled, allow) = run_phase1(&gw, &peer, &payload("req-p1", NODE_A));
        assert!(handled && allow, "已信任免确认：自动放行不弹窗");
        assert_eq!(
            peer.responses.lock().unwrap().as_slice(),
            &[("req-p1".to_string(), true)],
            "自动放行应答宿主 allow"
        );
        // 阶段 1 请求已登记（respond-consent 阶段 2 仍可找到对端信息）
        assert!(take_pending("req-p1").is_some());
    }

    #[test]
    fn phase1_unknown_peer_asks_without_respond() {
        // 场景：未知 peer 无意向 → ask → 弹窗（handled=false），不应应答宿主
        let gw = FakeGateway::new(vec![Ok(decision(DecisionKind::Ask, "req-p2"))]);
        let peer = FakePeer::new();
        let (handled, allow) = run_phase1(&gw, &peer, &payload("req-p2", NODE_A));
        assert!(!handled && !allow, "未知 peer → ask 弹窗");
        assert!(
            peer.responses.lock().unwrap().is_empty(),
            "ask 不触碰宿主应答"
        );
    }

    #[test]
    fn phase1_deny_responds_deny() {
        // 场景：认证中心裁决拒绝（阶段 1 理论不可达，防御处理）→ 应答 deny
        let gw = FakeGateway::new(vec![Ok(decision(DecisionKind::Deny, "req-p3"))]);
        let peer = FakePeer::new();
        let (handled, allow) = run_phase1(&gw, &peer, &payload("req-p3", NODE_A));
        assert!(handled && !allow);
        assert_eq!(
            peer.responses.lock().unwrap().as_slice(),
            &[("req-p3".to_string(), false)],
            "拒绝应答宿主 deny"
        );
    }

    #[test]
    fn phase1_auth_center_down_falls_back_to_ask() {
        // 场景：认证中心不可用（未激活/超时/门禁拒绝）→ 降级照旧弹窗
        // （双轨无单点；对照：迁移前此场景直接弹窗，行为等价）
        let gw = FakeGateway::new(vec![Err("bus error: api not declared (gate)".to_string())]);
        let peer = FakePeer::new();
        let (handled, allow) = run_phase1(&gw, &peer, &payload("req-p4", NODE_A));
        assert!(!handled && !allow, "降级：照旧弹窗询问");
        assert!(peer.responses.lock().unwrap().is_empty());
    }

    #[test]
    fn phase1_malformed_payload_asks_without_registry() {
        // 畸形载荷：不登记、不调认证中心，照旧弹窗（前端解析有同等保护）
        let gw = FakeGateway::new(vec![]);
        let peer = FakePeer::new();
        let (handled, allow) = run_phase1(&gw, &peer, &serde_json::json!({ "nodeId": NODE_A }));
        assert!(!handled && !allow);
        assert!(gw.received.lock().unwrap().is_empty(), "畸形载荷不触发互调");
        assert!(take_pending("req-x").is_none());
    }

    // ==================== 阶段 2 决策（对照测试：同一场景同一决策） ====================

    #[test]
    fn phase2_explicit_accept_applies_allow() {
        // 场景：用户接受 → 认证中心 accept(explicit) → 应答宿主 allow，返回 hit
        let mut d = decision(DecisionKind::Accept, "req-d1");
        d.reason = Some("explicit".to_string());
        // 阶段 1（登记）脚本 ask：未知 peer 预检 → 弹窗；阶段 2 脚本 accept(explicit)
        let mut ask = decision(DecisionKind::Ask, "req-d1");
        ask.reason = None;
        let gw = FakeGateway::new(vec![Ok(ask), Ok(d)]);
        let peer = FakePeer::new();
        run_phase1(&gw, &peer, &payload("req-d1", NODE_A)); // 登记（ask 不触碰应答）
        let hit = run_decide(&gw, &peer, "req-d1", true).expect("decide");
        assert!(hit, "命中待确认项");
        assert_eq!(
            peer.responses.lock().unwrap().as_slice(),
            &[("req-d1".to_string(), true)],
            "accept → 宿主 allow（对照：迁移前 accepted=true → respond true）"
        );
        // 阶段 2 请求携带用户意向 + 登记的对端信息（网关收到的请求断言）
        let received = gw.received.lock().unwrap();
        let sent = received.last().unwrap();
        assert_eq!(sent.user_decision, Some(UserDecision::Accept));
        assert_eq!(sent.node_id, NODE_A);
    }

    #[test]
    fn phase2_explicit_deny_applies_deny() {
        let gw = FakeGateway::new(vec![
            Ok(decision(DecisionKind::Ask, "req-d2")),
            Ok(decision(DecisionKind::Deny, "req-d2")),
        ]);
        let peer = FakePeer::new();
        run_phase1(&gw, &peer, &payload("req-d2", NODE_A)); // 登记
        let hit = run_decide(&gw, &peer, "req-d2", false).expect("decide");
        assert!(hit);
        assert_eq!(
            peer.responses.lock().unwrap().as_slice(),
            &[("req-d2".to_string(), false)],
            "deny → 宿主 deny（对照：迁移前 accepted=false → respond false）"
        );
        let received = gw.received.lock().unwrap();
        let sent = received.last().unwrap();
        assert_eq!(sent.user_decision, Some(UserDecision::Deny));
    }

    #[test]
    fn phase2_auth_center_down_falls_back_to_direct_respond() {
        // 场景：认证中心不可用 → 双轨降级直答宿主（对照：迁移前 respond-consent
        // 直答宿主，同一场景同一决策）。阶段 1/2 均告失败（未激活/门禁拒绝）
        let gw = FakeGateway::new(vec![
            Err("gate: api not declared".to_string()),
            Err("gate: api not declared".to_string()),
        ]);
        let peer = FakePeer::new();
        run_phase1(&gw, &peer, &payload("req-d3", NODE_A)); // 登记（降级弹窗）
        let hit = run_decide(&gw, &peer, "req-d3", true).expect("decide");
        assert!(hit);
        assert_eq!(
            peer.responses.lock().unwrap().as_slice(),
            &[("req-d3".to_string(), true)],
            "降级：直答宿主（accepted 原样透传）"
        );
    }

    #[test]
    fn phase2_unknown_request_falls_back_to_direct_respond() {
        // 场景：无登记请求（事件丢失 / 插件重启）→ 直答宿主（旧行为），
        // 不调认证中心（无对端信息可托付决策）
        let gw = FakeGateway::new(vec![]);
        let peer = FakePeer::new();
        let hit = run_decide(&gw, &peer, "req-unknown", false).expect("decide");
        assert!(hit);
        assert_eq!(
            peer.responses.lock().unwrap().as_slice(),
            &[("req-unknown".to_string(), false)],
            "降级：直答宿主"
        );
        assert!(
            gw.received.lock().unwrap().is_empty(),
            "无登记请求不触发互调"
        );
    }

    #[test]
    fn phase2_ask_anomaly_errors_not_silent() {
        // 场景：带显式意向仍返回 ask（协议异常，理论上不可能）→ 显性报错，
        // 不猜测用户意图也不静默放行
        let gw = FakeGateway::new(vec![
            Ok(decision(DecisionKind::Ask, "req-d4")),
            Ok(decision(DecisionKind::Ask, "req-d4")),
        ]);
        let peer = FakePeer::new();
        run_phase1(&gw, &peer, &payload("req-d4", NODE_A)); // 登记
        let err = run_decide(&gw, &peer, "req-d4", true).expect_err("ask 必须报错");
        assert!(err.contains("req-d4"), "got: {err}");
        assert!(peer.responses.lock().unwrap().is_empty(), "不吞错也不应答");
    }

    // ==================== 信任列表 ====================

    #[test]
    fn list_trusted_maps_through_auth_center() {
        let view = serde_json::json!({
            "devices": [{
                "kind": "peer",
                "id": NODE_A,
                "name": "可信对端A",
                "fingerprintShort": &NODE_A[..8],
                "addedAt": "2026-09-19T08:00:00+00:00",
                "active": true,
            }],
            "peerError": null,
        });
        let mut gw = FakeGateway::new(vec![]);
        gw.list_result = Ok(view);
        let peer = FakePeer::new();
        let mapped = run_list_trusted(&gw, &peer).expect("list");
        let arr = mapped.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["nodeId"], NODE_A);
        // 认证中心可用时不得回落到宿主（peer 应答面未被触碰）
        assert_eq!(*peer.list_calls.lock().unwrap(), 0, "不得降级直查宿主");
    }

    #[test]
    fn list_trusted_auth_center_down_falls_back_to_host() {
        let gw = FakeGateway::new(vec![]); // list_result 默认 Err
        let peer = FakePeer::new();
        let err =
            run_list_trusted(&gw, &peer).expect_err("认证中心不可用 → 宿主也报错（同场景等价）");
        assert!(err.contains("mock host"), "降级直查宿主错误透出: {err}");
        assert_eq!(*peer.list_calls.lock().unwrap(), 1, "降级路径直查宿主一次");
    }

    #[test]
    fn list_trusted_auth_center_down_host_succeeds() {
        let gw = FakeGateway::new(vec![]); // list_result 默认 Err
        let mut peer = FakePeer::new();
        peer.list_result = Ok(serde_json::json!([{
            "nodeId": NODE_A,
            "displayName": "宿主对端",
            "fingerprintShort": &NODE_A[..8],
            "addedAt": "2026-09-19T08:00:00+00:00",
        }]));
        let mapped = run_list_trusted(&gw, &peer).expect("fallback list");
        let arr = mapped.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["nodeId"], NODE_A, "降级直查宿主（迁移前 wire）");
        assert_eq!(*peer.list_calls.lock().unwrap(), 1);
    }
}
