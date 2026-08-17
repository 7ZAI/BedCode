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
//! - 其余消息记录到 `received` 供断言；测试用 `send_message` / `force_close` 主动驱动

use std::net::SocketAddr;
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

/// 协议级 mock 桌面端服务器
pub struct MockDesktopServer {
    pub addr: SocketAddr,
    /// 收到的全部文本消息（解析后的 JSON，按到达顺序）
    pub received: Arc<Mutex<Vec<serde_json::Value>>>,
    sink: Arc<Mutex<Option<MockSink>>>,
    task: JoinHandle<()>,
}

impl MockDesktopServer {
    /// 启动服务器（接受一个连接后保持服务，直到 force_close / drop）
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let sink: Arc<Mutex<Option<MockSink>>> = Arc::new(Mutex::new(None));

        let received_task = received.clone();
        let sink_task = sink.clone();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws = accept_async(stream).await.unwrap();
            let (tx, mut rx) = ws.split();
            // 发布发送端：测试的 send_message / 本任务的自动应答共用
            *sink_task.lock().await = Some(tx);

            while let Some(Ok(msg)) = rx.next().await {
                match msg {
                    WsMsg::Text(t) => {
                        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&t) else {
                            continue;
                        };
                        received_task.lock().await.push(parsed.clone());
                        respond_auth_if_needed(&parsed, &sink_task).await;
                    }
                    // 心跳应答
                    WsMsg::Ping(d) => {
                        if let Some(sink) = sink_task.lock().await.as_mut() {
                            let _ = sink.send(WsMsg::Pong(d)).await;
                        }
                    }
                    WsMsg::Close(_) | WsMsg::Binary(_) => {}
                    _ => {}
                }
            }
        });

        Self {
            addr,
            received,
            sink,
            task,
        }
    }

    /// 主动推送一条消息给客户端（终端输出等）
    pub async fn send_message(&self, msg: &Message) {
        let json = msg.to_json().unwrap();
        let mut guard = self.sink.lock().await;
        let sink = guard.as_mut().expect("client not connected");
        sink.send(WsMsg::Text(json.into())).await.unwrap();
    }

    /// 协议级优雅关闭：发 Close 帧后关闭连接（模拟桌面端优雅停机）
    pub async fn graceful_close(&self, reason: &str) {
        let mut guard = self.sink.lock().await;
        if let Some(sink) = guard.as_mut() {
            let _ = sink
                .send(WsMsg::Close(Some(
                    tokio_tungstenite::tungstenite::protocol::CloseFrame {
                        code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
                        reason: reason.to_string().into(),
                    },
                )))
                .await;
        }
    }

    /// 强制断开客户端（abort 服务器任务 = 底层连接直接关闭，无 Close 帧；
    /// 注意客户端对裸 TCP EOF 是静默的——断连事件依赖 Close 帧/传输错误）
    pub fn force_close(&self) {
        self.task.abort();
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

    /// 已收到消息总数
    pub async fn received_count(&self) -> usize {
        self.received.lock().await.len()
    }
}

impl Drop for MockDesktopServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// 自动应答 JWT 首消息认证（服务端视角）
///
/// 认证已 HTTP 化，WS 上的认证语义收敛为「首消息 JWT」：收到
/// `reauthenticate`（WS 首消息 stage，见桌面端 terminal_ws.rs::handle_auth）
/// 回 `Authenticated` 并附会话 token。应答形状：`Message::auth`（带请求
/// message_id 回填 + token 透传），与桌面端 auth 响应语义一致。
async fn respond_auth_if_needed(parsed: &serde_json::Value, sink: &Arc<Mutex<Option<MockSink>>>) {
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
        if let Some(sink) = sink.lock().await.as_mut() {
            let _ = sink.send(WsMsg::Text(json.into())).await;
        }
    }
}
