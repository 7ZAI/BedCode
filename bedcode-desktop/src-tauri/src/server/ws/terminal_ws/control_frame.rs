//! 每会话终端路由（/ws/terminal/session/{id}）JSON 控制帧协议
//!
//! spec §5.3：控制帧为简化协议——无 message_id/expect_response 请求-响应
//! 机制，连接级状态机替代（auth → auth_ok → subscribe → 快照流）。
//! 与旧路由的 `Message` 枚举（带 message_id 的完整协议）互不相干：
//! 旧路由结构不变（spec §7 兼容策略），新路由专用此类型。

use serde::{Deserialize, Serialize};

use crate::enums::auth::CryptoProposal;
use crate::enums::special_key::KeyCombo;

/// 客户端 → 服务端控制帧
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    /// 首消息 JWT 认证（spec §4.3 规则与旧路由一致）；
    /// crypto 可选：携带即发起链路加密协商（issue 04，auth_ok 回带服务端临时公钥）
    Auth {
        token: String,
        #[serde(default)]
        crypto: Option<CryptoProposal>,
    },
    /// 订阅绑定会话（无参：连接创建即绑定，快照协议全量重播）
    Subscribe,
    /// PTY 输入（data 为 Base64；special_key 为按键组合字符串）
    Input {
        data: String,
        #[serde(default)]
        special_key: Option<KeyCombo>,
    },
}

/// 服务端 → 客户端加密协商回执载荷（auth_ok.crypto；v 固定 1）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CryptoEcho {
    pub v: u8,
    pub ek: String,
}

/// 服务端 → 客户端控制帧
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    /// JWT 认证成功（此后客户端可发 subscribe）；
    /// crypto 回执存在表示服务端已接受加密协商，此后所有帧进入加密模式
    AuthOk {
        #[serde(skip_serializing_if = "Option::is_none")]
        crypto: Option<CryptoEcho>,
    },
    /// 订阅已建立（快照元数据；历史帧在其后按序到达）
    SubscribeOk {
        /// 订阅时刻队列最新序号（历史边界）
        snapshot_seq: u64,
        /// 队列中最早存续事件序号（环形淘汰后推进）
        min_seq: u64,
        /// 历史事件数量
        history_count: usize,
    },
    /// 历史段结束标记（此后为实时帧；空历史也必发）
    HistoryEnd { snapshot_seq: u64 },
    /// 会话停止通知（服务端主动推送，此后连接不再有输出）
    SessionStopped { session_id: String },
    /// 错误（code 语义与旧路由 error 消息一致）
    Error { code: String, message: String },
}

impl ServerFrame {
    /// 序列化为 JSON 文本帧
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"type":"error","code":"SERIALIZE_ERROR","message":"failed to serialize frame"}"#.to_string()
        })
    }
}

/// 解析客户端控制帧（失败返回错误描述）
pub fn parse_client_frame(text: &str) -> Result<ClientFrame, String> {
    serde_json::from_str::<ClientFrame>(text).map_err(|e| format!("invalid control frame: {e}"))
}

// ==================== 背压 ack 帧（spec 04-06，二进制） ====================
// 复用 TB v2 帧头（magic(2) + version(1) + flags(1) + seq(8 LE) + len(4 LE) = 16
// 字节）：客户端→服务端方向，flags 置 ACK 标志位，seq = 已渲染到的
// last_rendered_seq，len + payload = session_id UTF-8 字节。与服务端→客户端
// 输出帧的 WAITING/事件数位语义互不冲突（该方向不解析 ack）

/// TB v2 帧头长度
pub const TB_FRAME_HEADER_LEN: usize = 16;
const TB_FRAME_MAGIC: [u8; 2] = [0x54, 0x42]; // "TB"
const TB_FRAME_VERSION: u8 = 2;
/// 背压 ack 标志位（仅客户端→服务端使用）
const TB_FRAME_FLAG_ACK: u8 = 0x02;

/// 解析客户端背压 ack 帧，返回 `(acked_seq, session_id)`
///
/// 非法帧（长度不足 / 魔数版本不符 / 非 ack 标志 / 长度越界 / 非 UTF-8）
/// 返回 `Err(())`——调用方记日志忽略，不中断连接（ack 尽力而为，丢失时
/// 由水位暂停兜底，不缺字节不丢帧）
pub fn parse_ack_frame(bytes: &[u8]) -> Result<(u64, String), ()> {
    if bytes.len() < TB_FRAME_HEADER_LEN {
        return Err(());
    }
    if bytes[0] != TB_FRAME_MAGIC[0] || bytes[1] != TB_FRAME_MAGIC[1] || bytes[2] != TB_FRAME_VERSION {
        return Err(());
    }
    if bytes[3] & TB_FRAME_FLAG_ACK == 0 {
        return Err(());
    }
    let seq = u64::from_le_bytes(bytes[4..12].try_into().map_err(|_| ())?);
    let len = u32::from_le_bytes(bytes[12..16].try_into().map_err(|_| ())?) as usize;
    if bytes.len() < TB_FRAME_HEADER_LEN + len {
        return Err(());
    }
    let session_id = String::from_utf8(bytes[16..16 + len].to_vec()).map_err(|_| ())?;
    Ok((seq, session_id))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_auth_frame() {
        let frame = parse_client_frame(r#"{"type":"auth","token":"jwt-token"}"#).unwrap();
        match frame {
            ClientFrame::Auth { token, crypto } => {
                assert_eq!(token, "jwt-token");
                assert!(crypto.is_none(), "无 crypto 字段应解析为 None（老客户端兼容）");
            }
            _ => panic!("expected auth frame"),
        }

        // 携带加密协商的 auth 帧
        let frame = parse_client_frame(
            r#"{"type":"auth","token":"t","crypto":{"v":1,"ek":"QUJDREVG"}}"#,
        )
        .unwrap();
        match frame {
            ClientFrame::Auth { token, crypto } => {
                assert_eq!(token, "t");
                let proposal = crypto.expect("crypto 应被解析");
                assert_eq!(proposal.v, 1);
                assert_eq!(proposal.ek, "QUJDREVG");
            }
            _ => panic!("expected auth frame with crypto"),
        }
    }

    #[test]
    fn parse_subscribe_frame() {
        let frame = parse_client_frame(r#"{"type":"subscribe"}"#).unwrap();
        assert!(matches!(frame, ClientFrame::Subscribe));
    }

    #[test]
    fn parse_input_frame_plain() {
        let frame = parse_client_frame(r#"{"type":"input","data":"aGVsbG8="}"#).unwrap();
        match frame {
            ClientFrame::Input { data, special_key } => {
                assert_eq!(data, "aGVsbG8=");
                assert!(special_key.is_none());
            }
            _ => panic!("expected input frame"),
        }
    }

    #[test]
    fn parse_input_frame_with_special_key() {
        let frame = parse_client_frame(r#"{"type":"input","data":"","special_key":"ctrl_c"}"#).unwrap();
        match frame {
            ClientFrame::Input { data, special_key } => {
                assert_eq!(data, "");
                assert!(special_key.is_some());
            }
            _ => panic!("expected input frame"),
        }
    }

    #[test]
    fn parse_unknown_type_rejected() {
        let err = parse_client_frame(r#"{"type":"unknown"}"#).unwrap_err();
        assert!(err.contains("unknown variant"), "got: {err}");
    }

    #[test]
    fn parse_malformed_rejected() {
        assert!(parse_client_frame("not json").is_err());
    }

    #[test]
    fn serialize_subscribe_ok() {
        let json = ServerFrame::SubscribeOk {
            snapshot_seq: 42,
            min_seq: 0,
            history_count: 42,
        }
        .to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "subscribe_ok");
        assert_eq!(v["snapshot_seq"], 42);
        assert_eq!(v["min_seq"], 0);
        assert_eq!(v["history_count"], 42);
    }

    #[test]
    fn serialize_history_end() {
        let json = ServerFrame::HistoryEnd { snapshot_seq: 7 }.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "history_end");
        assert_eq!(v["snapshot_seq"], 7);
    }

    #[test]
    fn serialize_auth_ok_and_error() {
        // 无协商：crypto 省略，老客户端报文形状不变
        let plain: serde_json::Value =
            serde_json::from_str(&ServerFrame::AuthOk { crypto: None }.to_json()).unwrap();
        assert_eq!(plain["type"], "auth_ok");
        assert!(plain.get("crypto").is_none());

        // 带协商回执
        let negotiated: serde_json::Value = serde_json::from_str(
            &ServerFrame::AuthOk {
                crypto: Some(CryptoEcho { v: 1, ek: "QUJDREVG".to_string() }),
            }
            .to_json(),
        )
        .unwrap();
        assert_eq!(negotiated["type"], "auth_ok");
        assert_eq!(negotiated["crypto"]["v"], 1);
        assert_eq!(negotiated["crypto"]["ek"], "QUJDREVG");

        let v: serde_json::Value = serde_json::from_str(
            &ServerFrame::Error {
                code: "SESSION_NOT_FOUND".into(),
                message: "Session s-1 not found".into(),
            }
            .to_json(),
        )
        .unwrap();
        assert_eq!(v["type"], "error");
        assert_eq!(v["code"], "SESSION_NOT_FOUND");
    }

    #[test]
    fn serialize_session_stopped() {
        let json = ServerFrame::SessionStopped {
            session_id: "s-9".into(),
        }
        .to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "session_stopped");
        assert_eq!(v["session_id"], "s-9");
    }

    /// 构造客户端背压 ack 帧（与前端 buildAckFrame 逐字节对齐）
    fn build_ack(session_id: &str, acked_seq: u64) -> Vec<u8> {
        let payload = session_id.as_bytes();
        let mut frame = Vec::with_capacity(TB_FRAME_HEADER_LEN + payload.len());
        frame.extend_from_slice(&TB_FRAME_MAGIC);
        frame.push(TB_FRAME_VERSION);
        frame.push(TB_FRAME_FLAG_ACK);
        frame.extend_from_slice(&acked_seq.to_le_bytes());
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    #[test]
    fn parse_ack_frame_valid() {
        let bytes = build_ack("session-7", 12345);
        assert_eq!(parse_ack_frame(&bytes).unwrap(), (12345, "session-7".to_string()));
        // 超长负载（len 精确指向 payload 结尾）也能解析
        let bytes = build_ack("a-really-long-session-id", u64::MAX);
        assert_eq!(
            parse_ack_frame(&bytes).unwrap(),
            (u64::MAX, "a-really-long-session-id".to_string())
        );
    }

    #[test]
    fn parse_ack_frame_rejects_malformed() {
        // 长度不足
        assert!(parse_ack_frame(&[0x54, 0x42]).is_err());
        // 魔数/版本不符
        let mut bad = build_ack("s1", 1);
        bad[1] = 0x58;
        assert!(parse_ack_frame(&bad).is_err());
        // 非 ack 标志（装作普通输出帧 flags=0x00）
        let mut no_flag = build_ack("s1", 1);
        no_flag[3] = 0x00;
        assert!(parse_ack_frame(&no_flag).is_err());
        // len 声明超出实际
        let mut short_payload = build_ack("s1", 1);
        short_payload.truncate(short_payload.len() - 1);
        let _ = short_payload; // 注意：截尾后 len 字段不变 → 越界判定命中
        assert!(parse_ack_frame(&short_payload).is_err());
        // session_id 非 UTF-8
        let mut non_utf8 = build_ack("s1", 1);
        non_utf8[TB_FRAME_HEADER_LEN] = 0xFF;
        let _ = non_utf8;
        assert!(parse_ack_frame(&non_utf8).is_err());
    }

    #[test]
    fn parse_ack_frame_roundtrip_offsets() {
        // 前端帧头布局逐字节断言（magic/version/flags/seq(8 LE)/len(4 LE)）
        let bytes = build_ack("sv", 0x0102030405060708);
        assert_eq!(&bytes[0..2], &[0x54, 0x42]);
        assert_eq!(bytes[2], 2);
        assert_eq!(bytes[3], TB_FRAME_FLAG_ACK);
        assert_eq!(&bytes[4..12], &0x0102030405060708u64.to_le_bytes());
        assert_eq!(&bytes[12..16], &2u32.to_le_bytes());
        assert_eq!(&bytes[16..18], b"sv");
    }
}
