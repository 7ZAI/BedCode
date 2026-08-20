//! 协议级 mock 桌面端服务器（L1 集成测试共用基建）
//!
//! 协议形状对齐 `bedcode-desktop/src-tauri/src/server/ws/message.rs`：
//! `#[serde(tag = "type", content = "payload")]`、`auth` 变体、snake_case stage。
//! 应答构造直接复用移动端 `Message` 枚举（两端协议对称）——形状天然正确，
//! 仅回填请求的 message_id（客户端请求-响应匹配依赖）。
//!
//! 职责：
//! - 绑定 127.0.0.1:0（OS 分配端口，避免与真实桌面端实例冲突）
//! - 自动应答 JWT 首消息认证：`reauthenticate`（stage）→ `Authenticated`（附 mock token）
//! - 其余消息记录到 `received` 供断言；测试用 `send_message` / `graceful_close` 主动驱动
//! - 支持多连接（04 事件 WS 断线重建、多客户端场景）：每个连接独立处理任务，
//!   连接计数累计（`connection_count()`），`send_message`/`graceful_close` 广播到全部连接

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bedcode_lib::enums::auth::{AuthPayload, AuthStage};
use bedcode_lib::model::message::Message;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tokio_tungstenite::{accept_async, WebSocketStream};

/// mock 桌面端签发的会话令牌
pub const MOCK_SESSION_TOKEN: &str = "test-jwt-token";
/// 测试设备 ID
pub const TEST_DEVICE_ID: &str = "test-device-001";
/// 测试设备名
pub const TEST_DEVICE_NAME: &str = "test-phone";

type MockSink = futures_util::stream::SplitSink<WebSocketStream<TcpStream>, WsMsg>;
/// 共享发送端：SplitSink 不可 Clone（内部是 Arc<Mutex> 但拒绝导出），
/// 用 `Arc<tokio::sync::Mutex<MockSink>>` 在自动应答与外部队列之间共享——
/// 与旧单连接版的 `Arc<Mutex<Option<MockSink>>>` 思路一致
type SharedSink = Arc<Mutex<MockSink>>;

/// 协议级 mock 桌面端服务器
pub struct MockDesktopServer {
    pub addr: SocketAddr,
    /// 收到的全部文本消息（解析后的 JSON，按到达顺序，跨所有连接累计）
    pub received: Arc<Mutex<Vec<serde_json::Value>>>,
    /// 所有活动连接的共享发送端（send_message / graceful_close 广播用）
    sinks: Arc<Mutex<Vec<SharedSink>>>,
    /// 连接计数（累计 accept 次数；断线重建后递增，供「重建了一条连接」断言）
    connections: Arc<AtomicU32>,
    /// 连接处理任务句柄（force_close 时逐连接 abort，模拟传输层强制断开）
    connection_tasks: Arc<std::sync::Mutex<Vec<JoinHandle<()>>>>,
    /// accept 循环任务句柄
    task: JoinHandle<()>,
}

impl MockDesktopServer {
    /// 启动服务器（accept 循环，每个连接 spawn 独立处理任务，直到 force_close / drop）
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let sinks: Arc<Mutex<Vec<SharedSink>>> = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicU32::new(0));
        let connection_tasks: Arc<std::sync::Mutex<Vec<JoinHandle<()>>>> = Arc::new(std::sync::Mutex::new(Vec::new()));

        let listener_received = received.clone();
        let listener_sinks = sinks.clone();
        let listener_connections = connections.clone();
        let listener_tasks = connection_tasks.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let received = listener_received.clone();
                let sinks = listener_sinks.clone();
                let connections = listener_connections.clone();
                // 进入连接任务前计数：accept 到即算新连接（含尚未完成握手者）
                connections.fetch_add(1, Ordering::SeqCst);
                let handle = tokio::spawn(async move {
                    let ws = match accept_async(stream).await {
                        Ok(w) => w,
                        // 客户端在 accept 与握手之间断开：本连接任务退出
                        Err(_) => return,
                    };
                    let (tx, mut rx) = ws.split();
                    // 本任务持有共享发送端副本：Pong/自动应答与测试的
                    // send_message 广播经同一 Mutex 串行写，互不争用
                    let sink: SharedSink = Arc::new(Mutex::new(tx));
                    sinks.lock().await.push(sink.clone());

                    while let Some(msg) = rx.next().await {
                        match msg {
                            Ok(WsMsg::Text(t)) => {
                                let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&t) else {
                                    continue;
                                };
                                received.lock().await.push(parsed.clone());
                                respond_auth_if_needed(&parsed, &sink).await;
                            }
                            // 心跳应答
                            Ok(WsMsg::Ping(d)) => {
                                let _ = sink.lock().await.send(WsMsg::Pong(d)).await;
                            }
                            Ok(WsMsg::Close(_)) | Err(_) => break,
                            // Binary 与其它帧不处理，保持连接存活
                            _ => {}
                        }
                    }
                    // 连接结束：从广播列表摘除，避免 send_message 继续向死连接发送
                    sinks.lock().await.retain(|s| !Arc::ptr_eq(s, &sink));
                });
                listener_tasks.lock().unwrap().push(handle);
            }
        });

        Self {
            addr,
            received,
            sinks,
            connections,
            connection_tasks,
            task,
        }
    }

    /// 当前累计连接数（accept 次数；用于「断线后重建了一条新连接」断言）
    pub fn connection_count(&self) -> u32 {
        self.connections.load(Ordering::SeqCst)
    }

    /// 主动推送一条消息给所有客户端（终端输出 / SyncData 广播等）
    ///
    /// 空连接列表视为测试写出 bug（应在 client 建连后再推送），直接 panic；
    /// 个别连接已关闭时 send 返回 Err，忽略而不中断其余连接
    pub async fn send_message(&self, msg: &Message) {
        let json = msg.to_json().unwrap();
        let guard = self.sinks.lock().await;
        assert!(!guard.is_empty(), "client not connected");
        for sink in guard.iter() {
            let mut s = sink.lock().await;
            let _ = s.send(WsMsg::Text(json.clone().into())).await;
        }
    }

    /// 协议级优雅关闭：对所有连接发 Close 帧（模拟桌面端优雅停机）
    pub async fn graceful_close(&self, reason: &str) {
        let guard = self.sinks.lock().await;
        for sink in guard.iter() {
            let mut s = sink.lock().await;
            let _ = s
                .send(WsMsg::Close(Some(
                    tokio_tungstenite::tungstenite::protocol::CloseFrame {
                        code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
                        reason: reason.to_string().into(),
                    },
                )))
                .await;
        }
    }

    /// 强制断开所有连接（abort 服务器与各连接任务 = 底层连接直接关闭，无 Close
    /// 帧；注意客户端对裸 TCP EOF 是静默的——断连事件依赖 Close 帧/传输错误）
    pub fn force_close(&self) {
        self.task.abort();
        let tasks = std::mem::take(&mut *self.connection_tasks.lock().unwrap());
        for t in tasks {
            t.abort();
        }
    }

    /// 等待收到满足谓词的消息（轮询 + 超时，避免测试卡死）
    pub async fn wait_for_received<F>(&self, mut pred: F, timeout: Duration) -> Vec<serde_json::Value>
    where
        F: FnMut(&serde_json::Value) -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            {
                let guard = self.received.lock().await;
                let matched: Vec<_> = guard.iter().filter(|v| pred(v)).cloned().collect();
                if !matched.is_empty() {
                    return matched;
                }
            }
            if Instant::now() > deadline {
                panic!("timed out waiting for message");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

impl Drop for MockDesktopServer {
    fn drop(&mut self) {
        // 兜底终止全部任务，防止 worker 线程泄漏到后续测试
        self.task.abort();
        let tasks = std::mem::take(&mut *self.connection_tasks.lock().unwrap());
        for t in tasks {
            t.abort();
        }
    }
}

/// 自动应答 JWT 首消息认证（服务端视角）
///
/// 认证已 HTTP 化，WS 上的认证语义收敛为「首消息 JWT」：收到
/// `reauthenticate`（WS 首消息 stage，见桌面端 terminal_ws.rs::handle_auth）
/// 回 `Authenticated` 并附会话 token。应答形状：`Message::auth`（带请求
/// message_id 回填 + token 透传），与桌面端 auth 响应语义一致。
/// 用连接自己的共享发送端回写（与外部 send_message 广播互不干扰）。
async fn respond_auth_if_needed(parsed: &serde_json::Value, sink: &SharedSink) {
    if parsed["type"] != "auth" {
        return;
    }
    // adjacently tagged 嵌套：Message::Auth 变体字段中有一个也叫 payload 的字段
    // （AuthPayload），故 stage 在双层 payload 下：
    // {"type":"auth","payload":{"message_id":..,"payload":{"stage":..}}}
    let Some(stage) = parsed["payload"]["payload"]["stage"].as_str() else {
        return;
    };
    let req_id = parsed["payload"]["message_id"].as_str().unwrap_or("");
    let token = parsed["payload"]["token"].as_str().unwrap_or("");

    let response = match stage {
        // WS 首消息 JWT 认证成功：签发会话令牌
        "reauthenticate" => Some(Message::auth(
            None,
            AuthPayload {
                stage: AuthStage::Authenticated,
                session_token: Some(MOCK_SESSION_TOKEN.to_string()),
                ..Default::default()
            },
        )),
        _ => None,
    };

    if let Some(resp) = response {
        let resp = resp.with_request_id(req_id).with_token(token);
        let json = resp.to_json().unwrap();
        let mut s = sink.lock().await;
        let _ = s.send(WsMsg::Text(json.into())).await;
    }
}
