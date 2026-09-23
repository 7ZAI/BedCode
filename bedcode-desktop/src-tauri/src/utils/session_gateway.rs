//! 会话窄转发层（会话引擎整体下沉 P1，宿主侧单点）
//!
//! spec：`.scratch/2026-09-23-session-engine-downsink/spec.md`（P1「宿主侧：新增窄转发层」）。
//!
//! ## 为什么要有这一层
//!
//! 会话操作在宿主侧原本散在三条线上各自直连 `SessionManager`：桌面 Tauri 命令
//! （`commands.rs`）、移动端 HTTP 控制器（`server/http/controllers/session_controller.rs`）、
//! 移动端 WS 服务（`server/websocket/services/*`）。同一条规则因此被写两遍（尺寸裁决
//! 的「插件优先、内核降级」只在桌面命令面存在；移动端两线直连内核），改一处必漏一处。
//!
//! 本模块是**宿主侧调用会话的唯一收口点**：消费面只调这里的函数，不再直接碰
//! `SessionManager` / `session_*_bridge`。P1 后续阶段的真源切换（插件经 `host-pty`
//! 自持 PTY、宿主改调插件互调 api）只改本模块内部的实现，消费面零改动。
//!
//! ## 今日策略（P1-a/P1-b 之间：行为与迁移前逐字一致）
//!
//! | 操作 | 今日实现 | P1-b 后 |
//! | --- | --- | --- |
//! | 查询（list / get） | 内核 `SessionManager` 登记事实 | 插件互调 api（插件登记域为真源） |
//! | 创建（start） | `session_create_bridge` → 插件编排 + `host-session.create-with-spec`（**插件必需，无宿主降级**） | 插件 `host-pty.spawn`（插件自产 id） |
//! | 停止 / 移除 | 内核执行器（插件未参与；移动端两线同路） | 插件互调 api |
//! | 尺寸（桌面路径） | **插件裁决优先**（`session_action_bridge::resize_session_via_plugin`），不可用时内核执行器含内核裁决分支 | 插件互调 api（无降级） |
//! | 尺寸（移动端信号路径） | 内核执行器（裁决分支同插件规则） | 同上（统一经插件） |
//! | 输入（普通 / 特殊键） | 内核 `write_input` / `send_special_key`（P2 起改 `host-pty.write`） | 插件转发 |
//! | 历史快照 / 输出存在性 | `GlobalOutputManager`（业务输出环，P3 形态 B 改直读 `PtyRing`） | 宿主 server 直读 `PtyRing` |
//!
//! **移动端零改动的前提**：停止 / 移除 / 尺寸（信号路径）今日仍走内核执行器——
//! 真源切换（P1-b）会把这些路径一并改为经插件，届时移动端才第一次触到插件面；
//! 本层保证那次切换是**一处实现变更**，而不是三处散改。
//!
//! ## 不属于本层
//!
//! 会话**事件的形状与广播**（`events/sync_handler.rs` / `events/forwarder.rs`）与
//! WS 终端通道的状态订阅（`channel/terminal.rs` 的 `subscribe_status`）属事件面，
//! 随 P4（插件经 `host-events` 发布、宿主只做 WS 转发）收口，不在本层。

use crate::session::{GlobalOutputManager, RendererSource, ResizeOutcome, SessionInfoView, SessionManager};
use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::Result;

// ==================== 查询（内核登记事实 → P1-b 改插件 api） ====================

/// 全部会话的对外视图（记录 + 注解槽任务字段；形状与迁移前逐字段一致）
pub async fn list_views(sm: &SessionManager) -> Vec<SessionInfoView> {
    sm.session_views().await
}

/// 单个会话的对外视图
pub async fn view(sm: &SessionManager, session_id: &str) -> Option<SessionInfoView> {
    sm.session_view(session_id).await
}

// ==================== 创建（插件必需，宿主无降级） ====================

/// 经会话中心插件编排创建会话（`start = true` 创建即启动）
///
/// 插件未激活 / 互调失败一律显性报错（`host-business-decarriage` 收尾口径：
/// 创建只有一条编排入口，不存在宿主降级路径）。
pub async fn start(
    host_ctx: &WasmHostContext,
    config_id: &str,
    cols: Option<u16>,
    rows: Option<u16>,
    start: bool,
    source_device: Option<&str>,
) -> Result<String> {
    crate::utils::session_create_bridge::create_session_via_plugin(
        host_ctx, config_id, cols, rows, start, source_device,
    )
    .await
}

// ==================== 生命周期动作（内核执行器 → P1-b 改插件 api） ====================

/// 停止会话（终止 PTY + 置 `Stopped`，会话记录保留）
pub async fn stop(sm: &SessionManager, session_id: &str, source_device: Option<String>) -> Result<()> {
    sm.kill_session_with_source(session_id, source_device).await
}

/// 移除会话（摘除记录并清注解槽；PTY 缓存随 PTY 清理）
pub async fn remove(sm: &SessionManager, session_id: &str, source_device: Option<String>) -> Result<()> {
    sm.remove_session_with_source(session_id, source_device).await
}

/// 尺寸调整（桌面本地路径）：**插件裁决优先**，插件不可用时降级内核执行器
///
/// 两条路径对外行为等价（同一裁决规则、同一 `ResizeOutcome` 形状）：插件侧规则见
/// `plugins/terminal-session/rust/src/actions.rs::decide_resize`，内核侧见
/// `SessionManager::resize_session`——规则两份实现是 P1-b 要消掉的重复（真源切换后
/// 只剩插件一份）。
pub async fn resize_desktop(
    host_ctx: &WasmHostContext,
    sm: &SessionManager,
    session_id: &str,
    cols: u16,
    rows: u16,
    force: bool,
) -> Result<ResizeOutcome> {
    if let Some(outcome) = crate::utils::session_action_bridge::resize_session_via_plugin(
        host_ctx,
        session_id,
        cols,
        rows,
        &RendererSource::Desktop,
        force,
    )
    .await?
    {
        return Ok(outcome);
    }
    sm.resize_session(session_id, cols, rows, RendererSource::Desktop, force)
        .await
}

/// 尺寸调整（移动端信号路径：HTTP / WS 控制帧）
///
/// 请求方身份由调用方按 JWT claims 决定（无 claims 回退 `Desktop`，仍受
/// `NeedsConfirmation` 门控）。今日直连内核执行器（移动端零改动），P1-b 统一经插件。
pub async fn resize_from_signal(
    sm: &SessionManager,
    session_id: &str,
    cols: u16,
    rows: u16,
    requester: RendererSource,
    force: bool,
) -> Result<ResizeOutcome> {
    sm.resize_session(session_id, cols, rows, requester, force).await
}

// ==================== 输入（P2 起改 host-pty.write） ====================

/// 写入普通输入（提交行重建与插件钩子链仍在内核写入路径内，P2 迁插件）
pub async fn input(sm: &SessionManager, session_id: &str, data: &str) -> Result<()> {
    sm.write_input(session_id, data).await
}

/// 写入特殊键（转义序列）
pub async fn special_key(sm: &SessionManager, session_id: &str, key: &str) -> Result<()> {
    sm.send_special_key(session_id, key).await
}

// ==================== 输出面（P3 形态 B：改直读同进程 `PtyRing`） ====================

/// 一次性历史快照：`(data, min_offset, snapshot_offset, history_bytes)`
///
/// 今日读业务输出环 `GlobalOutputManager`；P3 形态 B 后由宿主 server 直读同进程
/// `PtyRing`（零跨 WASM 边界），本函数是那时的唯一改动点。
pub async fn history_snapshot(session_id: &str, from: u64) -> Option<(Vec<u8>, u64, u64, u64)> {
    GlobalOutputManager::global().snapshot_bytes(session_id, from).await
}

/// 取消某订阅者对该会话的输出订阅（WS 控制面停止 / 移除动作之后调用）
pub async fn unsubscribe_output(session_id: &str, subscriber: &str) {
    GlobalOutputManager::global().unsubscribe(session_id, subscriber).await;
}

// ==================== Tests（本层是行为保持的收口点：锁住今日语义，防真源切换前漂移） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::{ExecutionEnvironment, SessionLaunchConfig, SessionStatus};

    /// 「只创建不启动」的会话夹具（不 spawn 进程；与 `session_manager` 测试夹具同形）
    async fn seed_idle_session(sm: &SessionManager, config_id: &str) -> String {
        sm.create_session_from_spec(
            SessionLaunchConfig {
                name: config_id.to_string(),
                environment: ExecutionEnvironment::Linux,
                working_dir: "/tmp".to_string(),
                command: "bash".to_string(),
                command_args: vec!["bash".to_string()],
                env_vars: std::collections::HashMap::new(),
                cols: 120,
                rows: 40,
            },
            config_id.to_string(),
            None,
            false,
            None,
            None,
        )
        .await
        .expect("seed idle session")
    }

    /// 查询面：内核登记事实 → 对外视图（空管理器为空；播种后可见且字段透传）
    #[tokio::test]
    async fn list_and_view_expose_kernel_records() {
        let sm = SessionManager::default();
        assert!(list_views(&sm).await.is_empty());
        assert!(view(&sm, "ghost").await.is_none());

        let sid = seed_idle_session(&sm, "cfg-1").await;
        let views = list_views(&sm).await;
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].info.id, sid);
        assert_eq!(views[0].info.config_id, "cfg-1");
        assert_eq!(views[0].info.status, SessionStatus::Starting, "start=false → Starting");
        assert!(view(&sm, &sid).await.is_some());
    }

    /// 生命周期动作面：停止 → 记录翻 `Stopped`；移除 → 记录消失且未知 id 幂等成功
    /// （内核执行器语义，P1-b 换实现后须逐条不变）
    #[tokio::test]
    async fn stop_marks_stopped_and_remove_is_idempotent() {
        let sm = SessionManager::default();
        let sid = seed_idle_session(&sm, "cfg-1").await;
        assert_eq!(sm.get_session_status(&sid).await, Some(SessionStatus::Starting));

        stop(&sm, &sid, Some("Pixel-9".to_string())).await.expect("stop");
        assert_eq!(sm.get_session_status(&sid).await, Some(SessionStatus::Stopped));

        remove(&sm, &sid, None).await.expect("remove");
        assert!(view(&sm, &sid).await.is_none(), "移除后记录不再可见");
        assert!(
            remove(&sm, "ghost", None).await.is_ok(),
            "未知会话移除幂等成功（内核语义：删不存在的会话不是错误）"
        );
    }

    /// 「创建即启动」的会话夹具（正统端初始归属 = Desktop，因为 `source_device` 为空）
    ///
    /// 尺寸路径必须真到 PTY 层（未启动的会话 master 不可用、resize 必失败），故这里
    /// 起一个真实 `bash`；用例末尾显式 `stop` + `remove` 回收，不留后台进程。
    async fn seed_running_session(sm: &SessionManager, config_id: &str) -> String {
        sm.create_session_from_spec(
            SessionLaunchConfig {
                name: config_id.to_string(),
                environment: ExecutionEnvironment::Linux,
                working_dir: "/tmp".to_string(),
                command: "bash".to_string(),
                command_args: vec!["bash".to_string()],
                env_vars: std::collections::HashMap::new(),
                cols: 120,
                rows: 40,
            },
            config_id.to_string(),
            None,
            true,
            None,
            None,
        )
        .await
        .expect("seed running session")
    }

    /// 尺寸两条路径的今日语义（真源切换前后必须逐条不变）
    ///
    /// - **信号路径**（移动端 HTTP / WS）：内核裁决——他端未 `force` → `NeedsConfirmation`
    ///   且零改动；`force` → 应用并迁移归属。该路径的结构性保证写在签名上：本函数
    ///   **不含宿主上下文**，今日不可能触到插件面（P1-b 统一时改签名即显性可见）。
    /// - **桌面路径**：插件不可用（空 api 注册表 = 锚点未登记）→ 降级内核执行器并
    ///   成功应用。若真走了插件分支，`resize_session_via_plugin` 会以
    ///   `AppError::Plugin("session plugin not active …")` 报错——`expect` 通过即是
    ///   「确已降级内核」的判据。
    #[tokio::test]
    async fn resize_paths_keep_kernel_semantics_on_both_sides() {
        let ctx = crate::wasm_core::host_api::tests::build_host_ctx();
        let sm = ctx.session_manager.clone();
        let sid = seed_running_session(&sm, "cfg-1").await;

        // 信号路径：他端未 force → 需确认 + 零改动
        let other = RendererSource::Mobile {
            device_name: "Pixel-9".to_string(),
        };
        let pending = resize_from_signal(&sm, &sid, 120, 50, other.clone(), false)
            .await
            .expect("signal path");
        assert_eq!(
            pending,
            ResizeOutcome::NeedsConfirmation {
                current_canonical: RendererSource::Desktop
            }
        );
        assert_eq!(
            sm.canonical_renderer_of(&sid).await,
            Some(RendererSource::Desktop),
            "需确认时零改动（归属不迁移）"
        );

        // 桌面路径：插件不可用 → 内核执行器兜底并应用
        let applied = resize_desktop(&ctx, &sm, &sid, 90, 30, false)
            .await
            .expect("插件不可用 → 内核执行器兜底");
        assert_eq!(
            applied,
            ResizeOutcome::Applied {
                canonical: RendererSource::Desktop
            }
        );

        // 信号路径 force：应用并迁移归属
        let claimed = resize_from_signal(&sm, &sid, 110, 40, other.clone(), true)
            .await
            .expect("force 接管");
        assert_eq!(
            claimed,
            ResizeOutcome::Applied {
                canonical: other.clone()
            }
        );
        assert_eq!(sm.canonical_renderer_of(&sid).await, Some(other));

        // 回收：停止（杀 PTY）+ 移除（摘记录）
        stop(&sm, &sid, None).await.expect("stop");
        remove(&sm, &sid, None).await.expect("remove");
    }

    /// 输出面：未注册输出环的会话 → 无历史快照（HTTP 历史接口的 404 判据）
    #[tokio::test]
    async fn history_snapshot_is_none_without_output_ring() {
        assert!(history_snapshot("ghost", 0).await.is_none());
    }
}
