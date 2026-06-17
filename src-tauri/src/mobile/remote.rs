//! Remote Module - 远程连接与通信
//!
//! 包含 WebSocket 连接管理、输出接收器、配对服务、请求构建器

pub mod connection;
pub mod http_client;
pub mod output_receiver;
pub mod pairing_service;
pub mod request;

// Re-export public types
pub use connection::{ConnectionManager, ConnectionStatus, TargetDevice};
pub use output_receiver::{OutputEvent, OutputReceiver};
pub use pairing_service::PairingService;
pub use request::{AuthRequest, SessionRequest, TerminalRequest, ConfigRequest, ResponseParser};
pub use http_client::{HttpClient, FileTreeApi};