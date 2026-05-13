//! Database module
//!
//! 数据库模块 - 跨平台共享
//!
//! 模块划分:
//! - models.rs: 数据模型定义
//! - operations.rs: 数据库操作实现
//! - database.rs: 数据库连接管理
//! - schema.sql: 数据库表结构

mod database;
mod models;
mod operations;

pub use database::Database;
pub use models::{Pairing, QuickAction, SessionConfig, Setting};