//! Frontend Output Handler
//!
//! 向前端发送 PTY 输出事件（兼容通道：插件输出变换已迁移到
//! SessionOutputManager::on_output 统一真源，此处仅透传原始事件）
//! 通过 broadcast channel 订阅输出，在独立 task 中 recv 循环消费并 emit Tauri 事件
//! 事件名按 session 分 channel：pty-output-{session_id}，避免多会话时无意义 IPC

use crate::pty::PtyOutputEvent;
use crate::system::error_boundary::spawn_with_error_boundary;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

/// 向前端发送 PTY 输出事件的 Handler
///
/// 启动一个后台 task 从 broadcast receiver 循环接收 PtyOutputEvent 并 emit 到前端
pub struct FrontendOutputHandler;

impl FrontendOutputHandler {
    /// 启动输出转发 task
    ///
    /// 泛型 Runtime：生产环境为 Wry，测试环境可用 MockRuntime
    pub fn spawn<R: tauri::Runtime>(app_handle: AppHandle<R>, mut rx: broadcast::Receiver<PtyOutputEvent>) {
        spawn_with_error_boundary("frontend_output_handler", async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let event_name = format!("pty-output-{}", event.session_id);
                        if let Err(e) = app_handle.emit(&event_name, &event) {
                            tracing::error!("[FrontendOutputHandler] Failed to emit {}: {}", event_name, e);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        tracing::warn!("[FrontendOutputHandler] Lagged {} events, some output may be dropped", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("[FrontendOutputHandler] Broadcast channel closed, exiting");
                        break;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tauri::Listener;
    use tokio::sync::broadcast;

    /// 异步轮询等待条件满足（sleep 期间让出，runtime 才能轮询转发 task）
    async fn wait_until<F: Fn() -> bool>(mut cond: F) -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if cond() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
        }
        cond()
    }

    #[tokio::test]
    async fn emits_events_to_session_scoped_channel_and_exits_on_close() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();

        // 事件名契约：pty-output-{session_id}（前端按会话订阅）
        let received = Arc::new(AtomicUsize::new(0));
        let received_clone = received.clone();
        app.listen("pty-output-test-session", move |_| {
            received_clone.fetch_add(1, Ordering::SeqCst);
        });

        let (tx, rx) = broadcast::channel(16);
        FrontendOutputHandler::spawn(handle, rx);

        let event = PtyOutputEvent {
            session_id: "test-session".to_string(),
            data: "aGVsbG8=".to_string(),
            timestamp: Utc::now(),
            is_waiting: false,
            index: 0,
        };
        tx.send(event).unwrap();

        assert!(
            wait_until(|| received.load(Ordering::SeqCst) > 0).await,
            "event should be emitted to frontend"
        );
        assert_eq!(received.load(Ordering::SeqCst), 1);

        // 发送方关闭 → 循环应正常退出（无 panic 即通过）
        drop(tx);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
