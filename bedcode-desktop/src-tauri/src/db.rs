//! Database module
//!
//! 数据库模块 - 连接管理、数据模型和 CRUD 操作

mod database;
mod models;
mod operations;

pub use database::Database;
pub use models::{
    connection_method, connection_result, ConnectionHistory, Pairing, QuickAction, SessionConfig, Setting,
    CONNECTION_HISTORY_MAX_PER_DEVICE,
};
