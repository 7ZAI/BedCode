//! 宿主能力：系统通知与提醒反馈（WIT `host-notify`，ABI v18 新增）
//!
//! 收编原 `host-events.notify`（旧形震动/声音硬编码 true）：系统通知
//! （震动 / 声音由 [`NotifyOptions`] 分控）+ 通知权限查询 / 请求 + 即时
//! 震动 + 提示音。全部是「离宿主无法实现」的平台交互原语（零业务语义，
//! ADR 0022）——文案 / 时机 / 触发条件归插件，宿主只负责投递。
//!
//! 权限位 `notify`（fail-closed，未声明即拒）：通知 / 震动 / 声音是用户
//! 打扰面（高频弹通知或狂震会骚扰用户），独立成位。

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

/// 系统通知与提醒反馈（权限位 `notify`，fail-closed）
pub trait HostNotify {
    /// 发送系统通知（文案由插件 i18n 构建、原样透传，宿主不做文案加工）
    fn notify(&self, title: &str, body: &str, options: &NotifyOptions) -> Result<(), HostError>;

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
