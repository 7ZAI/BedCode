//! 宿主能力：系统通知（WIT `host-notify`，共享域契约）
//!
//! 2026-10-13 功能级统一（ADR 0052 D3 / domain-owned-contracts 票 04）：原桌面
//! `HostEvents::notify`（WIT `host-events-desktop`）与移动 `HostNotify::notify`
//! 是同一功能 → 统一为双端共享契约 `host-notify`
//! （真源 `packages/bedcode-notify-engine/wit/notify.wit`）。
//!
//! **平台不支持的选项静默忽略**（桌面无震动 / 提示音通道）：调用方按同一份代码
//! 跨端调用，不需要按端裁剪 options；非法 JSON / 字段类型错误由宿主显性 `Err`。

use super::HostError;

/// 通知选项（WIT `options-json` 的 SDK 形状）
///
/// 字段可省：`None` = 宿主缺省（震动 / 声音均为 true，与退役前行为一致）；
/// [`NotifyOptions::default`] 即全缺省。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyOptions {
    /// 是否震动（缺省 true；桌面无此通道 ⇒ 静默忽略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vibrate: Option<bool>,
    /// 是否播放提示音（缺省 true；桌面无此通道 ⇒ 静默忽略）
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

/// 系统通知（WIT `host-notify`，双端共享契约；桌面无权限门，移动端 `notify` 门控）
pub trait HostNotify {
    /// 发送系统通知（文案由插件 i18n 构建、原样透传，宿主不做文案加工）
    fn notify(&self, title: &str, body: &str, options: &NotifyOptions) -> Result<(), HostError>;
}
