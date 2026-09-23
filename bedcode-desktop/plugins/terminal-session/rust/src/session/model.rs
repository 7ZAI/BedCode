//! 会话登记域 wire / 存储模型（会话引擎整体下沉 P1）
//!
//! **wire 形状对齐红线**：`SessionStatus` 与宿主 `enums::session::SessionStatus`
//! 的 serde 形状逐字相同（externally tagged：`"starting"` / `"running"` /
//! `"stopped"` / `{"error":"…"}`）——P1 后续阶段本域要经互调 api 向宿主与移动端
//! 供会话事实，形状漂移即线协议静默错位。形状锁定用例见本文件 `tests`。
//!
//! 存储映射（snake_case 列 ↔ camelCase 字段）只存在于 [`super::store`] 的 wasm
//! 实现里；本文件是纯数据类型，native 单测可全覆盖。

use serde::{Deserialize, Serialize};

/// 会话状态（宿主 `SessionStatus` 的 wire 镜像）
///
/// 本域当前**只生产** `Starting` / `Running` / `Stopping` / `Stopped` / `Error`
/// 五态（状态机见 [`super::ops::transition`]）。`Idle` / `WaitingInput` 是宿主与
/// 移动端既有的线协议取值，本域保留同形变体：P1 后续阶段改由本域供事实时，
/// 形状不得缺格，否则宿主反序列化即失败。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    /// 空闲（移动端语义，本域不生产）
    Idle,
    /// 启动中（两阶段启动的第一阶段：`start = false`）
    Starting,
    /// 运行中
    Running,
    /// 等待输入（移动端语义，本域不生产）
    WaitingInput,
    /// 停止中（`Stopping` 生命周期到达后、`Stopped` 之前的过渡态）
    Stopping,
    /// 已停止（终态）
    Stopped,
    /// 出错（终态，可带错误描述）
    Error(Option<String>),
}

impl SessionStatus {
    /// 终态判据：终态不再接受任何迁移（唯一例外是幂等重复写同值）
    pub fn is_terminal(&self) -> bool {
        matches!(self, SessionStatus::Stopped | SessionStatus::Error(_))
    }
}

/// 会话记录（本插件私有库 `sessions` 表的一行）
///
/// 与宿主 `SessionInfo` 的差异是**刻意的、增量式**的：
/// - 多 `ptyId`：P1 后续阶段会话改由本插件经 `host-pty.spawn` 创建，PTY 句柄
///   （`pty-<uuid>`）是插件的自有事实；今天仍由宿主预生成会话 id，故为 `None`；
/// - 多 `canonicalRenderer` / `owner` / `updatedAt`：宿主分别存在独立的
///   `CanonicalRendererRegistry` / `session_owners` 表与 `SessionInfo` 之外；
/// - 不含 `sessionType`：宿主该字段恒为 `pty`（写死），无真源价值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    /// 会话标识（线协议与终端订阅键；重启保持同一 id）
    pub id: String,
    /// 自有 PTY 句柄（P1 后续阶段由本插件 `host-pty.spawn` 产出；今天为 `None`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pty_id: Option<String>,
    /// 来源配置 id（`session_configs.id`；无配置直接启动时为空串）
    pub config_id: String,
    /// 展示名（唯一化由 `launch::generate_unique_name` 决策）
    pub name: String,
    /// 状态（写库存 JSON 文本，保 `Error` 载荷无损）
    pub status: SessionStatus,
    /// 记录创建时间（RFC3339 UTC，秒级）
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_at: Option<String>,
    /// 正统渲染端归属（尺寸裁决的登记事实；`None` = 无归属）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_renderer: Option<crate::actions::RendererSource>,
    /// 创建方插件 id（`None` = 宿主自建，插件一律不可操作）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// 最后一次变更时间（RFC3339 UTC，秒级）
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 状态 wire 形状锁定：宿主 `SessionStatus`（externally tagged camelCase）
    /// 的逐字形态——`{"error": …}` 是宿主 `Error(Option<String>)` 的 serde 产物
    #[test]
    fn status_wire_shape_matches_host_enum() {
        let cases = [
            (SessionStatus::Idle, serde_json::json!("idle")),
            (SessionStatus::Starting, serde_json::json!("starting")),
            (SessionStatus::Running, serde_json::json!("running")),
            (SessionStatus::WaitingInput, serde_json::json!("waitingInput")),
            (SessionStatus::Stopping, serde_json::json!("stopping")),
            (SessionStatus::Stopped, serde_json::json!("stopped")),
            (
                SessionStatus::Error(Some("pty closed".to_string())),
                serde_json::json!({ "error": "pty closed" }),
            ),
            (SessionStatus::Error(None), serde_json::json!({ "error": null })),
        ];
        for (status, expected) in cases {
            let json = serde_json::to_value(&status).expect("serialize status");
            assert_eq!(json, expected, "状态 wire 形状必须与宿主一致");
            let back: SessionStatus = serde_json::from_value(json).expect("deserialize status");
            assert_eq!(back, status, "状态可往返");
        }
    }

    /// 存储文本形态同样是无损往返的（写库存 JSON 文本的理由）
    #[test]
    fn status_text_roundtrip_is_lossless() {
        let status = SessionStatus::Error(Some("boom".to_string()));
        let text = serde_json::to_string(&status).expect("serialize");
        assert_eq!(text, r#"{"error":"boom"}"#);
        let back: SessionStatus = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, status);
    }

    /// 终态判据
    #[test]
    fn terminal_states_are_stopped_and_error() {
        assert!(SessionStatus::Stopped.is_terminal());
        assert!(SessionStatus::Error(None).is_terminal());
        for status in [
            SessionStatus::Idle,
            SessionStatus::Starting,
            SessionStatus::Running,
            SessionStatus::WaitingInput,
            SessionStatus::Stopping,
        ] {
            assert!(!status.is_terminal(), "{status:?} 不是终态");
        }
    }
}
