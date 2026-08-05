//! 宿主定时器能力（v6）
//!
//! 插件注册周期回调后，宿主按固定间隔调用插件指定 command（附当前时间参数）。
//! 宿主只负责"到点调用"，具体到点做什么、幂等与否归插件
//! （插件以数据库中的到期时间做幂等判断），见 ADR 0003。

use super::HostError;

/// 宿主定时器
pub trait HostTimer {
    /// 注册周期定时器
    ///
    /// - `interval_secs`：触发间隔（秒），最小 1 秒
    /// - `command`：到点调用的插件 command 名（需在 manifest contributes.commands 声明）
    ///
    /// 重复注册会替换该插件已有的定时器（同一插件仅一个定时器实例）。
    /// 每次触发时宿主在 command 参数中附带当前时间：
    /// `now_ms`（Unix 毫秒时间戳）与 `now_utc`（"YYYY-MM-DD HH:MM:SS" UTC，
    /// 与 SQLite `datetime('now')` 同格式，便于 SQL 字符串比较）。
    ///
    /// 插件未激活时到点调用被宿主跳过（不报错）；应用退出时定时器随之停止。
    /// 需要 `timer:schedule` 权限。
    fn timer_register(&self, interval_secs: u64, command: &str) -> Result<(), HostError>;
}
