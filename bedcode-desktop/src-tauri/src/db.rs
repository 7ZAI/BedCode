//! Database module
//!
//! 数据库模块 - 连接管理、数据模型和 CRUD 操作

mod database;
mod models;
mod operations;

pub use database::Database;
pub use models::{Pairing, QuickAction, SessionConfig, Setting};
