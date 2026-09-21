//! WebSocket 基础能力服务（WIT `host-websocket`，ABI v14）
//!
//! 宿主 WS 传输原语，插件零业务语义（ADR 0022）：客户端域（出站连接）与服务端域
//! （入站端点）。**全部函数仅属主可调**：他人句柄/端点 → `Err`；
//! 插件停用时宿主自动回收其全部句柄与端点。
//!
//! # 事件与帧的两条投递通道（务必按此订阅）
//!
//! 1. **状态事件**走消息总线属主私有 topic（用 [`ws_event_topic`] 或
//!    [`WS_OPEN`] / [`WS_ERROR`] / [`WS_CLOSE`] / [`WS_CLIENT_CONNECT`] /
//!    [`WS_CLIENT_DISCONNECT`] 生成，勿手拼）：
//!
//!    | topic | payload |
//!    | --- | --- |
//!    | `<owner>::ws:open` | `{ handle, url, protocol? }` |
//!    | `<owner>::ws:error` | `{ handle, message }` |
//!    | `<owner>::ws:close` | `{ handle, code?, reason?, wasClean }` |
//!    | `<owner>::ws:client-connect` | `{ endpointId, clientId, addr, authenticated }` |
//!    | `<owner>::ws:client-disconnect` | `{ endpointId, clientId, code?, reason?, wasClean }` |
//!
//!    **必须在 `activate` 期（或首次 connect / register-endpoint 之前）完成
//!    `bus_subscribe`**：宿主不缓冲、不重放，晚订阅期间的事件永久丢失
//!    （不报错，只静默丢事件）。丢失后的自愈靠快照查询原语：
//!    `ws_is_connected` / `ws_list_clients` / `ws_list_endpoints`。
//! 2. **消息帧**（text + binary）经 [`crate::wasm::WasmPlugin::on_ws_message`] /
//!    `on_ws_client_message` 回调投递（同连接内保序）；插件未实现这两个回调时
//!    宿主丢弃消息帧并打印一次 `warn`（状态事件仍照常投递）。

use crate::host::HostError;

// ==================== 状态事件 topic（owner 作用域，勿手拼） ====================

/// 连接建立（客户端域）
pub const WS_OPEN: &str = "ws:open";
/// 连接错误（客户端域）
pub const WS_ERROR: &str = "ws:error";
/// 连接关闭（客户端域）
pub const WS_CLOSE: &str = "ws:close";
/// 端点客户端接入（服务端域）
pub const WS_CLIENT_CONNECT: &str = "ws:client-connect";
/// 端点客户端断开（服务端域）
pub const WS_CLIENT_DISCONNECT: &str = "ws:client-disconnect";

/// 生成属主私有状态事件 topic：`<owner>::ws:<event>`
///
/// `owner` 必须传本插件 ID（票 05 命名空间：宿主把状态事件定向投进属主
/// 收件箱，他人订阅被宿主拒绝）。`event` 用本模块的 `WS_*` 常量，避免手拼
/// 拼错导致「订阅了却永远收不到」（漏订阅不报错）。
pub fn ws_event_topic(event: &str, plugin_id: &str) -> String {
    super::bus::owned_topic(plugin_id, event)
}

/// WebSocket 能力 trait —— 函数签名与 WIT `host-websocket` 一一对应
pub trait HostWebsocket {
    // ==================== 客户端域（出站连接） ====================

    /// 建立出站 WS 连接（**同步阻塞至握手完成**，上限 `connect-timeout-secs`）。
    ///
    /// config-json（camelCase）：
    /// `{ url, headers?, protocols?, connect-timeout-secs?, max-message-bytes? }`；
    /// `url` **仅接受 `ws://`**（`wss://` 本期不支持，返回明确错误）。
    /// 成功 → 返回连接句柄 `wsc-<uuid>` 并发布 [`WS_OPEN`] 事件；
    /// 失败 → 错误上抛且**不发布任何事件**。
    fn ws_connect(&self, config_json: &str) -> Result<String, HostError>;
    /// 发送文本帧；连接不存在 / 已关闭 / 发送队列满 → 错误
    fn ws_send_text(&self, handle: &str, text: &str) -> Result<(), HostError>;
    /// 发送二进制帧
    fn ws_send_binary(&self, handle: &str, payload: &[u8]) -> Result<(), HostError>;
    /// 主动关闭连接（`{ code?, reason? }`，缺省 1000）；返回是否命中
    fn ws_close(&self, handle: &str, close_json: &str) -> Result<bool, HostError>;
    /// 查询连接是否处于 open 态（丢失状态事件后的自愈入口）
    fn ws_is_connected(&self, handle: &str) -> Result<bool, HostError>;

    // ==================== 服务端域（入站端点） ====================

    /// 注册插件端点（实际路径 `/ws/plugin/<plugin-id>/<path>`，命名空间由宿主注入）。
    ///
    /// config-json（camelCase）：
    /// `{ path, auth?, max-message-bytes?, max-clients? }`
    /// `auth = "none"`（默认，插件自管认证）| `"jwt"`（宿主校验首消息
    /// `{"type":"auth","token":"<jwt>"}`）
    fn ws_register_endpoint(&self, config_json: &str) -> Result<String, HostError>;
    /// 向端点指定客户端发文本帧
    fn ws_send_text_to_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        text: &str,
    ) -> Result<(), HostError>;
    /// 向端点指定客户端发二进制帧
    fn ws_send_binary_to_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        payload: &[u8],
    ) -> Result<(), HostError>;
    /// 向端点全部客户端广播文本帧 → 成功入队客户端数
    fn ws_broadcast_text(&self, endpoint_id: &str, text: &str) -> Result<u32, HostError>;
    /// 向端点全部客户端广播二进制帧 → 成功入队客户端数
    fn ws_broadcast_binary(&self, endpoint_id: &str, payload: &[u8]) -> Result<u32, HostError>;
    /// 踢出端点指定客户端（`{ code?, reason? }`，缺省 4004）；返回是否命中
    fn ws_close_client(
        &self,
        endpoint_id: &str,
        client_id: &str,
        close_json: &str,
    ) -> Result<bool, HostError>;
    /// 关闭端点并回收句柄（含下线全部客户端）；返回是否存在该端点
    fn ws_unregister_endpoint(&self, endpoint_id: &str) -> Result<bool, HostError>;
    /// 端点在线的客户端清单（JSON 数组字符串）
    fn ws_list_clients(&self, endpoint_id: &str) -> Result<String, HostError>;
    /// 本插件已注册端点清单（JSON 数组字符串）
    fn ws_list_endpoints(&self) -> Result<String, HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_event_topic_uses_owner_namespace() {
        // 形状由 `bus::owned_topic` 单源决定：宿主投递侧与本构造侧共用同一函数，
        // 不存在「逐字节一致」漂移面（票 05）
        assert_eq!(ws_event_topic(WS_OPEN, "com.x"), "com.x::ws:open");
        assert_eq!(ws_event_topic(WS_CLOSE, "com.x"), "com.x::ws:close");
        assert_eq!(ws_event_topic(WS_ERROR, "com.x"), "com.x::ws:error");
        assert_eq!(
            ws_event_topic(WS_CLIENT_CONNECT, "com.x"),
            "com.x::ws:client-connect"
        );
        assert_eq!(
            ws_event_topic(WS_CLIENT_DISCONNECT, "com.x"),
            "com.x::ws:client-disconnect"
        );
        // 属主隔离：不同插件的 topic 互不相等（他人订阅被宿主拒绝）
        assert_ne!(ws_event_topic(WS_OPEN, "a"), ws_event_topic(WS_OPEN, "b"));
    }
}
