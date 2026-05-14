//! Desktop Server Module
//!
//! 提供移动端远程控制功能的 WebSocket 服务

// 模块声明 - 使用目录名.rs模式
pub mod message;
pub mod client_info;
pub mod connection_types;
pub mod services;
pub mod handlers;
// pub mod handler;  // TODO: MessageHandler 实现（待完善）

// 重新导出所有公开类型
pub use message::*;
pub use client_info::ClientInfo;
pub use connection_types::*;
pub use handlers::ControlAction;

// Re-export DeviceConnectionInfo from connection module
pub use crate::desktop::server::connection_types::DeviceConnectionInfo;

// WebSocket Server 实现
use crate::desktop::server::connection_types::DeviceConnectionEvent as DevConnEvent;
use crate::desktop::plugin::PluginManager;
use crate::desktop::session::SessionManager;
use crate::desktop::server::client_info::ClientInfo as CliInfo;
use crate::desktop::server::handlers::{handle_auth, handle_control, handle_input};
use crate::desktop::server::message::Message as ServerMsg;
use crate::desktop::server::services::OutputForwarder as OutForwarder;
use crate::shared::auth::{PairingService, QrTokenManager};
use crate::shared::db::Database;
// TODO: 未来迁移到 shared WsServer
// use crate::shared::websocket::{WsServer, WsServerConfig, WsServerEvent};
use crate::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tauri::{AppHandle, Emitter};