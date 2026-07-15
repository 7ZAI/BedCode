//! Custom SQLite Tables (Mobile)
//!
//! ai-chatbox 插件的自定义数据库表操作
//! 所有表名以 plugin_com_bedcode_ai_chatbox_ 为前缀，确保宿主校验通过

use bedcode_plugin_api_mobile::WasmHost;
use serde::{Deserialize, Serialize};

/// 对话元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub provider_name: String,
}

/// 消息记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageRecord {
    pub id: i64,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

const TABLE_CONVERSATIONS: &str = "plugin_com_bedcode_ai_chatbox_conversations";
const TABLE_MESSAGES: &str = "plugin_com_bedcode_ai_chatbox_messages";

/// 将 Rust 值转为 SQL 字面量（安全，无注入风险）
fn sql_value<T: serde::Serialize>(val: &T) -> String {
    match serde_json::to_value(val) {
        Ok(serde_json::Value::String(s)) => {
            let escaped = s.replace('\'', "''");
            format!("'{}'", escaped)
        }
        Ok(serde_json::Value::Number(n)) => n.to_string(),
        Ok(serde_json::Value::Bool(b)) => if b { "1" } else { "0" }.to_string(),
        Ok(serde_json::Value::Null) => "NULL".to_string(),
        _ => "NULL".to_string(),
    }
}

/// 初始化自定义数据库表
pub fn init(host: &WasmHost) -> anyhow::Result<()> {
    let sql1 = format!(
        "CREATE TABLE IF NOT EXISTS {} (\
            id TEXT PRIMARY KEY, \
            title TEXT NOT NULL, \
            created_at TEXT NOT NULL, \
            updated_at TEXT NOT NULL, \
            provider_name TEXT NOT NULL\
        )",
        TABLE_CONVERSATIONS
    );

    let sql2 = format!(
        "CREATE TABLE IF NOT EXISTS {} (\
            id INTEGER PRIMARY KEY AUTOINCREMENT, \
            conversation_id TEXT NOT NULL, \
            role TEXT NOT NULL, \
            content TEXT NOT NULL, \
            timestamp TEXT NOT NULL, \
            FOREIGN KEY (conversation_id) REFERENCES {}(id)\
        )",
        TABLE_MESSAGES, TABLE_CONVERSATIONS
    );

    let result1 = host.db_execute(&sql1);
    if result1 < 0 {
        return Err(anyhow::anyhow!("Failed to create conversations table: error code {}", result1));
    }

    let result2 = host.db_execute(&sql2);
    if result2 < 0 {
        return Err(anyhow::anyhow!("Failed to create messages table: error code {}", result2));
    }

    host.log_info("Custom DB tables initialized");
    Ok(())
}

/// 列出所有对话
pub fn list_conversations(host: &WasmHost) -> anyhow::Result<Vec<ConversationMeta>> {
    let sql = format!(
        "SELECT id, title, created_at, updated_at, provider_name FROM {} ORDER BY updated_at DESC",
        TABLE_CONVERSATIONS
    );

    let rows = host.db_query(&sql)
        .ok_or_else(|| anyhow::anyhow!("Failed to query conversations"))?;

    let conversations: Vec<ConversationMeta> = serde_json::from_value(rows)
        .unwrap_or_default();

    Ok(conversations)
}

/// 获取对话的所有消息
pub fn get_messages(host: &WasmHost, conversation_id: &str) -> anyhow::Result<Vec<ChatMessageRecord>> {
    let sql = format!(
        "SELECT id, conversation_id, role, content, timestamp FROM {} WHERE conversation_id = {} ORDER BY timestamp ASC",
        TABLE_MESSAGES, sql_value(&conversation_id)
    );

    let rows = host.db_query(&sql)
        .ok_or_else(|| anyhow::anyhow!("Failed to query messages"))?;

    let messages: Vec<ChatMessageRecord> = serde_json::from_value(rows)
        .unwrap_or_default();

    Ok(messages)
}

/// 保存对话（INSERT OR REPLACE）
pub fn save_conversation(host: &WasmHost, conv: &ConversationMeta) -> anyhow::Result<()> {
    let sql = format!(
        "INSERT OR REPLACE INTO {} (id, title, created_at, updated_at, provider_name) VALUES ({}, {}, {}, {}, {})",
        TABLE_CONVERSATIONS,
        sql_value(&conv.id),
        sql_value(&conv.title),
        sql_value(&conv.created_at),
        sql_value(&conv.updated_at),
        sql_value(&conv.provider_name)
    );

    let result = host.db_execute(&sql);
    if result < 0 {
        return Err(anyhow::anyhow!("Failed to save conversation: error code {}", result));
    }
    Ok(())
}

/// 保存消息
pub fn save_message(host: &WasmHost, conversation_id: &str, role: &str, content: &str, timestamp: &str) -> anyhow::Result<()> {
    let sql = format!(
        "INSERT INTO {} (conversation_id, role, content, timestamp) VALUES ({}, {}, {}, {})",
        TABLE_MESSAGES,
        sql_value(&conversation_id),
        sql_value(&role),
        sql_value(&content),
        sql_value(&timestamp)
    );

    let result = host.db_execute(&sql);
    if result < 0 {
        return Err(anyhow::anyhow!("Failed to save message: error code {}", result));
    }
    Ok(())
}

/// 删除对话及其所有消息
pub fn delete_conversation(host: &WasmHost, conversation_id: &str) -> anyhow::Result<()> {
    let sql_msgs = format!(
        "DELETE FROM {} WHERE conversation_id = {}",
        TABLE_MESSAGES,
        sql_value(&conversation_id)
    );
    host.db_execute(&sql_msgs);

    let sql_conv = format!(
        "DELETE FROM {} WHERE id = {}",
        TABLE_CONVERSATIONS,
        sql_value(&conversation_id)
    );
    let result = host.db_execute(&sql_conv);
    if result < 0 {
        return Err(anyhow::anyhow!("Failed to delete conversation: error code {}", result));
    }
    Ok(())
}
