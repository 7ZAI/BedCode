//! 宿主能力：系统通知与提醒反馈
//!
//! 2026-10-13 功能级统一（ADR 0052 D3 / domain-owned-contracts 票 04）后分两个
//! interface：
//! - **`host-notify`（共享域契约，真源 `packages/bedcode-notify-engine/wit/notify.wit`）**：
//!   `notify` 本体是双端功能级交集 → 统一签名 `notify(title, body, options)`，
//!   桌面/移动各消费同一份契约；移动端权限位 `notify`（fail-closed）。
//! - **`host-notify-mobile`（移动扩展，cap-mobile）**：通知权限查询 / 请求 +
//!   即时震动 + 提示音（桌面无对应平台概念，不归交集）。
//!
//! 全部是「离宿主无法实现」的平台交互原语（零业务语义，ADR 0022）——文案 /
//! 时机 / 触发条件归插件，宿主只负责投递。

use super::HostError;

/// 通知选项（WIT `options-json` 的 SDK 形状）
///
/// 字段可省：`None` = 用宿主缺省（震动 / 声音均为 true，与退役前
/// `HostEvents::notify` 行为一致）；[`NotifyOptions::default`] 即全缺省。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyOptions {
    /// 是否震动（缺省 true）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vibrate: Option<bool>,
    /// 是否播放提示音（缺省 true）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<bool>,
}

impl NotifyOptions {
    /// 全缺省（宿主按震动 / 声音均 true 投递）
    pub fn new() -> Self {
        Self::default()
    }

    /// 显式指定震动 / 声音开关
    pub fn with(vibrate: bool, sound: bool) -> Self {
        Self {
            vibrate: Some(vibrate),
            sound: Some(sound),
        }
    }
}

/// 系统通知（共享 `host-notify` 契约；权限位 `notify`，fail-closed）
pub trait HostNotify {
    /// 发送系统通知（文案由插件 i18n 构建、原样透传，宿主不做文案加工）
    fn notify(&self, title: &str, body: &str, options: &NotifyOptions) -> Result<(), HostError>;
}

/// 移动端通知扩展（`host-notify-mobile`，cap-mobile；权限位同 `notify`）
pub trait HostNotifyMobile {
    /// 通知权限是否已授予（Android 13+ 需 POST_NOTIFICATIONS 且系统通知
    /// 总开关开启；Android 12 及以下恒 true）
    fn notify_check_permission(&self) -> Result<bool, HostError>;

    /// 请求通知权限（未授权时弹系统授权框并阻塞至用户响应）
    fn notify_request_permission(&self) -> Result<bool, HostError>;

    /// 立即震动一次（毫秒；直接走 Vibrator 服务、不经通知渠道，
    /// 无需通知权限；时长由宿主钳制到 [1, 5000] ms）
    fn notify_vibrate(&self, duration_ms: u32) -> Result<(), HostError>;

    /// 播放系统默认通知提示音（重复触发先停上一次，避免叠音）
    fn notify_play_sound(&self) -> Result<(), HostError>;
}
