//! 会话动作域（票 10）：重启 / 移除 / 改名 / 尺寸裁决
//!
//! 职责边界（spec D3/D4）——「会话动作怎么走」这条决策线：
//!
//! - **在插件**：动作编排（先校验会话存在性再动作 → 失败同步可见；调用顺序）；
//!   **尺寸裁决规则**——正统端判定（无归属 / 单端 / 多端争用）与覆盖确认策略
//!   （何时回执 `needsConfirmation` 而不落任何改动）；**重启编排**——插件读自身
//!   配置真源算 launch spec，走「`remove` 旧会话 + 以同一 id `create-with-spec`」两步
//!   （host-business-decarriage 收尾：不再依赖内核 `restart` 读主库配置投影）
//! - **留宿主**：执行器与登记事实——`host-session` 的 `remove` / `rename` / `resize`
//!   原语（`resize` **只登记与执行、不裁决**）与 `create-with-spec` 的 id 冲突仲裁；
//!   「谁是当前正统渲染端」的登记事实经既有 `get` 原语的 `canonicalRenderer` 字段读取
//!
//! 移动端 HTTP / WS 路径与插件未激活时的降级轨仍直连宿主 `SessionManager`
//! 执行器（宿主裁决分支保留，双轨无单点）——故本模块的规则迁移不改变今天
//! 「桌面 resize 需确认覆盖」的外部行为。
//!
//! 模块构成：
//! - [`RendererSource`] / [`ResizeOutcome`]：与宿主 serde 同形的 wire 模型
//! - [`decide_resize`]：裁决规则（纯函数，native 单测覆盖四态）
//! - `*_via_host`：wasm 编排入口（native 显性失败）；请求解析与规则纯逻辑
//!   在 native 单测全覆盖

// ==================== wire 模型（与宿主 serde 同形） ====================

/// 正统渲染端身份（wire 形状 = 宿主 `RendererSource`：
/// `{"kind":"desktop"}` / `{"kind":"mobile","deviceName":"Pixel"}`）
///
/// 字段名 camelCase 必须与宿主一致——`rename_all_fields` 是必须的：容器级
/// `rename_all` 只作用于变体名（tag 值），漏了它 `device_name` 会以 snake_case
/// 出网，宿主反序列化即失败（宿主侧同一坑的历史事故见 session_components.rs）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RendererSource {
    /// 桌面端（会话宿主：本地命令 / 本地环回 WS）
    Desktop,
    /// 移动端设备（设备名来自 JWT claims）
    Mobile { device_name: String },
}

/// resize 裁决结果（wire 形状 = 宿主 `ResizeOutcome`，宿主命令面直接反序列化）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ResizeOutcome {
    /// 已应用：调用方是正统端，或覆盖确认通过（force）
    Applied { canonical: RendererSource },
    /// 需确认：另一个端正在渲染输出，本次**未应用任何改动**；
    /// 客户端弹窗确认后带 `force` 重发
    NeedsConfirmation { current_canonical: RendererSource },
}

// ==================== 尺寸裁决规则（纯函数，四态） ====================

/// 裁决决策（规则产物；执行结果另以 [`ResizeOutcome`] 回执）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResizeDecision {
    /// 判定可应用：调用方随后经 `host-session.resize` 执行 + 登记
    Apply,
    /// 判定需覆盖确认：**不调用原语**（零改动），把当前正统端回执给调用方
    NeedsConfirmation { current: RendererSource },
}

/// 正统渲染端裁决规则（spec D3：规则归插件，登记事实归内核）
///
/// 四态（`current` = 内核登记事实）：
///
/// 1. **无渲染端**（`None`）：首个请求方即位正统 → 应用
/// 2. **单端**（`Some(当前) == 请求方`）：请求方即正统端 → 应用
/// 3. **多端争用**（`Some(其他端)` 且未 `force`）：不应用 → 需覆盖确认
/// 4. **端接管**（`Some(其他端)` 且 `force`）：用户已确认覆盖（或高位端主动接管）
///    → 应用并移交归属
///
/// 第 4 态当前以「显式 `force`」为接管信号：原正统端**是否已下线**的判据需要
/// 连接清单（spec D4 归票 11 的 `connections-list`），本票不臆测存活状态——
/// 保守地把「无法判定」一律按「仍在渲染」处理（要求确认），绝不静默抢占在线端。
/// 票 11 落 `connections-list` 后由本规则消费该事实（届时端下线 → 直接接管）。
pub fn decide_resize(
    current: Option<&RendererSource>,
    requester: &RendererSource,
    force: bool,
) -> ResizeDecision {
    match current {
        // 无归属（首次设置者即位正统）或归属 = 请求方：直接应用
        None => ResizeDecision::Apply,
        Some(c) if c == requester => ResizeDecision::Apply,
        // 归属 ≠ 请求方：未确认覆盖 → 需确认（不落任何改动）
        Some(c) if !force => ResizeDecision::NeedsConfirmation { current: c.clone() },
        // force：覆盖应用并移交归属
        Some(_) => ResizeDecision::Apply,
    }
}

/// 从 `host-session.get` 回执中读取登记事实（票 10 起的 `canonicalRenderer` 字段）
///
/// - 字段缺失 / `null` → `None`（无归属：宿主未提供登记事实时按无归属处理）
/// - 字段存在但形状非法 → 显性报错（不静默降级为「无归属」，否则会误抢占）
pub fn parse_canonical(session_json: &serde_json::Value) -> Result<Option<RendererSource>, String> {
    match session_json.get("canonicalRenderer") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| format!("invalid canonicalRenderer in session payload: {}", e)),
    }
}

// ==================== 请求解析（camelCase wire） ====================

/// 单个会话目标（重启 / 移除共用）
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTargetRequest {
    pub session_id: String,
}

impl SessionTargetRequest {
    pub fn parse(value: &serde_json::Value) -> Result<Self, String> {
        serde_json::from_value(value.clone()).map_err(|e| format!("invalid request: {}", e))
    }
}

/// 改名请求
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRequest {
    pub session_id: String,
    pub name: String,
}

impl RenameRequest {
    pub fn parse(value: &serde_json::Value) -> Result<Self, String> {
        serde_json::from_value(value.clone()).map_err(|e| format!("invalid request: {}", e))
    }
}

/// 尺寸调整请求（`force` = 覆盖确认已通过；缺省 false）
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeRequest {
    pub session_id: String,
    pub cols: u16,
    pub rows: u16,
    /// 请求方身份（宿主恒传：桌面命令面 = desktop；移动端经宿主降级轨不走本编排）
    pub requester: RendererSource,
    #[serde(default)]
    pub force: bool,
}

impl ResizeRequest {
    pub fn parse(value: &serde_json::Value) -> Result<Self, String> {
        serde_json::from_value(value.clone()).map_err(|e| format!("invalid request: {}", e))
    }
}

// ==================== Tests（规则与解析：纯逻辑，native 全覆盖） ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop() -> RendererSource {
        RendererSource::Desktop
    }

    fn mobile(name: &str) -> RendererSource {
        RendererSource::Mobile {
            device_name: name.to_string(),
        }
    }

    // ==================== 四态裁决矩阵 ====================

    /// 态 1 无渲染端：首个请求方即位正统（不弹确认）
    #[test]
    fn resize_state_no_renderer_claims() {
        assert_eq!(
            decide_resize(None, &desktop(), false),
            ResizeDecision::Apply
        );
        assert_eq!(
            decide_resize(None, &mobile("Pixel"), false),
            ResizeDecision::Apply
        );
        // 无归属时 force 与不 force 同解（没有可覆盖的归属）
        assert_eq!(decide_resize(None, &desktop(), true), ResizeDecision::Apply);
    }

    /// 态 2 单端：请求方就是正统端 → 直接应用
    #[test]
    fn resize_state_single_renderer_applies() {
        let me = mobile("Pixel");
        assert_eq!(decide_resize(Some(&me), &me, false), ResizeDecision::Apply);
        assert_eq!(
            decide_resize(Some(&desktop()), &desktop(), false),
            ResizeDecision::Apply
        );
    }

    /// 态 3 多端争用：归属在其他端且未确认 → 需覆盖确认（**且不改动任何东西**）
    #[test]
    fn resize_state_multi_renderer_needs_confirmation() {
        let other = mobile("Pixel");
        assert_eq!(
            decide_resize(Some(&other), &desktop(), false),
            ResizeDecision::NeedsConfirmation {
                current: other.clone()
            }
        );
        assert_eq!(
            decide_resize(Some(&desktop()), &mobile("Redmi"), false),
            ResizeDecision::NeedsConfirmation { current: desktop() }
        );
    }

    /// 态 4 端接管：覆盖确认通过（force）→ 应用并移交归属
    #[test]
    fn resize_state_takeover_with_force() {
        let other = mobile("Pixel");
        assert_eq!(
            decide_resize(Some(&other), &desktop(), true),
            ResizeDecision::Apply
        );
        assert_eq!(
            decide_resize(Some(&desktop()), &mobile("Redmi"), true),
            ResizeDecision::Apply
        );
    }

    // ==================== 登记事实读取 ====================

    /// 登记事实：desktop / mobile / null / 缺字段
    #[test]
    fn parse_canonical_reads_host_fact() {
        assert_eq!(
            parse_canonical(
                &serde_json::json!({"id": "s1", "canonicalRenderer": {"kind": "desktop"}})
            )
            .expect("desktop"),
            Some(desktop())
        );
        assert_eq!(
            parse_canonical(
                &serde_json::json!({"canonicalRenderer": {"kind": "mobile", "deviceName": "Pixel"}})
            )
            .expect("mobile"),
            Some(mobile("Pixel"))
        );
        assert_eq!(
            parse_canonical(&serde_json::json!({"canonicalRenderer": null})).expect("null"),
            None
        );
        assert_eq!(
            parse_canonical(&serde_json::json!({"id": "s1"})).expect("absent"),
            None
        );
    }

    /// 形状非法 → 显性报错（不静默降级为「无归属」，否则会误抢占他端）
    #[test]
    fn parse_canonical_rejects_malformed_shape() {
        let err = parse_canonical(&serde_json::json!({"canonicalRenderer": {"kind": "tablet"}}))
            .expect_err("must reject");
        assert!(
            err.contains("invalid canonicalRenderer"),
            "unexpected: {err}"
        );
    }

    // ==================== wire 形状锁定（与宿主 serde 逐字对齐） ====================

    /// 关键回归：`deviceName` 必须 camelCase 出网（宿主按同形反序列化）
    #[test]
    fn renderer_source_wire_shape_is_camel_case() {
        let json = serde_json::to_value(mobile("Pixel 9")).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({"kind": "mobile", "deviceName": "Pixel 9"})
        );
        assert_eq!(
            serde_json::to_value(desktop()).expect("serialize"),
            serde_json::json!({"kind": "desktop"})
        );
        // 反序列化同形（宿主回执 → 本插件模型）
        let back: RendererSource = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, mobile("Pixel 9"));
    }

    /// 回执形状锁定：宿主命令面直接反序列化为 `ResizeOutcome`
    #[test]
    fn resize_outcome_wire_shape() {
        assert_eq!(
            serde_json::to_value(ResizeOutcome::Applied {
                canonical: desktop()
            })
            .expect("serialize"),
            serde_json::json!({"status": "applied", "canonical": {"kind": "desktop"}})
        );
        assert_eq!(
            serde_json::to_value(ResizeOutcome::NeedsConfirmation {
                current_canonical: mobile("Pixel")
            })
            .expect("serialize"),
            serde_json::json!({
                "status": "needsConfirmation",
                "currentCanonical": {"kind": "mobile", "deviceName": "Pixel"}
            })
        );
    }

    // ==================== 请求解析 ====================

    #[test]
    fn resize_request_parse_defaults_force_false() {
        let req = ResizeRequest::parse(&serde_json::json!({
            "sessionId": "s1", "cols": 120, "rows": 40, "requester": {"kind": "desktop"}
        }))
        .expect("parse");
        assert_eq!(req.session_id, "s1");
        assert_eq!((req.cols, req.rows), (120, 40));
        assert_eq!(req.requester, desktop());
        assert!(!req.force, "force 缺省 false（未确认不得覆盖他端）");
    }

    #[test]
    fn resize_request_parse_force_and_mobile_requester() {
        let req = ResizeRequest::parse(&serde_json::json!({
            "sessionId": "s1", "cols": 80, "rows": 24,
            "requester": {"kind": "mobile", "deviceName": "Pixel"}, "force": true
        }))
        .expect("parse");
        assert!(req.force);
        assert_eq!(req.requester, mobile("Pixel"));
    }

    #[test]
    fn requests_parse_rejects_missing_fields() {
        assert!(ResizeRequest::parse(&serde_json::json!({"cols": 1, "rows": 1})).is_err());
        assert!(
            ResizeRequest::parse(&serde_json::json!({
                "sessionId": "s1", "cols": 1, "rows": 1
            }))
            .is_err(),
            "缺 requester 必须报错（不能默认成桌面端冒充请求方）"
        );
        assert!(RenameRequest::parse(&serde_json::json!({"sessionId": "s1"})).is_err());
        assert!(SessionTargetRequest::parse(&serde_json::json!({})).is_err());
    }
}

// ==================== 编排入口（wasm 运行时薄包装，native 显性失败） ====================
//
// lib.rs 的互调 api 面只调这些入口。native（cargo test）下 `WasmHost` 没有
// `HostSession` impl（wasm 专属 import 符号不在 native 链接），故 native 分支
// 显性失败——与 `config/mod.rs` / `launch.rs` 同模式。规则与解析的纯逻辑已在上面
// native 单测全覆盖，这里只是「读事实 → 判定 → 调原语」的调用编排。

#[cfg(target_arch = "wasm32")]
use crate::config::store::ConfigStore;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostEvents, HostLog, HostSession};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 前端「重启完成」事件名（与宿主 `system/constants/event.rs::SESSION_RESTARTED`
/// 逐字一致；前端 `useSessionStatusListener` 监听同名事件）
#[cfg(target_arch = "wasm32")]
const EVENT_SESSION_RESTARTED: &str = "session-restarted";

/// 待补发「重启完成」事件的会话：`session-id → 重启前会话名`
///
/// 重启 = 「remove 旧会话 + 以同一 id 重建」两步，而前端 `session-restarted` 必须
/// 在会话**真正就绪后**发（内核执行器的旧实现就是最后一步广播）——就绪信号取
/// Created 生命周期事件，故在此登记、由 [`flush_pending_restart`] 消费并发射。
/// 条目仅在宿主异步创建失败时残留（该失败无同步返回通道），体量极小且按 id 覆盖。
#[cfg(target_arch = "wasm32")]
static PENDING_RESTART: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Created 生命周期到达时补发重启完成事件（lib.rs 的 Created 分支调用）
///
/// 载荷形状与宿主 `SessionRestartEvent`（camelCase）逐字一致——重启保持同一 id，
/// 故 `oldSessionId == newSessionId`，与迁移前的前端可观察结果相同。
#[cfg(target_arch = "wasm32")]
pub fn flush_pending_restart(host: &WasmHost, session_id: &str) {
    let name = {
        let mut pending = PENDING_RESTART.lock().unwrap_or_else(|e| e.into_inner());
        match pending.iter().position(|(id, _)| id == session_id) {
            Some(idx) => Some(pending.remove(idx).1),
            None => None,
        }
    };
    let Some(name) = name else { return };
    host.emit_event(
        EVENT_SESSION_RESTARTED,
        &serde_json::json!({
            "oldSessionId": session_id,
            "newSessionId": session_id,
            "sessionName": name,
        }),
    );
    host.log_debug(&format!(
        "restart: session-restarted emitted after Created, session_id={}",
        session_id
    ));
}

/// 重启编排：存在性预检 → 读配置真源算 spec → `remove` 旧会话 → 同 id `create-with-spec`
///
/// 外部行为与内核执行器逐字等价：同一 session id（线协议与终端订阅键不变）、
/// 名字与 configId 保持、正统渲染端归属回到桌面端（spec 不带 `sourceDevice`）、
/// 生命周期事件序列 Creating → Created、同步事件 SessionRemoved → SessionCreated。
/// 唯一差别是**配置来源**：插件私有库（真源）而不是主库投影——这也是本迁移的目的。
#[cfg(target_arch = "wasm32")]
pub fn restart_via_host(draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request = SessionTargetRequest::parse(draft_json)?;
    if request.session_id.trim().is_empty() {
        return Err("empty sessionId".to_string());
    }
    // 编排职责 1：存在性预检——「会话不存在」在此同步可见（重建为宿主异步执行）
    let session = read_session(&request.session_id)?;
    let config_id = session
        .get("configId")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let name = session
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if config_id.is_empty() {
        return Err(format!("会话缺少 configId，无法重建：{}", request.session_id));
    }
    // 编排职责 2：配置真源在插件私有库 → 同一 id 的 launch spec（沿用旧名，不重命名）
    let config = WasmHost
        .get(&config_id)
        .map_err(|e| format!("config read failed: {}", e))?
        .ok_or_else(|| format!("会话配置不存在：{}", config_id))?;
    let mut spec = crate::launch::build_launch_spec(&config, None, None, true)?;
    spec.name = name.clone();
    spec.session_id = Some(request.session_id.clone());
    // 编排职责 3：先摘除旧会话（内核执行器同序；SessionRemoved 同步事件形状不变）
    WasmHost
        .session_remove(&request.session_id)
        .map_err(|e| format!("host remove failed: {}", e.message))?;
    // 编排职责 4：同 id 重建（宿主异步执行 + id 冲突仲裁；此处已先行摘除故不冲突）
    let spec_json =
        serde_json::to_value(&spec).map_err(|e| format!("launch spec serialize failed: {}", e))?;
    let created = WasmHost
        .session_create_with_spec(&spec_json)
        .map_err(|e| format!("host create-with-spec failed: {}", e.message))?;
    if created != request.session_id {
        return Err(format!(
            "重启未保持同一 session id：created={} expected={}",
            created, request.session_id
        ));
    }
    // 编排职责 5：登记待补发前端事件（Created 到达即发，与内核执行器同序）
    {
        let mut pending = PENDING_RESTART.lock().unwrap_or_else(|e| e.into_inner());
        pending.retain(|(id, _)| id != &request.session_id);
        pending.push((request.session_id.clone(), name.clone()));
    }
    Ok(serde_json::json!({
        "sessionId": request.session_id,
        "name": name,
    }))
}

/// 移除编排：存在性预检 → `host-session.remove`（同步执行，失败可见）
#[cfg(target_arch = "wasm32")]
pub fn remove_via_host(draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request = SessionTargetRequest::parse(draft_json)?;
    if request.session_id.trim().is_empty() {
        return Err("empty sessionId".to_string());
    }
    read_session(&request.session_id)?;
    WasmHost
        .session_remove(&request.session_id)
        .map_err(|e| format!("host remove failed: {}", e.message))?;
    Ok(serde_json::json!({ "sessionId": request.session_id, "removed": true }))
}

/// 停止编排（票 13）：存在性预检 → `host-session.close`（宿主异步执行：停止 PTY
/// 并置 `Stopped`，会话记录保留；与用户手动关闭一致）。
///
/// 与 restart 同口径：原语异步执行无同步失败通道，「会话不存在」由插件预检变成
/// 同步可见的失败；会话表单据此关闭对应终端窗口。
#[cfg(target_arch = "wasm32")]
pub fn close_via_host(draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request = SessionTargetRequest::parse(draft_json)?;
    if request.session_id.trim().is_empty() {
        return Err("empty sessionId".to_string());
    }
    read_session(&request.session_id)?;
    WasmHost
        .session_close(&request.session_id)
        .map_err(|e| format!("host close failed: {}", e.message))?;
    Ok(serde_json::json!({ "sessionId": request.session_id, "stopped": true }))
}

/// 改名编排（名字合法性由宿主原语最终仲裁，此处只做空值 UX 校验）
#[cfg(target_arch = "wasm32")]
pub fn rename_via_host(draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request = RenameRequest::parse(draft_json)?;
    if request.session_id.trim().is_empty() {
        return Err("empty sessionId".to_string());
    }
    if request.name.trim().is_empty() {
        return Err("empty session name".to_string());
    }
    read_session(&request.session_id)?;
    let previous = WasmHost
        .session_rename(&request.session_id, &request.name)
        .map_err(|e| format!("host rename failed: {}", e.message))?;
    Ok(serde_json::json!({
        "sessionId": request.session_id,
        "name": request.name,
        "previousName": previous,
    }))
}

/// 尺寸裁决编排：读登记事实 → 插件侧裁决 → 仅「可应用」时调用 `host-session.resize`
///
/// 需确认时**不触碰任何状态**（零改动），把当前正统端回执给调用方弹窗确认；
/// 确认后调用方带 `force` 重发，走接管分支。
#[cfg(target_arch = "wasm32")]
pub fn resize_via_host(draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request = ResizeRequest::parse(draft_json)?;
    if request.session_id.trim().is_empty() {
        return Err("empty sessionId".to_string());
    }
    if request.cols == 0 || request.rows == 0 {
        return Err("invalid size (cols/rows must be > 0)".to_string());
    }
    let session = read_session(&request.session_id)?;
    let current = parse_canonical(&session)?;
    match decide_resize(current.as_ref(), &request.requester, request.force) {
        ResizeDecision::NeedsConfirmation { current } => {
            Ok(serde_json::to_value(ResizeOutcome::NeedsConfirmation {
                current_canonical: current,
            })
            .map_err(|e| format!("outcome serialize failed: {}", e))?)
        }
        ResizeDecision::Apply => {
            let applied = WasmHost
                .session_resize(
                    &request.session_id,
                    request.cols,
                    request.rows,
                    &serde_json::json!(&request.requester),
                )
                .map_err(|e| format!("host resize failed: {}", e.message))?;
            // 归属以宿主登记事实为准（原语回执 canonical）：不臆造执行结果
            let canonical = applied
                .get("canonical")
                .cloned()
                .ok_or_else(|| format!("host resize reply missing canonical: {}", applied))
                .and_then(|v| {
                    serde_json::from_value::<RendererSource>(v)
                        .map_err(|e| format!("invalid canonical in resize reply: {}", e))
                })?;
            Ok(serde_json::to_value(ResizeOutcome::Applied { canonical })
                .map_err(|e| format!("outcome serialize failed: {}", e))?)
        }
    }
}

/// 读单个会话（不存在 → 显性报错：所有动作的可见失败面）
#[cfg(target_arch = "wasm32")]
fn read_session(session_id: &str) -> Result<serde_json::Value, String> {
    WasmHost
        .session_get(session_id)
        .map_err(|e| format!("session read failed: {}", e.message))?
        .ok_or_else(|| format!("会话不存在：{}", session_id))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn restart_via_host(_draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session restart unavailable outside wasm runtime".to_string())
}

/// native 无宿主事件面：补发重启完成事件是 wasm 专属（native 生命周期回调由测试
/// 直接驱动，不需要前端事件）
#[cfg(not(target_arch = "wasm32"))]
pub fn flush_pending_restart(_host: &bedcode_plugin_api::wasm_host::WasmHost, _session_id: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn remove_via_host(_draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session remove unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn close_via_host(_draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session close unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn rename_via_host(_draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session rename unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn resize_via_host(_draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("session resize unavailable outside wasm runtime".to_string())
}
