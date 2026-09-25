//! 会话登记域纯逻辑：状态机 / 记录构造 / 判据（native 全覆盖）
//!
//! 本文件不碰宿主、不碰私有库——所有策略在这里可确定性断言，wasm 侧只做搬运
//! （见 [`super::store`] 与 [`super::mod`] 的门面）。
//!
//! 状态机出处：宿主 `session/session_manager.rs` 的会话生命周期分发与
//! `start_lifecycle_handler`（自然退出翻 `Stopped`）。迁入本域的语义与宿主逐格
//! 对齐——合法性表只做「插件侧自己不再制造非法态」的自守，不发明新语义。

use super::model::{SessionRecord, SessionStatus};
use crate::actions::RendererSource;

/// 生成新会话 id（UUID v4 形态字符串）
///
/// 复用配置域的生成单点（[`crate::config::model::new_config_id`]）：插件侧
/// UUID v4 只应有一处实现（同一随机源 `getrandom` + 同一形态），避免两套生成器
/// 漂移。P1 后续阶段会话 id 由插件自产（宿主不再预生成）。
pub fn new_session_id() -> Result<String, String> {
    crate::config::model::new_config_id()
}

/// 会话是否「活跃」（未停止）——判据与宿主 `filter_active_by_config`
/// （`status != Stopped`）逐字对齐：`Error` 也算活跃（会话记录仍在册）
pub fn is_active(status: &SessionStatus) -> bool {
    !matches!(status, SessionStatus::Stopped)
}

/// 关窗确认判据：会话是否处于「运行中、直接关窗会丢现场」的状态
///
/// 唯一消费方是宿主窗口关闭守卫（问「要不要弹确认弹窗」）。判据集合
/// （Running / Starting / WaitingInput）是本域会话语义——2026-09-25 清理
/// 时从宿主 `lib.rs` 守卫下沉至此，宿主不再持有任何会话状态集合判断。
pub fn needs_close_confirmation(status: &SessionStatus) -> bool {
    matches!(
        status,
        SessionStatus::Running | SessionStatus::Starting | SessionStatus::WaitingInput
    )
}

/// 状态迁移合法性
///
/// - 同态 → 合法（幂等写：重复的 `Stopped` 迁移不报错）
/// - 终态（`Stopped` / `Error`）→ 任何异态：**非法**（会话记录不能死而复生；
///   重启走「移除 + 同 id 重建」，是新记录而不是迁移）
/// - `Idle` 只能被 `Starting` 认领（宿主会把 `Idle` 视作起始态）；
///   本域今天不生产 `Idle`
/// - `Stopping` 不可回到 `Running`（宿主 `kill` 路径单调向前）
pub fn is_legal(from: &SessionStatus, to: &SessionStatus) -> bool {
    if from == to {
        return true;
    }
    match from {
        SessionStatus::Idle => matches!(to, SessionStatus::Starting | SessionStatus::Stopped),
        SessionStatus::Starting => matches!(
            to,
            SessionStatus::Running | SessionStatus::Stopping | SessionStatus::Stopped | SessionStatus::Error(_)
        ),
        SessionStatus::Running => matches!(
            to,
            SessionStatus::WaitingInput
                | SessionStatus::Stopping
                | SessionStatus::Stopped
                | SessionStatus::Error(_)
        ),
        SessionStatus::WaitingInput => matches!(
            to,
            SessionStatus::Running | SessionStatus::Stopping | SessionStatus::Stopped | SessionStatus::Error(_)
        ),
        SessionStatus::Stopping => matches!(to, SessionStatus::Stopped | SessionStatus::Error(_)),
        SessionStatus::Stopped | SessionStatus::Error(_) => false,
    }
}

/// 按状态迁移规则产出新记录（不做 I/O）
///
/// - 同态：原样返回（不刷新时间戳，避免无意义的 `updatedAt` 抖动）
/// - 非法迁移：显性报错（调用方据此 warn 留痕，不静默吞）
/// - `Running`：补 `started_at`（已有值不覆盖——`start = true` 的创建路径已填）
/// - `Stopped` / `Error`：补 `stopped_at`
/// - 任何**发生**的迁移都刷新 `updated_at`
pub fn transition(record: &SessionRecord, to: SessionStatus, now: &str) -> Result<SessionRecord, String> {
    if record.status == to {
        return Ok(record.clone());
    }
    if !is_legal(&record.status, &to) {
        return Err(format!(
            "illegal session status transition: {:?} -> {:?} (session {})",
            record.status, to, record.id
        ));
    }
    let mut next = record.clone();
    match &to {
        SessionStatus::Running => {
            if next.started_at.is_none() {
                next.started_at = Some(now.to_string());
            }
        }
        SessionStatus::Stopped | SessionStatus::Error(_) => {
            if next.stopped_at.is_none() {
                next.stopped_at = Some(now.to_string());
            }
        }
        _ => {}
    }
    next.status = to;
    next.updated_at = now.to_string();
    Ok(next)
}

/// 新建记录（创建编排产出 id 后调用）
///
/// `start = true` → `Running` 且 `started_at` 已填、正统渲染端初始归属 = 启动端
/// （桌面本地启动 = `Desktop`，移动端经 HTTP/WS 启动 = `Mobile{device_name}`），
/// 与宿主 `create_session_from_spec` 的归属规则逐字一致；
/// `start = false` → `Starting`、无归属、`started_at` 留空（两阶段第一阶段）。
pub fn new_record(
    id: &str,
    config_id: &str,
    name: &str,
    start: bool,
    source_device: Option<&str>,
    owner: Option<&str>,
    now: &str,
) -> SessionRecord {
    SessionRecord {
        id: id.to_string(),
        // P1 后续阶段由本插件经 host-pty.spawn 自持句柄；今天宿主内部持有
        pty_id: None,
        config_id: config_id.to_string(),
        name: name.to_string(),
        status: if start {
            SessionStatus::Running
        } else {
            SessionStatus::Starting
        },
        created_at: now.to_string(),
        started_at: if start { Some(now.to_string()) } else { None },
        stopped_at: None,
        canonical_renderer: if start {
            Some(match source_device {
                Some(device_name) => RendererSource::Mobile {
                    device_name: device_name.to_string(),
                },
                None => RendererSource::Desktop,
            })
        } else {
            None
        },
        owner: owner.map(str::to_string),
        updated_at: now.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> &'static str {
        "2026-09-23T10:00:00Z"
    }

    fn record(status: SessionStatus) -> SessionRecord {
        SessionRecord {
            id: "s1".to_string(),
            pty_id: None,
            config_id: "c1".to_string(),
            name: "会话".to_string(),
            status,
            created_at: "2026-09-23T09:00:00Z".to_string(),
            started_at: None,
            stopped_at: None,
            canonical_renderer: None,
            owner: None,
            updated_at: "2026-09-23T09:00:00Z".to_string(),
        }
    }

    // ==================== 状态机合法性矩阵 ====================

    /// 正常生命周期：Starting → Running → Stopping → Stopped 全链合法
    #[test]
    fn happy_path_transitions_are_legal() {
        let mut current = record(SessionStatus::Starting);
        for next in [
            SessionStatus::Running,
            SessionStatus::Stopping,
            SessionStatus::Stopped,
        ] {
            let updated = transition(&current, next.clone(), now()).expect("合法迁移");
            assert_eq!(updated.status, next);
            current = updated;
        }
        assert_eq!(current.status, SessionStatus::Stopped);
    }

    /// 终态不可复活：Stopped / Error 出向全非法（重启是新记录，不是迁移）
    #[test]
    fn terminal_states_reject_any_outgoing_transition() {
        for terminal in [
            SessionStatus::Stopped,
            SessionStatus::Error(Some("boom".to_string())),
        ] {
            for to in [
                SessionStatus::Starting,
                SessionStatus::Running,
                SessionStatus::Stopping,
            ] {
                let err = transition(&record(terminal.clone()), to.clone(), now()).expect_err("终态出向必须拒绝");
                assert!(err.contains("illegal session status transition"), "got: {err}");
            }
        }
    }

    /// Stopping 单调向前：不得回到 Running（宿主 kill 路径不会回头）
    #[test]
    fn stopping_never_returns_to_running() {
        assert!(!is_legal(&SessionStatus::Stopping, &SessionStatus::Running));
        assert!(is_legal(&SessionStatus::Stopping, &SessionStatus::Stopped));
        assert!(is_legal(
            &SessionStatus::Stopping,
            &SessionStatus::Error(None)
        ));
    }

    /// 同态幂等：不报错、不刷时间戳（避免 updatedAt 抖动）
    #[test]
    fn same_status_is_idempotent_and_does_not_touch_timestamps() {
        let before = record(SessionStatus::Running);
        let after = transition(&before, SessionStatus::Running, "2026-09-23T11:00:00Z").expect("同态合法");
        assert_eq!(after, before, "同态写不得改动任何字段");
    }

    /// 幂等的 Stopped：重复到达的 Stopped 事件不报错也不改 stopped_at
    #[test]
    fn repeated_stopped_keeps_first_stop_timestamp() {
        let mut running = record(SessionStatus::Running);
        running.started_at = Some("2026-09-23T09:30:00Z".to_string());
        let stopped = transition(&running, SessionStatus::Stopped, "2026-09-23T10:00:00Z").expect("停止");
        assert_eq!(stopped.stopped_at.as_deref(), Some("2026-09-23T10:00:00Z"));
        let again = transition(&stopped, SessionStatus::Stopped, "2026-09-23T12:00:00Z").expect("重复 Stopped 幂等");
        assert_eq!(again, stopped, "重复 Stopped 不得改写首次停止时间");
    }

    /// 时间戳填充：Running 补 started_at、Stopped 补 stopped_at、每次变更刷 updated_at
    #[test]
    fn transition_fills_timestamps_once_and_refreshes_updated_at() {
        let starting = record(SessionStatus::Starting);
        let running = transition(&starting, SessionStatus::Running, "2026-09-23T10:00:00Z").expect("running");
        assert_eq!(running.started_at.as_deref(), Some("2026-09-23T10:00:00Z"));
        assert_eq!(running.updated_at, "2026-09-23T10:00:00Z");
        assert!(running.stopped_at.is_none());

        let stopped = transition(&running, SessionStatus::Stopped, "2026-09-23T11:00:00Z").expect("stopped");
        assert_eq!(stopped.stopped_at.as_deref(), Some("2026-09-23T11:00:00Z"));
        assert_eq!(
            stopped.started_at.as_deref(),
            Some("2026-09-23T10:00:00Z"),
            "既有的 started_at 不被覆盖"
        );
        assert_eq!(stopped.updated_at, "2026-09-23T11:00:00Z");
    }

    // ==================== 活跃判据（对齐宿主 filter_active_by_config） ====================

    /// 活跃 = 非 Stopped：Error 也算活跃（宿主同一判据，会话记录仍在册）
    #[test]
    fn active_matches_host_filter_active_by_config() {
        assert!(is_active(&SessionStatus::Starting));
        assert!(is_active(&SessionStatus::Running));
        assert!(is_active(&SessionStatus::Stopping));
        assert!(is_active(&SessionStatus::Error(None)), "Error 仍算活跃（宿主判据）");
        assert!(!is_active(&SessionStatus::Stopped));
    }

    // ==================== 关窗确认判据（自宿主 lib.rs 守卫下沉） ====================

    /// 运行中三态（Running / Starting / WaitingInput）→ 需要确认；
    /// 其余（Idle / Stopping / Stopped / Error）→ 直接关窗不需打扰用户。
    /// 判定矩阵钉死：与宿主旧守卫的 filter 集合逐字一致（2026-09-25 下沉）。
    #[test]
    fn close_confirmation_matrix() {
        for needs in [
            SessionStatus::Running,
            SessionStatus::Starting,
            SessionStatus::WaitingInput,
        ] {
            assert!(
                needs_close_confirmation(&needs),
                "{needs:?} 必须需要关窗确认"
            );
        }
        for not_needs in [
            SessionStatus::Idle,
            SessionStatus::Stopping,
            SessionStatus::Stopped,
            SessionStatus::Error(None),
        ] {
            assert!(
                !needs_close_confirmation(&not_needs),
                "{not_needs:?} 不得需要关窗确认"
            );
        }
    }

    // ==================== 记录构造 ====================

    /// `start = true`：Running + started_at + 正统端 = 启动端（桌面 → Desktop）
    #[test]
    fn new_record_start_true_is_running_with_desktop_canonical() {
        let rec = new_record("s1", "c1", "会话", true, None, Some("com.bedcode.terminal-session"), now());
        assert_eq!(rec.status, SessionStatus::Running);
        assert_eq!(rec.started_at.as_deref(), Some(now()));
        assert_eq!(rec.stopped_at, None);
        assert_eq!(rec.canonical_renderer, Some(RendererSource::Desktop));
        assert_eq!(rec.owner.as_deref(), Some("com.bedcode.terminal-session"));
        assert!(rec.pty_id.is_none(), "P1 阶段 PTY 句柄仍由宿主内部持有");
    }

    /// `start = true` + sourceDevice：正统端初始归属 = 移动端（防移动端首次 resize 误弹确认）
    #[test]
    fn new_record_start_true_with_source_device_claims_mobile_canonical() {
        let rec = new_record("s1", "c1", "会话", true, Some("Pixel-9"), None, now());
        assert_eq!(
            rec.canonical_renderer,
            Some(RendererSource::Mobile {
                device_name: "Pixel-9".to_string()
            })
        );
    }

    /// `start = false`（两阶段第一阶段）：Starting + 无 started_at + 无归属
    #[test]
    fn new_record_start_false_is_starting_without_canonical() {
        let rec = new_record("s1", "c1", "会话", false, Some("Pixel-9"), None, now());
        assert_eq!(rec.status, SessionStatus::Starting);
        assert!(rec.started_at.is_none());
        assert!(rec.canonical_renderer.is_none(), "不启动 → 无正统端归属");
    }

    /// id 生成：UUID v4 形态（36 字符、版本位 4、变体位 RFC 4122）
    #[test]
    fn new_session_id_is_uuid_v4_shaped() {
        let a = new_session_id().expect("entropy");
        let b = new_session_id().expect("entropy");
        assert_ne!(a, b, "两次生成不得相同");
        assert_eq!(a.len(), 36);
        let parts: Vec<&str> = a.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert_eq!(&parts[2][0..1], "4", "版本位");
        assert!(
            ["8", "9", "a", "b"].contains(&&parts[3][0..1]),
            "变体位必须为 RFC 4122 形态, got: {a}"
        );
    }
}
