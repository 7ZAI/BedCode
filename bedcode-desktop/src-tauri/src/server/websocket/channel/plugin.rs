//! 插件端点通道处理器（`/ws/plugin/{plugin_id}/{path}`，spec §2.4 / 票据 05）
//!
//! 通道语义：插件在宿主 WS 服务器上挂载的入站端点。宿主只做引擎级事情——
//! 认证策略执行、帧转发给属主插件、接入/断开事件上报；**业务语义完全归插件**
//! （消息格式、房间、协议、重连策略宿主一概不解读，spec D1）。
//!
//! ## 认证策略（spec D8）
//!
//! - `auth:"none"`（缺省）：跳过首消息认证状态机（[`AuthMode::None`]）——连上即可
//!   收发，认证由插件自管；
//! - `auth:"jwt"`：骨架保证 [`AuthMode::Required`]（窗口内未认证 → close 4001），
//!   本通道校验首消息 `{"type":"auth","token":"<jwt>"}`（**不引入 `message.rs`
//!   类型**）。未认证期间的一切业务帧**丢弃 + warn，不缓存**（spec §4.3）。
//!
//! ## 帧与事件
//!
//! - 入站文本 / 二进制帧经骨架过滤链后转投属主插件（`events-ws` 可选导出），
//!   同连接内保序（单条投递任务串行消费）；
//! - 接入事件 `<owner>::ws:client-connect` 在连接**可用**时发布一次（`auth:"none"`
//!   为连接建立、`auth:"jwt"` 为认证通过），先于该连接的首个业务帧；
//!   断开事件 `<owner>::ws:client-disconnect` 在骨架停止前发布一次
//!   （且仅当接入事件已发布——认证失败/超时不留「无接入的断开」噪音）。
//!
//! 连接生命周期（心跳 / 认证超时 / 帧级过滤链 / 注册表）全部由骨架
//! [`crate::server::websocket::conn`] 承担，本文件只实现通道协议。

use actix::prelude::*;
use actix_web_actors::ws::{CloseCode, CloseReason};
use bedcode_plugin_api::host::bus::owned_topic;
use bedcode_plugin_api::host::ws::{WS_CLIENT_CONNECT, WS_CLIENT_DISCONNECT};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::super::conn::{AuthMode, ChannelHandler, ConnCtx, WsConnBase};
use super::super::endpoint::{EndpointAuth, EndpointEntry};
use crate::wasm_core::bus::MessageBus;
use crate::wasm_core::host_api::ws::deliver_endpoint_frame;
use crate::server::websocket::registry::WsSessionRegistry;
use crate::utils::auth::jwt::{jwt_error_message, JwtService};

/// 插件端点认证失败 / 超时的关闭码（spec D8；4000 段为应用自定义码）
const CLOSE_AUTH_FAILED: u16 = 4001;

/// 插件端点首消息认证帧（D8：宿主自有的极小契约，不引入 `message.rs` 类型）
#[derive(Debug, serde::Deserialize)]
struct AuthFrame {
    #[serde(rename = "type")]
    frame_type: String,
    token: String,
}

/// 待投递帧（通道 → 投递任务）
struct FrameJob {
    kind: &'static str,
    payload: Vec<u8>,
}

/// 插件端点通道处理器
///
/// 只持有本连接自有状态（接入/断开上报守卫、帧投递队列）；连接级状态
/// （认证态、地址、关闭原因）在骨架中，经 `&mut WsConnBase` 访问。
pub struct PluginChannel {
    /// 属主插件 id（事件 topic 的属主段 + 帧投递目标）
    owner: String,
    /// 端点句柄（事件 payload 标识）
    endpoint_id: String,
    /// 对端连接 id（= 注册表会话键 = 对端地址字符串，与 `list-clients` 同源）
    client_id: String,
    /// 首消息认证策略
    auth: EndpointAuth,
    /// 端点事件总线（端点上登记，连接侧不依赖 `AppContext` 全局单例）
    bus: Arc<MessageBus>,
    /// 接入事件已发布（守卫保证每连接恰好一次）
    connected_announced: bool,
    /// 断开事件已发布（守卫保证每连接恰好一次）
    disconnect_reported: bool,
    /// 帧投递队列发送端（投递任务串行消费 → 同连接保序）
    frames: mpsc::UnboundedSender<FrameJob>,
    /// 帧投递队列接收端（`on_started` 时移交给投递任务）
    frames_rx: Option<mpsc::UnboundedReceiver<FrameJob>>,
}

impl PluginChannel {
    /// 以已注册端点构造通道（`addr` 为对端地址，即会话注册键）
    pub fn new(entry: &EndpointEntry, addr: std::net::SocketAddr) -> Self {
        let (frames, frames_rx) = mpsc::unbounded_channel();
        Self {
            owner: entry.owner.clone(),
            endpoint_id: entry.endpoint_id.clone(),
            client_id: addr.to_string(),
            auth: entry.auth,
            bus: Arc::clone(&entry.bus),
            connected_announced: false,
            disconnect_reported: false,
            frames,
            frames_rx: Some(frames_rx),
        }
    }

    /// 入队待投递帧（投递任务已退出 → fail-visible warn，不静默丢）
    fn enqueue_frame(&self, kind: &'static str, payload: Vec<u8>) {
        if self
            .frames
            .send(FrameJob {
                kind,
                payload: payload.clone(),
            })
            .is_err()
        {
            tracing::warn!(
                plugin_id = %self.owner,
                endpoint_id = %self.endpoint_id,
                client_id = %self.client_id,
                bytes = payload.len(),
                "plugin endpoint frame dropped: delivery task is gone"
            );
        }
    }

    /// 未认证期的帧处理（`auth:"jwt"` 专属）
    ///
    /// 首消息必须是认证帧；其余帧丢弃 + warn（不缓存，spec §4.3）。
    /// 认证失败 → close 4001 并断开（不做重试协商，重连编排归插件）。
    fn handle_unauthenticated_frame(&mut self, conn: &mut WsConnBase, text: String, ctx: &mut ConnCtx) {
        let addr = conn.session.addr;
        let frame = match serde_json::from_str::<AuthFrame>(&text) {
            Ok(frame) if frame.frame_type == "auth" => frame,
            _ => {
                tracing::warn!(
                    plugin_id = %self.owner,
                    endpoint_id = %self.endpoint_id,
                    peer = %addr,
                    "plugin endpoint frame dropped: not authenticated (expecting first-message auth frame)"
                );
                return;
            }
        };

        if frame.token.trim().is_empty() {
            self.reject_auth(conn, "no token provided", ctx);
            return;
        }

        match verify_endpoint_jwt(conn, &frame.token) {
            Ok(()) => {
                // 注册表认证态同步（异步态：list-clients 的 authenticated 字段来源）。
                // 不经 `authenticate_jwt`：插件端点的第三方客户端**不参与配对设备
                // 在线语义**（不快照 last_seen、不向前端发 DEVICE_CONNECTED）
                let client_id = self.client_id.clone();
                let device_name = conn.session.device_name.clone();
                let fingerprint = conn.session.fingerprint.clone();
                actix::spawn(async move {
                    WsSessionRegistry::global()
                        .set_authenticated(&client_id, device_name, fingerprint)
                        .await;
                });
                tracing::info!(
                    plugin_id = %self.owner,
                    endpoint_id = %self.endpoint_id,
                    peer = %addr,
                    "plugin endpoint client authenticated"
                );
            }
            Err((_code, message)) => self.reject_auth(conn, &message, ctx),
        }
    }

    /// 认证失败收尾：close(4001) + 停止 actor
    fn reject_auth(&mut self, conn: &mut WsConnBase, message: &str, ctx: &mut ConnCtx) {
        tracing::warn!(
            plugin_id = %self.owner,
            endpoint_id = %self.endpoint_id,
            peer = %conn.session.addr,
            reason = %message,
            "plugin endpoint auth failed, closing 4001"
        );
        ctx.close(Some(CloseReason {
            code: CloseCode::Other(CLOSE_AUTH_FAILED),
            description: Some(message.to_string()),
        }));
        ctx.stop();
    }

    /// 发布接入事件（恰好一次）
    fn announce_connect(&mut self, conn: &WsConnBase) {
        if self.connected_announced {
            return;
        }
        self.connected_announced = true;
        let addr = conn.session.addr.to_string();
        self.bus.publish(
            &owned_topic(&self.owner, WS_CLIENT_CONNECT),
            "host",
            serde_json::json!({
                "endpointId": self.endpoint_id,
                "clientId": addr,
                "addr": addr,
                "authenticated": conn.session.authenticated,
            }),
        );
        tracing::info!(
            plugin_id = %self.owner,
            endpoint_id = %self.endpoint_id,
            client_id = %self.client_id,
            "plugin endpoint client connected"
        );
    }
}

/// 插件端点 JWT 校验（只置会话认证态，不触达配对设备在线语义）
fn verify_endpoint_jwt(conn: &mut WsConnBase, token: &str) -> Result<(), (String, String)> {
    let claims = JwtService::new()
        .verify_token_with_expiry(token)
        .map_err(|e| ("AUTH_FAILED".to_string(), jwt_error_message(&e).to_string()))?;
    conn.session.authenticated = true;
    conn.session.device_id = Some(claims.sub.clone());
    conn.session.device_name = claims.device_name.clone();
    conn.session.fingerprint = claims.fingerprint.clone();
    Ok(())
}

/// 帧投递任务：串行消费队列并投给属主插件（同连接内保序，spec §2.2 D2）
///
/// 通道随连接销毁时发送端被 drop → 队列排空后任务自然结束。
async fn run_endpoint_delivery(
    mut rx: mpsc::UnboundedReceiver<FrameJob>,
    bus: Arc<MessageBus>,
    owner: String,
    endpoint_id: String,
    client_id: String,
) {
    while let Some(job) = rx.recv().await {
        deliver_endpoint_frame(&bus, &owner, &endpoint_id, &client_id, job.kind, job.payload).await;
    }
}

impl ChannelHandler for PluginChannel {
    /// 认证策略由端点注册时声明（spec D8）
    fn auth_mode(&self) -> AuthMode {
        match self.auth {
            EndpointAuth::None => AuthMode::None,
            EndpointAuth::Jwt => AuthMode::Required,
        }
    }

    /// 认证超时统一 close 4001（`auth:"none"` 无认证窗口，不适用）
    fn auth_timeout_close_code(&self) -> Option<u16> {
        match self.auth {
            EndpointAuth::None => None,
            EndpointAuth::Jwt => Some(CLOSE_AUTH_FAILED),
        }
    }

    fn on_started(&mut self, _conn: &mut WsConnBase, _ctx: &mut ConnCtx) {
        // 启动帧投递任务（队列在工作线程上串行消费；不阻塞 actor）
        //
        // **必须派生到 ambient runtime**（不能用 `actix::spawn`）：投递是同步桥
        // （`block_on_async`），会阻塞所在线程直到客人回调返回；若落在 arbiter 线程上，
        // 客人回调内的宿主 WS 原语（`send-text-to-client` / `broadcast-*` / `close-client`）
        // 需要 await arbiter 上的连接 actor —— arbiter 被投递自己占住，双方互等即自锁
        // （插件端点回显的必经路径）。派生到 ambient 后 arbiter 保持空闲，actor 正常推进。
        if let Some(rx) = self.frames_rx.take() {
            let bus = Arc::clone(&self.bus);
            let owner = self.owner.clone();
            let endpoint_id = self.endpoint_id.clone();
            let client_id = self.client_id.clone();
            crate::system::error_boundary::spawn_with_error_boundary_on(
                &crate::wasm_core::manager::runtime::ambient_handle(),
                "ws_endpoint_delivery",
                run_endpoint_delivery(rx, bus, owner, endpoint_id, client_id),
            );
        }
    }

    /// 连接可用（`auth:"none"` 建立即可用 / `auth:"jwt"` 认证通过）→ 接入事件一次
    ///
    /// 骨架保证本回调先于该连接的首个业务帧投递（认证帧除外）
    fn on_auth_ok(&mut self, conn: &mut WsConnBase, _ctx: &mut ConnCtx) {
        self.announce_connect(conn);
    }

    fn on_text(&mut self, conn: &mut WsConnBase, text: String, ctx: &mut ConnCtx) {
        if self.auth == EndpointAuth::Jwt && !conn.session.authenticated {
            self.handle_unauthenticated_frame(conn, text, ctx);
            return;
        }
        self.enqueue_frame("text", text.into_bytes());
    }

    fn on_binary(&mut self, conn: &mut WsConnBase, data: Vec<u8>, ctx: &mut ConnCtx) {
        if self.auth == EndpointAuth::Jwt && !conn.session.authenticated {
            // 二进制帧不能承载认证帧（认证帧是 JSON 文本）→ 一律丢弃 + warn
            tracing::warn!(
                plugin_id = %self.owner,
                endpoint_id = %self.endpoint_id,
                peer = %conn.session.addr,
                bytes = data.len(),
                "plugin endpoint binary frame dropped: not authenticated"
            );
            let _ = ctx;
            return;
        }
        self.enqueue_frame("binary", data);
    }

    /// 断开上报（骨架停止前回调，恰好一次）
    ///
    /// 仅在接入事件已发布时上报（认证失败 / 超时连接从未「接入」）；
    /// 关闭码与 `wasClean` 取自骨架记录的连接终止原因（spec §4.5 / D11）：
    /// 对端 Close 且 code ∈ {1000,1001} → clean；宿主主动断开（踢出 4004 /
    /// 端点注销 · 属主停用 4005 / 停机 1001）→ 恒 false。
    fn on_close(&mut self, conn: &mut WsConnBase) {
        if !self.connected_announced || self.disconnect_reported {
            return;
        }
        self.disconnect_reported = true;

        let (code, reason, was_clean) = match conn.close_outcome() {
            Some(outcome) if outcome.peer_initiated => (
                outcome.code,
                outcome.reason.clone(),
                matches!(outcome.code, Some(1000) | Some(1001)),
            ),
            Some(outcome) => (outcome.code, outcome.reason.clone(), false),
            // 异常断开（传输错误 / 心跳超时 / 订阅链路终止）：无 Close 交换
            None => (None, String::new(), false),
        };

        let mut payload = serde_json::json!({
            "endpointId": self.endpoint_id,
            "clientId": self.client_id,
            "wasClean": was_clean,
        });
        if let Some(code) = code {
            payload["code"] = serde_json::Value::Number(code.into());
        }
        if !reason.is_empty() {
            payload["reason"] = serde_json::Value::String(reason);
        }
        self.bus
            .publish(&owned_topic(&self.owner, WS_CLIENT_DISCONNECT), "host", payload);
        tracing::info!(
            plugin_id = %self.owner,
            endpoint_id = %self.endpoint_id,
            client_id = %self.client_id,
            was_clean,
            "plugin endpoint client disconnected"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::websocket::endpoint::{register, EndpointAuth};
    use std::net::SocketAddr;
    use std::sync::Arc;

    fn test_endpoint(seed: &str, auth: EndpointAuth) -> EndpointEntry {
        let owner = format!("test-plugin-channel-{seed}");
        register(&owner, "echo", auth, None, None, Arc::new(MessageBus::new())).expect("register endpoint")
    }

    fn addr(port: u16) -> SocketAddr {
        format!("127.0.0.1:{port}").parse().unwrap()
    }

    #[test]
    fn auth_mode_follows_endpoint_declaration() {
        let open = PluginChannel::new(&test_endpoint("open", EndpointAuth::None), addr(41001));
        assert_eq!(open.auth_mode(), AuthMode::None);
        assert_eq!(open.auth_timeout_close_code(), None, "none 模式无认证窗口");

        let guarded = PluginChannel::new(&test_endpoint("jwt", EndpointAuth::Jwt), addr(41002));
        assert_eq!(guarded.auth_mode(), AuthMode::Required);
        assert_eq!(guarded.auth_timeout_close_code(), Some(CLOSE_AUTH_FAILED));
    }

    #[test]
    fn client_id_is_peer_addr_key() {
        // clientId 必须与注册表会话键同源（对端地址字符串），否则 list-clients /
        // 单发寻址对不上号
        let channel = PluginChannel::new(&test_endpoint("cid", EndpointAuth::None), addr(41003));
        assert_eq!(channel.client_id, "127.0.0.1:41003");
    }

    #[test]
    fn connect_and_disconnect_guards_are_idempotent() {
        let mut channel = PluginChannel::new(&test_endpoint("guard", EndpointAuth::None), addr(41004));
        assert!(!channel.connected_announced);
        // 接入守卫：重复调用只认第一次（调用方在 on_auth_ok 里）
        channel.connected_announced = true;
        channel.connected_announced = true;
        assert!(!channel.disconnect_reported);
        // 断开守卫：未接入（认证失败）时不得上报
        channel.connected_announced = false;
        assert!(!channel.disconnect_reported);
    }

    #[test]
    fn auth_frame_requires_type_and_token() {
        // 形状契约（D8）：仅接受 {"type":"auth","token":"..."}
        let ok: AuthFrame = serde_json::from_str(r#"{"type":"auth","token":"t"}"#).unwrap();
        assert_eq!(ok.frame_type, "auth");
        assert_eq!(ok.token, "t");
        assert!(
            serde_json::from_str::<AuthFrame>(r#"{"type":"ping","token":"t"}"#).is_err()
                || serde_json::from_str::<AuthFrame>(r#"{"type":"ping","token":"t"}"#)
                    .map(|f| f.frame_type != "auth")
                    .unwrap()
        );
        // 缺 token / 非 JSON：形状不匹配 → 走「丢弃 + warn」分支（不 panic）
        assert!(serde_json::from_str::<AuthFrame>(r#"{"type":"auth"}"#).is_err());
        assert!(serde_json::from_str::<AuthFrame>("not json").is_err());
    }
}
