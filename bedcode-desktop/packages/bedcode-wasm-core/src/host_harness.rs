//! 测试 harness（wasm-core-whole-crate §4.5，`#[cfg(test)]`）
//!
//! `ws_e2e.rs` 等 crate 内测试要起 HTTP+WS 服务器，原经 lib 的
//! `crate::host_harness::start_http_server`（lib 组合根）。迁入 crate 后
//! 该助手无法再引用 lib（循环），故在 crate 测试内给一个本地实现——`composition.rs`
//! 的 `start_http_server` 只是 `bedcode_server_core::app::serve(port, config,
//! transport_faces())` 的薄壳，而 `transport_faces()` 只是两个 face 结构体。
//!
//! 本 harness 同时是「第三方宿主如何装配本 crate」的可执行范例（测试缝即复用缝）。

use bedcode_server_base::config::NetworkConfig;
use std::sync::Arc;

/// 与 lib `server::composition::transport_faces()` 同款（面 crate 结构体）
pub(crate) fn transport_faces() -> Vec<Arc<dyn bedcode_server_core::app::TransportFace>> {
    vec![
        Arc::new(bedcode_server_http::HttpTransportFace),
        Arc::new(bedcode_server_websocket::WebSocketTransportFace),
    ]
}

/// 与 lib `server::composition::start_http_server` 同款（薄壳）
pub(crate) async fn start_http_server(
    port: u16,
    config: &NetworkConfig,
) -> std::io::Result<(
    actix_web::dev::ServerHandle,
    impl std::future::Future<Output = std::io::Result<()>>,
)> {
    bedcode_server_core::app::serve(port, config, transport_faces()).await
}
