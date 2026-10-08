//! WebSocket 出站连接能力（WIT `host-websocket` **客户端域**，ABI v14，ADR 0022）
//!
//! 纯传输原语：连接生命周期 + 帧收发 + 断连事实。**订阅协议 / 心跳 / 退避重连 /
//! ack-resync 等编排一律归插件**（票 12 迁`terminal_link` 时自带）。
//!
//! 移动端只做客户端域（5 函数），服务端域 9 函数与 `connection-context` 不跟演
//! （ADR 0018 移动端是消费端，不跑 WS 服务器）；不引入 `ws:server` 权限位。
//!
//! 权限位 `ws:client`：出站连接是 SSRF 面（插件可代宿主访问任意 ws:// 地址），
//! fail-closed，未在 manifest 声明即一律拒绝。
//!
//! # 两条投递通道（务必按此订阅）
//!
//! 1. **状态事件**走消息总线属主私有 topic（JSON，`host-bus` 的 `subscribe`）：
//!
//!    | topic | payload |
//!    | --- | --- |
//!    | `<plugin-id>:ws:open` | `{ handle, url, protocol? }` |
//!    | `<plugin-id>:ws:error` | `{ handle, message }` |
//!    | `<plugin-id>:ws:close` | `{ handle, code?, reason?, wasClean }` |
//!
//!    宿主不缓冲、不重放：**必须在 `activate` 期（首次 `connect` 之前）订阅**，
//!    晚订阅期间的事件永久丢失（不报错，只静默丢）；丢失后自愈靠
//!    [`HostWs::ws_is_connected`]。用 [`ws_event_topic`] 生成 topic，勿手拼。
//! 2. **消息帧**走二进制属主私有 topic [`ws_message_topic`]（`host-bus` 的
//!    `subscribe-binary`）：载荷为帧信封，解析见 [`WsIncomingFrame`]。
//!    零 JSON 编解码（spec C3 性能红线：输出字节禁止经 JSON 命令通道搬运）。
//!
//! 两条通道都经消息总线 ⇒ **manifest 须同时声明 `bus` 权限位**（仅 `ws:client`
//! 时订阅被 host-bus 权限门拒绝，事件与帧静默不可达）。

use crate::host::HostError;

// ==================== 状态事件 topic（属主私有命名空间，勿手拼） ====================

/// 连接建立（客户端域）
pub const WS_OPEN: &str = "ws:open";
/// 连接错误（客户端域）
pub const WS_ERROR: &str = "ws:error";
/// 连接关闭（客户端域）
pub const WS_CLOSE: &str = "ws:close";
/// 重连退避排期（客户端域，ABI v15：config 声明 `auto-reconnect` 时宿主在
/// 每轮退避前发布）payload `{ handle, retryInMs }`（handle 为旧句柄）
pub const WS_RECONNECT_SCHEDULED: &str = "ws:reconnect-scheduled";

/// 生成属主私有状态事件 topic：`<plugin-id>:ws:<event>`
///
/// `plugin_id` 必须传本插件 ID。`event` 用本模块的 `WS_*` 常量——手拼易错，
/// 而拼错的后果是「订阅了却永远收不到」（漏订阅不报错，只静默丢事件）。
pub fn ws_event_topic(event: &str, plugin_id: &str) -> String {
    format!("{plugin_id}:{event}")
}

/// 二进制帧 topic 后缀（属主私有；经 `host-bus` 的 `subscribe-binary` 订阅）
pub const WS_MESSAGE: &str = "ws:message";

/// 生成属主私有帧 topic：`<plugin-id>:ws:message`
pub fn ws_message_topic(plugin_id: &str) -> String {
    ws_event_topic(WS_MESSAGE, plugin_id)
}

// ==================== 帧信封（宿主 → 插件的入站帧） ====================

/// 帧类型：文本
pub const WS_FRAME_KIND_TEXT: u8 = 1;
/// 帧类型：二进制
pub const WS_FRAME_KIND_BINARY: u8 = 2;

/// 帧信封头部长度：`kind(1) + handle 长度 u16 BE(2)`
pub const WS_FRAME_HEADER_LEN: usize = 3;

/// 解析后的入站帧（零拷贝视图：借用宿主投递的字节缓冲）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsIncomingFrame<'a> {
    /// 帧类型（[`WS_FRAME_KIND_TEXT`] / [`WS_FRAME_KIND_BINARY`]）
    pub kind: u8,
    /// 连接句柄（`wsc-<uuid>`）
    pub handle: &'a str,
    /// 原始帧字节（文本帧为 UTF-8 文本的字节）
    pub payload: &'a [u8],
}

impl WsIncomingFrame<'_> {
    /// 文本帧解码为`&str`（非法 UTF-8 返回 `None`，不 panic）
    pub fn as_text(&self) -> Option<&str> {
        std::str::from_utf8(self.payload).ok()
    }
}

/// 解析帧信封；格式不符（头部长度 / handle 长度越界 / 未知 kind）返回 `Err`。
///
/// 插件侧**不要手写这段解析**：宿主与 SDK 的信封形状必须同步演进。
pub fn parse_ws_frame(bytes: &[u8]) -> Result<WsIncomingFrame<'_>, String> {
    if bytes.len() < WS_FRAME_HEADER_LEN {
        return Err(format!(
            "ws frame too short: {} bytes (< header {WS_FRAME_HEADER_LEN})",
            bytes.len()
        ));
    }
    let kind = bytes[0];
    if kind != WS_FRAME_KIND_TEXT && kind != WS_FRAME_KIND_BINARY {
        return Err(format!("ws frame: unknown kind {kind}"));
    }
    let handle_len = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    let body = &bytes[WS_FRAME_HEADER_LEN..];
    if handle_len > body.len() {
        return Err(format!(
            "ws frame: handle length {handle_len} exceeds payload {}",
            body.len()
        ));
    }
    let handle = std::str::from_utf8(&body[..handle_len])
        .map_err(|e| format!("ws frame: handle is not utf-8: {e}"))?;
    Ok(WsIncomingFrame {
        kind,
        handle,
        payload: &body[handle_len..],
    })
}

// ==================== 能力 trait ====================

/// WebSocket 出站连接能力 trait —— 函数签名与 WIT `host-websocket` 一一对应
pub trait HostWs {
    /// 建立出站 WS 连接（**同步阻塞至握手完成**），返回句柄 `wsc-<uuid>`。
    ///
    /// config-json（camelCase）：`{ url, headers?, protocols?, connect-timeout-secs?,
    /// max-message-bytes? }`；url仅接受 `ws://`。
    /// 失败（权限拒 / 握手失败 / 超时 / 达到本插件连接数上限）时**不发布任何事件**。
    fn ws_connect(&self, config_json: &str) -> Result<String, HostError>;

    /// 发送文本帧（UTF-8）
    fn ws_send_text(&self, handle: &str, text: &str) -> Result<(), HostError>;

    /// 发送二进制帧
    fn ws_send_binary(&self, handle: &str, payload: &[u8]) -> Result<(), HostError>;

    /// 主动关闭连接（close-json：`{ code?, reason? }`，缺省 1000）；
    /// 返回是否命中该句柄（幂等：未知句柄 false）
    fn ws_close(&self, handle: &str, close_json: &str) -> Result<bool, HostError>;

    /// 查询连接是否 open（握手完成且未关闭）；仅属主可查，句柄不存在返回 false
    fn ws_is_connected(&self, handle: &str) -> Result<bool, HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造帧信封（与宿主 `host_impl::ws::frame_envelope` 同形状）
    fn envelope(kind: u8, handle: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend_from_slice(&(handle.len() as u16).to_be_bytes());
        out.extend_from_slice(handle.as_bytes());
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn parse_text_frame_roundtrip() {
        let raw = envelope(WS_FRAME_KIND_TEXT, "wsc-abc", "hello".as_bytes());
        let f = parse_ws_frame(&raw).expect("parse ok");
        assert_eq!(f.kind, WS_FRAME_KIND_TEXT);
        assert_eq!(f.handle, "wsc-abc");
        assert_eq!(f.as_text(), Some("hello"));
    }

    #[test]
    fn parse_binary_frame_keeps_raw_bytes() {
        // 非 UTF-8 载荷必须原样保留（终端输出是任意字节，不能按文本处理）
        let raw = envelope(WS_FRAME_KIND_BINARY, "wsc-1", &[0xff, 0x00, 0x1b, 0x5b]);
        let f = parse_ws_frame(&raw).expect("parse ok");
        assert_eq!(f.kind, WS_FRAME_KIND_BINARY);
        assert_eq!(f.payload, &[0xff, 0x00, 0x1b, 0x5b]);
        assert_eq!(f.as_text(), None);
    }

    #[test]
    fn parse_empty_payload_is_valid() {
        // 空文本帧合法（对端可发零长帧）
        let raw = envelope(WS_FRAME_KIND_TEXT, "wsc-1", b"");
        let f = parse_ws_frame(&raw).expect("parse ok");
        assert_eq!(f.payload, b"");
        assert_eq!(f.as_text(), Some(""));
    }

    #[test]
    fn parse_rejects_short_buffer() {
        // 头部长度不足 → 显性错误，不 panic、不越界读
        assert!(parse_ws_frame(&[WS_FRAME_KIND_TEXT, 0]).is_err());
        assert!(parse_ws_frame(&[]).is_err());
    }

    #[test]
    fn parse_rejects_unknown_kind() {
        let mut raw = envelope(WS_FRAME_KIND_TEXT, "wsc-1", b"x");
        raw[0] = 9;
        assert!(parse_ws_frame(&raw).is_err());
    }

    #[test]
    fn parse_rejects_handle_length_overflow() {
        // handle 长度声明超过实际载荷 → 显性错误（防越界切片 panic）
        let raw = envelope(WS_FRAME_KIND_TEXT, "wsc-1", b"x");
        let mut bad = raw.clone();
        bad[1..3].copy_from_slice(&9999u16.to_be_bytes());
        assert!(parse_ws_frame(&bad).is_err());
    }

    #[test]
    fn event_topics_are_owner_private() {
        // topic 必须是属主私有（他人订阅被宿主总线按plugin_id 过滤）
        assert_eq!(ws_event_topic(WS_OPEN, "com.bedcode.x"), "com.bedcode.x:ws:open");
        assert_eq!(ws_event_topic(WS_CLOSE, "com.bedcode.x"), "com.bedcode.x:ws:close");
        assert_eq!(
            ws_event_topic(WS_RECONNECT_SCHEDULED, "com.bedcode.x"),
            "com.bedcode.x:ws:reconnect-scheduled"
        );
        assert_eq!(ws_message_topic("com.bedcode.x"), "com.bedcode.x:ws:message");
    }
}